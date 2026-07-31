// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Runnable demonstration: two independently-authored, α-equivalent `math:` expressions
//! resolve to the SAME α-equivalence-class IRI on a real production surface.
//!
//! Drives ONLY the public production entry point,
//! [`gmeow_logic::math_expression::check_math_expression_findings`] — the SAME function
//! `crates/logic/src/verify.rs`'s `verify_with_reasoning_result` dispatches on every
//! `make reason-verify` / `gmeow-dev verify` run — over a small, committed fixture
//! (`slices/grounding/math/tests/conformance-fixtures/alpha-equivalence-drift-join.ttl`):
//! two `math:BindingExpression`s, `ex:sumBinderI` (∑ᵢ i) and `ex:sumBinderJ` (∑ⱼ j),
//! structurally identical up to the bound-variable declaration's IRI, each carrying a
//! deliberately WRONG `math:structuralKey` so both raise `math:StructuralKeyDrift`. Each
//! drift finding cites (`Finding::cited_iris`) the α-equivalence-class IRI RECOMPUTED
//! from the expression's own genuine structure — never the wrong authored string — so
//! the two findings cite the IDENTICAL individual despite differing bound-variable names
//! and differing (wrong) authored keys.
//!
//! Run with:
//! ```text
//! cargo run -p gmeow-logic --example alpha_class_join
//! ```

const FIXTURE: &str = include_str!(
    "../../../slices/grounding/math/tests/conformance-fixtures/alpha-equivalence-drift-join.ttl"
);

fn main() {
    let dataset = purrdf::parse_dataset(FIXTURE.as_bytes(), "text/turtle", None)
        .expect("the committed alpha-equivalence-drift-join.ttl fixture parses");
    let findings = gmeow_logic::math_expression::check_math_expression_findings(
        dataset.as_ref(),
        dataset.as_ref(),
    );

    println!(
        "check_math_expression_findings over alpha-equivalence-drift-join.ttl: {} finding(s)\n",
        findings.len()
    );
    for finding in &findings {
        println!("code:       {}", finding.code);
        println!("message:    {}", finding.message);
        println!("cited_iris: {:?}\n", finding.cited_iris);
    }

    let drift_iris: Vec<&str> = findings
        .iter()
        .filter(|f| f.message.contains("math:StructuralKeyDrift"))
        .flat_map(|f| f.cited_iris.iter().map(String::as_str))
        .collect();
    assert_eq!(
        drift_iris.len(),
        2,
        "expected exactly two math:StructuralKeyDrift findings, each citing one IRI"
    );
    assert_eq!(
        drift_iris[0], drift_iris[1],
        "the two alpha-equivalent expressions must cite the SAME α-equivalence-class IRI"
    );
    println!(
        "OK: both alpha-equivalent expressions (ex:sumBinderI, ex:sumBinderJ) cite the SAME \
         α-equivalence-class IRI: {}",
        drift_iris[0]
    );
}
