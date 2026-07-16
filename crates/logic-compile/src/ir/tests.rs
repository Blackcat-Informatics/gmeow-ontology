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
        .join("../../slices/grounding/logic/module.ttl");
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
    // individuals declared in module.ttl (reviewer B1): the historical
    // logic:SemanticProfile class is retired, so the source of truth is now the
    // set of logic:ReasoningPreset individuals. Reuse individuals_of_type, which
    // only flags a block from a genuine rdf:type position (a bare `logic:T`
    // type-list entry or an inline `a … logic:T` clause) and so ignores the class
    // declaration itself, object positions (`rdfs:range`/`logic:expandsToFacet`),
    // quoted skos:definition prose, AND `#` comments that merely mention the term
    // in prose (e.g. the logic:EngineContract capability-manifest comment).
    let text = module_ttl_text();
    let from_ttl = individuals_of_type(&text, "ReasoningPreset");

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
fn reasoning_contract_permits_procedural_execution_only_with_procedural_execution_facet() {
    // Native builtin permission is facet-derived: a contract is procedural iff its
    // resource policy carries logic:ProceduralExecution — never via the budget facet.
    let mut c = ReasoningContract::new();
    assert!(!c.permits_procedural_execution());
    c.resource_policies
        .insert("BudgetBoundedResource".to_owned());
    assert!(
        !c.permits_procedural_execution(),
        "a budget-bounded contract must not imply procedural execution"
    );
    c.resource_policies
        .insert(PROCEDURAL_EXECUTION_FACET.to_owned());
    assert!(
        c.permits_procedural_execution(),
        "the ProceduralExecution facet licenses native builtins even alongside a budget"
    );
}

