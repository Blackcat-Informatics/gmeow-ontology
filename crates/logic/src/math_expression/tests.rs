// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reasoned-graph tests for the `math:` expression-identity gate. Each drives
//! [`check_math_expression_findings`] over a frozen dataset — the same read substrate
//! the reason-verify pass hands it — and asserts the typed `math:` failure class fires
//! in isolation from the other two.

use super::*;

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix math: <https://blackcatinformatics.ca/math/> .\n\
     @prefix ex: <https://example.org/> .\n";

/// The four failure classes this gate decides.
const ALL_CLASSES: [&str; 4] = [
    "math:StructuralKeyDrift",
    "math:SurfaceLeakInNormalForm",
    "math:StructuralKeyOnRejectedExpression",
    "math:MalformedStructuralKey",
];

fn dataset(turtle: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).expect("valid Turtle")
}

fn findings(turtle: &str) -> Vec<Finding> {
    // A bare parse is both substrates here: these unit cases author no derived edges, so the
    // asserted graph and the closure coincide. The production split is exercised in
    // tests/math_expression_reasoned_substrate.rs, which reasons for real.
    let ds = dataset(turtle);
    check_math_expression_findings(ds.as_ref(), ds.as_ref())
}

fn count_class(findings: &[Finding], needle: &str) -> usize {
    findings
        .iter()
        .filter(|f| f.severity == Severity::Error && f.message.contains(needle))
        .count()
}

fn has_class(findings: &[Finding], needle: &str) -> bool {
    count_class(findings, needle) >= 1
}

/// Assert `f` raises `expected` (at least once) and raises NONE of the other two
/// classes in [`ALL_CLASSES`].
fn assert_exactly(f: &[Finding], expected: &str) {
    assert!(has_class(f, expected), "expected {expected} to fire: {f:?}");
    for other in ALL_CLASSES {
        if other != expected {
            assert!(
                !has_class(f, other),
                "expected ONLY {expected} to fire, but {other} also fired: {f:?}"
            );
        }
    }
}

// ── math:StructuralKeyDrift ─────────────────────────────────────────────────

#[test]
fn drifted_structural_key_on_a_well_formed_root_is_flagged() {
    // ex:e1 is a well-formed math:NumberLiteral root; its authored math:structuralKey is
    // an obviously wrong string, so the recomputed blake3 digest cannot match it.
    let f = findings(&format!(
        "{PREFIXES}\
         ex:e1 a math:NumberLiteral ; math:literalValue 1 ; \
           math:structuralKey \"not-the-real-digest\" .\n"
    ));
    assert_exactly(&f, "math:StructuralKeyDrift");
}

// ── math:SurfaceLeakInNormalForm ─────────────────────────────────────────────

#[test]
fn normalization_source_carrying_renders_as_is_flagged() {
    // ex:src is the math:normalizes source of a math:NormalizationDeclaration and
    // directly carries math:rendersAs — a surface-stratum leak into the structural
    // normal-form identity computation.
    let f = findings(&format!(
        "{PREFIXES}\
         ex:decl a math:NormalizationDeclaration ; math:normalizes ex:src ; \
           math:normalizesTo ex:tgt .\n\
         ex:src a math:NumberLiteral ; math:literalValue 1 ; math:rendersAs ex:rendered .\n\
         ex:tgt a math:NumberLiteral ; math:literalValue 1 .\n"
    ));
    assert_exactly(&f, "math:SurfaceLeakInNormalForm");
}

// ── math:StructuralKeyOnRejectedExpression ──────────────────────────────────

#[test]
fn structural_key_on_a_rejected_expression_is_flagged() {
    // ex:app is a math:ApplicationExpression missing its required math:operator — the
    // math: expression lowering REJECTS it — yet it carries an authored
    // math:structuralKey, claiming an identity for a thing the grammar refutes.
    let f = findings(&format!(
        "{PREFIXES}\
         ex:app a math:ApplicationExpression ; math:structuralKey \"whatever\" .\n"
    ));
    assert_exactly(&f, "math:StructuralKeyOnRejectedExpression");
}

// ── math:MalformedStructuralKey ──────────────────────────────────────────────

#[test]
fn two_structural_key_literals_are_flagged_as_malformed_not_drift() {
    // ex:e1 carries TWO math:structuralKey literals — even though one of them happens
    // to equal the real recomputed digest, a plural key is ambiguous by construction and
    // must be reported as math:MalformedStructuralKey, never silently read as "the first
    // one found" (which could paper over the second, contradictory value).
    let unkeyed = format!("{PREFIXES}ex:e1 a math:NumberLiteral ; math:literalValue 1 .\n");
    let ds = dataset(&unkeyed);
    let keys = crate::physical::lower::math_expression_structural_keys(ds.as_ref());
    let digest = keys
        .get("https://example.org/e1")
        .expect("ex:e1 is a recognized expression root")
        .as_ref()
        .expect("a well-formed math:NumberLiteral lowers cleanly")
        .clone();

    let f = findings(&format!(
        "{PREFIXES}ex:e1 a math:NumberLiteral ; math:literalValue 1 ; \
         math:structuralKey \"{digest}\" ; math:structuralKey \"also-not-the-digest\" .\n"
    ));
    assert_exactly(&f, "math:MalformedStructuralKey");
}

