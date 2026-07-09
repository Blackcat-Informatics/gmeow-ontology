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

use gmeow_errors::Diag;

use crate::adapter::assert_ir_isomorphic;
use crate::cgif::{parse_cgif_str, project_cgif};
use crate::clif::{parse_clif_str, project_clif};
use crate::frontend::{Diagnostic, Severity};
use crate::ir::LogicProgram;
use crate::projections::ProjectionResult;
use crate::xcl::{parse_xcl_str, project_xcl};

/// Prove each CL dialect is a lossless idempotent codec at its own fixpoint, and that the
/// three dialects converge on the SAME canonical IR (cross-dialect equivalence).
///
/// Each dialect `d` reconstructs its program from an embedded RDF meta channel, so a raw
/// program is not yet a single-leg fixpoint: the first `parse_d ∘ project_d` normalizes it
/// to `d`'s own fixpoint `fp_d`, and the Exact claim is that EVERY further round-trip is the
/// identity (`parse_d(project_d(fp_d)) == fp_d`). The dialects do NOT share a fixpoint with
/// each other's *serializers* byte-for-byte, but their reconstructed IR must agree — so the
/// cross-dialect leg asserts `fp_clif ≡ fp_cgif ≡ fp_xcl` via **all three explicit edges**
/// (`assert_ir_isomorphic` is a *directional* differ, so the third edge is checked outright
/// rather than inferred by transitivity). This is the CL round-trip acceptance: "IR →
/// dialect → IR isomorphic" and "CLIF ≡ CGIF ≡ XCL for one IR (all three parse to the same
/// program)".
///
/// # Errors
/// Returns a human-readable, dialect-prefixed error on a projection failure, a fatal
/// re-parse, a `Severity::Error` diagnostic, a non-idempotent fixpoint, or a cross-dialect
/// disagreement.
pub fn assert_all_dialects_isomorphic(program: &LogicProgram) -> gmeow_errors::Result<()> {
    let [clif, cgif, xcl] = dialect_fixpoints(program)?;

    // Cross-dialect equivalence — all three edges explicit (do NOT lean on transitivity
    // of a directional differ). This is the "CLIF ≡ CGIF ≡ XCL for one IR" cross-dialect
    // acceptance.
    cross_edge(clif.0, &clif.1, cgif.0, &cgif.1)?;
    cross_edge(cgif.0, &cgif.1, xcl.0, &xcl.1)?;
    cross_edge(clif.0, &clif.1, xcl.0, &xcl.1)?;

    Ok(())
}