#[test]
fn procedural_preset_carries_procedural_execution_facet() {
    // Tie native builtin permission to the ontology surface: exactly the presets
    // whose bundle includes logic:ProceduralExecution are procedural.
    let module_ttl = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding/logic/module.ttl");
    let text = std::fs::read_to_string(&module_ttl)
        .unwrap_or_else(|e| panic!("read {}: {e}", module_ttl.display()));

    // Collect, per top-level preset block, whether it names logic:ProceduralExecution.
    let mut carries: std::collections::BTreeMap<String, bool> = std::collections::BTreeMap::new();
    let mut current: Option<String> = None;
    let mut has_facet = false;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("logic:") {
            if let Some(subj) = current.take() {
                carries
                    .entry(subj)
                    .and_modify(|seen| *seen |= has_facet)
                    .or_insert(has_facet);
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
        carries
            .entry(subj)
            .and_modify(|seen| *seen |= has_facet)
            .or_insert(has_facet);
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
            id.permits_procedural_execution(),
            "preset {} procedural-execution permission must agree between module.ttl \
             ProceduralExecution \
             bundle ({in_ttl}) and SemanticProfileId::permits_procedural_execution ({})",
            id.as_str(),
            id.permits_procedural_execution()
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
        .join("../../slices/grounding/logic/module.ttl");
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
    for kind in PreservationKind::ALL {
        assert_eq!(PreservationKind::from_local(kind.as_str()), Some(kind));
    }
    assert_eq!(PreservationKind::from_local("NotAPreservationKind"), None);
}

#[test]
fn preservation_kind_is_a_bounded_lattice() {
    use gmeow_errors::BoundedLattice;

    // An EXHAUSTIVE proof over all seven variants (mirrors errors::grade's
    // assert_lattice_laws): a finite carrier means the whole domain is checked, not
    // sampled. This pins the algebra that governs how two ProjectionLoss witnesses
    // merging at one ledger anchor combine their preservation readings.
    let all = PreservationKind::ALL;
    for &a in &all {
        assert_eq!(a.join(a), a, "join idempotence");
        assert_eq!(a.meet(a), a, "meet idempotence");
        assert_eq!(a.join(PreservationKind::BOTTOM), a, "join bottom identity");
        assert_eq!(a.meet(PreservationKind::TOP), a, "meet top identity");
        assert_eq!(
            a.join(PreservationKind::TOP),
            PreservationKind::TOP,
            "join top absorbs"
        );
        assert_eq!(
            a.meet(PreservationKind::BOTTOM),
            PreservationKind::BOTTOM,
            "meet bottom absorbs"
        );
        for &b in &all {
            assert_eq!(a.join(b), b.join(a), "join commutative");
            assert_eq!(a.meet(b), b.meet(a), "meet commutative");
            assert_eq!(a.join(a.meet(b)), a, "absorption join/meet");
            assert_eq!(a.meet(a.join(b)), a, "absorption meet/join");
            assert_eq!(a.leq(b), a.join(b) == b, "leq via join");
            assert_eq!(a.leq(b), a.meet(b) == a, "leq via meet");
            for &c in &all {
                assert_eq!(a.join(b).join(c), a.join(b.join(c)), "join associative");
                assert_eq!(a.meet(b).meet(c), a.meet(b.meet(c)), "meet associative");
            }
        }
    }

    assert_eq!(PreservationKind::BOTTOM, PreservationKind::Exact);
    assert_eq!(PreservationKind::TOP, PreservationKind::Unsupported);
}

#[test]
fn preservation_join_is_worst_preservation_wins() {
    use gmeow_errors::BoundedLattice;

    // The load-bearing property for loss-witness merge: the join returns the
    // LESS-preserving of the two, so the surviving preservation at a shared anchor is
    // the WORST disclosed reading.
    assert_eq!(
        PreservationKind::Exact.join(PreservationKind::Unsupported),
        PreservationKind::Unsupported,
        "Exact join Unsupported = Unsupported (worst wins)"
    );
    assert_eq!(
        PreservationKind::SoundUnder.join(PreservationKind::CompleteOver),
        PreservationKind::CompleteOver,
        "the higher-rank (less-preserving) reading survives"
    );
    assert_eq!(
        PreservationKind::ValidationOnly.join(PreservationKind::Exact),
        PreservationKind::ValidationOnly,
        "ValidationOnly is worse than Exact"
    );
    assert_eq!(
        PreservationKind::Exact.meet(PreservationKind::Unsupported),
        PreservationKind::Exact,
        "Exact meet Unsupported = Exact (best survives the meet)"
    );

    // The chain is total and strictly monotone Exact -> Unsupported.
    for (i, &lo) in PreservationKind::ALL.iter().enumerate() {
        for &hi in &PreservationKind::ALL[i..] {
            assert!(lo.leq(hi), "chain leq");
            assert_eq!(lo.join(hi), hi, "chain join = hi");
        }
    }
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
fn formula_shape_values_match_module_ttl() {
    let from_rust: std::collections::BTreeSet<&str> =
        FormulaShape::ALL.iter().map(|s| s.as_str()).collect();

    let from_ttl = individuals_of_type(&module_ttl_text(), "FormulaShape");
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        from_rust, from_ttl_refs,
        "FormulaShape enum must match the logic:FormulaShape individuals in module.ttl"
    );

    // as_str ↔ from_local round-trips for every variant; ALL is in canonical order.
    for s in FormulaShape::ALL {
        assert_eq!(FormulaShape::from_local(s.as_str()), Some(s));
    }
    assert!(FormulaShape::from_local("NotAShape").is_none());
    let ordered: Vec<&str> = FormulaShape::ALL.iter().map(|s| s.as_str()).collect();
    let mut sorted = ordered.clone();
    sorted.sort_unstable();
    assert_eq!(
        ordered, sorted,
        "ALL must be declared in as_str-lexical order"
    );
}

#[test]
fn shape_tags_classify_the_residue_constructs() {
    let var = |n: &str| Term::var(n).unwrap();
    let rel = |l: &str, args: Vec<Term>| {
        Formula::atom(
            Term::iri(format!("https://blackcatinformatics.ca/logic/{l}")).unwrap(),
            args,
        )
        .unwrap()
    };

    // ∀x.(p(x) ∧ ¬q(x)) → quantified + strong-negation + nested. The unary atoms
    // p(x)/q(x) are fixed-arity and now evaluable via reification, so they are NOT
    // Variadic — only a genuine sequence marker carries that tag.
    let f1 = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::And(vec![
            rel("p", vec![var("x")]),
            Formula::Not(Box::new(rel("q", vec![var("x")]))),
        ])),
    };
    assert_eq!(
        f1.shape_tags(),
        vec![
            FormulaShape::Nested,
            FormulaShape::Quantified,
            FormulaShape::StrongNegation,
        ]
    );

    // ∃y.(r(y) ∨ s(y)) → disjunctive + quantified + nested. Unary atoms are fixed-arity
    // (reifiable), so no Variadic tag.
    let f2 = Formula::Exists {
        vars: vec!["y".into()],
        body: Box::new(Formula::Or(vec![
            rel("r", vec![var("y")]),
            rel("s", vec![var("y")]),
        ])),
    };
    assert_eq!(
        f2.shape_tags(),
        vec![
            FormulaShape::Disjunctive,
            FormulaShape::Nested,
            FormulaShape::Quantified,
        ]
    );

    // A flat implication of binary atoms is still a connective tree → nested only.
    let f3 = Formula::Implies(
        Box::new(rel("p", vec![var("x"), var("z")])),
        Box::new(rel("q", vec![var("x"), var("z")])),
    );
    assert_eq!(f3.shape_tags(), vec![FormulaShape::Nested]);

    // A genuine sequence-marker (unbounded) atom → variadic.
    let f4 = Formula::atom(
        Term::iri("https://blackcatinformatics.ca/logic/rel".to_owned()).unwrap(),
        vec![Term::sequence_marker("xs").unwrap()],
    )
    .unwrap();
    assert_eq!(f4.shape_tags(), vec![FormulaShape::Variadic]);

    // Totality: every non-trivially-Horn formula yields at least one tag.
    for f in [&f1, &f2, &f3, &f4] {
        assert!(!f.shape_tags().is_empty());
    }
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
    assert!(
        ComplexityClass::new("")
            .unwrap_err()
            .message()
            .contains("non-empty")
    );
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
        let scope =
            ContextualScope::new(None, None, Some(c), LogicModality::None, None, None).unwrap();
        assert_eq!(scope.confidence, Some(c));
    }
}

