use std::collections::HashMap;

use itertools::Itertools;
use once_cell::sync::Lazy;
use rand::{random, seq::SliceRandom};
use ring::{
    hkdf,
    hkdf::{KeyType, Prk, HKDF_SHA256},
    hmac,
    hmac::Key,
    rand::SystemRandom,
};
use rustls::internal::msgs::enums::ExtensionType;
use rustls::internal::msgs::handshake::ECDHEServerKeyExchange;
use rustls::internal::msgs::message::OpaqueMessage;
use rustls::{
    internal::msgs::{
        alert::AlertMessagePayload,
        base::{Payload, PayloadU16},
        ccs::ChangeCipherSpecPayload,
        codec::Codec,
        enums::{
            AlertDescription, Compression,
            ContentType::{ApplicationData, ChangeCipherSpec, Handshake},
            HandshakeType, NamedGroup, ServerNameType,
        },
        handshake::{
            CertificatePayload, ClientExtension, ClientHelloPayload, HandshakeMessagePayload,
            HandshakePayload, HandshakePayload::Certificate, KeyShareEntry, Random,
            ServerExtension, ServerHelloPayload, ServerKeyExchangePayload, ServerName,
            ServerNamePayload, SessionID,
        },
        message::{Message, MessagePayload},
    },
    CipherSuite, ProtocolVersion, SignatureScheme,
};
use HandshakePayload::EncryptedExtensions;

use crate::register_fn;
use crate::term::{make_dynamic, DynamicFunction, TypeShape};

// -----
// utils
// -----

enum SecretKind {
    ResumptionPSKBinderKey,
    ClientEarlyTrafficSecret,
    ClientHandshakeTrafficSecret,
    ServerHandshakeTrafficSecret,
    ClientApplicationTrafficSecret,
    ServerApplicationTrafficSecret,
    ExporterMasterSecret,
    ResumptionMasterSecret,
    DerivedSecret,
}

impl SecretKind {
    fn to_bytes(&self) -> &'static [u8] {
        match self {
            SecretKind::ResumptionPSKBinderKey => b"res binder",
            SecretKind::ClientEarlyTrafficSecret => b"c e traffic",
            SecretKind::ClientHandshakeTrafficSecret => b"c hs traffic",
            SecretKind::ServerHandshakeTrafficSecret => b"s hs traffic",
            SecretKind::ClientApplicationTrafficSecret => b"c ap traffic",
            SecretKind::ServerApplicationTrafficSecret => b"s ap traffic",
            SecretKind::ExporterMasterSecret => b"exp master",
            SecretKind::ResumptionMasterSecret => b"res master",
            SecretKind::DerivedSecret => b"derived",
        }
    }
}

fn derive_secret<L, F, T>(
    secret: &hkdf::Prk,
    kind: SecretKind,
    algorithm: L,
    context: &Vec<u8>,
    into: F,
) -> T
where
    L: KeyType,
    F: for<'b> FnOnce(hkdf::Okm<'b, L>) -> T,
{
    const LABEL_PREFIX: &[u8] = b"tls13 ";

    let label = kind.to_bytes();
    let output_len = u16::to_be_bytes(algorithm.len() as u16);
    let label_len = u8::to_be_bytes((LABEL_PREFIX.len() + label.len()) as u8);
    let context_len = u8::to_be_bytes(context.len() as u8);

    let info = &[
        &output_len[..],
        &label_len[..],
        LABEL_PREFIX,
        label,
        &context_len[..],
        context,
    ];
    let okm = secret.expand(info, algorithm).unwrap();
    into(okm)
}

// ----
// Types
// ----

/// Special type which is used in [`crate::trace::InputAction`]. This is used if an recipe outputs
/// more or less than exactly one message.
#[derive(Clone)]
pub struct MultiMessage {
    pub messages: Vec<Message>,
}

// ----
// Concrete implementations
// ----

// ----
// TLS 1.3 Message constructors (Return type is message)
// ----

pub fn op_client_hello(
    client_version: &ProtocolVersion,
    random: &Random,
    session_id: &SessionID,
    cipher_suites: &Vec<CipherSuite>,
    compression_methods: &Vec<Compression>,
    extensions: &Vec<ClientExtension>,
) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::ClientHello,
            payload: HandshakePayload::ClientHello(ClientHelloPayload {
                client_version: client_version.clone(),
                random: random.clone(),
                session_id: session_id.clone(),
                cipher_suites: cipher_suites.clone(),
                compression_methods: compression_methods.clone(),
                extensions: extensions.clone(),
            }),
        }),
    }
}

