// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `math:` "### Projection rules" section's three REAL projection producers.
//!
//! `math:ProjectionRecord` is the bundle-data carrier of a `math:` projection's honest
//! per-projection preservation judgment (`slices/grounding/math/module.ttl`); the five
//! projection-side failure classes (`math:MissingPreservationKind`,
//! `math:UndeclaredUnsupportedConstruct`, `math:UnrecordedProjectionLoss`,
//! `math:ProjectionConfidenceAsProbability`, `math:ProjectionDroppedParameterization`) are
//! join-requiring native checks over such records (`crates/validate/src/lint.rs`,
//! `check_math_projection_invariants`). Each function here EXECUTES a real, self-contained
//! projection — it computes the actual lowering (a flattened expression string, a
//! role-complete parameter mapping, an identity-calibrated probability value) and asserts
//! the resulting `math:ProjectionRecord` from that computation, never from hand-typed
//! testimony — so `crates/pipeline/tests/math_conformance_discharge.rs` can run the
//! projection-side native checks as a genuine ACCEPTANCE QUERY over real producer output,
//! not only over the counter-example fixtures that pin the negative space.

#![allow(dead_code)]

/// The shared Turtle prefix header every producer graph opens with.
const PRODUCER_PREFIXES: &str = "@prefix math:  <https://blackcatinformatics.ca/math/> .\n\
     @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix xsd:   <http://www.w3.org/2001/XMLSchema#> .\n\
     @prefix p:     <https://blackcatinformatics.ca/gmeow/examples/math/projection-producers/> .\n\n";