#[test]
fn contextual_scope_confidence_out_of_range() {
    for bad in [-0.1, 1.01, 2.0, -1.0] {
        let r = ContextualScope::new(None, None, Some(bad), LogicModality::None, None, None);
        assert!(r.is_err());
        assert!(r.unwrap_err().message().contains("confidence"));
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
    assert!(r.unwrap_err().message().contains("subject"));
    let r = LogicAxiom::ground("ex:s", "", "ex:o", false);
    assert!(r.unwrap_err().message().contains("predicate"));
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

// ── ReasoningContract ─────────────────────────────────────────────────

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
        Some("https://example.org/moduleA".to_owned()),
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
    let scope_pos =
        ContextualScope::new(None, None, Some(0.0), LogicModality::None, None, None).unwrap();
    let scope_neg =
        ContextualScope::new(None, None, Some(-0.0), LogicModality::None, None, None).unwrap();
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

// ── Path shapes ──────────────────────────────────────────────────────

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
    assert!(err.message().contains("must not exceed"), "got: {err}");
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
    assert!(err.message().contains(">= 1"), "got: {err}");
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
    assert!(err.message().contains("non-empty IRI"), "got: {err}");
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
        err.message().contains("exceeds the hard cap"),
        "error must mention the cap: {err}"
    );
    assert!(
        err.message().contains(&MAX_PATH_DEPTH.to_string()),
        "error must include the cap value: {err}"
    );
    assert!(
        err.message().contains(&(MAX_PATH_DEPTH + 1).to_string()),
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
        err.message().contains("namespace_scope"),
        "error must mention namespace_scope: {err}"
    );
    assert!(
        err.message().contains("non-empty"),
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
    assert!(err.message().contains("namespace_scope"), "got: {err}");
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
        err.message().contains("min_depth") && err.message().contains("hard cap"),
        "min_depth over the cap must be rejected with a hard-cap message: {err}"
    );
    assert!(
        err.message().contains(&(MAX_PATH_DEPTH + 1).to_string()),
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
        err.message().contains("depth_param") && err.message().contains("non-empty"),
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
    assert!(err.message().contains("depth_param"), "got: {err}");
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
        err.message().contains("namespace_scope") && err.message().contains("wildcard"),
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

// ── The correspondence calculus ──────────────────────────────────────

/// Build a minimal valid correspondence with the given IRI and law claims.
fn corr(iri: &str, law_claims: Vec<LawClaimIr>) -> Correspondence {
    Correspondence::new(
        iri,
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        None,
        None,
        law_claims,
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap()
}

/// Assert a Rust facet enum's `as_str` set exactly matches the `logic:<Class>`
/// individuals in `module.ttl`, and that every member round-trips through `from_local`.
fn assert_facet_matches_ttl(rust: &[&str], type_local: &str) {
    let from_rust: std::collections::BTreeSet<&str> = rust.iter().copied().collect();
    let from_ttl = individuals_of_type(&module_ttl_text(), type_local);
    let from_ttl_refs: std::collections::BTreeSet<&str> =
        from_ttl.iter().map(String::as_str).collect();
    assert_eq!(
        from_rust, from_ttl_refs,
        "{type_local} enum must match the logic:{type_local} individuals in module.ttl"
    );
}

#[test]
fn correspondence_relation_values_match_module_ttl() {
    let rust = [
        CorrespondenceRelation::Equiv,
        CorrespondenceRelation::Subsumes,
        CorrespondenceRelation::SubsumedBy,
        CorrespondenceRelation::Overlaps,
        CorrespondenceRelation::RelatedMatch,
        CorrespondenceRelation::Disjoint,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_facet_matches_ttl(&names, "CorrespondenceRelation");
    for r in &rust {
        assert_eq!(CorrespondenceRelation::from_local(r.as_str()), Some(*r));
    }
}

#[test]
fn morphism_class_values_match_module_ttl() {
    let rust = [
        MorphismClass::Isomorphism,
        MorphismClass::SectionRetraction,
        MorphismClass::WellBehavedLens,
        MorphismClass::LossyLens,
        MorphismClass::Prism,
        MorphismClass::AffineCorrespondence,
        MorphismClass::BridgeView,
    ];
    assert_eq!(rust.len(), 7, "the law-spine has seven rungs");
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_facet_matches_ttl(&names, "MorphismClass");
    for r in &rust {
        assert_eq!(MorphismClass::from_local(r.as_str()), Some(*r));
    }
    // The derived Ord is the spine order (strongest first): Isomorphism is the top,
    // BridgeView the floor.
    assert!(MorphismClass::Isomorphism < MorphismClass::BridgeView);
    assert!(MorphismClass::Prism < MorphismClass::AffineCorrespondence);
}

#[test]
fn morphism_kind_values_match_module_ttl() {
    let rust = [
        MorphismKind::InstitutionMorphism,
        MorphismKind::CommitmentShiftingBridge,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_facet_matches_ttl(&names, "MorphismKind");
    for r in &rust {
        assert_eq!(MorphismKind::from_local(r.as_str()), Some(*r));
    }
}

#[test]
fn determinacy_values_match_module_ttl() {
    let rust = [Determinacy::Crisp, Determinacy::Vague];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_facet_matches_ttl(&names, "Determinacy");
    for r in &rust {
        assert_eq!(Determinacy::from_local(r.as_str()), Some(*r));
    }
}

#[test]
fn correspondence_law_values_match_module_ttl() {
    let rust = [
        CorrespondenceLaw::GetPut,
        CorrespondenceLaw::PutGet,
        CorrespondenceLaw::PutPut,
        CorrespondenceLaw::SectionLaw,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_facet_matches_ttl(&names, "CorrespondenceLaw");
    for r in &rust {
        assert_eq!(CorrespondenceLaw::from_local(r.as_str()), Some(*r));
    }
}

#[test]
fn discharge_verdict_values_match_module_ttl() {
    // Reused from the foundation's non-entailment machinery; the IR enum mirrors it.
    let rust = [
        DischargeVerdict::ObligationDischarged,
        DischargeVerdict::ObligationUnknown,
        DischargeVerdict::ObligationViolated,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_facet_matches_ttl(&names, "DischargeVerdict");
    for r in &rust {
        assert_eq!(DischargeVerdict::from_local(r.as_str()), Some(*r));
    }
}

#[test]
fn discharge_condition_values_match_module_ttl() {
    let rust = [
        DischargeCondition::DischargeCertifiedFragment,
        DischargeCondition::DischargeFiniteClosure,
        DischargeCondition::DischargeSyntacticReachability,
        DischargeCondition::DischargeConservativeExtension,
        DischargeCondition::DischargeBoundedCorpus,
    ];
    let names: Vec<&str> = rust.iter().map(|r| r.as_str()).collect();
    assert_facet_matches_ttl(&names, "DischargeCondition");
    for r in &rust {
        assert_eq!(DischargeCondition::from_local(r.as_str()), Some(*r));
    }
}

#[test]
fn correspondences_sort_canonically() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![
            corr(&format!("{LOGIC}z"), vec![]),
            corr(&format!("{LOGIC}a"), vec![]),
        ])
        .expect("distinct correspondence IRIs, no recovery cases");
    let iris: Vec<&str> = prog
        .correspondences
        .iter()
        .map(|c| c.iri.as_str())
        .collect();
    assert_eq!(iris, vec![format!("{LOGIC}a"), format!("{LOGIC}z")]);
}

#[test]
fn canonical_key_is_unchanged_for_correspondence_free_program() {
    // Corpus safety: attaching no correspondences must not alter the historical key.
    let ax = axiom(&format!("{LOGIC}x"), &kind_pred(), &format!("{LOGIC}y"));
    let base = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
    let attached = LogicProgram::new(vec![ax], vec![], vec![], None)
        .with_correspondences(vec![])
        .expect("no correspondences");
    assert_eq!(base.canonical_key(), attached.canonical_key());
    assert!(!base.canonical_key().contains("CORRESPONDENCES"));
}

#[test]
fn canonical_key_appends_correspondences_when_present() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![corr(&format!("{LOGIC}c"), vec![])])
        .expect("single correspondence, no recovery cases");
    assert!(prog.canonical_key().contains("CORRESPONDENCES"));
}

#[test]
fn correspondence_content_key_is_law_claim_order_independent() {
    let claims_ab = vec![
        LawClaimIr {
            law: CorrespondenceLaw::GetPut,
            verdict: DischargeVerdict::ObligationDischarged,
            condition: Some(DischargeCondition::DischargeCertifiedFragment),
        },
        LawClaimIr {
            law: CorrespondenceLaw::PutGet,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        },
    ];
    let mut claims_ba = claims_ab.clone();
    claims_ba.reverse();
    let prog_ab = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![corr(&format!("{LOGIC}c"), claims_ab)])
        .expect("single correspondence, no recovery cases");
    let prog_ba = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![corr(&format!("{LOGIC}c"), claims_ba)])
        .expect("single correspondence, no recovery cases");
    assert_eq!(prog_ab.canonical_key(), prog_ba.canonical_key());
}

#[test]
fn correspondence_recovery_cases_sort_and_key_by_full_content() {
    let case_a = RecoveryCaseIr::new("https://example.org/recovery/a", pred("a", vec![tv("x")]))
        .expect("case a");
    let case_b = RecoveryCaseIr::new("https://example.org/recovery/b", pred("b", vec![tv("x")]))
        .expect("case b");

    let ab = corr(&format!("{LOGIC}c"), vec![])
        .with_recovery_cases(vec![case_a.clone(), case_b.clone()])
        .expect("unique cases");
    let ba = corr(&format!("{LOGIC}c"), vec![])
        .with_recovery_cases(vec![case_b, case_a.clone()])
        .expect("unique cases");
    assert_eq!(ab.recovery_cases, ba.recovery_cases);

    let changed = corr(&format!("{LOGIC}c"), vec![])
        .with_recovery_cases(vec![
            RecoveryCaseIr::new(case_a.iri, Formula::Not(Box::new(pred("a", vec![tv("x")]))))
                .expect("changed case"),
        ])
        .expect("unique changed case");
    assert_ne!(ab.content_key(), changed.content_key());
}

#[test]
fn correspondence_recovery_case_iris_are_unique() {
    let case = RecoveryCaseIr::new(
        "https://example.org/recovery/duplicate",
        pred("a", vec![tv("x")]),
    )
    .expect("case");
    let error = corr(&format!("{LOGIC}c"), vec![])
        .with_recovery_cases(vec![case.clone(), case])
        .expect_err("duplicate case identity must hard-fail");
    assert!(error.message().contains("duplicated"), "{error}");
}

#[test]
fn program_rejects_recovery_case_iri_reused_across_correspondences() {
    // Two DIFFERENT correspondences each declare exactly one recovery case, and both
    // cases share one IRI. Within a single correspondence this collision is already
    // rejected by `Correspondence::with_recovery_cases`; the gap this test closes is the
    // program-wide case: the shared IRI is a GLOBAL RDF subject, so `LogicProgram::
    // with_correspondences` — the one place every correspondence in the program is
    // visible together — must hard-fail rather than silently let the second
    // correspondence's case alias the first's.
    let shared_case_iri = "https://example.org/recovery/shared";
    let case_x = RecoveryCaseIr::new(shared_case_iri, pred("a", vec![tv("x")])).expect("case x");
    let case_y = RecoveryCaseIr::new(shared_case_iri, pred("b", vec![tv("y")])).expect("case y");

    let corr_x = corr(&format!("{LOGIC}corrX"), vec![])
        .with_recovery_cases(vec![case_x])
        .expect("single case, unique within corrX");
    let corr_y = corr(&format!("{LOGIC}corrY"), vec![])
        .with_recovery_cases(vec![case_y])
        .expect("single case, unique within corrY");

    let error = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![corr_x, corr_y])
        .expect_err("recovery-case IRI reused across two correspondences must hard-fail");
    assert!(
        error.message().contains(shared_case_iri),
        "diagnostic must name the duplicated IRI: {error}"
    );
    assert!(
        error.message().contains(&format!("{LOGIC}corrX"))
            && error.message().contains(&format!("{LOGIC}corrY")),
        "diagnostic must name both owning correspondences: {error}"
    );
}