/// The canonical dialect renderings for all three CL dialects, as `[(name, text); 3]` in
/// `clif, cgif, xcl` order — each the idempotent-stable `project_d(fp_d)` text.
///
/// The conformance harness uses this to pin byte-exact dialect goldens for the
/// `cl-roundtrip` corpus: pinning the fixpoint's rendering (not the raw program's) keeps
/// the golden aligned with what the round-trip gate proved and guarantees a second bless
/// is a no-op. Hard-fails on the same projection / re-parse errors as the round-trip.
pub fn dialect_fixpoint_projections(
    program: &LogicProgram,
) -> gmeow_errors::Result<[(&'static str, String); 3]> {
    let [clif, cgif, xcl] = dialect_fixpoints(program)?;
    Ok([
        ("clif", project_content("clif", project_clif(&clif.1))?),
        ("cgif", project_content("cgif", project_cgif(&cgif.1))?),
        ("xcl", project_content("xcl", project_xcl(&xcl.1))?),
    ])
}

/// Compute each dialect's idempotence-proved fixpoint, returned as
/// `[(name, fp); 3]` in `clif, cgif, xcl` order.
fn dialect_fixpoints(
    program: &LogicProgram,
) -> gmeow_errors::Result<[(&'static str, LogicProgram); 3]> {
    let clif = dialect_fixpoint(
        "clif",
        program,
        |p| project_content("clif", project_clif(p)),
        |text| {
            parse_clif_str(text, program.source_iri.clone())
                .map_err(|e| Diag::of_kind(crate::error::Roundtrip { detail: e.0 }))
        },
    )?;
    let cgif = dialect_fixpoint(
        "cgif",
        program,
        |p| project_content("cgif", project_cgif(p)),
        |text| {
            parse_cgif_str(text, program.source_iri.clone())
                .map_err(|e| Diag::of_kind(crate::error::Roundtrip { detail: e.0 }))
        },
    )?;
    let xcl = dialect_fixpoint(
        "xcl",
        program,
        |p| project_content("xcl", project_xcl(p)),
        |text| {
            parse_xcl_str(text, program.source_iri.clone())
                .map_err(|e| Diag::of_kind(crate::error::Roundtrip { detail: e.0 }))
        },
    )?;
    Ok([("clif", clif), ("cgif", cgif), ("xcl", xcl)])
}

/// Reach one dialect's fixpoint and prove it is idempotent.
///
/// First leg `fp1 = parse_d(project_d(program))` reaches `d`'s fixpoint; second leg
/// `fp2 = parse_d(project_d(fp1))` must be IR-isomorphic to `fp1` (the Exact identity at
/// the fixpoint). Hard-fails on any projection failure, fatal re-parse, `Severity::Error`
/// diagnostic, or non-idempotence.
fn dialect_fixpoint(
    dialect: &str,
    program: &LogicProgram,
    project: impl Fn(&LogicProgram) -> gmeow_errors::Result<String>,
    parse: impl Fn(&str) -> gmeow_errors::Result<(LogicProgram, Vec<Diagnostic>)>,
) -> gmeow_errors::Result<LogicProgram> {
    let fp1 = parse_checked(dialect, &project(program)?, &parse)?;
    let fp2 = parse_checked(dialect, &project(&fp1)?, &parse)?;
    assert_ir_isomorphic(&fp1, &fp2).map_err(|e| {
        Diag::of_kind(crate::error::Roundtrip {
            detail: format!(
                "cl-roundtrip [{dialect}]: not idempotent at its fixpoint: {}",
                e.0
            ),
        })
    })?;
    Ok(fp1)
}

/// Re-parse `text` in one dialect, hard-failing on a fatal parse or a `Severity::Error`
/// diagnostic (a lossy round-trip must never be silently tolerated).
fn parse_checked(
    dialect: &str,
    text: &str,
    parse: impl Fn(&str) -> gmeow_errors::Result<(LogicProgram, Vec<Diagnostic>)>,
) -> gmeow_errors::Result<LogicProgram> {
    let (program, diagnostics) = parse(text).map_err(|e| {
        Diag::of_kind(crate::error::Roundtrip {
            detail: format!("cl-roundtrip [{dialect}]: re-parse failed: {e}"),
        })
    })?;
    if let Some(err) = diagnostics.iter().find(|d| d.severity == Severity::Error) {
        return Err(Diag::of_kind(crate::error::Roundtrip {
            detail: format!(
                "cl-roundtrip [{dialect}]: re-parse emitted a Severity::Error diagnostic [{}]: {}",
                err.code, err.message
            ),
        }));
    }
    Ok(program)
}

/// Unwrap one dialect projection into its content text, dialect-prefixing any error.
fn project_content(
    dialect: &str,
    projected: gmeow_errors::Result<ProjectionResult>,
) -> gmeow_errors::Result<String> {
    projected.map(|p| p.content).map_err(|e| {
        Diag::of_kind(crate::error::Roundtrip {
            detail: format!("cl-roundtrip [{dialect}]: projection failed: {e}"),
        })
    })
}

/// Assert two dialect reconstructions are IR-isomorphic (one cross-dialect edge).
fn cross_edge(da: &str, a: &LogicProgram, db: &str, b: &LogicProgram) -> gmeow_errors::Result<()> {
    assert_ir_isomorphic(a, b).map_err(|e| {
        Diag::of_kind(crate::error::Roundtrip {
            detail: format!("cl-roundtrip cross-dialect {da} != {db}: {}", e.0),
        })
    })
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
        assert!(
            err.message().contains("cross-dialect clif != cgif"),
            "{err}"
        );
    }

    #[test]
    fn dialect_fixpoint_flags_non_idempotence() {
        // A dialect whose second round-trip returns a DIFFERENT program is caught as
        // non-idempotent. The parse closure returns socrates on the first leg and plato on
        // the second, so `assert_ir_isomorphic(fp1, fp2)` must fail.
        let calls = std::cell::Cell::new(0u8);
        let err = dialect_fixpoint(
            "clif",
            &fixture("socrates"),
            |_p| Ok(String::from("ignored-projection")),
            |_text| {
                let n = calls.get();
                calls.set(n + 1);
                Ok((
                    if n == 0 {
                        fixture("socrates")
                    } else {
                        fixture("plato")
                    },
                    Vec::new(),
                ))
            },
        )
        .expect_err("non-idempotent fixpoint must fail");
        assert!(
            err.message().contains("not idempotent at its fixpoint"),
            "{err}"
        );
    }

    #[test]
    fn parse_checked_flags_error_diagnostic() {
        // A Severity::Error diagnostic from the re-parse is a hard failure (a lossy
        // round-trip must never be silently tolerated).
        let err = parse_checked("cgif", "ignored", |_text| {
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
        assert!(err.message().contains("Severity::Error"), "{err}");
    }
}
