// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the canonical IR — the Rust mirror of `tests/test_logic_ir.py`.

use super::*;

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

fn kind_pred() -> String {
    format!("{LOGIC}Kind")
}

fn axiom(subj: &str, pred: &str, obj: &str) -> LogicAxiom {
    LogicAxiom::ground(subj, pred, obj, false).unwrap()
}

// ── Enum surface (local names match module.ttl verbatim) ─────────────────────

#[test]
fn semantic_profile_ids_match_module_ttl() {
    let got: std::collections::BTreeSet<&str> = [
        SemanticProfileId::PositiveHorn,
        SemanticProfileId::StratifiedNaf,
        SemanticProfileId::WellFounded,
        SemanticProfileId::StableModel,
        SemanticProfileId::ProceduralProlog,
        SemanticProfileId::Probabilistic,
    ]
    .iter()
    .map(|p| p.as_str())
    .collect();
    let expected: std::collections::BTreeSet<&str> = [
        "PositiveHornProfile",
        "StratifiedNAFProfile",
        "WellFoundedProfile",
        "StableModelProfile",
        "ProceduralPrologProfile",
        "ProbabilisticProfile",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, expected);
    // Round-trip through from_local.
    for p in expected {
        assert_eq!(SemanticProfileId::from_local(p).unwrap().as_str(), p);
    }
}

#[test]
fn compatibility_rule_ids_match_module_ttl() {
    // The Rust authority (compat.rs ALL_RULE_IDS) and the ontology surface
    // (logic:CompatibilityRule individuals in module.ttl) must never diverge:
    // every rust rule id is an individual local name and vice versa.
    use crate::compile::compat::ALL_RULE_IDS;

    let module_ttl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/core/logic/module.ttl");
    let text = std::fs::read_to_string(&module_ttl)
        .unwrap_or_else(|e| panic!("read {}: {e}", module_ttl.display()));

    // Each individual is declared at column 0 as `logic:<Name>` and carries a
    // `logic:CompatibilityRule` rdf:type within its statement block (terminated by
    // a line-final ` .`).  Walk the blocks and collect the subjects whose block
    // names logic:CompatibilityRule as a type.
    let mut from_ttl: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut current_subject: Option<String> = None;
    let mut block_is_rule = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("logic:") {
            // A new top-level subject block begins.
            if let (Some(subj), true) = (current_subject.take(), block_is_rule) {
                from_ttl.insert(subj);
            }
            let name: String = rest.chars().take_while(|c| !c.is_whitespace()).collect();
            current_subject = Some(name);
            block_is_rule = false;
        }
        if line.contains("logic:CompatibilityRule") && !line.contains("a owl:Class") {
            // Skip the class declaration itself (`logic:CompatibilityRule a owl:Class`);
            // a type reference inside an individual block flags it as a rule.
            if current_subject.as_deref() != Some("CompatibilityRule") {
                block_is_rule = true;
            }
        }
    }
    if let (Some(subj), true) = (current_subject, block_is_rule) {
        from_ttl.insert(subj);
    }

    let from_rust: std::collections::BTreeSet<String> =
        ALL_RULE_IDS.iter().map(|s| (*s).to_owned()).collect();

    assert_eq!(
        from_rust, from_ttl,
        "Rust compat rule ids must match logic:CompatibilityRule individuals in module.ttl"
    );
}

#[test]
fn preservation_kind_values_match_module_ttl() {
    let got: std::collections::BTreeSet<&str> = [
        PreservationKind::Exact,
        PreservationKind::SoundUnder,
        PreservationKind::CompleteOver,
        PreservationKind::ValidationOnly,
        PreservationKind::InconsistencyPreserving,
        PreservationKind::InconsistencyReflecting,
    ]
    .iter()
    .map(|k| k.as_str())
    .collect();
    let expected: std::collections::BTreeSet<&str> = [
        "ExactPreservation",
        "SoundUnderApproximation",
        "CompleteOverApproximation",
        "ValidationOnly",
        "InconsistencyPreserving",
        "InconsistencyReflecting",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, expected);
}