#[test]
fn program_accepts_distinct_recovery_case_iris_across_correspondences() {
    // Distinct case IRIs owned by distinct correspondences must NOT be flagged as a
    // collision — the program-wide check must not over-fire on ordinary, well-formed
    // correspondences that each declare their own case.
    let case_x = RecoveryCaseIr::new("https://example.org/recovery/x", pred("a", vec![tv("x")]))
        .expect("case x");
    let case_y = RecoveryCaseIr::new("https://example.org/recovery/y", pred("b", vec![tv("y")]))
        .expect("case y");

    let corr_x = corr(&format!("{LOGIC}corrX"), vec![])
        .with_recovery_cases(vec![case_x])
        .expect("single case, unique within corrX");
    let corr_y = corr(&format!("{LOGIC}corrY"), vec![])
        .with_recovery_cases(vec![case_y])
        .expect("single case, unique within corrY");

    let program = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![corr_x, corr_y])
        .expect("distinct recovery-case IRIs across correspondences must be accepted");
    assert_eq!(program.correspondences.len(), 2);
}

#[test]
fn correspondence_dedups_duplicate_law_claims() {
    let claim = LawClaimIr {
        law: CorrespondenceLaw::SectionLaw,
        verdict: DischargeVerdict::ObligationDischarged,
        condition: Some(DischargeCondition::DischargeFiniteClosure),
    };
    let c = corr(&format!("{LOGIC}c"), vec![claim, claim, claim]);
    assert_eq!(c.law_claims.len(), 1, "identical law claims are deduped");
}

