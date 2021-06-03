use rustls::hash_hs::HandshakeHash;
use rustls::internal::msgs::enums::{Compression, NamedGroup};
use rustls::internal::msgs::handshake::{Random, ServerECDHParams, ServerExtension, SessionID};
use rustls::{CipherSuite, NoKeyLog, ProtocolVersion};
use super::NoneError;
use crate::tls::{FnError};

pub fn fn_protocol_version12() -> Result<ProtocolVersion, NoneError> {
    Ok(ProtocolVersion::TLSv1_2)
}

pub fn fn_session_id() -> Result<SessionID, NoneError> {
    Ok(SessionID::empty())
}

pub fn fn_random() -> Result<Random, NoneError> {
    let random_data: [u8; 32] = [1; 32];
    Ok(Random::from(random_data))
}

pub fn fn_cipher_suites() -> Result<Vec<CipherSuite>, NoneError> {
    Ok(vec![CipherSuite::TLS13_AES_128_GCM_SHA256])
}

pub fn fn_compressions() -> Result<Vec<Compression>, NoneError> {
    Ok(vec![Compression::Null])
}

pub fn fn_verify_data(
    server_extensions: &Vec<ServerExtension>,
    verify_transcript: &HandshakeHash,
    client_handshake_traffic_secret_transcript: &HandshakeHash,
) -> Result<Vec<u8>, FnError> {
    let client_random = &[1u8; 32]; // todo see op_random()
    let suite = &rustls::suites::TLS13_AES_128_GCM_SHA256; // todo see op_cipher_suites()

    let group = NamedGroup::X25519; // todo

    let keyshare = super::get_server_public_key(server_extensions)?;
    let server_public_key = keyshare.payload.0.as_slice();

    let mut key_schedule = super::create_handshake_key_schedule(server_public_key, suite, group)?;

    key_schedule.client_handshake_traffic_secret(
        &client_handshake_traffic_secret_transcript.get_current_hash(),
        &NoKeyLog,
        client_random,
    );

    let pending = key_schedule.into_traffic_with_client_finished_pending();

    let bytes = pending.sign_client_finish(&verify_transcript.get_current_hash());
    Ok(Vec::from(bytes.as_ref()))
}

// ----
// seed_client_attacker12()
// ----

pub fn fn_cipher_suites12() -> Result<Vec<CipherSuite>, NoneError> {
    Ok(vec![CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256])
}

pub fn fn_sign_transcript(
    server_random: &Random,
    server_ecdh_params: &ServerECDHParams,
    transcript: &HandshakeHash,
) -> Result<Vec<u8>, FnError> {
    let secrets = super::new_secrets(server_random, server_ecdh_params)?;

    let vh = transcript.get_current_hash();
    Ok(secrets.client_verify_data(&vh))
}
