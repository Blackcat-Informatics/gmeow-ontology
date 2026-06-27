// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for the canonical IR.  These are the authoritative IR tests; the Python
//! `tests/test_logic_ir.py` they superseded has been retired.

use super::*;

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

fn kind_pred() -> String {
    format!("{LOGIC}Kind")
}

fn axiom(subj: &str, pred: &str, obj: &str) -> LogicAxiom {
    LogicAxiom::ground(subj, pred, obj, false).unwrap()
}

/// Read the canonical `module.ttl` (relative to the crate manifest).
fn module_ttl_text() -> String {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/core/logic/module.ttl");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The `logic:` local name at the head of a Turtle subject line: everything up to
/// the first whitespace or Turtle terminal punctuation (`;`, `,`, `.`).  Trimming
/// the punctuation keeps these parser helpers correct even if `module.ttl` is ever
/// reformatted to put a separator flush against the subject (e.g. `logic:Foo;`).
fn local_name(rest: &str) -> String {
    rest.chars()
        .take_while(|c| !c.is_whitespace() && !matches!(c, ';' | ',' | '.'))
        .collect()
}

/// Collect the local names of every individual in `module.ttl` whose block names
/// `logic:<type_local>` in an rdf:type position — either the inline `a … logic:T`
/// clause or a bare `logic:T ;`/`logic:T ,` type-list continuation.  Deliberately
/// ignores `rdfs:range logic:T` and other object positions (so the `logic:<prop>`
/// property whose range is the taxonomy class is not mistaken for one of its
/// members) and the class declaration itself.
fn individuals_of_type(text: &str, type_local: &str) -> std::collections::BTreeSet<String> {
    let type_ref = format!("logic:{type_local}");
    let mut out: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut current: Option<String> = None;
    let mut is_member = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("logic:") {
            if let (Some(subj), true) = (current.take(), is_member) {
                out.insert(subj);
            }
            current = Some(local_name(rest));
            is_member = false;
        }
        let trimmed = line.trim_start();
        let bare_type_entry = trimmed
            .strip_prefix(&type_ref)
            .is_some_and(|r| r.is_empty() || r.starts_with([' ', '\t', ';', ',']));
        let inline_type = trimmed.starts_with("a ") && line.contains(&type_ref);
        if (bare_type_entry || inline_type) && current.as_deref() != Some(type_local) {
            is_member = true;
        }
    }
    if let (Some(subj), true) = (current, is_member) {
        out.insert(subj);
    }
    out
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

    // The six preset local names must be EXACTLY the logic:ReasoningPreset named
    // individuals declared in module.ttl (#767, reviewer B1): the historical
    // logic:SemanticProfile class is retired, so the source of truth is now the
    // set of logic:ReasoningPreset individuals. Walk the top-level subject blocks
    // and collect every subject whose block names logic:ReasoningPreset as a type
    // (skipping the class declaration itself).
    let module_ttl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/core/logic/module.ttl");
    let text = std::fs::read_to_string(&module_ttl)
        .unwrap_or_else(|e| panic!("read {}: {e}", module_ttl.display()));

    let mut from_ttl: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut current_subject: Option<String> = None;
    let mut block_is_preset = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("logic:") {
            if let (Some(subj), true) = (current_subject.take(), block_is_preset) {
                from_ttl.insert(subj);
            }
            current_subject = Some(local_name(rest));
            block_is_preset = false;
        }
        // Only an rdf:type reference flags the block — a type-list line, never the
        // class declaration itself, the expandsToFacet property's prose, or any
        // other skos:definition prose (which is quoted, so exclude quoted lines).
        if line.contains("logic:ReasoningPreset")
            && !line.contains('"')
            && !line.contains("a owl:Class")
            && current_subject.as_deref() != Some("ReasoningPreset")
            && current_subject.as_deref() != Some("expandsToFacet")
        {
            block_is_preset = true;
        }
    }
    if let (Some(subj), true) = (current_subject, block_is_preset) {
        from_ttl.insert(subj);
    }

    let from_rust: std::collections::BTreeSet<&str> = got.iter().copied().collect();
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        from_rust, from_ttl_refs,
        "SemanticProfileId enum must match the logic:ReasoningPreset individuals in module.ttl"
    );

    // Round-trip through from_local.
    for p in &got {
        assert_eq!(SemanticProfileId::from_local(p).unwrap().as_str(), *p);
    }
}

