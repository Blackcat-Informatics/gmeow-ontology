// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only grading of authenticated source-only math lowering observations.
//! No test parses or lowers an authored source or regenerates a missing record.

use super::super::math_lowering::{
    ALPHA_A, ALPHA_B, CHANNEL, COUNTER_EXAMPLES, DEPTH, MODULE, Observations, REFERENCE, SHADOW,
    SourceObservation,
};
use gmeow_logic::math_expression::analysis::MathLoweringError;
use std::sync::OnceLock;

fn observations() -> &'static Observations {
    static OBSERVATIONS: OnceLock<
        Result<super::source_artifact::Selected<Observations>, gmeow_errors::Diag>,
    > = OnceLock::new();
    super::source_artifact::get(&OBSERVATIONS, CHANNEL)
}

fn source(path: &str) -> &'static SourceObservation {
    let result = if matches!(path, MODULE | REFERENCE) {
        let grounding = super::gmn_grounding::observations();
        grounding
            .sources
            .get(path)
            .unwrap_or_else(|| panic!("missing grounding source {path}"))
            .math_lowering
            .as_ref()
            .unwrap_or_else(|| panic!("missing math lowering observation {path}"))
    } else {
        assert!(
            observations().source_digests.contains_key(path),
            "missing original source identity {path}"
        );
        observations()
            .sources
            .get(path)
            .unwrap_or_else(|| panic!("missing math lowering source {path}"))
    };
    assert_eq!(result.source_path, path);
    result
}

fn required_key<'a>(source: &'a SourceObservation, root: &str) -> &'a String {
    source
        .keys
        .get(root)
        .unwrap_or_else(|| panic!("missing expression root {root}"))
        .as_ref()
        .unwrap_or_else(|error| panic!("expression root {root} rejected: {error}"))
}

fn counter_example_fixtures() -> Vec<(String, &'static SourceObservation)> {
    observations()
        .sources
        .iter()
        .filter(|(path, _)| path.starts_with(&format!("{COUNTER_EXAMPLES}/")))
        .map(|(path, observed)| {
            (
                path.rsplit('/').next().expect("source filename").to_owned(),
                observed,
            )
        })
        .collect()
}

fn sample_variants() -> Vec<MathLoweringError> {
    vec![
        MathLoweringError::NumberLiteralMissingValue {
            node: "n".to_owned(),
        },
        MathLoweringError::UnrecognizedExpressionType {
            node: "n".to_owned(),
            types: vec!["https://blackcatinformatics.ca/math/SomeUnrecognizedType".to_owned()],
        },
        MathLoweringError::ArgumentSlotMissingIndex {
            slot: "s".to_owned(),
        },
        MathLoweringError::ArgumentSlotMultipleIndexes {
            slot: "s".to_owned(),
            count: 2,
        },
        MathLoweringError::ArgumentSlotIndexNotInteger {
            slot: "s".to_owned(),
            lexical: "x".to_owned(),
        },
        MathLoweringError::ArgumentSlotMissingExpression {
            slot: "s".to_owned(),
        },
        MathLoweringError::NonContiguousArgumentSlots {
            node: "n".to_owned(),
            index: 2,
            expected_position: 1,
        },
        MathLoweringError::DuplicateArgumentSlotIndex {
            node: "n".to_owned(),
            index: 0,
        },
        MathLoweringError::NegativeArgumentSlotIndex {
            node: "n".to_owned(),
            slot: "s".to_owned(),
            index: -1,
        },
        MathLoweringError::ApplicationMissingOperator {
            node: "n".to_owned(),
        },
        MathLoweringError::ApplicationMultipleOperators {
            node: "n".to_owned(),
            count: 2,
        },
        MathLoweringError::BindingMissingOperator {
            node: "n".to_owned(),
        },
        MathLoweringError::BindingMultipleOperators {
            node: "n".to_owned(),
            count: 2,
        },
        MathLoweringError::BindingMissingBody {
            node: "https://example.org/binder".to_owned(),
        },
        MathLoweringError::BindingMissingBoundVariable {
            node: "n".to_owned(),
        },
        MathLoweringError::BindingMultipleBoundVariables {
            node: "n".to_owned(),
            count: 2,
        },
        MathLoweringError::VariableExpressionMissingOccurrence {
            node: "n".to_owned(),
        },
        MathLoweringError::VariableExpressionMultipleOccurrences {
            node: "n".to_owned(),
            count: 2,
        },
        MathLoweringError::OccurrenceMissingDeclaredVariable {
            occurrence: "o".to_owned(),
        },
        MathLoweringError::OccurrenceMultipleDeclaredVariables {
            occurrence: "o".to_owned(),
            count: 2,
        },
        MathLoweringError::UnscopedOccurrence {
            occurrence: "o".to_owned(),
            declaration: "d".to_owned(),
        },
        MathLoweringError::CyclicExpressionGraph {
            node: "n".to_owned(),
        },
        MathLoweringError::ExpressionDepthExceeded {
            node: "n".to_owned(),
            depth: 501,
        },
    ]
}