#[test]
fn correspondence_axes_signed_zero_normalized() {
    // -0.0 and 0.0 confidence must produce the same content key (determinism).
    let mk = |conf: f64| {
        Correspondence::new(
            format!("{LOGIC}c"),
            CorrespondenceRelation::Equiv,
            MorphismClass::Isomorphism,
            MorphismKind::InstitutionMorphism,
            false,
            None,
            None,
            None,
            vec![],
            Some(conf),
            None,
            None,
            None,
            None,
            None,
        )
        .unwrap()
    };
    let pos = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![mk(0.0)])
        .expect("single correspondence, no recovery cases");
    let neg = LogicProgram::new(vec![], vec![], vec![], None)
        .with_correspondences(vec![mk(-0.0)])
        .expect("single correspondence, no recovery cases");
    assert_eq!(pos.canonical_key(), neg.canonical_key());
}

#[test]
fn correspondence_new_rejects_empty_iri() {
    let err = corr_err("", None);
    assert!(err.contains("non-empty IRI"), "got: {err}");
}

#[test]
fn correspondence_new_rejects_empty_optional_leg() {
    // Some("") must never be constructible (it collides with None in the content key).
    let err = corr_err(&format!("{LOGIC}c"), Some(String::new()));
    assert!(err.contains("non-empty IRI"), "got: {err}");
}

#[test]
fn correspondence_new_rejects_out_of_range_confidence() {
    let err = Correspondence::new(
        format!("{LOGIC}c"),
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        None,
        None,
        vec![],
        Some(1.5),
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err();
    assert!(err.message().contains("[0, 1]"), "got: {err}");
}

/// Build a correspondence with an optional `get_leg`, returning the construction error.
fn corr_err(iri: &str, get_leg: Option<String>) -> String {
    Correspondence::new(
        iri,
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        false,
        None,
        get_leg,
        None,
        vec![],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap_err()
    .message()
    .to_owned()
}

// ── Full first-order Formula AST ──────────────────────────────────────

/// A variable term.
fn tv(name: &str) -> Term {
    Term::var(name).unwrap()
}

/// A `logic:`-local IRI term.
fn ti(local: &str) -> Term {
    Term::iri(format!("{LOGIC}{local}")).unwrap()
}

/// A predication `local(args…)` with a reified (IRI) relation.
fn pred(local: &str, args: Vec<Term>) -> Formula {
    Formula::atom(ti(local), args).unwrap()
}

#[test]
fn principal_predicate_peels_negation_and_quantifiers() {
    // A bare atom → its relation IRI.
    assert_eq!(
        pred("p", vec![tv("x")]).principal_predicate().as_deref(),
        Some(format!("{LOGIC}p").as_str())
    );
    // ∀x. p(x) → p (the universal-claim anti-conjecture forbids the predicate).
    let forall = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(pred("p", vec![tv("x")])),
    };
    assert_eq!(forall.principal_predicate(), Some(format!("{LOGIC}p")));
    // ¬∀x. p(x) → p (strong negation is peeled too).
    let neg = Formula::Not(Box::new(forall));
    assert_eq!(neg.principal_predicate(), Some(format!("{LOGIC}p")));
    // A ∀-Horn rule ∀x. body(x) → head(x) → the HEAD predicate (the conclusion the
    // anti-conjecture obligation forbids the closure from drawing).
    let rule = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(Formula::Implies(
            Box::new(pred("body", vec![tv("x")])),
            Box::new(pred("head", vec![tv("x")])),
        )),
    };
    assert_eq!(rule.principal_predicate(), Some(format!("{LOGIC}head")));
    // A compound formula names no single principal predicate.
    let conj = Formula::And(vec![pred("p", vec![tv("x")]), pred("q", vec![tv("x")])]);
    assert_eq!(conj.principal_predicate(), None);
    let disj = Formula::Or(vec![pred("p", vec![tv("x")]), pred("q", vec![tv("x")])]);
    assert_eq!(disj.principal_predicate(), None);
}