pub fn op_server_hello(
    legacy_version: &ProtocolVersion,
    random: &Random,
    session_id: &SessionID,
    cipher_suite: &CipherSuite,
    compression_method: &Compression,
    extensions: &Vec<ServerExtension>,
) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2,
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::ServerHello,
            payload: HandshakePayload::ServerHello(ServerHelloPayload {
                legacy_version: legacy_version.clone(),
                random: random.clone(),
                session_id: session_id.clone(),
                cipher_suite: cipher_suite.clone(),
                compression_method: compression_method.clone(),
                extensions: extensions.clone(),
            }),
        }),
    }
}

pub fn op_change_cipher_spec() -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::ChangeCipherSpec(ChangeCipherSpecPayload {}),
    }
}

pub fn op_application_data(data: &Payload) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::ApplicationData(data.clone()),
    }
}


// ----
// TLS 1.3 Unused
// ----

pub fn op_encrypted_certificate(server_extensions: &Vec<ServerExtension>) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::EncryptedExtensions,
            payload: EncryptedExtensions(server_extensions.clone()),
        }),
    }
}

pub fn op_certificate(certificate: &CertificatePayload) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::Certificate,
            payload: Certificate(certificate.clone()),
        }),
    }
}

// ----
// Unused
// ----


pub fn op_hmac256_new_key() -> Key {
    // todo maybe we need a context for rng? Maybe also for hs_hash?
    let random = SystemRandom::new();
    let key = hmac::Key::generate(hmac::HMAC_SHA256, &random).unwrap();
    key
}

pub fn op_arbitrary_to_key(key: &Vec<u8>) -> Key {
    Key::new(hmac::HMAC_SHA256, key.as_slice())
}

pub fn op_hmac256(key: &Key, msg: &Vec<u8>) -> Vec<u8> {
    let tag = hmac::sign(&key, msg);
    Vec::from(tag.as_ref())
}

// https://github.com/ctz/rustls/blob/d03bf27e0b520fe73c901d0027bab12753a42bb6/rustls/src/key_schedule.rs#L164
pub fn op_client_handshake_traffic_secret(secret: &hkdf::Prk, hs_hash: &Vec<u8>) -> Prk {
    let secret: hkdf::Prk = derive_secret(
        secret,
        SecretKind::ClientHandshakeTrafficSecret,
        HKDF_SHA256, // todo make configurable
        hs_hash,
        |okm| okm.into(),
    );

    secret
}

pub fn op_random_cipher_suite() -> CipherSuite {
    *vec![
        CipherSuite::TLS13_AES_128_CCM_SHA256,
        CipherSuite::TLS13_AES_128_CCM_8_SHA256,
        CipherSuite::TLS13_AES_128_GCM_SHA256,
        CipherSuite::TLS13_AES_256_GCM_SHA384,
        CipherSuite::TLS_DHE_RSA_WITH_AES_128_CBC_SHA,
    ]
    .choose(&mut rand::thread_rng())
    .unwrap()
}

pub fn op_random_session_id() -> SessionID {
    SessionID::random().unwrap()
}

pub fn op_random_protocol_version() -> ProtocolVersion {
    ProtocolVersion::TLSv1_3
}

pub fn op_random_random_data() -> Random {
    let random_data: [u8; 32] = random();
    Random::from(random_data)
}

pub fn op_compression() -> Compression {
    *vec![Compression::Null, Compression::Deflate, Compression::LSZ]
        .choose(&mut rand::thread_rng())
        .unwrap()
}

pub fn op_server_name_extension(dns_name: &String) -> ClientExtension {
    ClientExtension::ServerName(vec![ServerName {
        typ: ServerNameType::HostName,
        payload: ServerNamePayload::HostName((
            PayloadU16(dns_name.clone().into_bytes()),
            webpki::DnsNameRef::try_from_ascii_str(dns_name.as_str())
                .unwrap()
                .to_owned(),
        )),
    }])
}

pub fn op_x25519_support_group_extension() -> ClientExtension {
    ClientExtension::NamedGroups(vec![NamedGroup::X25519])
}