#[test]
fn reasoning_contract_permits_cut_only_with_procedural_execution_facet() {
    // The cut-confinement (AC-2) decision is facet-derived: a contract licenses cut iff
    // its resource policy carries logic:ProceduralExecution — never via the budget facet.
    let mut c = ReasoningContract::new();
    assert!(!c.permits_cut(), "empty contract must not license cut");
    c.resource_policies
        .insert("BudgetBoundedResource".to_owned());
    assert!(
        !c.permits_cut(),
        "a budget-bounded contract must NOT license cut (budget != procedural)"
    );
    c.resource_policies
        .insert(PROCEDURAL_EXECUTION_FACET.to_owned());
    assert!(
        c.permits_cut(),
        "the ProceduralExecution facet licenses cut even alongside a budget"
    );
}

#[test]
fn procedural_preset_carries_procedural_execution_facet() {
    // Tie the Rust cut gate (SemanticProfileId::permits_cut) to the ontology surface:
    // exactly the presets whose module.ttl expandsToFacet bundle includes
    // logic:ProceduralExecution may license cut — so the two can never silently diverge.
    let module_ttl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/core/logic/module.ttl");
    let text = std::fs::read_to_string(&module_ttl)
        .unwrap_or_else(|e| panic!("read {}: {e}", module_ttl.display()));

    // Collect, per top-level preset block, whether it names logic:ProceduralExecution.
    let mut carries: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let mut current: Option<String> = None;
    let mut has_facet = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("logic:") {
            if let Some(subj) = current.take() {
                carries.insert(subj, has_facet);
            }
            current = Some(local_name(rest));
            has_facet = false;
        }
        if line.contains("logic:ProceduralExecution")
            && current.as_deref() != Some("ProceduralExecution")
        {
            has_facet = true;
        }
    }
    if let Some(subj) = current.take() {
        carries.insert(subj, has_facet);
    }

    for id in [
        SemanticProfileId::PositiveHorn,
        SemanticProfileId::StratifiedNaf,
        SemanticProfileId::WellFounded,
        SemanticProfileId::StableModel,
        SemanticProfileId::ProceduralProlog,
        SemanticProfileId::Probabilistic,
    ] {
        let in_ttl = carries.get(id.as_str()).copied().unwrap_or(false);
        assert_eq!(
            in_ttl,
            id.permits_cut(),
            "preset {} cut-licensing must agree between module.ttl ProceduralExecution \
             bundle ({in_ttl}) and SemanticProfileId::permits_cut ({})",
            id.as_str(),
            id.permits_cut()
        );
    }
}

#[test]
fn compatibility_rule_ids_match_module_ttl() {
    // The Rust authority (compat.rs ALL_RULE_IDS) and the ontology surface
    // (logic:CompatibilityRule individuals in module.ttl) must never diverge:
    // every rust rule id is an individual local name and vice versa.
    use crate::compat::ALL_RULE_IDS;

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
            current_subject = Some(local_name(rest));
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
        PreservationKind::Unsupported,
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
        "Unsupported",
    ]
    .into_iter()
    .collect();
    assert_eq!(got, expected);

    // The seven enum values must be EXACTLY the logic:PreservationKind individuals
    // declared in module.ttl — so the new Unsupported floor is pinned to the ontology.
    let from_ttl = individuals_of_type(&module_ttl_text(), "PreservationKind");
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        got, from_ttl_refs,
        "PreservationKind enum must match the logic:PreservationKind individuals in module.ttl"
    );
    assert!(
        from_ttl.contains("Unsupported"),
        "the Unsupported floor is declared"
    );
}

