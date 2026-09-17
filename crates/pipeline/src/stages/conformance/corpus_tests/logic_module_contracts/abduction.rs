// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Completeness advice reconstructs as typed FOL without becoming asserted knowledge.

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

/// Explicit ownership controls retained alongside every currently declared schema root.
/// `besForall` remains an independent constraint-integrity nonassertion control.
const COMPLETENESS_ROOTS: &[&str] = &[
    "relatorMediationComplete",
    "referenceFrameComplete",
    "wemiChainComplete",
    "besForall",
];

pub(super) fn abductive_vocabulary_parses_without_malformed_nodes() {
    let diags = &super::observations().diagnostics;
    let malformed: Vec<String> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_FORMULA" || d.code == "MALFORMED_CONSTRAINT")
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        malformed.is_empty(),
        "abductive vocabulary introduced malformed nodes: {malformed:?}"
    );
}

pub(super) fn completeness_formulas_reconstruct_but_never_become_axioms() {
    let observed = super::observations();
    assert!(
        !observed.declared_completeness_roots.is_empty(),
        "the authored schema completeness census must be non-vacuous"
    );
    let roots: std::collections::BTreeSet<_> = COMPLETENESS_ROOTS
        .iter()
        .map(|name| format!("{LOGIC}{name}"))
        .chain(observed.declared_completeness_roots.iter().cloned())
        .collect();
    for iri in roots {
        // (2a) Reconstructable as a well-formed first-order formula through the public entry.
        let formula = observed
            .reconstructed
            .get(&iri)
            .unwrap_or_else(|| panic!("required completeness root {iri} observation is absent"))
            .as_ref()
            .unwrap_or_else(|error| panic!("completeness root {iri} must reconstruct: {error}"));
        // (2b) NOT asserted as a free-standing top-level formula.
        assert!(
            !observed.top_level_formulas.contains(formula),
            "completeness formula {iri} leaked into the top-level formula set (would be asserted \
             as an always-true axiom)"
        );
    }

    // The schema's structural edges never leak into the domain axiom set.
    for pred_local in ["completenessFormula", "repairStrategy", "repairsDiscipline"] {
        let pred = format!("{LOGIC}{pred_local}");
        assert!(
            !observed.domain_axiom_predicates.contains(&pred),
            "logic:{pred_local} leaked into prog.axioms"
        );
    }
}
