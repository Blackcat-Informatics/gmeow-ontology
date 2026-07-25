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

/// The three failure classes this gate decides.
const ALL_CLASSES: [&str; 3] = [
    "math:StructuralKeyDrift",
    "math:SurfaceLeakInNormalForm",
    "math:StructuralKeyOnRejectedExpression",
];

fn dataset(turtle: &str) -> std::sync::Arc<RdfDataset> {
    purrdf::parse_dataset(turtle.as_bytes(), "text/turtle", None).expect("valid Turtle")
}

fn findings(turtle: &str) -> Vec<Finding> {
    check_math_expression_findings(dataset(turtle).as_ref())
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
    assert!(
        f.is_empty(),
        "a well-formed expression with a matching authored math:structuralKey and no \
         surface leak must raise nothing: {f:?}"
    );
}
