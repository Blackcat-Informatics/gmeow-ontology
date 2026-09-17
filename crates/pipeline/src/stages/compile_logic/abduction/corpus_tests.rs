// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only consumers of the producer's abductive-schema observation.

use super::*;

/// Load the exact producer-selected abductive-schema observation.
fn observation() -> Observation {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = crate::fixture::authenticated_artifact(&root, "stage-compile-logic", CHANNEL)
        .expect("authenticated producer-selected abduction observation");
    serde_json::from_slice(&bytes).expect("complete typed abduction observation")
}

/// Authored abductive vocabulary must survive compilation without malformed nodes.
#[test]
fn abductive_vocabulary_parses_without_malformed_nodes() {
    let observed = observation();
    let malformed: Vec<_> = observed
        .diagnostics
        .iter()
        .filter(|diagnostic| {
            diagnostic.code == "MALFORMED_FORMULA" || diagnostic.code == "MALFORMED_CONSTRAINT"
        })
        .collect();
    assert!(
        malformed.is_empty(),
        "abductive vocabulary introduced malformed nodes: {malformed:?}"
    );
}

/// Completeness roots remain reconstructable metadata and never become domain axioms.
#[test]
fn completeness_formulas_reconstruct_but_never_become_axioms() {
    let observed = observation();
    assert!(
        !observed.declared_roots.is_empty(),
        "schema completeness census must be non-vacuous"
    );
    let selected: BTreeSet<_> = observed
        .declared_roots
        .iter()
        .cloned()
        .chain(REQUIRED_ROOTS.iter().map(|name| format!("{LOGIC}{name}")))
        .collect();
    for iri in selected {
        let formula = observed
            .reconstructed
            .get(&iri)
            .unwrap_or_else(|| panic!("required root {iri} observation is absent"))
            .as_ref()
            .unwrap_or_else(|error| panic!("completeness root {iri} must reconstruct: {error}"));
        assert!(
            !observed.asserted.contains(formula),
            "completeness formula {iri} leaked into asserted formulas"
        );
    }
    for predicate in ["completenessFormula", "repairStrategy", "repairsDiscipline"] {
        assert!(
            !observed
                .axiom_predicates
                .contains(&format!("{LOGIC}{predicate}")),
            "logic:{predicate} leaked into domain axioms"
        );
    }
}
