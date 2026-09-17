// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated native gate agreement over every current grade and the public projection.
use gmeow_errors::grade::{
    Blocking, FindingCategory, GateVerdict, Grade, Severity, Standpoint, gate,
};
use gmeow_ns::{GMEOW_NS, LOGIC_NS};
use std::collections::BTreeSet;

/// The seeded `gmeow:severity*` individual IRI a [`Severity`] projects to.
fn severity_iri(s: Severity) -> String {
    let local = match s {
        Severity::Info => "severityInfo",
        Severity::Note => "severityNote",
        Severity::Warning => "severityWarning",
        Severity::Error => "severityError",
    };
    format!("{GMEOW_NS}{local}")
}

/// The seeded `logic:Finding*` category individual IRI a [`FindingCategory`] projects to.
fn category_iri(c: FindingCategory) -> String {
    let local = match c {
        FindingCategory::DataShapeViolation => "FindingDataShapeViolation",
        FindingCategory::ModelingDisciplineViolation => "FindingModelingDisciplineViolation",
        FindingCategory::ContradictionWitness => "FindingContradictionWitness",
        FindingCategory::PermittedEpistemicConflict => "FindingPermittedEpistemicConflict",
        FindingCategory::UnsupportedSemanticFeature => "FindingUnsupportedSemanticFeature",
        FindingCategory::IncompleteCheck => "FindingIncompleteCheck",
        FindingCategory::ProjectionLoss => "FindingProjectionLoss",
        FindingCategory::PolicyWarning => "FindingPolicyWarning",
        FindingCategory::Corroboration => "FindingCorroboration",
        FindingCategory::Transient => "FindingTransientChatter",
    };
    format!("{LOGIC_NS}{local}")
}

/// The seeded `gmeow:standpoint*` individual IRI a [`Standpoint`] projects to.
fn standpoint_iri(p: Standpoint) -> String {
    let local = match p {
        Standpoint::Advisory => "standpointAdvisory",
        Standpoint::Perspectival => "standpointPerspectival",
        Standpoint::Binding => "standpointBinding",
    };
    format!("{GMEOW_NS}{local}")
}

/// The seeded `gmeow:blocking*` individual IRI a [`Blocking`] projects to.
fn blocking_iri(b: Blocking) -> String {
    let local = match b {
        Blocking::Coherent => "blockingCoherent",
        Blocking::Blocking => "blockingBlocking",
    };
    format!("{GMEOW_NS}{local}")
}

/// Every grade in the finite bilattice, each paired with the stable finding IRI
/// that encodes it.
fn all_grades() -> Vec<(Grade, String)> {
    let mut out = Vec::new();
    for &s in &Severity::ALL {
        for &c in &FindingCategory::ALL {
            for &p in &Standpoint::ALL {
                let iri = format!(
                    "{GMEOW_NS}examples/diagnostics/gate-conformance/g-{:?}-{:?}-{:?}",
                    s, c, p
                );
                out.push((Grade::new(s, c, p), iri));
            }
        }
    }
    out
}