pub fn op_signature_algorithm_extension() -> ClientExtension {
    ClientExtension::SignatureAlgorithms(vec![
        SignatureScheme::RSA_PKCS1_SHA256,
        SignatureScheme::RSA_PSS_SHA256,
    ])
}

pub fn op_random_key_share_extension() -> ClientExtension {
    let key = Vec::from(rand::random::<[u8; 32]>()); // 32 byte public key
    ClientExtension::KeyShare(vec![KeyShareEntry {
        group: NamedGroup::X25519,
        payload: PayloadU16::new(key),
    }])
}

pub fn op_supported_versions_extension() -> ClientExtension {
    ClientExtension::SupportedVersions(vec![ProtocolVersion::TLSv1_3])
}

pub fn op_random_extensions() -> Vec<ClientExtension> {
    let server_name: ClientExtension = op_server_name_extension(&"maxammann.org".to_string());

    let supported_groups: ClientExtension = op_x25519_support_group_extension();
    let signature_algorithms: ClientExtension = op_signature_algorithm_extension();
    let key_share: ClientExtension = op_random_key_share_extension();
    let supported_versions: ClientExtension = op_supported_versions_extension();

    vec![
        server_name,
        supported_groups,
        signature_algorithms,
        key_share,
        supported_versions,
    ]
}

// ----
// Utils
// ----

pub fn op_concat_messages_2(msg1: &Message, msg2: &Message) -> MultiMessage {
    MultiMessage {
        messages: vec![msg1.clone(), msg2.clone()],
    }
}

pub fn op_concat_messages_3(msg1: &Message, msg2: &Message, msg3: &Message) -> MultiMessage {
    MultiMessage {
        messages: vec![msg1.clone(), msg2.clone(), msg3.clone()],
    }
}

// ----
// TLS 1.2, all used in seed_successful12
// ----

pub fn op_server_certificate(certs: &CertificatePayload) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::Certificate,
            payload: HandshakePayload::Certificate(certs.clone()),
        }),
    }
}

pub fn op_server_key_exchange(ske_payload: &ServerKeyExchangePayload) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::ServerKeyExchange,
            payload: HandshakePayload::ServerKeyExchange(ske_payload.clone()),
        }),
    }
}

pub fn op_server_hello_done() -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::ServerHelloDone,
            payload: HandshakePayload::ServerHelloDone,
        }),
    }
}

pub fn op_client_key_exchange(data: &Payload) -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::Handshake(HandshakeMessagePayload {
            typ: HandshakeType::ClientKeyExchange,
            payload: HandshakePayload::ClientKeyExchange(data.clone()),
        }),
    }
}

pub fn op_change_cipher_spec12() -> Message {
    Message {
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: MessagePayload::ChangeCipherSpec(ChangeCipherSpecPayload),
    }
}

pub fn op_handshake_finished12(data: &Payload) -> OpaqueMessage {
    OpaqueMessage {
        typ: Handshake,
        version: ProtocolVersion::TLSv1_2, // todo this is not controllable
        payload: data.clone(),
    }
}

// ----
// Attack operations
// ----

// https://cve.mitre.org/cgi-bin/cvename.cgi?name=CVE-2021-3449

pub fn op_attack_cve_2021_3449(extensions: &Vec<ClientExtension>) -> Vec<ClientExtension> {
    extensions
        .clone()
        .into_iter()
        .filter(|extension| extension.get_type() != ExtensionType::SignatureAlgorithms)
        .collect_vec()
}

// ----
// Registry
// ----

register_fn!(
    REGISTERED_FN,
    REGISTERED_TYPES,
    op_hmac256_new_key,
    op_arbitrary_to_key,
    op_hmac256,
    op_client_handshake_traffic_secret,
    op_client_hello,
    op_server_hello,
    op_change_cipher_spec,
    op_encrypted_certificate,
    op_certificate,
    op_application_data,
    op_random_cipher_suite,
    op_random_session_id,
    op_random_protocol_version,
    op_random_random_data,
    op_compression,
    op_server_name_extension,
    op_signature_algorithm_extension,
    op_random_key_share_extension,
    op_supported_versions_extension,
    op_x25519_support_group_extension,
    op_random_extensions,
    op_concat_messages_2,
    op_concat_messages_3,
);

// todo it would be possible generate dynamic functions like in criterion_group! macro
// or via a procedural macro.
// https://gitlab.inria.fr/mammann/tlspuffin/-/issues/28
