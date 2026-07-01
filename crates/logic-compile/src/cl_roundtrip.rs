// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The Common Logic **round-trip isomorphism** authority.
//!
//! Each of the three ISO 24707 dialects — CLIF (s-expressions), CGIF (conceptual
//! graphs), and XCL (XML) — is a bidirectional `PreservationKind::Exact` projection
//! of the canonical `logic:` IR. This module is the SINGLE place that proves that
//! claim over an arbitrary program: it round-trips the program through every dialect
//! (`project_* → parse_*_str → assert_ir_isomorphic`) and then proves the three
//! reconstructions agree with one another.
//!
//! It is the reusable core behind both teeth:
//! * the **universal invariant** the conformance harness runs on every case, and
//! * the dedicated `cl-roundtrip` corpus cases.
//!
//! Every failure mode — a projection error, a fatal re-parse, a `Severity::Error`
//! diagnostic, or an IR mismatch — is a HARD error (no-optionality doctrine); the
//! Exact claim forbids a silent drop, so there is no fallback path.

use crate::adapter::assert_ir_isomorphic;
use crate::cgif::{parse_cgif_str, project_cgif};
use crate::clif::{parse_clif_str, project_clif};
use crate::frontend::{Diagnostic, Severity};
use crate::ir::LogicProgram;
use crate::projections::ProjectionResult;
use crate::xcl::{parse_xcl_str, project_xcl};

/// Prove `program` round-trips through all three CL dialects with IR isomorphism, and
/// that the three reconstructions are cross-dialect equivalent.
///
/// The reference is the **canonical fixpoint** `parse ∘ canonical-rdf12 ∘ program`, not
/// the raw `program`: a program that is not yet a single-leg dialect fixpoint (a
/// hand-built one, or a parse whose rules carry source blank-node labels) is first
/// normalized through the Exact `canonical-rdf12` target. Proving the CL dialects
/// round-trip THAT fixpoint exactly proves they preserve everything the Exact reference
/// does — the genuine Exact bar, matching the per-dialect production round-trip tests.
///
/// The per-dialect leg asserts `project_d(fixpoint) → parse_d_str → back` is isomorphic
/// to the fixpoint. The cross-dialect leg then asserts **all three explicit edges**
/// (`clif ≡ cgif`, `cgif ≡ xcl`, `clif ≡ xcl`): [`assert_ir_isomorphic`] is a
/// *directional* differ, so the third edge is checked outright rather than inferred by
/// transitivity.
///
/// # Errors
/// Returns a human-readable, dialect-prefixed error on canonical normalization failure,
/// a projection failure, a fatal re-parse, a `Severity::Error` diagnostic, or an
/// IR-isomorphism mismatch.
pub fn assert_all_dialects_isomorphic(program: &LogicProgram) -> Result<(), String> {
    let fixpoint = canonical_fixpoint(program)?;

    let clif_back = roundtrip(&fixpoint, "clif", project_clif(&fixpoint), |text| {
        parse_clif_str(text, fixpoint.source_iri.clone()).map_err(|e| e.0)
    })?;
    let cgif_back = roundtrip(&fixpoint, "cgif", project_cgif(&fixpoint), |text| {
        parse_cgif_str(text, fixpoint.source_iri.clone()).map_err(|e| e.0)
    })?;
    let xcl_back = roundtrip(&fixpoint, "xcl", project_xcl(&fixpoint), |text| {
        parse_xcl_str(text, fixpoint.source_iri.clone()).map_err(|e| e.0)
    })?;

    // Cross-dialect equivalence — all three edges explicit (do NOT lean on transitivity
    // of a directional differ). This is the C6 "CLIF ≡ CGIF ≡ XCL for one IR" acceptance.
    cross_edge("clif", &clif_back, "cgif", &cgif_back)?;
    cross_edge("cgif", &cgif_back, "xcl", &xcl_back)?;
    cross_edge("clif", &clif_back, "xcl", &xcl_back)?;

    Ok(())
}

/// Normalize `program` to its canonical fixpoint `parse ∘ canonical-rdf12 ∘ program`.
///
/// `canonical-rdf12` is the Exact reference target; a single re-parse of its output is a
/// stable dialect fixpoint (source blank-node labels are replaced by minted structural
/// IRIs). Hard-fails on a canonical projection error or a fatal fixpoint re-parse.
fn canonical_fixpoint(program: &LogicProgram) -> Result<LogicProgram, String> {
    let canon = crate::projections::rdf::project_canonical_rdf12(program)
        .map_err(|e| format!("cl-roundtrip: canonical-rdf12 normalization failed: {e}"))?;
    let (fixpoint, _diags) =
        crate::frontend::parse_logic_str(&canon.content, program.source_iri.clone())
            .map_err(|e| format!("cl-roundtrip: canonical fixpoint re-parse failed: {}", e.0))?;
    Ok(fixpoint)
}

