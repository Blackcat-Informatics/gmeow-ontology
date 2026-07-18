// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The two glossary interop lowerings' `lang:ProjectionEmission` records pass the LIVE native
//! projection-loss lint (the gate that runs on `make validate` / `make check`).
//!
//! Deliverable G4 folds an OntoLex `vartrans:translation` and a TBX (ISO-30042) lowering of the
//! per-slice terminology glossary, each carrying an honest `lang:ProjectionEmission` record in
//! `graph/lang-projection-corpus`. Both lowerings are LOSSY, so an honest record MUST declare a
//! non-Exact `logic:preservationKind` and enumerate every dropped construct — a silent-lossy
//! record trips one of `lang:MissingPreservationKind`, `lang:UndeclaredUnsupportedConstruct`, or
//! `lang:UnrecordedEpistemicLoss`. This test unions the real glossary corpus (which types each
//! projected `lang:Sense`) with the real emission records and drives
//! `gmeow_validate::lint::structural_lint_dataset` (the SAME native gate) fresh, asserting NONE of
//! the projection-loss failure classes fires — the overclaim floor, carried as bundle data.

use std::collections::{BTreeSet, HashSet};
use std::path::{Path, PathBuf};

use gmeow_pipeline::stages::lang_glossary;
use gmeow_validate::lint::{LintConfig, structural_lint_dataset};
use purrdf::RdfDataset;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// The minimal lint config the native structural lint needs (GMEOW namespace + ontology IRI);
/// the projection-loss checks read only the `lang:`/`logic:` vocabulary and the GMEOW `vantage`
/// predicate off `cfg.namespace`.
fn lint_config() -> LintConfig {
    LintConfig {
        namespace: "https://blackcatinformatics.ca/gmeow/".into(),
        ontology_iri: "https://blackcatinformatics.ca/gmeow".into(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: HashSet::new(),
    }
}

#[test]
fn glossary_lowering_emissions_pass_the_native_projection_loss_lint() {
    let root = repo_root();

    // The real glossary corpus (types each projected lang:Sense + its lang:LexicalConcept) …
    let glossary = lang_glossary::build_corpus(&root).expect("build glossary corpus");
    // … and the real emission records the mappings stage appends to graph/lang-projection-corpus.
    let lowering = lang_glossary::build_lowering_corpus(&root).expect("build lowering corpus");

    let glossary_ds = purrdf::parse_dataset(&glossary.ntriples, "application/n-triples", None)
        .expect("parse glossary corpus");
    let emission_ds =
        purrdf::parse_dataset(&lowering.emission_ntriples, "application/n-triples", None)
            .expect("parse emission records");
    let ds = RdfDataset::union(&[glossary_ds.as_ref(), emission_ds.as_ref()]);

    let report = structural_lint_dataset(&ds, &lint_config());
    let errors = report.errors();

    // The projection-loss overclaim floor: none of the three projection-emission failure classes
    // may fire over the honest lossy records.
    for class in [
        "lang:MissingPreservationKind",
        "lang:UndeclaredUnsupportedConstruct",
        "lang:UnrecordedEpistemicLoss",
    ] {
        let offenders: Vec<&String> = errors.iter().filter(|e| e.contains(class)).collect();
        assert!(
            offenders.is_empty(),
            "the honest glossary lowering emissions must not trip {class}, but the native lint \
             raised: {offenders:?}"
        );
    }

    // Positive control: the two records ARE present and lossy (so the checks above actually ran
    // over lossy emissions, not an empty graph).
    let nt = String::from_utf8(lowering.emission_ntriples).expect("utf8");
    assert_eq!(
        nt.matches("ProjectionEmission> .").count(),
        2,
        "both glossary interop emissions must be present for the lint to grade"
    );
}
