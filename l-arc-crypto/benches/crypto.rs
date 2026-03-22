//! Criterion benchmarks for la-crypto core operations.

#![allow(missing_docs)]

use criterion::{Criterion, criterion_group, criterion_main};
use l_arc_crypto::compare::constant_time_eq;
use l_arc_crypto::derive::derive_key;
use l_arc_crypto::encrypt::{open, seal};
use l_arc_crypto::hash::hmac_hash;
use l_arc_crypto::sign::{keypair_from_seed, sign, verify};
use l_arc_crypto::verses::find_verse;
use secrecy::SecretString;

fn bench_derive_key(c: &mut Criterion) {
    let pepper = SecretString::from("bench-pepper");
    let verse = find_verse("John 1:1").expect("bench setup");
    let ikm = vec![0u8; 32];

    c.bench_function("derive_key", |b| {
        b.iter(|| derive_key(&pepper, &ikm, verse, "bench"));
    });
}

fn bench_hmac_hash(c: &mut Criterion) {
    let pepper = SecretString::from("bench-pepper");
    let data = vec![0u8; 1024];

    c.bench_function("hmac_hash_1kb", |b| {
        b.iter(|| hmac_hash(&pepper, &data));
    });
}

fn bench_seal_open(c: &mut Criterion) {
    let key = [0x42u8; 32];
    let plaintext = vec![0u8; 1024];
    let aad = b"bench-aad";

    c.bench_function("seal_open_1kb", |b| {
        b.iter(|| {
            let sealed = seal(&key, &plaintext, aad).expect("bench seal");
            open(&key, &sealed, aad).expect("bench open");
        });
    });
}

fn bench_sign_verify(c: &mut Criterion) {
    let seed = [0xABu8; 32];
    let (sk, vk) = keypair_from_seed(&seed);
    let message = vec![0u8; 256];

    c.bench_function("sign_verify", |b| {
        b.iter(|| {
            let sig = sign(&sk, &message);
            let _ = verify(&vk, &message, &sig);
        });
    });
}

fn bench_constant_time_eq(c: &mut Criterion) {
    let a = [0xAAu8; 32];
    let b = [0xAAu8; 32];

    c.bench_function("constant_time_eq_32b", |b_iter| {
        b_iter.iter(|| constant_time_eq(&a, &b));
    });
}

criterion_group!(
    benches,
    bench_derive_key,
    bench_hmac_hash,
    bench_seal_open,
    bench_sign_verify,
    bench_constant_time_eq,
);
criterion_main!(benches);