#[test]
fn alpha_equivalence_renames_bound_variable() {
    // ∀x.p(x) ≡ ∀y.p(y)
    let a = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(pred("p", vec![tv("x")])),
    };
    let b = Formula::Forall {
        vars: vec!["y".into()],
        body: Box::new(pred("p", vec![tv("y")])),
    };
    assert_eq!(a.content_key(), b.content_key());
}

#[test]
fn alpha_equivalence_holds_for_nested_binders() {
    // ∀x∃y.r(x,y) ≡ ∀a∃b.r(a,b)
    let mk = |outer: &str, inner: &str| Formula::Forall {
        vars: vec![outer.into()],
        body: Box::new(Formula::Exists {
            vars: vec![inner.into()],
            body: Box::new(pred("r", vec![tv(outer), tv(inner)])),
        }),
    };
    assert_eq!(mk("x", "y").content_key(), mk("a", "b").content_key());
}

#[test]
fn free_variable_rename_is_not_alpha_equivalent() {
    // p(x) with free x is NOT p(y) with free y — free vars are meaning.
    assert_ne!(
        pred("p", vec![tv("x")]).content_key(),
        pred("p", vec![tv("y")]).content_key()
    );
}

#[test]
fn conjunction_is_commutative_and_associative() {
    let a = pred("p", vec![tv("x")]);
    let b = pred("q", vec![tv("x")]);
    let c = pred("r", vec![tv("x")]);
    // commutative
    assert_eq!(
        Formula::And(vec![a.clone(), b.clone()]).content_key(),
        Formula::And(vec![b.clone(), a.clone()]).content_key()
    );
    // associative / flattened: And[And[a,b],c] ≡ And[a,b,c]
    let nested = Formula::And(vec![Formula::And(vec![a.clone(), b.clone()]), c.clone()]);
    let flat = Formula::And(vec![a, b, c]);
    assert_eq!(nested.content_key(), flat.content_key());
}

#[test]
fn biconditional_is_commutative_but_implication_is_ordered() {
    let a = pred("p", vec![tv("x")]);
    let b = pred("q", vec![tv("x")]);
    assert_eq!(
        Formula::Iff(Box::new(a.clone()), Box::new(b.clone())).content_key(),
        Formula::Iff(Box::new(b.clone()), Box::new(a.clone())).content_key()
    );
    assert_ne!(
        Formula::Implies(Box::new(a.clone()), Box::new(b.clone())).content_key(),
        Formula::Implies(Box::new(b), Box::new(a)).content_key()
    );
}

#[test]
fn binder_block_order_is_significant() {
    // ∀{x,y}.r(x,y) is NOT ∀{y,x}.r(x,y) (renaming, not prefix permutation).
    let xy = Formula::Forall {
        vars: vec!["x".into(), "y".into()],
        body: Box::new(pred("r", vec![tv("x"), tv("y")])),
    };
    let yx = Formula::Forall {
        vars: vec!["y".into(), "x".into()],
        body: Box::new(pred("r", vec![tv("x"), tv("y")])),
    };
    assert_ne!(xy.content_key(), yx.content_key());
}

#[test]
fn sequence_marker_keys_distinctly_from_variable() {
    // p(...x) is not p(x): a sequence marker binds a sequence, a var binds one term.
    let marker = Formula::atom(ti("p"), vec![Term::sequence_marker("x").unwrap()]).unwrap();
    let single = pred("p", vec![tv("x")]);
    assert_ne!(marker.content_key(), single.content_key());
}

#[test]
fn term_constructors_reject_empty_and_blank_strings() {
    assert!(Term::var("").is_err());
    assert!(Term::var("   ").is_err());
    assert!(Term::iri("  ").is_err());
    assert!(Term::sequence_marker("").is_err());
    // Some("") datatype collides with None — rejected; a None datatype is fine.
    assert!(Term::literal("v", Some(String::new())).is_err());
    assert!(Term::literal("", None).is_ok()); // an empty lexical is a legal RDF literal
}

#[test]
fn atom_relation_must_be_an_iri() {
    // A predicate variable would break first-orderness.
    assert!(Formula::atom(tv("P"), vec![tv("x")]).is_err());
    assert!(Formula::atom(Term::literal("L", None).unwrap(), vec![tv("x")]).is_err());
    assert!(Formula::atom(ti("p"), vec![tv("x")]).is_ok());
}

#[test]
fn canonical_key_is_unchanged_for_formula_free_program() {
    // Corpus safety: attaching no formulas must not alter the historical key.
    let ax = axiom(&format!("{LOGIC}x"), &kind_pred(), &format!("{LOGIC}y"));
    let base = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
    let attached = LogicProgram::new(vec![ax], vec![], vec![], None).with_formulas(vec![]);
    assert_eq!(base.canonical_key(), attached.canonical_key());
    assert!(!base.canonical_key().contains("FORMULAS"));
}

#[test]
fn canonical_key_appends_formulas_when_present() {
    let f = Formula::Forall {
        vars: vec!["x".into()],
        body: Box::new(pred("p", vec![tv("x")])),
    };
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
    assert!(prog.canonical_key().contains("FORMULAS"));
}

#[test]
fn with_formulas_sorts_canonically() {
    // A unary atom is non-trivial (arity ≠ 2), so it may live in `formulas`.
    let pa = pred("a", vec![tv("x")]);
    let pz = pred("z", vec![tv("x")]);
    let prog =
        LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![pz.clone(), pa.clone()]);
    let keys: Vec<String> = prog.formulas.iter().map(Formula::content_key).collect();
    let mut expected = vec![pa.content_key(), pz.content_key()];
    expected.sort();
    assert_eq!(keys, expected);
}

