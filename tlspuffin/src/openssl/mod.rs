use std::{
    any::Any,
    cell::RefCell,
    fmt::{Debug, Formatter},
    io,
    io::ErrorKind,
    rc::Rc,
};

use openssl::{
    error::ErrorStack,
    pkey::{PKeyRef, Private},
    ssl::{Ssl, SslContext, SslMethod, SslStream, SslVerifyMode},
    stack::Stack,
    x509::{
        store::{X509Store, X509StoreBuilder},
        X509Ref, X509StoreContext, X509,
    },
};
use rustls::msgs::message::OpaqueMessage;
use smallvec::SmallVec;

use crate::{
    agent::{AgentDescriptor, AgentName, AgentType, TLSVersion},
    claims::{
        Claim, ClaimData, ClaimDataMessage, ClaimDataTranscript, ClientHello, Finished,
        TlsTranscript, TranscriptCertificate, TranscriptClientFinished, TranscriptClientHello,
        TranscriptPartialClientHello, TranscriptServerFinished, TranscriptServerHello,
    },
    error::Error,
    io::{MemoryStream, MessageResult, Stream},
    openssl::util::{set_max_protocol_version, static_rsa_cert},
    put::{Put, PutConfig, PutName},
    put_registry::{Factory, OPENSSL111_PUT},
    static_certs::{ALICE_CERT, ALICE_PRIVATE_KEY, BOB_CERT, BOB_PRIVATE_KEY, EVE_CERT},
};

#[cfg(feature = "deterministic")]
mod deterministic;
mod util;

/*
   Change openssl version:
   cargo clean -p openssl-src
   cd openssl-src/openssl
   git checkout OpenSSL_1_1_1j
*/

pub fn new_openssl_factory() -> Box<dyn Factory> {
    struct OpenSSLFactory;
    impl Factory for OpenSSLFactory {
        fn create(
            &self,
            agent: &AgentDescriptor,
            config: PutConfig,
        ) -> Result<Box<dyn Put>, Error> {
            Ok(Box::new(OpenSSL::new(agent, config)?))
        }

        fn put_name(&self) -> PutName {
            OPENSSL111_PUT
        }

        fn put_version(&self) -> &'static str {
            OpenSSL::version()
        }

        fn make_deterministic(&self) {
            OpenSSL::make_deterministic()
        }
    }

    Box::new(OpenSSLFactory)
}

impl From<ErrorStack> for Error {
    fn from(err: ErrorStack) -> Self {
        Error::OpenSSL(err.to_string())
    }
}

pub struct OpenSSL {
    stream: SslStream<MemoryStream>,
    config: PutConfig,
}

impl Drop for OpenSSL {
    fn drop(&mut self) {
        #[cfg(feature = "claims")]
        self.deregister_claimer();
    }
}

impl Stream for OpenSSL {
    fn add_to_inbound(&mut self, result: &OpaqueMessage) {
        self.stream.get_mut().add_to_inbound(result)
    }

    fn take_message_from_outbound(&mut self) -> Result<Option<MessageResult>, Error> {
        self.stream.get_mut().take_message_from_outbound()
    }
}

