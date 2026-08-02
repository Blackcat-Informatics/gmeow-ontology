// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native half of the W2b bundle-explorer `describe` WITNESS (T1/F2).
//!
//! The browser bundle explorer answers `describe <term>` by running a `DESCRIBE`
//! over the object-level **core** bundle via the vendored purrdf wasm engine. The
//! vendored engine is purrdf's own — pinned + anti-rot-gated
//! (`crates/docs/tests/purrdf_asset.rs`) and native↔wasm-parity-proven on purrdf's
//! CI — so the browser describe is exactly the native purrdf describe. This test
//! pins the NATIVE describe of a deterministic term over the core bundle to a
//! committed content-addressed attestation
//! (`crates/docs/assets/purrdf/WITNESS.describe.nt`): the explorer's describe is
//! proven against the same purrdf engine + the same core bundle the site ships.
//!
//! Refreshed with the bundle/asset via `GMEOW_WITNESS_BLESS=1`.

use std::path::PathBuf;

use purrdf::{DatasetView, GraphMatch, TermRef};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

fn attestation_path() -> PathBuf {
    repo_root().join("crates/docs/assets/purrdf/WITNESS.describe.nt")
}

/// Render `subject`'s describe (every quad with it as subject) as sorted N-Triples —
/// a deterministic, engine-independent describe projection.
fn describe(dataset: &purrdf::RdfDataset, subject_iri: &str) -> String {
    let term = |t: TermRef<'_>| -> String {
        match t {
            TermRef::Iri(iri) => format!("<{iri}>"),
            TermRef::Blank { label, .. } => format!("_:{label}"),
            TermRef::Literal { lexical, .. } => format!("\"{}\"", lexical.replace('"', "\\\"")),
            TermRef::Triple { .. } => "<<triple>>".to_owned(),
        }
    };
    let mut lines: Vec<String> = dataset
        .quads_for_pattern(None, None, None, GraphMatch::Any)
        .filter(|q| matches!(dataset.resolve(q.s), TermRef::Iri(iri) if iri == subject_iri))
        .map(|q| {
            format!(
                "{} {} {} .",
                term(dataset.resolve(q.s)),
                term(dataset.resolve(q.p)),
                term(dataset.resolve(q.o))
            )
        })
        .collect();
    lines.sort();
    lines.dedup();
    lines.join("\n")
}

#[test]
fn native_core_bundle_describe_matches_the_witness_attestation() {
    let root = repo_root();
    let full = std::fs::read(root.join("generated/dist/gmeow.gts"))
        .unwrap_or_else(|e| panic!("witness needs the generated bundle (run `make check`): {e}"));
    let core_nq = gmeow_validate::store::core_browser_bundle_nquads(&full, &[])
        .expect("build core browser bundle");
    let dataset = purrdf::parse_dataset(core_nq.as_bytes(), "application/n-quads", None)
        .expect("parse core bundle N-Quads");

    // A deterministic subject: the lexicographically smallest GMEOW-namespace IRI
    // that appears in subject position (the same term the explorer would describe).
    let ns = "https://blackcatinformatics.ca/gmeow/";
    let mut subject: Option<String> = None;
    for q in dataset.quads_for_pattern(None, None, None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = dataset.resolve(q.s)
            && iri.starts_with(ns)
            && subject.as_deref().map(|s| iri < s).unwrap_or(true)
        {
            subject = Some(iri.to_owned());
        }
    }
    let subject = subject.expect("core bundle carries a GMEOW-namespace subject");
    let rendered = describe(&dataset, &subject);
    assert!(
        !rendered.is_empty(),
        "the describe of {subject} must be non-empty"
    );

    let path = attestation_path();
    // Require the EXACT documented value: only `GMEOW_WITNESS_BLESS=1` may overwrite the
    // committed witness (an empty or `=0` value must not silently replace it).
    if std::env::var("GMEOW_WITNESS_BLESS").as_deref() == Ok("1") {
        std::fs::write(&path, format!("# describe <{subject}>\n{rendered}\n")).expect("write");
        eprintln!("blessed describe witness at {}", path.display());
        return;
    }
    let committed = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "describe witness attestation {} missing (bless with GMEOW_WITNESS_BLESS=1): {e}",
            path.display()
        )
    });
    assert_eq!(
        format!("# describe <{subject}>\n{rendered}\n"),
        committed,
        "native core-bundle describe drifted from the committed witness attestation — re-bless"
    );
}
