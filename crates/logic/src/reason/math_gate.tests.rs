// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

/// The embedded `math/module.ttl` compiles cleanly (no error-severity diagnostic) and
/// carries the two builtin-bound-consequent dimension-gate constraints PLUS the three
/// `math:UndimensionedQuantity` coverage obligations (R4: every `math:homogeneousOperand`
/// / integral `math:integrand` / `math:withRespectTo` target itself carries a
/// `math:hasDimension`, closing the gap the `math:Quantity`-scoped `hasDimension min 1`
/// restriction alone does not reach). This does NOT exercise the `verify()` production
/// surface (that is `dimension_gate.rs`'s job) — it pins the compile-source contract the
/// `make-validate` SHACL surface then derives from, so a future authoring mistake in
/// `module.ttl` is caught here rather than only downstream.
#[test]
fn embedded_module_ttl_compiles_and_carries_the_dimension_gate_constraints() {
    let observed = &crate::verify::prepared_gates::shared().math_compilation;
    let errors: Vec<_> = observed
        .diagnostics
        .iter()
        .filter(|d| d.severity == gmeow_logic_compile::frontend::Severity::Error)
        .collect();
    assert!(
        errors.is_empty(),
        "the embedded math/module.ttl must compile with no error diagnostics: {errors:?}"
    );

    let expect_constraint = |name: &str, expected_target: &str, expected_class: &str| {
        let constraint = observed
            .constraints
            .iter()
            .find(|c| c.iri.ends_with(name))
            .unwrap_or_else(|| panic!("expected constraint {name} to be present"));
        assert_eq!(
            constraint.target, expected_target,
            "{name} must target {expected_target}"
        );
        assert_eq!(
            constraint.failure_class.as_deref(),
            Some(expected_class),
            "{name} must enforce {expected_class}"
        );
        // The SHACL/SPARQL derivation the make-validate surface consumes must render
        // non-empty — proves the Exists-consequent constraint shape is projectable, not
        // just parseable.
        let sparql = &constraint.projected_sparql;
        assert!(
            !sparql.trim().is_empty(),
            "{name} must project a non-empty sh:SPARQLConstraint"
        );
    };
    expect_constraint(
        "HomogeneousOperandDimensionedConstraint",
        "ObjectsOf(\"https://blackcatinformatics.ca/math/homogeneousOperand\")",
        "https://blackcatinformatics.ca/math/UndimensionedQuantity",
    );
    expect_constraint(
        "IntegrandDimensionedConstraint",
        "ObjectsOf(\"https://blackcatinformatics.ca/math/integrand\")",
        "https://blackcatinformatics.ca/math/UndimensionedQuantity",
    );
    expect_constraint(
        "WithRespectToDimensionedConstraint",
        "ObjectsOf(\"https://blackcatinformatics.ca/math/withRespectTo\")",
        "https://blackcatinformatics.ca/math/UndimensionedQuantity",
    );
}
