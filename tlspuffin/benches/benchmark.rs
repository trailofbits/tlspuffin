use std::any::Any;

use criterion::{criterion_group, criterion_main, Criterion};
use libafl::{
    bolts::rands::{RomuDuoJrRand, StdRand},
    corpus::InMemoryCorpus,
    mutators::Mutator,
    state::StdState,
};
use tlspuffin::agent::PutName;
use tlspuffin::{
    agent::AgentName,
    fuzzer::{
        mutations::{util::TermConstraints, ReplaceReuseMutator},
        seeds::*,
    },
    term,
    term::dynamic_function::make_dynamic,
    tls::{error::FnError, fn_impl::*},
    trace::{Trace, TraceContext},
};

#[cfg(feature = "openssl-binding")]
const PUT: PutName = tlspuffin::registry::OPENSSL111;
#[cfg(feature = "wolfssl-binding")]
const PUT: PutName = tlspuffin::registry::WOLFSSL510;

fn fn_benchmark_example(a: &u64) -> Result<u64, FnError> {
    Ok(*a * *a)
}

fn benchmark_dynamic(c: &mut Criterion) {
    let mut group = c.benchmark_group("op_hmac256");

    group.bench_function("fn_benchmark_example static", |b| {
        b.iter(|| fn_benchmark_example(&5))
    });

    group.bench_function("fn_benchmark_example dynamic", |b| {
        b.iter(|| {
            let (_, dynamic_fn) = make_dynamic(&fn_benchmark_example);
            let args: Vec<Box<dyn Any>> = vec![Box::new(5)];
            dynamic_fn(&args)
        })
    });

    group.finish()
}

fn create_state() -> StdState<InMemoryCorpus<Trace>, Trace, RomuDuoJrRand, InMemoryCorpus<Trace>> {
    let rand = StdRand::with_seed(1235);
    let corpus: InMemoryCorpus<Trace> = InMemoryCorpus::new();
    StdState::new(rand, corpus, InMemoryCorpus::new(), &mut (), &mut ()).unwrap()
}

fn benchmark_mutations(c: &mut Criterion) {
    let mut group = c.benchmark_group("mutations");

    group.bench_function("ReplaceReuseMutator", |b| {
        let mut state = create_state();
        let client = AgentName::first();
        let mut mutator = ReplaceReuseMutator::new(TermConstraints {
            min_term_size: 0,
            max_term_size: 200,
        });
        let mut trace = seed_client_attacker12(client, PUT);

        b.iter(|| {
            mutator.mutate(&mut state, &mut trace, 0).unwrap();
        })
    });
}

fn benchmark_trace(c: &mut Criterion) {
    let mut group = c.benchmark_group("trace");

    group.bench_function("term clone", |b| {
        let client_hello = term! {
              fn_client_hello(
                fn_protocol_version12,
                fn_new_random,
                fn_new_session_id,
                (fn_append_cipher_suite(
                    (fn_new_cipher_suites()),
                    // force TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
                    fn_cipher_suite12
                )),
                fn_compressions,
                (fn_client_extensions_append(
                    (fn_client_extensions_append(
                        (fn_client_extensions_append(
                            (fn_client_extensions_append(
                                (fn_client_extensions_append(
                                    (fn_client_extensions_append(
                                        fn_client_extensions_new,
                                        fn_secp384r1_support_group_extension
                                    )),
                                    fn_signature_algorithm_extension
                                )),
                                fn_ec_point_formats_extension
                            )),
                            fn_signed_certificate_timestamp_extension
                        )),
                         // Enable Renegotiation
                        (fn_renegotiation_info_extension(fn_empty_bytes_vec))
                    )),
                    // Add signature cert extension
                    fn_signature_algorithm_cert_extension
                ))
            )
        };

        b.iter(|| client_hello.clone())
    });
}

fn benchmark_seeds(c: &mut Criterion) {
    let mut group = c.benchmark_group("seeds");

    group.bench_function("seed_successful", |b| {
        b.iter(|| {
            let mut ctx = TraceContext::new();
            let client = AgentName::first();
            let server = client.next();
            let trace = seed_successful(client, server, PUT);

            trace.execute(&mut ctx).unwrap();
        })
    });

    group.bench_function("seed_successful12", |b| {
        b.iter(|| {
            let mut ctx = TraceContext::new();
            let client = AgentName::first();
            let server = client.next();
            let trace = seed_successful12(client, server, PUT);

            trace.execute(&mut ctx).unwrap()
        })
    });

    group.bench_function("seed_client_attacker", |b| {
        b.iter(|| {
            let mut ctx = TraceContext::new();
            let client = AgentName::first();
            let trace = seed_client_attacker(client, PUT);

            trace.execute(&mut ctx).unwrap();
        })
    });

    group.bench_function("seed_client_attacker12", |b| {
        b.iter(|| {
            let mut ctx = TraceContext::new();
            let client = AgentName::first();
            let trace = seed_client_attacker12(client, PUT);

            trace.execute(&mut ctx).unwrap();
        })
    });

    group.bench_function("seed_session_resumption_dhe", |b| {
        b.iter(|| {
            let mut ctx = TraceContext::new();
            let initial_server = AgentName::first();
            let server = initial_server.next();
            let trace = seed_session_resumption_dhe(initial_server, server, PUT);

            trace.execute(&mut ctx).unwrap();
        })
    });

    group.bench_function("seed_session_resumption_ke", |b| {
        b.iter(|| {
            let mut ctx = TraceContext::new();
            let initial_server = AgentName::first();
            let server = initial_server.next();
            let trace = seed_session_resumption_ke(initial_server, server, PUT);

            trace.execute(&mut ctx).unwrap();
        })
    });

    group.bench_function("seed_session_resumption_dhe_full", |b| {
        b.iter(|| {
            let mut ctx = TraceContext::new();
            let initial_server = AgentName::first();
            let server = initial_server.next();
            let trace = seed_session_resumption_dhe_full(initial_server, server, PUT);

            trace.execute(&mut ctx).unwrap();
        })
    });

    group.finish()
}

criterion_group!(
    benches,
    benchmark_dynamic,
    benchmark_trace,
    benchmark_mutations,
    benchmark_seeds,
);
criterion_main!(benches);