#[test]
fn logic_modality_has_none_sentinel() {
    assert_eq!(LogicModality::None.as_str(), "none");
    assert_eq!(LogicModality::default(), LogicModality::None);
    // NONE + 7 world types == 8.
    for (s, m) in [
        ("none", LogicModality::None),
        ("alethic", LogicModality::Alethic),
        ("epistemic", LogicModality::Epistemic),
        ("doxastic", LogicModality::Doxastic),
        ("telic", LogicModality::Telic),
        ("deontic", LogicModality::Deontic),
        ("representational", LogicModality::Representational),
        ("counterfactual", LogicModality::Counterfactual),
    ] {
        assert_eq!(m.as_str(), s);
        assert_eq!(LogicModality::from_str_value(s), Some(m));
    }
}

// ── ComplexityClass ──────────────────────────────────────────────────────────

#[test]
fn complexity_class_round_trips() {
    let cc = ComplexityClass::new("PTIME").unwrap();
    assert_eq!(cc.to_string(), "PTIME");
    assert_eq!(cc.label(), "PTIME");
}

#[test]
fn complexity_class_rejects_empty_and_whitespace() {
    assert!(ComplexityClass::new("").is_err());
    assert!(ComplexityClass::new("   ").is_err());
    assert!(ComplexityClass::new("").unwrap_err().contains("non-empty"));
}

// ── ContextualScope validation ───────────────────────────────────────────────

#[test]
fn contextual_scope_defaults_are_none() {
    let scope = ContextualScope::default();
    assert!(scope.standpoint.is_none());
    assert!(scope.time.is_none());
    assert!(scope.confidence.is_none());
    assert_eq!(scope.modality, LogicModality::None);
    assert!(scope.provenance.is_none());
}

#[test]
fn contextual_scope_confidence_valid_bounds() {
    for c in [0.0, 0.5, 1.0] {
        let scope = ContextualScope::new(None, None, Some(c), LogicModality::None, None).unwrap();
        assert_eq!(scope.confidence, Some(c));
    }
}

#[test]
fn contextual_scope_confidence_out_of_range() {
    for bad in [-0.1, 1.01, 2.0, -1.0] {
        let r = ContextualScope::new(None, None, Some(bad), LogicModality::None, None);
        assert!(r.is_err());
        assert!(r.unwrap_err().contains("confidence"));
    }
}

// ── LogicAxiom ───────────────────────────────────────────────────────────────

#[test]
fn logic_axiom_equality() {
    let a1 = axiom("ex:s", &kind_pred(), "ex:o");
    let a2 = axiom("ex:s", &kind_pred(), "ex:o");
    assert_eq!(a1, a2);
}

#[test]
fn logic_axiom_rejects_empty_subject_and_predicate() {
    let r = LogicAxiom::ground("", kind_pred(), "ex:o", false);
    assert!(r.unwrap_err().contains("subject"));
    let r = LogicAxiom::ground("ex:s", "", "ex:o", false);
    assert!(r.unwrap_err().contains("predicate"));
}

#[test]
fn logic_axiom_literal_flag() {
    let a = LogicAxiom::ground("ex:s", format!("{LOGIC}confidence"), "0.9", true).unwrap();
    assert!(a.obj_is_literal);
}

#[test]
fn logic_axiom_sort_key_byte_parity() {
    // Mirrors the Python `_sort_key`: null-byte separators, Python bool Display.
    let a = axiom("ex:s", "p", "ex:o");
    assert_eq!(a.sort_key(), "ex:s\u{0}p\u{0}ex:o\u{0}False");
    let lit = LogicAxiom::ground("ex:s", "p", "v", true).unwrap();
    assert_eq!(lit.sort_key(), "ex:s\u{0}p\u{0}v\u{0}True");
    // negated appends only when true.
    let neg =
        LogicAxiom::new("ex:s", "p", "ex:o", false, true, ContextualScope::default()).unwrap();
    assert_eq!(neg.sort_key(), "ex:s\u{0}p\u{0}ex:o\u{0}False\u{0}True");
}

