// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The non-vacuous chase-certificate golden (AC1/AC3): the production ontology's own
//! existential obligations (the imported foundational vocabularies carry
//! `owl:someValuesFrom` restrictions) make the native existential chase fire, so the
//! SHIPPED bundle `generated/dist/gmeow.gts` carries a non-vacuous
//! `chase.certificate.weakly-acyclic` finding AND a decomposable `gmeow:InventedWitness`
//! for every chase-invented null.
//!
//! This is the golden AC3 demands: the fold is exercised over the REAL committed
//! production artifact (the shipped `gmeow.gts`), read back through the production
//! diagnostics reader — NOT a synthetic in-crate EDB hand-built to force the chase.

use std::path::{Path, PathBuf};

use gmeow_pipeline::diagnostics_reader::{read_findings, read_invented_witnesses};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn shipped_bundle_carries_a_non_vacuous_certificate_and_explainable_witnesses() {
    let bytes = std::fs::read(repo_root().join("generated/dist/gmeow.gts"))
        .expect("read the committed gmeow.gts bundle");
    let graph = purrdf::gts::read_all_segments(&bytes).expect("read GTS segments");
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph).expect("fold GTS dataset");

    // AC1 — a production existential program surfaces its certificate as a gmeow:Finding
    // in the shipped bundle.
    let findings = read_findings(&dataset).expect("read graph/diagnostics findings");
    let has_certificate = |code: &str| findings.findings.values().any(|f| f.code == code);
    assert!(
        has_certificate("chase.certificate.weakly-acyclic"),
        "the shipped bundle carries NO weakly-acyclic chase certificate — the production \
         existential chase did not fire (the fold is vacuous)"
    );

    // The full termination-class ladder is visible in the shipped deliverable: the
    // demonstrator worlds ship a certificate for every broader class the reasoner can
    // establish (the engine dogfooding its full termination-certification power into
    // gmeow.gts, not just the weakly-acyclic class its own restrictions need).
    for code in [
        "chase.certificate.jointly-acyclic",
        "chase.certificate.super-weakly-acyclic",
        "chase.certificate.model-summarizing-acyclic",
    ] {
        assert!(
            has_certificate(code),
            "the shipped bundle is missing the `{code}` demonstrator certificate — the \
             termination-ladder demonstrator world did not fold into gmeow.gts"
        );
    }

    // AC3 non-vacuity — the chase actually MINTED invented nulls into the shipped bundle
    // (not a structurally present, empty certificate). This is the structured, non-free-
    // text proof the existential program ran in production.
    let witnesses = read_invented_witnesses(&dataset).expect("read invented witnesses");
    assert!(
        !witnesses.is_empty(),
        "the shipped bundle carries no gmeow:InventedWitness null — the fold is vacuous \
         over production sources"
    );

    // Every invented null carries the recipe an explain(witness) consumer decomposes:
    // the firing rule and the frontier binding. Absence would make explain vacuous.
    assert!(
        witnesses
            .witnesses
            .values()
            .all(|w| !w.rule_iri.is_empty() && !w.frontier.is_empty()),
        "an invented null reached the shipped diagnostics graph without its firing rule / \
         frontier binding — explain(witness) would be vacuous"
    );
}