fn to_claim_data(protocol_version: TLSVersion, claim: security_claims::Claim) -> Option<ClaimData> {
    match claim.typ {
        // Transcripts
        security_claims::ClaimType::CLAIM_TRANSCRIPT_CH => Some(ClaimData::Transcript(
            ClaimDataTranscript::ClientHello(TranscriptClientHello(TlsTranscript(
                claim.transcript.data,
                claim.transcript.length,
            ))),
        )),
        security_claims::ClaimType::CLAIM_TRANSCRIPT_PARTIAL_CH => Some(ClaimData::Transcript(
            ClaimDataTranscript::PartialClientHello(TranscriptPartialClientHello(TlsTranscript(
                claim.transcript.data,
                claim.transcript.length,
            ))),
        )),
        security_claims::ClaimType::CLAIM_TRANSCRIPT_CH_SH => Some(ClaimData::Transcript(
            ClaimDataTranscript::ServerHello(TranscriptServerHello(TlsTranscript(
                claim.transcript.data,
                claim.transcript.length,
            ))),
        )),
        security_claims::ClaimType::CLAIM_TRANSCRIPT_CH_SERVER_FIN => Some(ClaimData::Transcript(
            ClaimDataTranscript::ServerFinished(TranscriptServerFinished(TlsTranscript(
                claim.transcript.data,
                claim.transcript.length,
            ))),
        )),
        security_claims::ClaimType::CLAIM_TRANSCRIPT_CH_CLIENT_FIN => Some(ClaimData::Transcript(
            ClaimDataTranscript::ClientFinished(TranscriptClientFinished(TlsTranscript(
                claim.transcript.data,
                claim.transcript.length,
            ))),
        )),
        security_claims::ClaimType::CLAIM_TRANSCRIPT_CH_CERT => Some(ClaimData::Transcript(
            ClaimDataTranscript::Certificate(TranscriptCertificate(TlsTranscript(
                claim.transcript.data,
                claim.transcript.length,
            ))),
        )),
        // Messages
        // Transcripts in these messages are not up-to-date. They get updated after the Message has
        // been processed
        security_claims::ClaimType::CLAIM_FINISHED => {
            Some(ClaimData::Message(ClaimDataMessage::Finished(Finished {
                outbound: claim.write > 0,
                client_random: SmallVec::from(claim.client_random.data),
                server_random: SmallVec::from(claim.server_random.data),
                session_id: SmallVec::from_slice(
                    &claim.session_id.data[..claim.session_id.length as usize],
                ),
                authenticate_peer: false,             // FIXME
                peer_certificate: Default::default(), // FIXME
                master_secret: match protocol_version {
                    TLSVersion::V1_3 => SmallVec::from_slice(&claim.master_secret.secret),
                    TLSVersion::V1_2 => SmallVec::from_slice(&claim.master_secret_12.secret),
                },
                chosen_cipher: claim.chosen_cipher.data,
                available_ciphers: SmallVec::from_iter(
                    claim.available_ciphers.ciphers[..claim.available_ciphers.length as usize]
                        .iter()
                        .map(|cipher| cipher.data),
                ),
                signature_algorithm: claim.signature_algorithm,
                peer_signature_algorithm: claim.peer_signature_algorithm,
            })))
        }
        security_claims::ClaimType::CLAIM_CLIENT_HELLO => None,
        security_claims::ClaimType::CLAIM_CCS => None,
        security_claims::ClaimType::CLAIM_END_OF_EARLY_DATA => None,
        security_claims::ClaimType::CLAIM_CERTIFICATE => None,
        security_claims::ClaimType::CLAIM_KEY_EXCHANGE => None,
        // FIXME it is weird that this returns the correct transcript
        security_claims::ClaimType::CLAIM_CERTIFICATE_VERIFY => {
            if claim.write == 0 {
                Some(ClaimData::Transcript(ClaimDataTranscript::ServerFinished(
                    TranscriptServerFinished(TlsTranscript(
                        claim.transcript.data,
                        claim.transcript.length,
                    )),
                )))
            } else {
                None
            }
        }
        security_claims::ClaimType::CLAIM_KEY_UPDATE => None,
        security_claims::ClaimType::CLAIM_HELLO_REQUEST => None,
        security_claims::ClaimType::CLAIM_SERVER_HELLO => None,
        security_claims::ClaimType::CLAIM_CERTIFICATE_REQUEST => None,
        security_claims::ClaimType::CLAIM_SERVER_DONE => None,
        security_claims::ClaimType::CLAIM_SESSION_TICKET => None,
        security_claims::ClaimType::CLAIM_CERTIFICATE_STATUS => None,
        security_claims::ClaimType::CLAIM_EARLY_DATA => None,
        security_claims::ClaimType::CLAIM_ENCRYPTED_EXTENSIONS => None,
        _ => None,
    }
}

impl Put for OpenSSL {
    fn new(agent: &AgentDescriptor, config: PutConfig) -> Result<OpenSSL, Error> {
        let ssl = match config.typ {
            AgentType::Server => Self::create_server(agent)?,
            AgentType::Client => Self::create_client(agent)?,
        };

        let stream = SslStream::new(ssl, MemoryStream::new())?;

        let mut openssl = OpenSSL { config, stream };

        #[cfg(feature = "claims")]
        openssl.register_claimer(agent.name);

        Ok(openssl)
    }