#[test]
fn reasoner_gate_verdict_equals_rust_gate_over_every_grade() {
    let grades = all_grades();

    // (1) The authored category→Blocking projection MUST equal Rust blocking() for
    // every category — so the map the reasoner reads is provably the one gate() uses.
    let observed = super::super::diagnostic_observations()
        .gate
        .as_ref()
        .expect("native gate observation");
    let authored = &observed.category_blocking;
    assert_eq!(
        authored.len(),
        FindingCategory::ALL.len(),
        "the diagnostics slice must wire gmeow:categoryBlocking for every FindingCategory"
    );
    for &c in &FindingCategory::ALL {
        let cat = category_iri(c);
        let expected = blocking_iri(c.blocking());
        let authored_b = authored
            .get(&cat)
            .unwrap_or_else(|| panic!("no authored gmeow:categoryBlocking for {cat}"));
        assert_eq!(
            authored_b, &expected,
            "authored gmeow:categoryBlocking for {cat} disagrees with Rust FindingCategory::blocking()"
        );
    }

    assert_eq!(observed.coordinates.len(), grades.len());
    for (grade, iri) in &grades {
        assert_eq!(
            observed.coordinates.get(iri),
            Some(&(
                severity_iri(grade.severity),
                category_iri(grade.category),
                standpoint_iri(grade.standpoint),
            )),
            "producer must encode every expected grade exactly"
        );
    }

    let derived_fatal = &observed.fatal;

    // The set of grade IRIs Rust gate() calls Fatal.
    let rust_fatal: BTreeSet<String> = grades
        .iter()
        .filter(|(g, _)| gate(*g) == GateVerdict::Fatal)
        .map(|(_, iri)| iri.clone())
        .collect();

    // Sanity: the derivation actually fired somewhere and did NOT fire everywhere,
    // so a degenerate rule cannot pass the equality vacuously.
    assert!(
        !rust_fatal.is_empty(),
        "the Rust gate() Fatal set must be non-empty (there ARE fatal grades)"
    );
    assert!(
        rust_fatal.len() < grades.len(),
        "the Rust gate() Fatal set must be a PROPER subset (most grades are not fatal)"
    );

    // The crux: the reasoner-derived Fatal set equals Rust gate()'s, EXACTLY, both
    // directions. This is the ontology's policy morphism proved equal to Rust's.
    let missed_by_reasoner: Vec<&String> = rust_fatal.difference(derived_fatal).collect();
    let over_derived: Vec<&String> = derived_fatal.difference(&rust_fatal).collect();
    assert!(
        missed_by_reasoner.is_empty() && over_derived.is_empty(),
        "reasoner-derived gatesFatal set != Rust gate() Fatal set over {} grades.\n  \
         reasoner MISSED (gate()=Fatal, reasoner=Collected): {:?}\n  \
         reasoner OVER-DERIVED (gate()=Collected, reasoner=Fatal): {:?}",
        grades.len(),
        missed_by_reasoner,
        over_derived,
    );

    // Never-gate theorem (a): NO advisory-standpoint grade is ever derived fatal.
    for (grade, iri) in &grades {
        if grade.standpoint == Standpoint::Advisory {
            assert!(
                !derived_fatal.contains(iri),
                "advisory-standpoint grade must never be derived fatal: {grade:?}"
            );
        }
    }

    // Never-gate theorem (b): NO permitted-epistemic-conflict grade is ever derived fatal.
    for (grade, iri) in &grades {
        if grade.category == FindingCategory::PermittedEpistemicConflict {
            assert!(
                !derived_fatal.contains(iri),
                "permitted-epistemic-conflict grade must never be derived fatal: {grade:?}"
            );
        }
    }
}

/// The public projection contract: the gate verdict is a
/// genuine ENTAILMENT of the authored `logic:ruleGateFatalVerdict` over the ACTUAL
/// `gmeow_errors::render::to_gmeow_rdf` projection — not a Rust `gate()` hand-assertion the
/// projection pre-materializes. An up-set finding (Error / blocking category / Binding) is
/// derived `gateFatal`; a never-gate finding (Advisory standpoint) is derived NOTHING; and
/// the projection itself carries no pre-computed verdict.
#[test]
fn projected_finding_gate_verdict_is_reasoner_derived_not_hand_asserted() {
    use gmeow_errors::render::to_gmeow_rdf;
    let observed = super::super::diagnostic_observations()
        .gate
        .as_ref()
        .expect("native projected gate observation");
    use gmeow_errors::{Finding, Report};

    // (1) An up-set finding → the reasoner derives gateFatal over the real projection.
    let mut fatal = Report::new("validate");
    fatal.add_finding(
        Finding::new(Severity::Error, "x.upset", "up-set finding")
            .with_category(FindingCategory::DataShapeViolation)
            .with_standpoint(Standpoint::Binding),
    );
    let fatal_nq = to_gmeow_rdf(&fatal);
    // The projection emits the grade coordinates but NEVER pre-materializes the verdict.
    assert!(
        !fatal_nq.contains("findingGateVerdict"),
        "to_gmeow_rdf must not hand-assert the reasoner-derived verdict:\n{fatal_nq}"
    );
    assert_eq!(fatal_nq, observed.projections["fatal"].nquads);
    let derived = &observed.projections["fatal"].fatal;
    assert_eq!(
        derived.len(),
        1,
        "the authored gate rule must derive exactly one gateFatal over the projected up-set finding, got {derived:?}"
    );

    // (2) A never-gate finding (Advisory) → the reasoner derives NOTHING (the up-set
    // construction structurally excludes it — the first never-gate theorem, over the real
    // projection this time).
    let mut advisory = Report::new("validate");
    advisory.add_finding(
        Finding::new(Severity::Error, "x.adv", "advisory never gates")
            .with_category(FindingCategory::DataShapeViolation)
            .with_standpoint(Standpoint::Advisory),
    );
    let advisory_nq = to_gmeow_rdf(&advisory);
    assert_eq!(advisory_nq, observed.projections["advisory"].nquads);
    assert!(
        observed.projections["advisory"].fatal.is_empty(),
        "an Advisory-standpoint finding must never be derived gateFatal, even over the real projection"
    );
}
