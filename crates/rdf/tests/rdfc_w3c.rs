// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! W3C RDF Dataset Canonicalization (RDFC-1.0) conformance gate (#910).
//!
//! The vendored W3C `rdf-canon` test suite (`tests/fixtures/rdfc/`, see
//! `SOURCE.md`) is the acceptance gate for the native canonicalizer. Each
//! `testNNN-in.nq` input is parsed (with the oxttl dev-dependency parser — the only
//! oxigraph touch, and it is test-only; production `gmeow-rdf-core` stays
//! oxigraph-free), bridged into the IR, canonicalized by
//! [`gmeow_rdf::canonical_nquads`], and its canonical N-Quads compared to the
//! expected `testNNN-rdfc10.nq`. Inputs WITHOUT an expected output are **negative**
//! (poison / complexity-limit) tests that must abort rather than canonicalize.
//!
//! The suite includes the hard automorphism vectors (test053–test058 etc.) whose
//! blank-node symmetries can only be resolved by RDFC-1.0's n-degree permutation
//! backtracking — a weaker (hash-only) implementation fails them.

#![cfg(feature = "oxigraph")]

use std::path::{Path, PathBuf};

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::Quad;

use gmeow_rdf::{canonical_nquads_with, CanonHash};

/// Tests that specify `rdfc:hashAlgorithm "SHA384"` in the W3C manifest (the rest
/// use the SHA-256 default). As of the vendored suite this is exactly `test075`
/// ("blank node - diamond (uses SHA-384)").
const SHA384_TESTS: &[&str] = &["test075"];

fn hash_for(stem: &str) -> CanonHash {
    if SHA384_TESTS.contains(&stem) {
        CanonHash::Sha384
    } else {
        CanonHash::Sha256
    }
}

fn fixtures_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rdfc")
}

fn parse_nquads(text: &str) -> Vec<Quad> {
    RdfParser::from_format(RdfFormat::NQuads)
        .for_reader(text.as_bytes())
        .map(|q| q.expect("valid W3C fixture quad"))
        .collect()
}

/// Compare canonical N-Quads as sorted non-empty line sets (robust to trailing
/// newline conventions; the spec already mandates sorted output, so a real
/// difference still surfaces).
fn norm_lines(s: &str) -> Vec<String> {
    let mut v: Vec<String> = s
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect();
    v.sort();
    v
}

#[test]
fn w3c_rdfc10_suite() {
    let dir = fixtures_dir();
    let mut inputs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("fixtures/rdfc present")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.ends_with("-in.nq"))
        })
        .collect();
    inputs.sort();
    // Exact count, not a floor: silent fixture loss must fail the gate rather
    // than degrade coverage unnoticed. Bump this (and the eval/negative split
    // below) when the vendored W3C suite is intentionally re-synced.
    assert_eq!(
        inputs.len(),
        65,
        "expected exactly 65 vendored W3C rdf-canon inputs, found {}",
        inputs.len()
    );

    // Suppress panic noise; we report failures by test name through `failures`.
    let prev_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut failures: Vec<String> = Vec::new();
    let mut eval = 0usize;
    let mut negative = 0usize;

    for input in &inputs {
        let stem = input
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_suffix("-in.nq"))
            .expect("input stem")
            .to_owned();
        let expected_path = dir.join(format!("{stem}-rdfc10.nq"));
        let in_text = std::fs::read_to_string(input).expect("read input");
        let quads = parse_nquads(&in_text);

        if expected_path.exists() {
            eval += 1;
            let outcome = std::panic::catch_unwind(|| {
                canonical_nquads_with(quads.iter(), hash_for(&stem)).expect("canonicalize")
            });
            match outcome {
                Ok(actual) => {
                    let expected = std::fs::read_to_string(&expected_path).expect("read expected");
                    if norm_lines(&actual) != norm_lines(&expected) {
                        failures.push(format!(
                            "{stem}: canonical output mismatch\n--- expected ---\n{expected}\n--- actual ---\n{actual}"
                        ));
                    }
                }
                Err(_) => failures.push(format!(
                    "{stem}: canonicalization PANICKED on a positive test"
                )),
            }
        } else {
            // Negative (poison) test: canonicalization must abort (the poison call
            // budget trips on the pathological blank graph).
            negative += 1;
            let outcome = std::panic::catch_unwind(|| {
                canonical_nquads_with(quads.iter(), hash_for(&stem)).expect("canonicalize")
            });
            match outcome {
                Ok(_) => failures.push(format!(
                    "{stem}: NEGATIVE poison test did not abort (expected the call-budget guard to trip)"
                )),
                Err(payload) => {
                    // The abort MUST be the poison call-budget guard, not an
                    // incidental parse/bridge panic — otherwise a future
                    // regression would masquerade as a poison abort and pass.
                    let msg = payload
                        .downcast_ref::<String>()
                        .map(String::as_str)
                        .or_else(|| payload.downcast_ref::<&str>().copied())
                        .unwrap_or("<non-string panic payload>");
                    if !msg.contains("call budget") {
                        failures.push(format!(
                            "{stem}: NEGATIVE test panicked, but not via the call-budget guard \
                             (payload: {msg:?}); a non-budget panic must not count as a poison abort"
                        ));
                    }
                }
            }
        }
    }

    std::panic::set_hook(prev_hook);

    eprintln!(
        "W3C RDFC-1.0: {eval} eval + {negative} negative tests, {} failures",
        failures.len()
    );
    // Pin the exact eval/negative split so a fixture that loses its expected
    // output (silently turning an eval vector into a negative one, or vice
    // versa) fails the gate instead of quietly weakening it.
    assert_eq!(
        (eval, negative),
        (64, 1),
        "expected 64 eval + 1 negative W3C vectors, ran {eval} eval + {negative} negative"
    );
    assert!(
        failures.is_empty(),
        "W3C RDFC-1.0 conformance failures ({}):\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
}