#[test]
#[should_panic(expected = "trivially-Horn")]
fn with_formulas_rejects_binary_atom() {
    // A binary atom is a triple — it belongs in `axioms`, not `formulas`.
    let f = pred("r", vec![tv("x"), tv("y")]);
    let _ = LogicProgram::new(vec![], vec![], vec![], None).with_formulas(vec![f]);
}

// ── LegPath leg-body algebra ─────────────────────────────────────────────────────

use crate::projections::paths::leg_path_canonical;

fn step(p: &str) -> LegPath {
    LegPath::Step(format!("{LOGIC}{p}"))
}

#[test]
fn invert_of_a_step_is_its_inverse() {
    let p = step("foo");
    assert_eq!(p.invert(), LegPath::Inverse(Box::new(p)));
}

#[test]
fn invert_is_an_involution() {
    // reverse(reverse(x)) == x for every shape: step, inverse, seq, alt.
    let cases = [
        step("foo"),
        LegPath::Inverse(Box::new(step("foo"))),
        LegPath::Seq(vec![step("a"), step("b"), step("c")]),
        LegPath::Alt(vec![step("a"), LegPath::Inverse(Box::new(step("b")))]),
        LegPath::Seq(vec![
            LegPath::Inverse(Box::new(step("a"))),
            LegPath::Alt(vec![step("b"), step("c")]),
        ]),
    ];
    for c in cases {
        assert_eq!(
            c.invert().invert().normalize(),
            c.normalize(),
            "reverse∘reverse must be identity on {c:?}"
        );
    }
}

#[test]
fn invert_of_a_sequence_reverses_order_and_each_step() {
    // reverse(a / b / c) = ^c / ^b / ^a.
    let seq = LegPath::Seq(vec![step("a"), step("b"), step("c")]);
    let expected = LegPath::Seq(vec![
        LegPath::Inverse(Box::new(step("c"))),
        LegPath::Inverse(Box::new(step("b"))),
        LegPath::Inverse(Box::new(step("a"))),
    ]);
    assert_eq!(seq.invert(), expected);
}

#[test]
fn normalize_cancels_double_inverse_and_flattens() {
    // ^^foo → foo
    let dbl = LegPath::Inverse(Box::new(LegPath::Inverse(Box::new(step("foo")))));
    assert_eq!(dbl.normalize(), step("foo"));
    // nested Seq flattens; singleton Seq collapses.
    let nested = LegPath::Seq(vec![
        LegPath::Seq(vec![step("a"), step("b")]),
        LegPath::Seq(vec![step("c")]),
    ]);
    assert_eq!(
        nested.normalize(),
        LegPath::Seq(vec![step("a"), step("b"), step("c")])
    );
}

#[test]
fn canonical_key_round_trips_inverse_to_get() {
    // Inversion constructs a deterministic candidate body.  This is path-algebra identity,
    // not recovery discharge: the native executor separately decides whether the candidate
    // reproduces a complete declared source graph.
    let get = LegPath::Seq(vec![step("foo"), LegPath::Inverse(Box::new(step("bar")))]);
    let lawful_put = get.invert();
    assert_eq!(
        leg_path_canonical(&lawful_put),
        leg_path_canonical(&get.invert()),
        "the lawful put is reproducibly get.invert()"
    );
    let wrong_put = LegPath::Seq(vec![step("foo"), LegPath::Inverse(Box::new(step("baz")))]);
    assert_ne!(
        leg_path_canonical(&wrong_put),
        leg_path_canonical(&get.invert()),
        "a put with a different predicate body must NOT match the derived inverse"
    );
}

#[test]
fn canonical_key_is_normal_form_invariant() {
    // Two structurally different but normalization-equal bodies share a canonical key.
    let a = LegPath::Inverse(Box::new(LegPath::Inverse(Box::new(step("foo")))));
    let b = LegPath::Seq(vec![step("foo")]);
    assert_eq!(leg_path_canonical(&a), leg_path_canonical(&b));
}

// ── Validation shapes (the closed-world SHACL/ShEx-shaped subset) ─────────────

fn vshape(iri: &str, target_class: &str, props: Vec<PropertyConstraintIr>) -> ValidationShapeIr {
    ValidationShapeIr::new(
        iri,
        ShapeTarget::Class(target_class.to_owned()),
        props,
        None,
    )
    .unwrap()
}

#[test]
fn validation_shape_constructor_hard_pins_node_kind() {
    // The whole point of the type: NodeKind::ValidationShape goes from dead to live.
    let s = vshape(&format!("{LOGIC}s"), "ex:C", vec![]);
    assert_eq!(s.node_kind, NodeKind::ValidationShape);
}

#[test]
fn validation_shape_rejects_empty_iri_and_target() {
    let e1 =
        ValidationShapeIr::new("", ShapeTarget::Class("ex:C".into()), vec![], None).unwrap_err();
    assert!(e1.message().contains("non-empty IRI"), "got: {e1}");
    let e2 = ValidationShapeIr::new(
        format!("{LOGIC}s"),
        ShapeTarget::Class("  ".into()),
        vec![],
        None,
    )
    .unwrap_err();
    assert!(e2.message().contains("non-empty IRI"), "got: {e2}");
}