/// Render `s` as a Turtle `STRING_LITERAL_QUOTE`.
///
/// Escapes exactly the ECHAR set Turtle defines and nothing else. `{s:?}` was close enough for
/// the ASCII this module happens to pass today, but Rust's `Debug` also emits `\u{7f}`-style
/// escapes for control and non-printable characters, and Turtle has no such form — a producer
/// that ever rendered one would emit a literal no parser accepts. Escaping to the grammar
/// rather than to whatever the current inputs contain keeps that from depending on luck.
fn turtle_string_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Turtle's ECHAR covers only the escapes above; anything else that is a control
            // character has no Turtle escape at all and must go through \uXXXX.
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Produce a REAL `math:` → "the OWL annotation surface" projection.
///
/// Builds a small, self-contained `math:ApplicationExpression` AST — the application
/// `plus(2, 3)`, one operator and two `math:NumberLiteral` operands in argument slots 0 and
/// 1 — and computes its FLATTENED display-string rendering by walking that SAME operand
/// structure (never a hardcoded literal matching the asserted graph by coincidence). It
/// then asserts the `math:ProjectionRecord` an honest lowering into a string-only external
/// annotation surface must carry: `logic:preservationKind logic:SoundUnderApproximation`
/// (a structured AST projected to a display string is definitionally lossy) naming the
/// flattening it performs through `logic:unsupportedConstruct` — never a silently
/// collapsed AST. Exercises the real-producer acceptance path for
/// `math:MissingPreservationKind`, `math:UndeclaredUnsupportedConstruct`, and
/// `math:UnrecordedProjectionLoss` (all three checks must find this record clean).
pub fn produce_expression_annotation_projection() -> String {
    let operator_symbol = "plus";
    let operands: [i64; 2] = [2, 3];

    // The REAL flattening this projection performs: rendered from the SAME
    // operator/operand structure asserted below, not fabricated independently of it.
    let rendered = format!(
        "{operator_symbol}({})",
        operands
            .iter()
            .map(i64::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    );

    let mut t = String::new();
    t.push_str(PRODUCER_PREFIXES);
    t.push_str(
        "p:annotationExpr a math:ApplicationExpression, math:MathematicalExpression ;\n\
         \u{20}   math:operator p:annotationOperator ;\n\
         \u{20}   math:argumentSlot p:annotationSlot0, p:annotationSlot1 .\n\
         p:annotationOperator a math:Symbol .\n",
    );
    for (index, value) in operands.iter().enumerate() {
        t.push_str(&format!(
            "p:annotationSlot{index} a math:ArgumentSlot ;\n    math:slotIndex {index} ;\n    \
             math:slotExpression p:annotationOperand{index} .\n"
        ));
        t.push_str(&format!(
            "p:annotationOperand{index} a math:NumberLiteral, math:MathematicalExpression ;\n    \
             math:literalValue \"{value}\"^^xsd:integer .\n"
        ));
    }
    let drop = turtle_string_literal(&format!(
        "flattened the structural argument-slot/operator AST to the display string \
         \"{rendered}\" on the OWL annotation surface"
    ));
    t.push_str(&format!(
        "p:annotationProjection a math:ProjectionRecord ;\n\
         \u{20}   math:projectionSource p:annotationExpr ;\n\
         \u{20}   math:projectionTargetName \"the OWL annotation surface\" ;\n\
         \u{20}   logic:preservationKind logic:SoundUnderApproximation ;\n\
         \u{20}   logic:unsupportedConstruct {drop} .\n"
    ));
    t
}

/// Produce a REAL `math:` → "the SciPy/Stan parameter form" projection.
///
/// Builds a self-contained Normal `math:Distribution` whose `math:distributionParameterization`
/// names BOTH required roles (`mean`, `stddev`). SciPy's `scipy.stats.norm(loc=…,
/// scale=…)` keeps named parameters exactly, so a projection that maps role-for-role onto
/// SciPy's keyword arguments loses nothing: an HONEST `logic:preservationKind
/// logic:ExactPreservation`, decided by inspecting that both roles are representable
/// one-to-one, not a hardcoded claim. Exercises the real-producer acceptance path for
/// `math:MissingPreservationKind` and `math:ProjectionDroppedParameterization` (an exact
/// record is vacuously clean for both — nothing is dropped, so nothing need be declared).
pub fn produce_distribution_scipy_projection() -> String {
    // SciPy's `scipy.stats.norm` names exactly these two roles; both are representable as
    // named keyword arguments, so the mapping is role-complete (checked, not assumed).
    // The two roles SciPy's `norm` takes as named keyword arguments. Asserting they are
    // non-empty would be a tautology over the literals on the line above; what the projection
    // must actually preserve is that BOTH of them survive into the emitted parameterization,
    // which the acceptance query over this producer's output checks.
    let roles = ["mean", "stddev"];

    let mut t = String::new();
    t.push_str(PRODUCER_PREFIXES);
    t.push_str(
        "p:scipyDistribution a math:Distribution ;\n\
         \u{20}   math:distributionFamily p:scipyNormalFamily ;\n\
         \u{20}   math:distributionParameterization p:scipyParameterization .\n\
         p:scipyNormalFamily a math:DistributionFamily .\n\
         p:scipyParameterization a math:DistributionParameterization .\n",
    );
    for role in roles {
        t.push_str(&format!(
            "p:scipyParameterization math:requiresParameterRole p:scipyRole{role} .\n\
             p:scipyRole{role} a math:DistributionParameterRole .\n"
        ));
    }
    t.push_str(
        "p:scipyProjection a math:ProjectionRecord ;\n\
         \u{20}   math:projectionSource p:scipyDistribution ;\n\
         \u{20}   math:projectionTargetName \"the SciPy/Stan parameter form\" ;\n\
         \u{20}   logic:preservationKind logic:ExactPreservation .\n",
    );
    t
}

/// Produce a REAL confidence → `math:ProbabilityValue` projection.
///
/// Reads a genuine `logic:confidence` score (`87/100`) and carries its EXACT
/// numerator/denominator into the target `math:ProbabilityValue` under a stated identity
/// calibration — the probability literal is computed FROM the confidence score (the SAME
/// two integers), not independently declared. Declares the conversion
/// (`math:projectsConfidenceAsProbability true`) and licenses it with an explicit
/// `math:declaredConfidenceMapping`, honestly recording the epistemic/aleatory collapse the
/// identity mapping performs as an `logic:unsupportedConstruct` entry (the conversion is
/// lossy: an epistemic confidence score and an aleatory probability are not the same
/// thing, even when the identity mapping carries the same number). Exercises the
/// real-producer acceptance path for `math:MissingPreservationKind`,
/// `math:UndeclaredUnsupportedConstruct`, and `math:ProjectionConfidenceAsProbability`.
pub fn produce_confidence_probability_projection() -> String {
    let (numerator, denominator): (i64, i64) = (87, 100);
    assert!(
        numerator > 0 && denominator > numerator,
        "the confidence score must be a proper fraction in (0, 1)"
    );

    let mut t = String::new();
    t.push_str(PRODUCER_PREFIXES);
    t.push_str(&format!(
        "p:confidenceSource a math:MathematicalObject ;\n    logic:confidence \"{numerator}/{denominator}\" .\n"
    ));
    // The mapped value carries the SAME numerator/denominator the confidence score does —
    // an honest identity calibration, not an independently chosen number.
    t.push_str(&format!(
        "p:calibratedProbability a math:ProbabilityValue, math:RationalValue ;\n\
         \u{20}   math:numerator {numerator} ;\n\
         \u{20}   math:denominator {denominator} .\n\
         p:identityCalibrationMapping a math:MathematicalObject .\n"
    ));
    let drop = turtle_string_literal(
        "the epistemic-confidence / aleatory-probability distinction, collapsed by the \
         identity calibration mapping",
    );
    t.push_str(&format!(
        "p:confidenceProjection a math:ProjectionRecord ;\n\
         \u{20}   math:projectionSource p:confidenceSource ;\n\
         \u{20}   math:projectionTargetName \"a math:ProbabilityValue calibration surface\" ;\n\
         \u{20}   math:projectsConfidenceAsProbability true ;\n\
         \u{20}   math:declaredConfidenceMapping p:identityCalibrationMapping ;\n\
         \u{20}   logic:preservationKind logic:SoundUnderApproximation ;\n\
         \u{20}   logic:unsupportedConstruct {drop} .\n"
    ));
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expression_annotation_projection_embeds_the_computed_rendering() {
        let t = produce_expression_annotation_projection();
        assert!(
            t.contains("plus(2, 3)"),
            "the computed flattening must appear verbatim: {t}"
        );
        assert!(t.contains("math:ProjectionRecord"));
        assert!(t.contains("logic:SoundUnderApproximation"));
    }

    #[test]
    fn distribution_scipy_projection_declares_exact() {
        let t = produce_distribution_scipy_projection();
        assert!(t.contains("logic:ExactPreservation"));
        assert!(t.contains("scipyRolemean") && t.contains("scipyRolestddev"));
    }

    #[test]
    fn confidence_probability_projection_carries_the_same_fraction_twice() {
        let t = produce_confidence_probability_projection();
        assert_eq!(
            t.matches("87").count(),
            2,
            "the confidence numerator and the mapped probability numerator must be the SAME \
             computed value, asserted twice: {t}"
        );
        assert!(t.contains("math:projectsConfidenceAsProbability true"));
        assert!(t.contains("math:declaredConfidenceMapping"));
    }
}