#[test]
fn non_literal_structural_key_value_is_flagged_as_malformed() {
    // ex:e1's math:structuralKey names an IRI, not a literal — structuralKey's range is
    // xsd:string, so a bare IRI standing in its place is not a digest at all.
    let f = findings(&format!(
        "{PREFIXES}ex:e1 a math:NumberLiteral ; math:literalValue 1 ; \
         math:structuralKey ex:notALiteral .\n\
         ex:notALiteral a ex:Thing .\n"
    ));
    assert_exactly(&f, "math:MalformedStructuralKey");
}

// ── production entry point: α-equivalence-class join ─────────────────────────

/// Two independently-named, alpha-equivalent `math:BindingExpression`s (∑ᵢ i and ∑ⱼ j —
/// same operator, same slot-indexed binding shape, differing ONLY in the bound-variable
/// declaration's IRI), each carrying a deliberately WRONG `math:structuralKey` so BOTH
/// raise `math:StructuralKeyDrift`. Committed once and shared with
/// `crates/logic/examples/alpha_class_join.rs` so the test and the runnable
/// demonstration read the identical fixture.
const ALPHA_EQUIVALENCE_DRIFT_JOIN: &str = include_str!(
    "../../../../slices/grounding/math/tests/conformance-fixtures/alpha-equivalence-drift-join.ttl"
);

#[test]
fn alpha_equivalent_drifted_expressions_cite_the_same_alpha_class_iri() {
    // Drives ONLY the real production entry point, `check_math_expression_findings` —
    // never `crate::physical::lower::alpha_class_iri` directly — over two
    // independently-authored, alpha-equivalent expressions.
    let f = findings(ALPHA_EQUIVALENCE_DRIFT_JOIN);

    let drift: Vec<&Finding> = f
        .iter()
        .filter(|finding| finding.message.contains("math:StructuralKeyDrift"))
        .collect();
    assert_eq!(
        drift.len(),
        2,
        "both alpha-equivalent binder expressions must drift: {f:?}"
    );
    for finding in &drift {
        assert_eq!(
            finding.cited_iris.len(),
            1,
            "each drift finding cites exactly one α-equivalence-class IRI: {finding:?}"
        );
        assert!(
            finding.cited_iris[0].starts_with("https://blackcatinformatics.ca/math/alphaClass/"),
            "the cited IRI is minted under the math: alpha-class namespace: {finding:?}"
        );
    }
    assert_eq!(
        drift[0].cited_iris[0], drift[1].cited_iris[0],
        "two alpha-equivalent expressions' drift findings cite the SAME α-equivalence-class \
         IRI — a consumer can join on it: {f:?}"
    );
}

// ── Clean / positive control ─────────────────────────────────────────────────

#[test]
fn well_formed_expression_with_matching_structural_key_and_no_surface_leak_is_clean() {
    // First lower the un-keyed expression to learn its real recomputed digest (never
    // hardcode a blake3 hex literal — it is a computed value of the term DAG's content
    // key, not something to guess), then author THAT digest and assert the gate raises
    // nothing over the result.
    let unkeyed = format!("{PREFIXES}ex:e1 a math:NumberLiteral ; math:literalValue 1 .\n");
    let ds = dataset(&unkeyed);
    let keys = crate::physical::lower::math_expression_structural_keys(ds.as_ref());
    let digest = keys
        .get("https://example.org/e1")
        .expect("ex:e1 is a recognized expression root")
        .as_ref()
        .expect("a well-formed math:NumberLiteral lowers cleanly")
        .clone();

    let keyed = format!(
        "{PREFIXES}ex:e1 a math:NumberLiteral ; math:literalValue 1 ; \
         math:structuralKey \"{digest}\" .\n"
    );
    let f = findings(&keyed);
    // "Clean" means no ERROR-grade violation — never that the gate is silent.
    // `report_alpha_equivalence_classes` is the gate's own positive verdict: it surfaces
    // a `Severity::Note` "verify.math.alpha-equivalence-class" for every root the lowering
    // ACCEPTS (see its doc comment), so a well-formed, correctly-keyed, leak-free root still
    // raises exactly that one Note — never zero findings.
    assert!(
        f.iter().all(|finding| finding.severity != Severity::Error),
        "a well-formed expression with a matching authored math:structuralKey and no \
         surface leak must raise no ERROR-grade finding: {f:?}"
    );
    assert_eq!(
        f.len(),
        1,
        "a clean root raises exactly the α-equivalence-class positive-verdict Note, nothing \
         else: {f:?}"
    );
    assert_eq!(f[0].severity, Severity::Note);
    assert_eq!(f[0].code, "verify.math.alpha-equivalence-class");
    assert!(
        f[0].message.contains("https://example.org/e1"),
        "the positive verdict must name the clean root: {f:?}"
    );
}