fn variant_label(v: &MathLoweringError) -> &'static str {
    match v {
        MathLoweringError::NumberLiteralMissingValue { .. } => "NumberLiteralMissingValue",
        MathLoweringError::UnrecognizedExpressionType { .. } => "UnrecognizedExpressionType",
        MathLoweringError::ArgumentSlotMissingIndex { .. } => "ArgumentSlotMissingIndex",
        MathLoweringError::ArgumentSlotMultipleIndexes { .. } => "ArgumentSlotMultipleIndexes",
        MathLoweringError::ArgumentSlotIndexNotInteger { .. } => "ArgumentSlotIndexNotInteger",
        MathLoweringError::ArgumentSlotMissingExpression { .. } => "ArgumentSlotMissingExpression",
        MathLoweringError::NonContiguousArgumentSlots { .. } => "NonContiguousArgumentSlots",
        MathLoweringError::DuplicateArgumentSlotIndex { .. } => "DuplicateArgumentSlotIndex",
        MathLoweringError::NegativeArgumentSlotIndex { .. } => "NegativeArgumentSlotIndex",
        MathLoweringError::ApplicationMissingOperator { .. } => "ApplicationMissingOperator",
        MathLoweringError::ApplicationMultipleOperators { .. } => "ApplicationMultipleOperators",
        MathLoweringError::BindingMissingOperator { .. } => "BindingMissingOperator",
        MathLoweringError::BindingMultipleOperators { .. } => "BindingMultipleOperators",
        MathLoweringError::BindingMissingBody { .. } => "BindingMissingBody",
        MathLoweringError::BindingMissingBoundVariable { .. } => "BindingMissingBoundVariable",
        MathLoweringError::BindingMultipleBoundVariables { .. } => "BindingMultipleBoundVariables",
        MathLoweringError::VariableExpressionMissingOccurrence { .. } => {
            "VariableExpressionMissingOccurrence"
        }
        MathLoweringError::VariableExpressionMultipleOccurrences { .. } => {
            "VariableExpressionMultipleOccurrences"
        }
        MathLoweringError::OccurrenceMissingDeclaredVariable { .. } => {
            "OccurrenceMissingDeclaredVariable"
        }
        MathLoweringError::OccurrenceMultipleDeclaredVariables { .. } => {
            "OccurrenceMultipleDeclaredVariables"
        }
        MathLoweringError::UnscopedOccurrence { .. } => "UnscopedOccurrence",
        MathLoweringError::CyclicExpressionGraph { .. } => "CyclicExpressionGraph",
        MathLoweringError::ExpressionDepthExceeded { .. } => "ExpressionDepthExceeded",
    }
}

fn the_depth_fixture_chain_is_pinned_to_the_depth_bound() {
    let nodes = source(DEPTH).application_nodes;
    let depth_limit = observations().depth_limit;
    assert!(
        nodes > depth_limit,
        "the depth fixture must EXCEED the bound it exercises: {nodes} application nodes \
         vs MAX_MATH_EXPRESSION_DEPTH {depth_limit} — regenerate the chain"
    );
}