/// Round-trip `program` through one dialect and return the reconstruction.
///
/// `projected` is the dialect's emitter output; `parse` re-parses the emitted text back
/// to IR (errors already mapped to `String`). Hard-fails on any projection error, fatal
/// re-parse, `Severity::Error` diagnostic, or IR mismatch against `program`.
fn roundtrip(
    program: &LogicProgram,
    dialect: &str,
    projected: Result<ProjectionResult, String>,
    parse: impl FnOnce(&str) -> Result<(LogicProgram, Vec<Diagnostic>), String>,
) -> Result<LogicProgram, String> {
    let text = projected
        .map_err(|e| format!("cl-roundtrip [{dialect}]: projection failed: {e}"))?
        .content;
    let (back, diagnostics) =
        parse(&text).map_err(|e| format!("cl-roundtrip [{dialect}]: re-parse failed: {e}"))?;
    if let Some(err) = diagnostics.iter().find(|d| d.severity == Severity::Error) {
        return Err(format!(
            "cl-roundtrip [{dialect}]: re-parse emitted a Severity::Error diagnostic [{}]: {}",
            err.code, err.message
        ));
    }
    assert_ir_isomorphic(program, &back).map_err(|e| {
        format!(
            "cl-roundtrip [{dialect}]: IR not isomorphic after round-trip: {}",
            e.0
        )
    })?;
    Ok(back)
}

/// Assert two dialect reconstructions are IR-isomorphic (one cross-dialect edge).
fn cross_edge(da: &str, a: &LogicProgram, db: &str, b: &LogicProgram) -> Result<(), String> {
    assert_ir_isomorphic(a, b)
        .map_err(|e| format!("cl-roundtrip cross-dialect {da} != {db}: {}", e.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{ContextualScope, Formula, LogicAxiom, LogicRule, Term};

    const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

    fn iri(local: &str) -> String {
        format!("{LOGIC}{local}")
    }

    /// A program exercising an axiom, a rule (negated body atom + distinct pair), and a
    /// full-FOL formula (quantifier + disjunction + strong negation + sequence marker) —
    /// the union of constructs the three dialects each prove Exact in isolation.
    fn fixture(subject: &str) -> LogicProgram {
        let axiom = LogicAxiom::ground(iri(subject), iri("knows"), iri("b"), false).expect("axiom");

        let rule = LogicRule::new(
            LogicAxiom::new(
                "?x",
                iri("ancestor"),
                "?z",
                false,
                false,
                ContextualScope::default(),
            )
            .expect("head"),
            vec![
                LogicAxiom::new(
                    "?x",
                    iri("parent"),
                    "?y",
                    false,
                    false,
                    ContextualScope::default(),
                )
                .expect("b1"),
                LogicAxiom::new(
                    "?y",
                    iri("parent"),
                    "?z",
                    false,
                    true,
                    ContextualScope::default(),
                )
                .expect("b2"),
            ],
            vec![("?x".to_owned(), "?z".to_owned())],
            ContextualScope::default(),
        );

        let inner_or = Formula::Or(vec![
            Formula::atom(
                Term::iri(iri("mortal")).unwrap(),
                vec![
                    Term::var("p").unwrap(),
                    Term::sequence_marker("rest").unwrap(),
                ],
            )
            .unwrap(),
            Formula::Not(Box::new(
                Formula::atom(
                    Term::iri(iri("mortal")).unwrap(),
                    vec![Term::var("p").unwrap()],
                )
                .unwrap(),
            )),
        ]);
        let formula = Formula::Forall {
            vars: vec!["p".to_owned()],
            body: Box::new(inner_or),
        };

        LogicProgram::new(
            vec![axiom],
            vec![rule],
            Vec::new(),
            Some("urn:test:cl".to_owned()),
        )
        .with_formulas(vec![formula])
    }

    #[test]
    fn all_dialects_round_trip_and_agree() {
        assert_all_dialects_isomorphic(&fixture("socrates")).expect("all dialects isomorphic");
    }

    #[test]
    fn cross_edge_flags_divergent_programs() {
        // Two programs that differ in one axiom subject must not pass a cross edge.
        let err = cross_edge("clif", &fixture("socrates"), "cgif", &fixture("plato"))
            .expect_err("divergent programs must fail the cross edge");
        assert!(err.contains("cross-dialect clif != cgif"), "{err}");
    }

    #[test]
    fn roundtrip_flags_non_isomorphic_reparse() {
        // A re-parse that returns a DIFFERENT program is caught as a non-isomorphism.
        let program = fixture("socrates");
        let err = roundtrip(&program, "clif", project_clif(&program), |_text| {
            Ok((fixture("plato"), Vec::new()))
        })
        .expect_err("non-isomorphic re-parse must fail");
        assert!(err.contains("not isomorphic after round-trip"), "{err}");
    }

    #[test]
    fn roundtrip_flags_error_diagnostic() {
        // A Severity::Error diagnostic from the re-parse is a hard failure, even if the
        // returned program would itself be isomorphic.
        let program = fixture("socrates");
        let err = roundtrip(&program, "cgif", project_cgif(&program), |_text| {
            Ok((
                fixture("socrates"),
                vec![Diagnostic {
                    severity: Severity::Error,
                    code: "CL_TEST".to_owned(),
                    message: "seeded error".to_owned(),
                    subject: None,
                }],
            ))
        })
        .expect_err("error diagnostic must fail");
        assert!(err.contains("Severity::Error"), "{err}");
    }
}