#[test]
fn property_constraint_binds_cardinality_to_provenance() {
    // A cardinality without a provenance (or vice versa) is rejected — the provenance is
    // what decides the loss-ledger polarity (OWL open-world vs OPT closed-world).
    let missing_prov = PropertyConstraintIr::new("ex:p", Some(1), None, None, vec![]).unwrap_err();
    assert!(
        missing_prov.message().contains("cardinality_provenance"),
        "got: {missing_prov}"
    );
    let dangling_prov = PropertyConstraintIr::new(
        "ex:p",
        None,
        None,
        Some(ConstraintProvenance::OptNative),
        vec![],
    )
    .unwrap_err();
    assert!(
        dangling_prov.message().contains("cardinality_provenance"),
        "got: {dangling_prov}"
    );
    // With both present it constructs.
    assert!(
        PropertyConstraintIr::new(
            "ex:p",
            Some(1),
            Some(1),
            Some(ConstraintProvenance::OptNative),
            vec![],
        )
        .is_ok()
    );
}

#[test]
fn property_constraint_rejects_inverted_cardinality() {
    let err = PropertyConstraintIr::new(
        "ex:p",
        Some(3),
        Some(1),
        Some(ConstraintProvenance::OptNative),
        vec![],
    )
    .unwrap_err();
    assert!(err.message().contains("must not exceed"), "got: {err}");
}

#[test]
fn validation_shape_content_key_is_component_order_independent() {
    // Supplying the same components in different orders yields the identical shape key.
    let p_ab = PropertyConstraintIr::new(
        "ex:p",
        None,
        None,
        None,
        vec![
            ConstraintComponent::Datatype("xsd:decimal".into()),
            ConstraintComponent::NumericRange {
                min: Some(0.0),
                max: Some(1000.0),
                min_inclusive: true,
                max_inclusive: false,
            },
        ],
    )
    .unwrap();
    let p_ba = PropertyConstraintIr::new(
        "ex:p",
        None,
        None,
        None,
        vec![
            ConstraintComponent::NumericRange {
                min: Some(0.0),
                max: Some(1000.0),
                min_inclusive: true,
                max_inclusive: false,
            },
            ConstraintComponent::Datatype("xsd:decimal".into()),
        ],
    )
    .unwrap();
    let s_ab = vshape(&format!("{LOGIC}s"), "ex:C", vec![p_ab]);
    let s_ba = vshape(&format!("{LOGIC}s"), "ex:C", vec![p_ba]);
    assert_eq!(s_ab.content_key(), s_ba.content_key());
}

#[test]
fn validation_shape_numeric_range_signed_zero_is_stable() {
    // -0.0 and 0.0 must fold to the same key (opt_axis_key contract).
    let pos = vshape(
        &format!("{LOGIC}s"),
        "ex:C",
        vec![
            PropertyConstraintIr::new(
                "ex:p",
                None,
                None,
                None,
                vec![ConstraintComponent::NumericRange {
                    min: Some(0.0),
                    max: None,
                    min_inclusive: true,
                    max_inclusive: false,
                }],
            )
            .unwrap(),
        ],
    );
    let neg = vshape(
        &format!("{LOGIC}s"),
        "ex:C",
        vec![
            PropertyConstraintIr::new(
                "ex:p",
                None,
                None,
                None,
                vec![ConstraintComponent::NumericRange {
                    min: Some(-0.0),
                    max: None,
                    min_inclusive: true,
                    max_inclusive: false,
                }],
            )
            .unwrap(),
        ],
    );
    assert_eq!(pos.content_key(), neg.content_key());
}

#[test]
fn has_lossy_component_flags_pattern_and_terminology() {
    let clean = vshape(
        &format!("{LOGIC}s"),
        "ex:C",
        vec![
            PropertyConstraintIr::new(
                "ex:p",
                Some(1),
                Some(1),
                Some(ConstraintProvenance::OptNative),
                vec![ConstraintComponent::Datatype("xsd:string".into())],
            )
            .unwrap(),
        ],
    );
    assert!(!clean.has_lossy_component());
    let lossy = vshape(
        &format!("{LOGIC}s"),
        "ex:C",
        vec![
            PropertyConstraintIr::new(
                "ex:p",
                None,
                None,
                None,
                vec![ConstraintComponent::Pattern {
                    regex: "^[A-Z]+$".into(),
                    flags: None,
                }],
            )
            .unwrap(),
        ],
    );
    assert!(lossy.has_lossy_component());
}

#[test]
fn with_validation_shapes_sorts_canonically() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None).with_validation_shapes(vec![
        vshape(&format!("{LOGIC}z"), "ex:C", vec![]),
        vshape(&format!("{LOGIC}a"), "ex:C", vec![]),
    ]);
    let iris: Vec<&str> = prog
        .validation_shapes
        .iter()
        .map(|s| s.iri.as_str())
        .collect();
    assert_eq!(iris, vec![format!("{LOGIC}a"), format!("{LOGIC}z")]);
}

#[test]
fn canonical_key_is_unchanged_for_validation_shape_free_program() {
    // Corpus safety: attaching no validation shapes must not alter the historical key.
    let ax = axiom(&format!("{LOGIC}x"), &kind_pred(), &format!("{LOGIC}y"));
    let base = LogicProgram::new(vec![ax.clone()], vec![], vec![], None);
    let attached = LogicProgram::new(vec![ax], vec![], vec![], None).with_validation_shapes(vec![]);
    assert_eq!(base.canonical_key(), attached.canonical_key());
    assert!(!base.canonical_key().contains("VALIDATIONSHAPES"));
}

#[test]
fn canonical_key_appends_validation_shapes_when_present() {
    let prog = LogicProgram::new(vec![], vec![], vec![], None)
        .with_validation_shapes(vec![vshape(&format!("{LOGIC}s"), "ex:C", vec![])]);
    assert!(prog.canonical_key().contains("VALIDATIONSHAPES"));
}