    fn progress(&mut self, _agent_name: &AgentName) -> Result<(), Error> {
        let result = if self.is_state_successful() {
            // Trigger another read
            let mut vec: Vec<u8> = Vec::from([1; 128]);
            let maybe_error: MaybeError = self.stream.ssl_read(&mut vec).into();
            maybe_error.into()
        } else {
            let maybe_error: MaybeError = self.stream.do_handshake().into();
            maybe_error.into()
        };

        result
    }

    fn reset(&mut self, agent_name: AgentName) -> Result<(), Error> {
        self.stream.clear();
        Ok(())
    }

    fn config(&self) -> &PutConfig {
        &self.config
    }

    #[cfg(feature = "claims")]
    fn register_claimer(&mut self, agent_name: AgentName) {
        unsafe {
            use foreign_types_shared::ForeignTypeRef;

            let claims = self.config.claims.clone();
            let protocol_version = self.config.tls_version;
            let origin = self.config.typ;

            security_claims::register_claimer(
                self.stream.ssl().as_ptr().cast(),
                move |claim: security_claims::Claim| {
                    if let Some(data) = to_claim_data(protocol_version, claim) {
                        claims.deref_borrow_mut().claim_sized(Claim {
                            agent_name,
                            origin,
                            protocol_version,
                            data,
                        })
                    }
                },
            );
        }
    }

    #[cfg(feature = "claims")]
    fn deregister_claimer(&mut self) {
        unsafe {
            use foreign_types_shared::ForeignTypeRef;
            security_claims::deregister_claimer(self.stream.ssl().as_ptr().cast());
        }
    }

    #[allow(unused_variables)]
    fn rename_agent(&mut self, agent_name: AgentName) -> Result<(), Error> {
        #[cfg(feature = "claims")]
        {
            self.deregister_claimer();
            self.register_claimer(agent_name);
        }
        Ok(())
    }

    fn describe_state(&self) -> &str {
        // Very useful for nonblocking according to docs:
        // https://www.openssl.org/docs/manmaster/man3/SSL_state_string.html
        // When using nonblocking sockets, the function call performing the handshake may return
        // with SSL_ERROR_WANT_READ or SSL_ERROR_WANT_WRITE condition,
        // so that SSL_state_string[_long]() may be called.
        self.stream.ssl().state_string_long()
    }

    fn is_state_successful(&self) -> bool {
        self.describe_state()
            .contains("SSL negotiation finished successfully")
    }

    fn version() -> &'static str {
        openssl::version::version()
    }

    fn make_deterministic() {
        #[cfg(all(feature = "deterministic", feature = "openssl111"))]
        deterministic::set_openssl_deterministic();
        #[cfg(not(feature = "openssl111"))]
        log::warn!("Unable to make PUT determinisitic!");
    }

    fn shutdown(&mut self) -> String {
        panic!("Unsupported with OpenSSL PUT")
    }
}

impl OpenSSL {
    fn create_server(descriptor: &AgentDescriptor) -> Result<Ssl, ErrorStack> {
        let mut ctx_builder = SslContext::builder(SslMethod::tls())?;

        let (cert, key) = static_rsa_cert(ALICE_PRIVATE_KEY.0.as_bytes(), ALICE_CERT.0.as_bytes())?;
        ctx_builder.set_certificate(&cert)?;
        ctx_builder.set_private_key(&key)?;

        if descriptor.client_authentication {
            let mut store = X509StoreBuilder::new()?;
            store.add_cert(X509::from_pem(BOB_CERT.0.as_bytes())?)?;
            store.add_cert(X509::from_pem(EVE_CERT.0.as_bytes())?)?;
            let store = store.build();

            /*let mut chain = Stack::new().unwrap();
            let mut context = X509StoreContext::new().unwrap();
            assert!(context
                .init(&store, &cert, &chain, |c| c.verify_cert())
                .unwrap());*/

            ctx_builder.set_verify(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT);
            ctx_builder.set_cert_store(store);
        } else {
            ctx_builder.set_verify(SslVerifyMode::NONE);
        }

        #[cfg(feature = "openssl111")]
        ctx_builder.clear_options(openssl::ssl::SslOptions::ENABLE_MIDDLEBOX_COMPAT);

        #[cfg(feature = "openssl111")]
        ctx_builder.set_options(openssl::ssl::SslOptions::ALLOW_NO_DHE_KEX);

        set_max_protocol_version(&mut ctx_builder, descriptor.tls_version)?;

        #[cfg(any(feature = "openssl101f", feature = "openssl102u"))]
        {
            ctx_builder.set_tmp_ecdh(
                &openssl::ec::EcKey::from_curve_name(openssl::nid::Nid::SECP384R1)?.as_ref(),
            )?;

            ctx_builder.set_tmp_rsa(&openssl::rsa::Rsa::generate(512)?)?;
        }

        // Allow EXPORT in server
        ctx_builder.set_cipher_list("ALL:EXPORT:!LOW:!aNULL:!eNULL:!SSLv2")?;

        let mut ssl = Ssl::new(&ctx_builder.build())?;
        ssl.set_accept_state();

        Ok(ssl)
    }