#[test]
fn node_kind_values_match_module_ttl() {
    let from_rust: std::collections::BTreeSet<&str> = [
        NodeKind::ObjectLevelFormula,
        NodeKind::MetaLevelFormula,
        NodeKind::Constraint,
        NodeKind::DerivationRule,
        NodeKind::Query,
        NodeKind::TransactionProgram,
        NodeKind::ActionSchema,
        NodeKind::ValidationShape,
        NodeKind::Correspondence,
    ]
    .iter()
    .map(|k| k.as_str())
    .collect();

    let from_ttl = individuals_of_type(&module_ttl_text(), "NodeKind");
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        from_rust, from_ttl_refs,
        "NodeKind enum must match the logic:NodeKind individuals in module.ttl"
    );

    // Round-trip through from_local, including the reserved ninth Correspondence slot.
    for k in &from_rust {
        assert_eq!(NodeKind::from_local(k).unwrap().as_str(), *k);
    }
    assert!(
        from_rust.contains("Correspondence"),
        "the reserved ninth kind is present"
    );
    assert_eq!(NodeKind::default(), NodeKind::ObjectLevelFormula);
}

#[test]
fn node_kind_folds_into_keys_only_when_non_default() {
    // Axiom: the default ObjectLevelFormula keeps the byte-identical historical key.
    let base = axiom("ex:s", "p", "ex:o");
    assert_eq!(base.sort_key(), "ex:s\u{0}p\u{0}ex:o\u{0}False");
    assert_eq!(base.node_kind, NodeKind::ObjectLevelFormula);
    assert!(!base.load_bearing);

    // A non-default kind diverges, appending the kind segment after the obj-literal flag.
    let meta = axiom("ex:s", "p", "ex:o").with_node_kind(NodeKind::MetaLevelFormula);
    assert_eq!(
        meta.sort_key(),
        "ex:s\u{0}p\u{0}ex:o\u{0}False\u{0}MetaLevelFormula"
    );
    assert_ne!(base, meta);

    // FIXED segment order: load_bearing (when true) BEFORE node_kind, both after negated.
    let lb = axiom("ex:s", "p", "ex:o").with_load_bearing(true);
    assert_eq!(lb.sort_key(), "ex:s\u{0}p\u{0}ex:o\u{0}False\u{0}True");
    let both = axiom("ex:s", "p", "ex:o")
        .with_load_bearing(true)
        .with_node_kind(NodeKind::Constraint);
    assert_eq!(
        both.sort_key(),
        "ex:s\u{0}p\u{0}ex:o\u{0}False\u{0}True\u{0}Constraint"
    );

    // Two axioms differing ONLY in kind are != and have distinct canonical content.
    let p_base = LogicProgram::new(vec![base.clone()], vec![], vec![], None);
    let p_meta = LogicProgram::new(vec![meta.clone()], vec![], vec![], None);
    assert_ne!(p_base.canonical_key(), p_meta.canonical_key());

    // Rule: the two-default-sentinel dual fold-point.  A default-DerivationRule rule
    // with a default head keeps its historical key; flipping the rule kind diverges;
    // flipping ONLY the head-axiom kind diverges independently; the two are distinct.
    let head = axiom("ex:h", "p", "ex:o");
    let r_default = LogicRule::new(head.clone(), vec![], vec![], Default::default());
    assert_eq!(r_default.node_kind, NodeKind::DerivationRule);
    let r_rule_kind = LogicRule::new(head.clone(), vec![], vec![], Default::default())
        .with_node_kind(NodeKind::Constraint);
    let head_meta = axiom("ex:h", "p", "ex:o").with_node_kind(NodeKind::MetaLevelFormula);
    let r_head_kind = LogicRule::new(head_meta, vec![], vec![], Default::default());

    assert_ne!(r_default.sort_key(), r_rule_kind.sort_key());
    assert_ne!(r_default.sort_key(), r_head_kind.sort_key());
    assert_ne!(r_rule_kind.sort_key(), r_head_kind.sort_key());
    // The rule's own kind segment is appended at the end of the rule key.
    assert!(r_rule_kind.sort_key().ends_with("\u{0}Constraint"));
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
    assert!(prog.path_shapes.is_empty());
}

