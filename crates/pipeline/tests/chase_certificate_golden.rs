// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The non-vacuous chase-certificate golden (AC1/AC3): the authored
//! existential-chase demonstrator in `slices/grounding/logic/module.ttl` — a genuine
//! `C ⊑ ∃p.D` obligation plus two witness individuals — makes the REAL production
//! reasoner ([`gmeow_pipeline::stages::reason::reason_over_dataset`]) fire the native
//! existential chase, so the reason artifacts carry a non-vacuous
//! `chase.certificate.weakly-acyclic` finding AND a decomposable derivation for each
//! chase-invented null.
//!
//! This is the golden AC3 demands: exercised over a REAL committed production source
//! (the authored demonstrator, read off disk) through the REAL production reason entry
//! point — NOT a synthetic in-crate EDB hand-built to force the chase.

use std::path::{Path, PathBuf};

use gmeow_pipeline::stages::reason::reason_over_dataset;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn authored_demonstrator_fires_a_non_vacuous_certificate_and_explainable_witness() {
    // The demonstrator lives in the logic grounding slice's module — the object-level
    // EDB source the production chase reasons over. Reason over that real, committed
    // source through the production entry point.
    let module = repo_root().join("slices/grounding/logic/module.ttl");
    let ttl = std::fs::read(&module).expect("read the authored logic module");
    let edb = purrdf::parse_dataset(&ttl, "text/turtle", None)
        .expect("parse the authored logic module into an EDB");

    let artifacts =
        reason_over_dataset(edb.as_ref()).expect("production reasoning over the authored module");

    // AC1 — the weakly-acyclic certificate surfaces as a gmeow:Finding.
    let codes = artifacts.chase_report.counts_by_code();
    assert!(
        codes.contains_key("chase.certificate.weakly-acyclic"),
        "the authored existential demonstrator did not surface a weakly-acyclic chase \
         certificate; codes seen: {codes:?}"
    );

    // AC3 non-vacuity — the chase actually MINTED invented nulls (not a structurally
    // present, empty certificate), and each carries the recipe an explain(witness)
    // consumer decomposes: the firing rule and the frontier binding.
    assert!(
        !artifacts.witness_derivations.is_empty(),
        "the fold is vacuous: the authored demonstrator minted no chase-invented null"
    );
    assert!(
        artifacts
            .witness_derivations
            .iter()
            .all(|w| !w.rule_iri.is_empty() && !w.frontier.is_empty()),
        "an invented null carries no firing rule / frontier binding — explain(witness) \
         would be vacuous: {:?}",
        artifacts.witness_derivations
    );
    // The demonstrator seeds TWO individuals with no asserted edge, so the chase mints
    // two distinct content-addressed nulls (one per frontier binding).
    assert!(
        artifacts.witness_derivations.len() >= 2,
        "expected >= 2 invented nulls (two seeded demonstrand individuals), got {}",
        artifacts.witness_derivations.len()
    );
}