    fn create_client(descriptor: &AgentDescriptor) -> Result<Ssl, ErrorStack> {
        let mut ctx_builder = SslContext::builder(SslMethod::tls())?;
        // Not sure whether we want this disabled or enabled: https://github.com/tlspuffin/tlspuffin/issues/67
        // The tests become simpler if disabled to maybe that's what we want. Lets leave it default
        // for now.
        // https://wiki.openssl.org/index.php/TLS1.3#Middlebox_Compatibility_Mode
        #[cfg(feature = "openssl111")]
        ctx_builder.clear_options(openssl::ssl::SslOptions::ENABLE_MIDDLEBOX_COMPAT);

        set_max_protocol_version(&mut ctx_builder, descriptor.tls_version)?;

        // Disallow EXPORT in client
        ctx_builder.set_cipher_list("ALL:!EXPORT:!LOW:!aNULL:!eNULL:!SSLv2")?;

        ctx_builder.set_verify(SslVerifyMode::NONE);

        if descriptor.client_authentication {
            let (cert, key) = static_rsa_cert(BOB_PRIVATE_KEY.0.as_bytes(), BOB_CERT.0.as_bytes())?;
            ctx_builder.set_certificate(&cert)?;
            ctx_builder.set_private_key(&key)?;
        }

        if descriptor.server_authentication {
            ctx_builder.set_verify(SslVerifyMode::PEER | SslVerifyMode::FAIL_IF_NO_PEER_CERT);

            let mut store = X509StoreBuilder::new()?;
            store.add_cert(X509::from_pem(ALICE_CERT.0.as_bytes())?)?;
            store.add_cert(X509::from_pem(EVE_CERT.0.as_bytes())?)?;
            let store = store.build();

            /*let mut chain = Stack::new().unwrap();
            let mut context = X509StoreContext::new().unwrap();
            assert!(context
                .init(&store, &cert, &chain, |c| c.verify_cert())
                .unwrap());*/

            ctx_builder.set_cert_store(store);
        } else {
            ctx_builder.set_verify(SslVerifyMode::NONE);
        }

        let mut ssl = Ssl::new(&ctx_builder.build())?;
        ssl.set_connect_state();

        Ok(ssl)
    }
}

pub enum MaybeError {
    Ok,
    Err(Error),
}

impl<T> From<Result<T, openssl::ssl::Error>> for MaybeError {
    fn from(result: Result<T, openssl::ssl::Error>) -> Self {
        if let Err(ssl_error) = result {
            if let Some(io_error) = ssl_error.io_error() {
                match io_error.kind() {
                    ErrorKind::WouldBlock => {
                        // Not actually an error, we just reached the end of the stream, thrown in MemoryStream
                        // debug!("Would have blocked but the underlying stream is non-blocking!");
                        MaybeError::Ok
                    }
                    _ => MaybeError::Err(Error::IO(format!("Unexpected IO Error: {}", io_error))),
                }
            } else if let Some(ssl_error) = ssl_error.ssl_error() {
                // OpenSSL threw an error, that means that there should be an Alert message in the
                // outbound channel
                MaybeError::Err(Error::OpenSSL(ssl_error.to_string()))
            } else {
                MaybeError::Ok
            }
        } else {
            MaybeError::Ok
        }
    }
}

impl Into<Result<(), Error>> for MaybeError {
    fn into(self) -> Result<(), Error> {
        match self {
            MaybeError::Ok => Ok(()),
            MaybeError::Err(err) => Err(err),
        }
    }
}