// ── Path shapes (#1010) ──────────────────────────────────────────────────────

fn shape(iri: &str, base: PathBase, min: u32, max: Option<u32>) -> PathShapeIr {
    PathShapeIr::new(iri, base, min, max, None, None).unwrap()
}

#[test]
fn path_shape_rejects_inverted_range() {
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        3,
        Some(1),
        None,
        None,
    )
    .unwrap_err();
    assert!(err.contains("must not exceed"), "got: {err}");
}

#[test]
fn path_shape_rejects_zero_min() {
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        0,
        Some(2),
        None,
        None,
    )
    .unwrap_err();
    assert!(err.contains(">= 1"), "got: {err}");
}

#[test]
fn path_shape_rejects_empty_named_predicate() {
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::NamedPredicate(String::new()),
        1,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(err.contains("non-empty IRI"), "got: {err}");
}

#[test]
fn with_path_shapes_sorts_canonically() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_path_shapes(vec![
        shape(&format!("{LOGIC}z"), PathBase::Wildcard, 1, Some(2)),
        shape(&format!("{LOGIC}a"), PathBase::Wildcard, 1, Some(2)),
    ]);
    let iris: Vec<&str> = prog.path_shapes.iter().map(|s| s.iri.as_str()).collect();
    assert_eq!(iris, vec![format!("{LOGIC}a"), format!("{LOGIC}z")]);
}

#[test]
fn canonical_key_is_unchanged_for_path_shape_free_program() {
    // Corpus safety: attaching no path shapes must not alter the historical key.
    let ax = axiom(&format!("{LOGIC}x"), &kind_pred(), &format!("{LOGIC}y"));
    let base = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
    let attached = LogicProgram::new(vec![ax], vec![], vec![], None).with_path_shapes(vec![]);
    assert_eq!(base.canonical_key(), attached.canonical_key());
    assert!(!base.canonical_key().contains("PATHSHAPES"));
}

#[test]
fn canonical_key_appends_path_shapes_when_present() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_path_shapes(vec![shape(
        &format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        Some(2),
    )]);
    assert!(prog.canonical_key().contains("PATHSHAPES"));
}

// ── G2: max_depth hard cap (CWE-400) ────────────────────────────────────────

#[test]
fn path_shape_rejects_max_depth_above_cap() {
    // G2: a max_depth of MAX_PATH_DEPTH + 1 must be hard-rejected.
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        Some((MAX_PATH_DEPTH + 1) as u32),
        None,
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("exceeds the hard cap"),
        "error must mention the cap: {err}"
    );
    assert!(
        err.contains(&MAX_PATH_DEPTH.to_string()),
        "error must include the cap value: {err}"
    );
    assert!(
        err.contains(&(MAX_PATH_DEPTH + 1).to_string()),
        "error must include the offending value: {err}"
    );
}

#[test]
fn path_shape_accepts_max_depth_at_cap() {
    // G2: exactly MAX_PATH_DEPTH must be accepted (the cap is inclusive).
    PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        Some(MAX_PATH_DEPTH as u32),
        None,
        None,
    )
    .expect("max_depth == MAX_PATH_DEPTH must be accepted");
}

// ── G8: namespace_scope must be non-empty ───────────────────────────────────