// ── LogicRule body canonicalization ──────────────────────────────────────────

#[test]
fn logic_rule_body_is_canonicalized() {
    let a1 = axiom("ex:a", &format!("{LOGIC}rigidlyAppliesTo"), "ex:b");
    let a2 = axiom("ex:c", &format!("{LOGIC}mediates"), "ex:d");
    let head = axiom("ex:s", &kind_pred(), "ex:o");

    let rule_ab = LogicRule::new(
        head.clone(),
        vec![a1.clone(), a2.clone()],
        vec![],
        Default::default(),
    );
    let rule_ba = LogicRule::new(head, vec![a2, a1], vec![], Default::default());

    assert_eq!(rule_ab, rule_ba);
    assert_eq!(rule_ab.body, rule_ba.body);
}

#[test]
fn logic_rule_distinct_pairs_canonicalized() {
    let head = axiom("ex:s", &kind_pred(), "ex:o");
    let r1 = LogicRule::new(
        head.clone(),
        vec![],
        vec![("?B".to_owned(), "?A".to_owned())],
        Default::default(),
    );
    let r2 = LogicRule::new(
        head,
        vec![],
        vec![("?A".to_owned(), "?B".to_owned())],
        Default::default(),
    );
    assert_eq!(r1, r2);
    assert_eq!(r1.distinct_pairs, vec![("?A".to_owned(), "?B".to_owned())]);
    // distinct segment appended only when present.
    assert!(r1.sort_key().contains("?A\u{0}?B"));
}

// ── ReasoningContract (#767) ─────────────────────────────────────────────────

#[test]
fn reasoning_contract_with_and_without_complexity() {
    let mut c = ReasoningContract::from_preset(SemanticProfileId::PositiveHorn);
    c.complexity = Some(ComplexityClass::new("PTIME").unwrap());
    c.formula_fragment = Some("HornFragment".to_owned());
    assert_eq!(c.preset, Some(SemanticProfileId::PositiveHorn));
    assert_eq!(c.complexity.as_ref().unwrap().to_string(), "PTIME");
    assert_eq!(c.formula_fragment.as_deref(), Some("HornFragment"));

    let c2 = ReasoningContract::from_preset(SemanticProfileId::StableModel);
    assert!(c2.complexity.is_none());
}

#[test]
fn reasoning_contract_sort_key_is_construction_order_independent() {
    // The set-valued facets must canonicalize regardless of insertion order.
    let mut c1 = ReasoningContract::from_preset(SemanticProfileId::StableModel);
    c1.negation_operators.insert("DefaultNegation".to_owned());
    c1.negation_operators.insert("ExplicitNegation".to_owned());
    c1.projection_targets.insert("OwlProjection".to_owned());

    let mut c2 = ReasoningContract::from_preset(SemanticProfileId::StableModel);
    c2.negation_operators.insert("ExplicitNegation".to_owned());
    c2.negation_operators.insert("DefaultNegation".to_owned());
    c2.projection_targets.insert("OwlProjection".to_owned());

    assert_eq!(c1, c2);
    assert_eq!(c1.sort_key(), c2.sort_key());
}

// ── LogicProgram order-independence (the core canonicalization contract) ──────

#[test]
fn logic_program_order_independence_equality() {
    let a1 = axiom("ex:x", &kind_pred(), "ex:o");
    let a2 = axiom("ex:y", &format!("{LOGIC}Role"), "ex:o");
    let p1 = ReasoningContract::from_preset(SemanticProfileId::PositiveHorn);
    let p2 = ReasoningContract::from_preset(SemanticProfileId::StratifiedNaf);

    let prog_ab = LogicProgram::new(
        vec![a1.clone(), a2.clone()],
        vec![],
        vec![p1.clone(), p2.clone()],
        None,
    );
    let prog_ba = LogicProgram::new(vec![a2, a1], vec![], vec![p2, p1], None);

    assert_eq!(prog_ab, prog_ba);
    assert_eq!(prog_ab.canonical_key(), prog_ba.canonical_key());
}