fn every_math_lowering_error_variant_is_produced_by_a_committed_fixture() {
    let fixtures = counter_example_fixtures();
    assert!(
        !fixtures.is_empty(),
        "tests/counter-examples has no fixtures to drive this liveness test"
    );

    // discriminant -> (label, fixtures that produced it) — built by ACTUALLY RUNNING the
    // real production lowering entry point over every committed fixture, never by
    // hand-constructing a variant.
    let mut produced: std::collections::HashMap<
        std::mem::Discriminant<MathLoweringError>,
        (&'static str, Vec<String>),
    > = std::collections::HashMap::new();
    for (name, ds) in &fixtures {
        for result in ds.keys.values().cloned() {
            if let Err(err) = result {
                let discriminant = std::mem::discriminant(&err);
                produced
                    .entry(discriminant)
                    .or_insert_with(|| (variant_label(&err), Vec::new()))
                    .1
                    .push(name.clone());
            }
        }
    }

    let samples = sample_variants();
    assert_eq!(
        samples.len(),
        23,
        "sample_variants() must enumerate every MathLoweringError variant exactly once \
         (update this count alongside the enum and both variant lists when a variant is \
         added or removed)"
    );
    // The count alone would still pass if one variant were listed twice and another
    // dropped — the two errors cancel. Pin DISTINCTNESS by discriminant so a duplicate
    // is caught as itself rather than hiding an omission.
    let distinct: std::collections::BTreeSet<_> = samples
        .iter()
        .map(|v| format!("{:?}", std::mem::discriminant(v)))
        .collect();
    assert_eq!(
        distinct.len(),
        samples.len(),
        "sample_variants() must list each variant ONCE; a duplicate silently masks a \
         missing one because only the total is checked"
    );
    let missing: Vec<&'static str> = samples
        .iter()
        .filter(|v| !produced.contains_key(&std::mem::discriminant(*v)))
        .map(variant_label)
        .collect();
    assert!(
        missing.is_empty(),
        "the following MathLoweringError variant(s) are produced by NO committed \
         counter-example fixture through the real production lowering entry point — each \
         is a phantom failure class (reachable-by-name in Rust, unreachable from any \
         authored data): {missing:?}. Fixtures that DID trip a variant: {:#?}",
        produced
            .values()
            .map(|(label, files)| (*label, files.clone()))
            .collect::<std::collections::BTreeMap<_, _>>()
    );
}

fn math_lowering_error_failure_class_is_exhaustive_and_non_empty() {
    let variants = sample_variants();

    // Every variant must decide a non-empty, properly-namespaced `math:` IRI, and the
    // Display impl must not panic (each variant is exercised through `{}`).
    for variant in &variants {
        let class = variant.failure_class();
        assert!(!class.is_empty(), "{variant:?} has an empty failure class");
        assert!(
            class.starts_with("https://blackcatinformatics.ca/math/"),
            "{variant:?} failure class {class} is not `math:`-namespaced"
        );
        let _ = format!("{variant}");
    }

    // Check the mapping against the ONTOLOGY, never against a copy of the mapping.
    //
    // This assertion used to be a second, hand-maintained `match` restating
    // `failure_class()` arm for arm. That form cannot fail for a WRONG bucket — it asserts
    // the function equals itself — and it is why two variants sat mis-typed against the
    // target class's own definition without any test noticing. Reading the authored
    // module.ttl instead means a class this code decides but the slice never authored, or
    // deletes, fails here.
    let module = source(MODULE);
    for variant in &variants {
        let class = variant.failure_class();
        let local = class
            .rsplit('/')
            .next()
            .expect("class IRI has a local name");
        assert!(
            module.subjects.contains(class),
            "{variant:?} decides math:{local}, which module.ttl does not author"
        );
    }

    // Distinct buckets, derived rather than hardcoded: the count follows the mapping, and
    // the assertion above is what pins each one to an authored class.
    let distinct_classes: std::collections::BTreeSet<&'static str> = variants
        .iter()
        .map(MathLoweringError::failure_class)
        .collect();
    assert_eq!(
        distinct_classes.len(),
        10,
        "the rejection algebra must keep EXACTLY its ten distinct failure-class buckets — \
         an inequality here would let two buckets silently collapse into one: \
         {distinct_classes:?}"
    );
}

fn committed_alpha_equivalence_fixtures_are_genuinely_alpha_equivalent() {
    let sum_root = "http://example.org/math/sumBinder";
    let digest_a = required_key(source(ALPHA_A), sum_root);
    let digest_b = required_key(source(ALPHA_B), sum_root);

    assert_eq!(
        digest_a, digest_b,
        "alpha-equivalent-pair-a.ttl and alpha-equivalent-pair-b.ttl differ only in \
         their bound-variable declaration IRI and must share one structural digest"
    );

    let shadowing_root = "http://example.org/math/outerSum";
    let shadow = source(SHADOW);
    let digest_shadow_1 = required_key(shadow, shadowing_root);
    let digest_shadow_2 = shadow
        .repeated_shadow
        .as_ref()
        .expect("independent shadow lowering observed")
        .as_ref()
        .expect("shadowing fixture lowers (second pass)");
    assert_eq!(
        digest_shadow_1, digest_shadow_2,
        "the committed shadowing fixture's digest is deterministic across separate lowerings"
    );
    assert_ne!(
        digest_shadow_1, digest_a,
        "the shadowing (nested binder) fixture is not alpha-equivalent to the bare \
         one-variable summation of pair-a/pair-b"
    );
}

fn reference_ast_act_structural_key_matches_recomputed_digest() {
    const NS: &str = "https://blackcatinformatics.ca/gmeow/examples/math/reference-act/";
    let observed = source(REFERENCE);
    let keys = &observed.keys;

    let ast_root = format!("{NS}matrixProductAst");
    let normal_root = format!("{NS}matrixProductNormalForm");
    let ast_digest = keys
        .get(&ast_root)
        .expect("matrixProductAst is a root")
        .as_ref()
        .expect("matrixProductAst lowers")
        .clone();
    let normal_digest = keys
        .get(&normal_root)
        .expect("matrixProductNormalForm is a root")
        .as_ref()
        .expect("matrixProductNormalForm lowers")
        .clone();

    // The two are declared `math:structuralNormalization`-equivalent (same
    // operator, same operand slots) — they really are the same structure, so the
    // same digest.
    assert_eq!(
        ast_digest, normal_digest,
        "matrixProductAst and matrixProductNormalForm must share one structural digest"
    );

    // The authored `math:structuralKey` on BOTH expressions must be the REAL,
    // recomputed digest — never the known placeholder.
    let authored_ast_key = observed
        .authored_keys
        .get(&ast_root)
        .and_then(|values| values.first())
        .expect("matrixProductAst carries a math:structuralKey");
    let authored_normal_key = observed
        .authored_keys
        .get(&normal_root)
        .and_then(|values| values.first())
        .expect("matrixProductNormalForm carries a math:structuralKey");

    assert_ne!(
        authored_ast_key, "placeholder-alpha-equivalent-digest-v1",
        "the authored math:structuralKey must be the REAL digest, not the known placeholder"
    );
    assert_eq!(
        authored_ast_key, &ast_digest,
        "the authored math:structuralKey on matrixProductAst must match the recomputed \
         digest"
    );
    assert_eq!(
        authored_normal_key, &normal_digest,
        "the authored math:structuralKey on matrixProductNormalForm must match the \
         recomputed digest"
    );
}

fn authored_finite_cardinalities_match_native_datatype_capacities() {
    let observed = &source(MODULE).finite_value_spaces;
    assert!(
        !observed.is_empty(),
        "math must author at least one finite datatype space"
    );
    let mut authored = std::collections::BTreeMap::new();
    for (subject, fields) in observed {
        assert_eq!(
            fields.datatypes.len(),
            1,
            "{subject}: exactly one datatype is required"
        );
        let datatype = fields.datatypes.first().unwrap().clone();
        let counts: std::collections::BTreeSet<_> = fields
            .counts
            .iter()
            .map(|count| {
                count
                    .trim()
                    .parse::<u128>()
                    .expect("finite count is an integer")
            })
            .collect();
        assert_eq!(counts.len(), 1, "{subject}: exactly one count is required");
        let count = *counts.first().unwrap();
        assert!(
            authored.insert(datatype, count).is_none(),
            "duplicate datatype inventory owner"
        );
    }
    for (datatype, count) in authored {
        assert_eq!(
            gmeow_logic::reason::finite_named_datatype_capacity(&datatype),
            Some(count),
            "authored finite capacity for {datatype} must agree with the native engine",
        );
    }
}

#[test]
fn authored_math_lowering_contracts() {
    authored_finite_cardinalities_match_native_datatype_capacities();
    the_depth_fixture_chain_is_pinned_to_the_depth_bound();
    every_math_lowering_error_variant_is_produced_by_a_committed_fixture();
    math_lowering_error_failure_class_is_exhaustive_and_non_empty();
    committed_alpha_equivalence_fixtures_are_genuinely_alpha_equivalent();
    reference_ast_act_structural_key_matches_recomputed_digest();
}