#[test]
fn path_shape_rejects_empty_namespace_scope() {
    // G8: an empty namespace_scope string must be hard-rejected.
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        None,
        Some(String::new()),
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("namespace_scope"),
        "error must mention namespace_scope: {err}"
    );
    assert!(
        err.contains("non-empty"),
        "error must describe the constraint: {err}"
    );
}

#[test]
fn path_shape_rejects_whitespace_only_namespace_scope() {
    // G8: a whitespace-only namespace_scope must be hard-rejected (trim check).
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        None,
        Some("   ".to_owned()),
        None,
    )
    .unwrap_err();
    assert!(err.contains("namespace_scope"), "got: {err}");
}

#[test]
fn path_shape_accepts_valid_namespace_scope() {
    // G8: a non-empty namespace_scope must be accepted.
    PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        None,
        Some("https://example.org/ns/".to_owned()),
        None,
    )
    .expect("valid namespace_scope must be accepted");
}

// ── CR3: min_depth hard cap (CWE-400, unbounded path) ───────────────────────

#[test]
fn path_shape_rejects_min_depth_above_cap() {
    // CR3: an unbounded path (max_depth = None) with min_depth = MAX_PATH_DEPTH + 1
    // would unroll a billion-line edge chain in datalog_text. Reject it, mirroring
    // the max_depth cap.
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::NamedPredicate(format!("{LOGIC}p")),
        (MAX_PATH_DEPTH + 1) as u32,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("min_depth") && err.contains("hard cap"),
        "min_depth over the cap must be rejected with a hard-cap message: {err}"
    );
    assert!(
        err.contains(&(MAX_PATH_DEPTH + 1).to_string()),
        "error must include the offending value: {err}"
    );
}

#[test]
fn path_shape_accepts_min_depth_at_cap() {
    // CR3: exactly MAX_PATH_DEPTH must be accepted (the cap is inclusive); use an
    // unbounded path so the min>max check does not preempt the cap check.
    PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::NamedPredicate(format!("{LOGIC}p")),
        MAX_PATH_DEPTH as u32,
        None,
        None,
        None,
    )
    .expect("min_depth == MAX_PATH_DEPTH (unbounded) must be accepted");
}

// ── CR4a: empty/whitespace depth_param is rejected (content-key determinism) ──

#[test]
fn path_shape_rejects_empty_depth_param() {
    // CR4a: Some("") would collide with None in content_key() — a content-addressing
    // determinism hazard. new() must reject it so Some("") can never be constructed.
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        Some(2),
        None,
        Some(String::new()),
    )
    .unwrap_err();
    assert!(
        err.contains("depth_param") && err.contains("non-empty"),
        "an empty depth_param must be rejected: {err}"
    );
}

#[test]
fn path_shape_rejects_whitespace_depth_param() {
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        Some(2),
        None,
        Some("   ".to_owned()),
    )
    .unwrap_err();
    assert!(err.contains("depth_param"), "got: {err}");
}

// ── CR4b: namespace_scope on a named-predicate path is rejected ──────────────

#[test]
fn path_shape_rejects_namespace_scope_on_named_predicate() {
    // CR4b: namespace_scope only scopes a wildcard step (projections apply it solely
    // to wildcards). A named-predicate path carrying one is malformed.
    let err = PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::NamedPredicate(format!("{LOGIC}p")),
        1,
        Some(2),
        Some("https://example.org/ns/".to_owned()),
        None,
    )
    .unwrap_err();
    assert!(
        err.contains("namespace_scope") && err.contains("wildcard"),
        "namespace_scope on a named-predicate step must be rejected: {err}"
    );
}

#[test]
fn path_shape_accepts_namespace_scope_on_wildcard() {
    // CR4b: the wildcard case (the only legitimate carrier) still works.
    PathShapeIr::new(
        format!("{LOGIC}s"),
        PathBase::Wildcard,
        1,
        Some(2),
        Some("https://example.org/ns/".to_owned()),
        None,
    )
    .expect("namespace_scope on a wildcard step must be accepted");
}