#[test]
fn logic_program_canonical_is_stable() {
    let a1 = axiom("ex:x", &kind_pred(), "ex:o");
    let a2 = axiom("ex:y", &format!("{LOGIC}Phase"), "ex:o");

    let prog1 = LogicProgram::new(vec![a1.clone(), a2.clone()], vec![], vec![], None);
    let prog2 = LogicProgram::new(vec![a2, a1], vec![], vec![], None);

    assert_eq!(prog1.canonical_key(), prog2.canonical_key());
}

#[test]
fn logic_program_canonical_round_trips_scope() {
    let scope = ContextualScope::new(
        Some("https://example.org/sp".to_owned()),
        None,
        Some(0.8),
        LogicModality::Epistemic,
        Some("https://example.org/agent".to_owned()),
    )
    .unwrap();
    let ax = LogicAxiom::new(
        "ex:s",
        format!("{LOGIC}rigidlyAppliesTo"),
        "ex:o",
        false,
        false,
        scope.clone(),
    )
    .unwrap();
    let prog = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
    // Scope survives into the canonical content key and round-trips through equality.
    assert_eq!(prog.axioms[0].scope, scope);
    assert!(prog.canonical_key().contains("epistemic"));
    assert!(prog.canonical_key().contains("https://example.org/sp"));
}

#[test]
fn logic_program_with_rules_order_independence() {
    let head = axiom("ex:s", &kind_pred(), "ex:o");
    let body1 = axiom("ex:a", &format!("{LOGIC}rigidlyAppliesTo"), "ex:b");
    let body2 = axiom("ex:c", &format!("{LOGIC}mediates"), "ex:d");
    let rule1 = LogicRule::new(head.clone(), vec![body1], vec![], Default::default());
    let rule2 = LogicRule::new(head, vec![body2], vec![], Default::default());

    let prog_12 = LogicProgram::new(vec![], vec![rule1.clone(), rule2.clone()], vec![], None);
    let prog_21 = LogicProgram::new(vec![], vec![rule2, rule1], vec![], None);

    assert_eq!(prog_12, prog_21);
    assert_eq!(prog_12.canonical_key(), prog_21.canonical_key());
}

#[test]
fn logic_program_canonical_treats_signed_zero_confidence_equally() {
    // Regression for D2: `-0.0` and `0.0` compare equal via PartialEq but
    // `(-0.0f64).to_string()` yields "-0", producing divergent dedup/canonical
    // keys for two logically-identical programs.  The fix normalises to `0.0`
    // before `to_string()` in both `content_key` (ir.rs) and
    // `content_dedup_key` (frontend.rs).
    let scope_pos = ContextualScope::new(None, None, Some(0.0), LogicModality::None, None).unwrap();
    let scope_neg =
        ContextualScope::new(None, None, Some(-0.0), LogicModality::None, None).unwrap();
    let ax_pos =
        LogicAxiom::new("ex:s", format!("{LOGIC}p"), "ex:o", false, false, scope_pos).unwrap();
    let ax_neg =
        LogicAxiom::new("ex:s", format!("{LOGIC}p"), "ex:o", false, false, scope_neg).unwrap();
    let prog_pos = LogicProgram::new(vec![ax_pos], vec![], vec![], None);
    let prog_neg = LogicProgram::new(vec![ax_neg], vec![], vec![], None);
    assert_eq!(prog_pos, prog_neg);
    assert_eq!(prog_pos.canonical_key(), prog_neg.canonical_key());
}

#[test]
fn logic_program_source_iri_preserved() {
    let prog = LogicProgram::new(
        vec![],
        vec![],
        vec![],
        Some("https://example.org/prog".to_owned()),
    );
    assert_eq!(prog.source_iri.as_deref(), Some("https://example.org/prog"));
    assert!(prog.axioms.is_empty());
    assert!(prog.rules.is_empty());
    assert!(prog.contracts.is_empty());
}
