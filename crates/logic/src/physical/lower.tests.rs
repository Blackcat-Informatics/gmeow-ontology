// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

use gmeow_term_arena::engine::TermDag;

fn forall_p_x() -> Formula {
    // ∀x. p(x)
    Formula::Forall {
        vars: vec!["x".to_owned()],
        body: Box::new(
            Formula::atom(
                Term::iri("https://example.org/p").expect("iri"),
                vec![Term::var("x").expect("var")],
            )
            .expect("atom"),
        ),
    }
}

/// A `lang:` form (an English sentence) whose meaning IS a `logic:` formula.
fn sentence_form() -> gmeow_lang_form::Form {
    gmeow_lang_form::Form::Composed {
        sign_system: "https://example.org/english".to_owned(),
        level: "sentence".to_owned(),
        analysis: None,
        head: None,
        slots: Vec::new(),
    }
}

// ── THE acceptance: one arena, three surfaces, alpha-equivalent ⇒ one node ──────────

#[test]
fn cross_consumer_alpha_equivalent_interns_to_one_node_and_key() {
    const P: &str = "https://example.org/p";
    let mut dag = TermDag::new();

    // (1) logic: ∀x. p(x) as an ir::Formula, lowered directly.
    let logic_formula = forall_p_x();
    let logic_node = lower_logic_formula(&mut dag, &logic_formula).expect("logic lowering");

    // (2) math: the SAME shape as a BindingExpression (∀-operator) binding one
    //     occurrence of p applied to the bound variable, authored as RDF.
    let math_ttl = format!(
        "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix op: <https://blackcatinformatics.ca/logic/dag/op/> .\n\
             ex:binder a math:BindingExpression ;\n\
             \x20 math:operator op:forall ;\n\
             \x20 math:boundVariable ex:xDecl ;\n\
             \x20 math:argumentSlot ex:bodySlot .\n\
             ex:xDecl a math:VariableDeclaration .\n\
             ex:bodySlot a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:app .\n\
             ex:app a math:ApplicationExpression ;\n\
             \x20 math:operator <{P}> ;\n\
             \x20 math:argumentSlot ex:appSlot0 .\n\
             ex:appSlot0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:xOcc .\n\
             ex:xOcc a math:VariableExpression ; math:variableOccurrence ex:xOccurrence .\n\
             ex:xOccurrence a math:VariableOccurrence ; math:declaredVariable ex:xDecl .\n"
    );
    let math_graph = MathGraph::from_turtle(math_ttl.as_bytes()).expect("math parse");
    let math_node = lower_math_expression(&mut dag, &math_graph, "https://example.org/binder")
        .expect("math lowering");

    // (3) lang: a form whose denotation IS the same logic formula.
    let denoted = LangDenotedForm {
        form: sentence_form(),
        denotation: LangDenotation::LogicFormula(forall_p_x()),
    };
    let lang_node = lower_lang_form(&mut dag, &denoted).expect("lang lowering");

    // All three alpha-equivalent inputs collapse to ONE NodeId and ONE content key.
    assert_eq!(
        logic_node, math_node,
        "logic: and math: alpha-equivalent inputs must intern to one NodeId"
    );
    assert_eq!(
        logic_node, lang_node,
        "lang: denotation must intern to the same NodeId as its logic: target"
    );
    assert_eq!(
        dag.key(logic_node),
        dag.key(math_node),
        "logic: and math: must share a byte-identical content key"
    );
    assert_eq!(
        dag.key(logic_node),
        dag.key(lang_node),
        "lang: must share the byte-identical content key"
    );

    // Guard against a vacuous acceptance: the shared node really is a binder over an
    // application over a bound-variable occurrence (BIND / APP / B kind tags present).
    let key = dag.key(logic_node);
    assert!(key.starts_with("BIND"), "shared node is a binder: {key}");
    assert!(key.contains("APP"), "binder body is an application: {key}");
    assert!(
        key.contains('B'),
        "application argument is a bound occurrence: {key}"
    );
}

// ── math: slotIndex ordering is respected; a slot gap hard-fails ────────────────────

fn application_ttl(slots: &[(i64, &str)]) -> String {
    let mut ttl = String::from(
        "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ",
    );
    for (i, _) in slots.iter().enumerate() {
        ttl.push_str(&format!("; math:argumentSlot ex:s{i} "));
    }
    ttl.push_str(".\n");
    for (i, (index, expr)) in slots.iter().enumerate() {
        ttl.push_str(&format!(
            "ex:s{i} a math:ArgumentSlot ; math:slotIndex {index} ; math:slotExpression {expr} .\n"
        ));
    }
    ttl
}

#[test]
fn math_slot_index_orders_operands_regardless_of_authoring_order() {
    let mut dag = TermDag::new();
    // p(a, b) authored slots-forward and slots-reversed must intern identically.
    let forward = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (1, "ex:b")]).as_bytes())
        .expect("parse");
    let reversed = MathGraph::from_turtle(application_ttl(&[(1, "ex:b"), (0, "ex:a")]).as_bytes())
        .expect("parse");
    let n_forward =
        lower_math_expression(&mut dag, &forward, "https://example.org/app").expect("forward");
    let n_reversed =
        lower_math_expression(&mut dag, &reversed, "https://example.org/app").expect("reversed");
    assert_eq!(
        n_forward, n_reversed,
        "operand order is carried by slotIndex, not authoring order"
    );

    // It matches the logic: atom p(a, b) — cross-consumer, same arena.
    let logic_pab = lower_logic_formula(
        &mut dag,
        &Formula::atom(
            Term::iri("https://example.org/p").unwrap(),
            vec![
                Term::iri("https://example.org/a").unwrap(),
                Term::iri("https://example.org/b").unwrap(),
            ],
        )
        .unwrap(),
    )
    .expect("logic p(a,b)");
    assert_eq!(n_forward, logic_pab, "math p(a,b) == logic p(a,b)");

    // Swapping the operands (p(b, a)) is a DISTINCT node — order is identity-bearing.
    let swapped = MathGraph::from_turtle(application_ttl(&[(0, "ex:b"), (1, "ex:a")]).as_bytes())
        .expect("parse");
    let n_swapped =
        lower_math_expression(&mut dag, &swapped, "https://example.org/app").expect("swapped");
    assert_ne!(n_forward, n_swapped, "p(a,b) and p(b,a) are distinct");
}

#[test]
fn math_slot_gap_hard_fails() {
    let mut dag = TermDag::new();
    // Indexes {0, 2} are not contiguous — a hard fail, never a silent renumber.
    let graph = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (2, "ex:b")]).as_bytes())
        .expect("parse");
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
        .expect_err("slot gap must hard-fail");
    assert_eq!(
        err,
        MathLoweringError::NonContiguousArgumentSlots {
            node: "https://example.org/app".to_owned(),
            index: 2,
            expected_position: 1,
        },
        "gap diagnostic names the non-contiguous slot family: {err:?}"
    );
    assert_eq!(
        err.failure_class(),
        "https://blackcatinformatics.ca/math/NonContiguousArgumentSlots"
    );
}

#[test]
fn math_duplicate_slot_index_hard_fails_distinctly_from_a_gap() {
    // Indexes {0, 0} are a DUPLICATE, not a gap — a distinct rejection variant/class.
    let mut dag = TermDag::new();
    let graph = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (0, "ex:b")]).as_bytes())
        .expect("parse");
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
        .expect_err("duplicate slot index must hard-fail");
    assert_eq!(
        err,
        MathLoweringError::DuplicateArgumentSlotIndex {
            node: "https://example.org/app".to_owned(),
            index: 0,
        }
    );
    assert_eq!(
        err.failure_class(),
        "https://blackcatinformatics.ca/math/DuplicateArgumentSlotIndex"
    );
}

#[test]
fn math_negative_slot_index_hard_fails_as_malformed() {
    let mut dag = TermDag::new();
    let graph = MathGraph::from_turtle(application_ttl(&[(-1, "ex:a")]).as_bytes()).expect("parse");
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
        .expect_err("negative slot index must hard-fail");
    assert!(
        matches!(
            err,
            MathLoweringError::NegativeArgumentSlotIndex { index: -1, .. }
        ),
        "{err:?}"
    );
    assert_eq!(
        err.failure_class(),
        "https://blackcatinformatics.ca/math/MalformedArgumentSlot"
    );
}

// ── math: a declared bound-variable domain becomes a distinct sort child ────────────

/// A binder over SEVERAL indexed operands folds to its operator applied to them, in slot
/// order — and a binder over ONE keeps the bare body.
///
/// The slice authors both shapes (`math:BindingExpression` "names its body through indexed
/// math:argumentSlot cells"; a `math:ModelFormula` is "a binder over indexed
/// math:ArgumentSlot operands"), and the many-operand fold is a canonical form nothing else
/// in this crate exercises: every other binder here has one slot, and the generator's
/// `GenExpr::Bind` carries a single body, so the proptest cannot reach it either. Without
/// this, a regrouping or an operand-order regression would be invisible to every direct
/// test and caught only end-to-end by the R-lift fixture.
#[test]
fn a_multi_operand_binder_folds_to_its_operator_applied_in_slot_order() {
    fn multi_binder_ttl(first: &str, second: &str) -> String {
        format!(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 @prefix op: <https://blackcatinformatics.ca/logic/dag/op/> .\n\
                 ex:binder a math:BindingExpression ;\n\
                 \x20 math:operator op:tilde ;\n\
                 \x20 math:boundVariable ex:xDecl ;\n\
                 \x20 math:argumentSlot ex:s0 , ex:s1 .\n\
                 ex:xDecl a math:VariableDeclaration .\n\
                 ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression {first} .\n\
                 ex:s1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression {second} .\n\
                 ex:a a math:SymbolReference ; math:hasMathematicalSymbol ex:symA .\n\
                 ex:b a math:SymbolReference ; math:hasMathematicalSymbol ex:symB .\n"
        )
    }

    let graph = MathGraph::from_turtle(multi_binder_ttl("ex:a", "ex:b").as_bytes())
        .expect("two-operand binder parses");
    let mut dag = TermDag::new();
    let lowered = lower_math_expression(&mut dag, &graph, "https://example.org/binder")
        .expect("a binder over two indexed operands lowers");

    // Hand-build the SAME term: bind(op, [sort], app(op, [a, b])).
    let op = dag.intern_leaf(TermValue::iri(
        "https://blackcatinformatics.ca/logic/dag/op/tilde",
    ));
    let sort = dag.intern_leaf(TermValue::iri(canon::SORT_INDIVIDUAL));
    let a = dag.intern_leaf(TermValue::iri("https://example.org/symA"));
    let b = dag.intern_leaf(TermValue::iri("https://example.org/symB"));
    let body = dag.intern_app(op, vec![a, b]);
    let expected = dag.intern_binder(op, vec![sort], body);
    assert_eq!(
        lowered, expected,
        "a binder over indexed operands must fold to its own operator applied to them"
    );

    // Operand ORDER is identity-bearing: swapping which symbol sits at index 0 is a
    // DIFFERENT term, so the fold cannot be collapsing the slots into a set.
    let swapped = MathGraph::from_turtle(multi_binder_ttl("ex:b", "ex:a").as_bytes())
        .expect("the swapped binder parses");
    let mut swapped_dag = TermDag::new();
    let swapped_node =
        lower_math_expression(&mut swapped_dag, &swapped, "https://example.org/binder")
            .expect("the swapped binder lowers");
    assert_ne!(
        structural_digest(&dag, lowered),
        structural_digest(&swapped_dag, swapped_node),
        "swapping the operands at index 0 and 1 must change the structural digest"
    );
}

fn binder_ttl(domain: Option<&str>) -> String {
    let domain_line = match domain {
        Some(d) => format!("ex:xDecl a math:VariableDeclaration ; math:domain {d} .\n"),
        None => "ex:xDecl a math:VariableDeclaration .\n".to_owned(),
    };
    format!(
        "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix op: <https://blackcatinformatics.ca/logic/dag/op/> .\n\
             ex:binder a math:BindingExpression ;\n\
             \x20 math:operator op:forall ;\n\
             \x20 math:boundVariable ex:xDecl ;\n\
             \x20 math:argumentSlot ex:bodySlot .\n\
             {domain_line}\
             ex:bodySlot a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:app .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:xOcc .\n\
             ex:xOcc a math:VariableExpression ; math:variableOccurrence ex:xOccurrence .\n\
             ex:xOccurrence a math:VariableOccurrence ; math:declaredVariable ex:xDecl .\n"
    )
}

#[test]
fn math_declared_domain_changes_binder_sort_child() {
    let mut dag = TermDag::new();
    let untyped = MathGraph::from_turtle(binder_ttl(None).as_bytes()).expect("parse");
    let typed = MathGraph::from_turtle(binder_ttl(Some("ex:Reals")).as_bytes()).expect("parse");
    let n_untyped =
        lower_math_expression(&mut dag, &untyped, "https://example.org/binder").expect("untyped");
    let n_typed =
        lower_math_expression(&mut dag, &typed, "https://example.org/binder").expect("typed");
    assert_ne!(
        n_untyped, n_typed,
        "a declared bound-variable domain is a distinct sort child (not lost)"
    );
    assert_ne!(
        dag.key(n_untyped),
        dag.key(n_typed),
        "distinct content keys"
    );

    // The untyped math binder collapses with the untyped logic ∀ (default sort shared).
    let logic_node = lower_logic_formula(&mut dag, &forall_p_x()).expect("logic");
    assert_eq!(
        n_untyped, logic_node,
        "an undeclared math: domain defaults to the untyped individual sort"
    );
}

// ── math: free vs unscoped occurrences ──────────────────────────────────────────────

#[test]
fn math_free_declaration_lowers_to_free_and_unscoped_hard_fails() {
    // A free occurrence (declaration is a math:FreeVariableDeclaration) → a Free node.
    let free_ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:yOcc .\n\
             ex:yOcc a math:VariableExpression ; math:variableOccurrence ex:yOccurrence .\n\
             ex:yOccurrence a math:VariableOccurrence ; math:declaredVariable ex:yDecl .\n\
             ex:yDecl a math:FreeVariableDeclaration .\n";
    let mut dag = TermDag::new();
    let graph = MathGraph::from_turtle(free_ttl.as_bytes()).expect("parse");
    let node = lower_math_expression(&mut dag, &graph, "https://example.org/app")
        .expect("free occurrence lowers");
    // p(free y): the argument is a Free node (kind tag `V`), not a Bound one.
    assert!(
        dag.key(node).contains('V'),
        "free var → Free node: {}",
        dag.key(node)
    );

    // An occurrence whose declaration is neither bound nor free-declared is a hard fail.
    let unscoped_ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:zOcc .\n\
             ex:zOcc a math:VariableExpression ; math:variableOccurrence ex:zOccurrence .\n\
             ex:zOccurrence a math:VariableOccurrence ; math:declaredVariable ex:zDecl .\n\
             ex:zDecl a math:VariableDeclaration .\n";
    let graph = MathGraph::from_turtle(unscoped_ttl.as_bytes()).expect("parse");
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
        .expect_err("unscoped occurrence hard-fails");
    assert!(
        matches!(err, MathLoweringError::UnscopedOccurrence { .. }),
        "diagnostic names the unscoped occurrence: {err:?}"
    );
    assert_eq!(
        err.failure_class(),
        "https://blackcatinformatics.ca/math/UnscopedVariableOccurrence"
    );
}

// ── logic: a sequence marker is a hard fail, never a silent single-term coercion ────

#[test]
fn logic_sequence_marker_hard_fails() {
    let mut dag = TermDag::new();
    let formula = Formula::atom(
        Term::iri("https://example.org/p").unwrap(),
        vec![Term::sequence_marker("xs").unwrap()],
    )
    .unwrap();
    let err = lower_logic_formula(&mut dag, &formula).expect_err("sequence marker hard-fails");
    assert!(
        err.message().contains("sequence marker"),
        "diagnostic names the sequence marker: {}",
        err.message()
    );
}

// ── overflow guard: minting a Bound occurrence past the field width hard-fails ──────

#[test]
fn bound_slot_overflow_hard_fails() {
    // A binder with u16::MAX + 2 slots, whose body references the last (slot 65536):
    // minting Bound{slot: 65536} must hard-fail rather than silently wrap a u16.
    let mut dag = TermDag::new();
    let count: usize = u16::MAX as usize + 2; // 65537
    let vars: Vec<String> = (0..count).map(|i| format!("v{i}")).collect();
    let last = format!("v{}", count - 1); // resolves to slot 65536
    let formula = Formula::Forall {
        vars,
        body: Box::new(
            Formula::atom(
                Term::iri("https://example.org/p").unwrap(),
                vec![Term::var(last).unwrap()],
            )
            .unwrap(),
        ),
    };
    let err = lower_logic_formula(&mut dag, &formula).expect_err("slot overflow hard-fails");
    assert!(
        err.message().contains("u16::MAX") && err.message().contains("slot"),
        "diagnostic names the slot overflow: {}",
        err.message()
    );
}

// ── lang: an ill-formed form (empty sign system) carrying a denotation hard-fails ──

#[test]
fn lang_empty_sign_system_hard_fails() {
    let mut dag = TermDag::new();
    let denoted = LangDenotedForm {
        form: gmeow_lang_form::Form::Composed {
            sign_system: String::new(),
            level: "sentence".to_owned(),
            analysis: None,
            head: None,
            slots: Vec::new(),
        },
        denotation: LangDenotation::LogicFormula(forall_p_x()),
    };
    let err = lower_lang_form(&mut dag, &denoted).expect_err("empty sign system hard-fails");
    assert!(
        err.message().contains("sign system"),
        "diagnostic names the sign system: {}",
        err.message()
    );
}

#[test]
fn lang_entity_and_class_denotations_lower_to_leaf() {
    let mut dag = TermDag::new();
    let entity = lower_lang_denotation(
        &mut dag,
        &LangDenotation::Entity("https://example.org/venus".to_owned()),
    )
    .expect("entity");
    // The same IRI as a bare logic: term leaf must intern to the same node.
    let term_leaf =
        lower_logic_term(&mut dag, &Term::iri("https://example.org/venus").unwrap()).expect("term");
    assert_eq!(
        entity, term_leaf,
        "lang: entity IRI and logic: IRI leaf coincide"
    );

    let empty = lower_lang_denotation(&mut dag, &LangDenotation::Class(String::new()))
        .expect_err("empty class IRI hard-fails");
    assert!(empty.message().contains("non-empty"), "{}", empty.message());
}

// ── logic: Term::App lowers into a real App node (the former hard-fail seam) ───────

#[test]
fn term_app_lowers_matching_hand_built_intern_app() {
    let mut dag = TermDag::new();
    let term = Term::app(
        "https://example.org/f",
        vec![
            Term::iri("https://example.org/a").unwrap(),
            Term::iri("https://example.org/b").unwrap(),
        ],
    )
    .unwrap();
    let lowered = lower_logic_term(&mut dag, &term).expect("Term::App lowers");

    // By hand, exactly mirroring `Formula::Atom`'s own lowering shape: a reified leaf
    // op applied to its lowered argument carriers.
    let op = dag.intern_leaf(TermValue::iri("https://example.org/f"));
    let a = dag.intern_leaf(TermValue::iri("https://example.org/a"));
    let b = dag.intern_leaf(TermValue::iri("https://example.org/b"));
    let hand_built = dag.intern_app(op, vec![a, b]);

    // Hash-consing means a matching by-hand build interns to the SAME NodeId, not
    // merely an equal content key.
    assert_eq!(
        lowered, hand_built,
        "Term::App lowering interns to the same node as a by-hand intern_app build"
    );
    assert_eq!(dag.key(lowered), dag.key(hand_built));
    assert!(
        dag.key(lowered).starts_with("APP"),
        "lowered node is an application: {}",
        dag.key(lowered)
    );
}

#[test]
fn nested_application_lowers_and_round_trips() {
    // cons(H, cons(1, nil)): the second argument is itself an application, so a nested
    // `Term::App` must round-trip through the lowering, not just a flat one.
    fn cons_h_cons_one_nil() -> Term {
        Term::app(
            "https://example.org/cons",
            vec![
                Term::var("H").unwrap(),
                Term::app(
                    "https://example.org/cons",
                    vec![
                        Term::literal("1", None).unwrap(),
                        Term::iri("https://example.org/nil").unwrap(),
                    ],
                )
                .unwrap(),
            ],
        )
        .unwrap()
    }

    // Built and lowered in two SEPARATE fresh arenas: the nested shape must fold to
    // the identical content key regardless of which arena minted it — hash-consing
    // determinism for a NESTED application, not just a flat one.
    let mut dag_a = TermDag::new();
    let node_a = lower_logic_term(&mut dag_a, &cons_h_cons_one_nil()).expect("nested lowers (a)");
    let mut dag_b = TermDag::new();
    let node_b = lower_logic_term(&mut dag_b, &cons_h_cons_one_nil()).expect("nested lowers (b)");
    assert_eq!(
        dag_a.key(node_a),
        dag_b.key(node_b),
        "the same nested-application shape interns to the same content key in a \
             separate arena"
    );

    // Inspect the lowered structure; content keys are opaque identity bytes.
    use gmeow_term_arena::engine::NodeData;
    let NodeData::App { args, .. } = dag_a.data(node_a) else {
        panic!("outer application must survive lowering");
    };
    let NodeData::Free(head) = dag_a.data(args[0]) else {
        panic!("H must remain a free occurrence");
    };
    assert_eq!(dag_a.atom_value(*head), &TermValue::simple_literal("H"));
    let NodeData::App { args, .. } = dag_a.data(args[1]) else {
        panic!("inner application must survive lowering");
    };
    let NodeData::Leaf(nil) = dag_a.data(args[1]) else {
        panic!("nil must remain a leaf");
    };
    assert_eq!(
        dag_a.atom_value(*nil),
        &TermValue::iri("https://example.org/nil")
    );
}

// ── commutative sort key is CONTENT KEY, never NodeId ───────────────────────────

#[test]
fn g4_and_operand_order_content_key_stable_across_separate_dags() {
    fn atom(name: &str) -> Formula {
        Formula::atom(
            Term::iri(format!("https://example.org/{name}")).unwrap(),
            Vec::new(),
        )
        .unwrap()
    }
    let pq = Formula::And(vec![atom("p"), atom("q")]);
    let qp = Formula::And(vec![atom("q"), atom("p")]);

    // Built and interned in two SEPARATE fresh DAGs, so `p`/`q`'s NodeIds are minted
    // in the OPPOSITE order between the two arenas — a NodeId-keyed sort would then
    // disagree on operand order between the two DAGs, while a content-key-keyed sort
    // agrees regardless.
    let mut dag1 = TermDag::new();
    let node_pq = lower_logic_formula(&mut dag1, &pq).expect("And[p,q] lowers");
    let mut dag2 = TermDag::new();
    let node_qp = lower_logic_formula(&mut dag2, &qp).expect("And[q,p] lowers");

    assert_eq!(
        dag1.key(node_pq),
        dag2.key(node_qp),
        "And[p,q] and And[q,p], each built in a SEPARATE fresh DAG, must intern to the \
             same content key regardless of interning order (sorted by content key, never \
             NodeId)"
    );
}

// ── a math:NumberLiteral's typed literalValue lowers, datatype preserved ────────

#[test]
fn g7_math_number_literal_preserves_typed_datatype() {
    // `math:literalValue` as a genuine RDF typed literal (`"42"^^xsd:integer`) must
    // lower to a TYPED leaf — not hard-fail, and not silently drop the datatype.
    let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:lit a math:NumberLiteral ; math:literalValue \"42\"^^xsd:integer .\n";
    let mut dag = TermDag::new();
    let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
    let node = lower_math_expression(&mut dag, &graph, "https://example.org/lit")
        .expect("typed math:NumberLiteral lowers, not a hard fail");

    let gmeow_term_arena::engine::NodeData::Leaf(value) = dag.data(node) else {
        panic!("math:NumberLiteral must lower to a native leaf");
    };
    assert_eq!(
        dag.atom_value(*value),
        &TermValue::typed_literal("42", "http://www.w3.org/2001/XMLSchema#integer")
    );

    // It interns to the SAME node as a by-hand `typed_literal` build through the arena.
    let hand_built = dag.intern_leaf(TermValue::typed_literal(
        "42",
        "http://www.w3.org/2001/XMLSchema#integer",
    ));
    assert_eq!(
        node, hand_built,
        "math:NumberLiteral lowering interns to the SAME node as a by-hand typed literal"
    );
}

// ── math: a cyclic slotExpression graph hard-fails, never stack-overflows ──────────

#[test]
fn math_cyclic_expression_graph_hard_fails() {
    // ex:a's argument slot points at ex:b, whose argument slot points back at ex:a —
    // a two-triple cycle through `math:slotExpression`.
    let cyclic_ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:a a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:sa .\n\
             ex:sa a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:b .\n\
             ex:b a math:ApplicationExpression ; math:operator ex:q ; math:argumentSlot ex:sb .\n\
             ex:sb a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n";
    let mut dag = TermDag::new();
    let graph = MathGraph::from_turtle(cyclic_ttl.as_bytes()).expect("parse");
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/a")
        .expect_err("a cyclic slotExpression graph must hard-fail, not stack-overflow");
    assert!(
        matches!(err, MathLoweringError::CyclicExpressionGraph { .. }),
        "{err:?}"
    );
    assert_eq!(
        err.failure_class(),
        "https://blackcatinformatics.ca/math/CyclicExpressionGraph"
    );
}

// ── a ROOTLESS (fully closed) cyclic component is still reached and rejected ──

/// A fully closed 2-node cycle — `ex:a`'s argument slot points at `ex:b`, whose
/// argument slot points back at `ex:a`, and NEITHER is referenced from outside the
/// cycle — has NO node satisfying [`MathGraph::expression_roots`]'s "not referenced"
/// test: EVERY member is both candidate-typed AND referenced by the OTHER member of
/// the SAME component. Before the `expression_typed_nodes` orphan-seeding pass,
/// [`math_expression_structural_keys`] therefore NEVER lowered either node at all —
/// `lower_math_expression` was never called, so `math:CyclicExpressionGraph` could
/// never fire and this case was silently invisible (zero entries, zero findings, ZERO
/// coverage) rather than a rejected root. This asserts the CAPABILITY: the closed
/// component is discovered and its cycle guard actually fires.
#[test]
fn math_expression_structural_keys_reaches_a_rootless_cyclic_component() {
    let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:a a math:ApplicationExpression ; math:operator ex:p ; math:argumentSlot ex:sa .\n\
             ex:sa a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:b .\n\
             ex:b a math:ApplicationExpression ; math:operator ex:q ; math:argumentSlot ex:sb .\n\
             ex:sb a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n";
    let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");

    // Confirm the premise: NEITHER ex:a nor ex:b is a root under the pure
    // "not referenced" test — the closed cycle has no unreferenced member.
    let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse graph");
    assert!(
        graph.expression_roots().is_empty(),
        "a fully closed cycle has NO unreferenced member: {:?}",
        graph.expression_roots()
    );

    let results = math_expression_structural_keys(&dataset);
    assert!(
        !results.is_empty(),
        "the closed cyclic component must still be SEEDED and lowered at least once, \
             not silently skipped: {results:?}"
    );
    // Every entry must be the SAME typed rejection: the cycle guard actually fires,
    // never a silent Ok (accepting a cyclic node as an opaque leaf) and never some
    // OTHER unrelated rejection reason.
    for (node, result) in &results {
        match result {
            Err(MathLoweringError::CyclicExpressionGraph { .. }) => {}
            other => panic!("{node}: expected CyclicExpressionGraph, got {other:?}"),
        }
    }
}

// ── math: a pathologically deep application chain hard-fails on depth ─────────────

#[test]
fn math_expression_depth_exceeded_hard_fails() {
    // A chain of NESTED single-argument applications, several times deeper than
    // MAX_MATH_EXPRESSION_DEPTH, generated programmatically (not hand-authored) —
    // ex:app0(ex:app1(ex:app2(...(ex:leaf)...))).
    const CHAIN_LEN: usize = 2_000;
    let mut ttl = String::from(
        "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:leaf a math:NumberLiteral ; math:literalValue \"1\" .\n",
    );
    for i in 0..CHAIN_LEN {
        let child = if i + 1 == CHAIN_LEN {
            "ex:leaf".to_owned()
        } else {
            format!("ex:app{}", i + 1)
        };
        ttl.push_str(&format!(
            "ex:app{i} a math:ApplicationExpression ; math:operator ex:p ; \
                 math:argumentSlot ex:s{i} .\n\
                 ex:s{i} a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression {child} .\n"
        ));
    }
    let mut dag = TermDag::new();
    let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/app0")
        .expect_err("a chain far deeper than the recursion bound must hard-fail");
    assert!(
        matches!(err, MathLoweringError::ExpressionDepthExceeded { .. }),
        "{err:?}"
    );
    assert_eq!(
        err.failure_class(),
        "https://blackcatinformatics.ca/math/ExpressionDepthExceeded"
    );
}

// ── math: per-root structural keys isolate one bad root from the rest ─────────────

#[test]
fn math_expression_structural_keys_isolates_a_bad_root_from_good_roots() {
    // Two INDEPENDENT root expressions in one dataset: ex:good is well-formed,
    // ex:bad has a slot gap. Neither is referenced as any other node's
    // math:slotExpression, so both are candidate roots.
    let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:good a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:gs0 .\n\
             ex:gs0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n\
             ex:bad a math:ApplicationExpression ; math:operator ex:q ; \
             math:argumentSlot ex:bs0 , ex:bs2 .\n\
             ex:bs0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:b .\n\
             ex:bs2 a math:ArgumentSlot ; math:slotIndex 2 ; math:slotExpression ex:c .\n";
    let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
    let results = math_expression_structural_keys(&dataset);

    assert_eq!(results.len(), 2, "both roots are candidates: {results:?}");
    let good = results
        .get("https://example.org/good")
        .expect("ex:good present");
    assert!(good.is_ok(), "the well-formed root still lowers: {good:?}");

    let bad = results
        .get("https://example.org/bad")
        .expect("ex:bad present");
    let err = bad.as_ref().expect_err("the malformed root still fails");
    assert!(
        matches!(err, MathLoweringError::NonContiguousArgumentSlots { .. }),
        "{err:?}"
    );
}

// ── math: structural_digest / alpha_class_iri_for_digest are deterministic ─────────

#[test]
fn structural_digest_matches_for_alpha_equivalent_roots_and_differs_otherwise() {
    // p(a, b) authored slots-forward vs slots-reversed: the SAME expression, so the
    // SAME structural digest (they already intern to the same NodeId — this checks
    // the digest built on top of that identity agrees too).
    let forward = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (1, "ex:b")]).as_bytes())
        .expect("parse");
    let reversed = MathGraph::from_turtle(application_ttl(&[(1, "ex:b"), (0, "ex:a")]).as_bytes())
        .expect("parse");

    let digest_a =
        arena_structural_key(&forward, "https://example.org/app").expect("forward lowers");
    let digest_b =
        arena_structural_key(&reversed, "https://example.org/app").expect("reversed lowers");

    assert_eq!(
        digest_a, digest_b,
        "alpha-equivalent expressions share one structural digest"
    );
    // Deterministic: computing it again from the same graph is byte-identical.
    assert_eq!(
        digest_a,
        arena_structural_key(&forward, "https://example.org/app").expect("forward lowers")
    );

    // p(b, a) is a DISTINCT expression (operand order is identity-bearing) — a
    // DIFFERENT digest.
    let swapped = MathGraph::from_turtle(application_ttl(&[(0, "ex:b"), (1, "ex:a")]).as_bytes())
        .expect("parse");

    let digest_c =
        arena_structural_key(&swapped, "https://example.org/app").expect("swapped lowers");
    assert_ne!(digest_a, digest_c, "p(a,b) and p(b,a) get distinct digests");

    // The live minting entry point — the SAME one production calls — is content-stable
    // over the digest and injective across distinct digests.
    let iri_a = alpha_class_iri_for_digest(&digest_a);
    assert_eq!(
        iri_a,
        alpha_class_iri_for_digest(&digest_a),
        "minting is deterministic"
    );
    assert_ne!(
        iri_a,
        alpha_class_iri_for_digest(&digest_c),
        "distinct digests mint distinct alpha-class IRIs"
    );
}

/// Build a one-variable-binder `math:` expression `∀x. p(x)` whose every
/// declaration/occurrence/slot subject is suffixed by `suffix` — a family of
/// alpha-variants differing ONLY in the bound-variable declaration's IRI (and its
/// `rdfs:label`) can therefore be generated programmatically instead of
/// hand-authoring near-duplicate fixtures.
fn one_var_binder_variant_ttl(suffix: &str) -> String {
    format!(
        "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix ex: <https://example.org/> .\n\
             @prefix op: <https://blackcatinformatics.ca/logic/dag/op/> .\n\
             ex:binder{suffix} a math:BindingExpression ;\n\
             \x20 math:operator op:forall ;\n\
             \x20 math:boundVariable ex:decl{suffix} ;\n\
             \x20 math:argumentSlot ex:bodySlot{suffix} .\n\
             ex:decl{suffix} a math:VariableDeclaration ; rdfs:label \"{suffix}\"@en .\n\
             ex:bodySlot{suffix} a math:ArgumentSlot ; math:slotIndex 0 ; \
             math:slotExpression ex:app{suffix} .\n\
             ex:app{suffix} a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:s0{suffix} .\n\
             ex:s0{suffix} a math:ArgumentSlot ; math:slotIndex 0 ; \
             math:slotExpression ex:occ{suffix} .\n\
             ex:occ{suffix} a math:VariableExpression ; math:variableOccurrence ex:o{suffix} .\n\
             ex:o{suffix} a math:VariableOccurrence ; math:declaredVariable ex:decl{suffix} .\n"
    )
}

fn one_var_binder_variant_root(suffix: &str) -> String {
    format!("https://example.org/binder{suffix}")
}

// ── 1. Alpha-equivalence: see the `interning` property-test module below ─────────
//
// The hand-enumerated "a handful of hardcoded suffixes share one digest" case that
// used to live here is now STRICTLY SUBSUMED by
// `interning::bound_variable_renaming_does_not_change_digest` (arbitrary generated
// renamings, not five fixed strings) and `interning::shadowing_changes_binder_resolution_and_digest`
// (the nested-shadowing case, generated rather than hand-authored) — both drive the
// REAL `MathGraph`/`lower_math_expression` pipeline the same way this test did, over
// a generated rather than enumerated input space. Deleted per the no-duplicate-of-
// record standing instruction rather than kept alongside its own superset.

// ── the leaf fallback HARD-FAILS an ill-typed node, never degrades it ─────────

/// A `math:slotExpression` pointed directly at a bare `math:VariableOccurrence` (the
/// SHAPE the committed alpha-equivalence fixtures originally used, before they were
/// corrected to the `math:VariableExpression`-wrapped shape) is a genuine type error —
/// `math:VariableOccurrence` is not itself a `math:MathematicalExpression`. It must be
/// REJECTED, never silently accepted as an opaque IRI leaf keyed on the occurrence's
/// own subject (which would let two non-alpha-equivalent expressions collide, or let
/// an authored `math:structuralKey` claim an identity for a thing the grammar itself
/// refutes).
#[test]
fn bare_variable_occurrence_as_slot_expression_hard_fails() {
    let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:occ .\n\
             ex:occ a math:VariableOccurrence ; math:declaredVariable ex:decl .\n\
             ex:decl a math:VariableDeclaration .\n";
    let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
    let mut dag = TermDag::new();
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/app")
        .expect_err("a bare math:VariableOccurrence slot target must hard-fail");
    match &err {
        MathLoweringError::UnrecognizedExpressionType { node, types } => {
            assert_eq!(node, "https://example.org/occ");
            assert_eq!(
                types,
                &vec!["https://blackcatinformatics.ca/math/VariableOccurrence".to_owned()]
            );
        }
        other => panic!("expected UnrecognizedExpressionType, got {other:?}"),
    }
    assert_eq!(
        err.failure_class(),
        "https://blackcatinformatics.ca/math/UnrecognizedExpressionType"
    );
}

/// A node carrying a genuinely UNKNOWN/typo'd `math:` type (never authored anywhere
/// in the `math:` vocabulary) used as a slot operand must hard-fail exactly the same
/// way — the fallback is not merely a denylist of the classes this file happens to
/// know about.
#[test]
fn typo_math_type_as_slot_expression_hard_fails() {
    let ttl = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:p ; \
             math:argumentSlot ex:s0 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:bogus .\n\
             ex:bogus a math:MathematicalSttaement .\n";
    let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("parse");
    let mut dag = TermDag::new();
    let err = lower_math_expression(&mut dag, &graph, "https://example.org/app").expect_err(
        "a typo'd math: class on a slot target must hard-fail, never silently \
                         degrade to an opaque leaf",
    );
    assert!(
        matches!(err, MathLoweringError::UnrecognizedExpressionType { .. }),
        "{err:?}"
    );
}

/// A `math:SymbolReference` leaf (the RECOGNIZED constant-operand type, e.g.
/// `slices/grounding/math/examples/reference-ast-act.ttl`'s `ex:leftMatrixRef`) is
/// still accepted through the fallback, interning its own IRI exactly like an
/// untyped external constant — the stricter fallback rejects UNRECOGNIZED `math:`
/// types, never every `math:` type whatsoever.
#[test]
fn symbol_reference_leaf_interns_on_its_symbol_not_its_own_iri() {
    // TWO independently authored copies of the same expression over the SAME symbols,
    // differing only in their occurrence-wrapper IRIs. This is the case the shipped
    // reference example cannot express, because it reuses one pair of occurrence nodes
    // across both of its expressions — holding constant the very IRIs the defect moved
    // with, which is why a digest keyed on the wrapper looked correct there.
    let ttl = |refl: &str, refr: &str, app: &str, s0: &str, s1: &str| {
        format!(
            "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
                 @prefix ex: <https://example.org/> .\n\
                 ex:{app} a math:ApplicationExpression ; math:operator ex:p ; \
                 math:argumentSlot ex:{s0} , ex:{s1} .\n\
                 ex:{s0} a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:{refl} .\n\
                 ex:{s1} a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:{refr} .\n\
                 ex:{refl} a math:SymbolReference ; math:hasMathematicalSymbol ex:symL .\n\
                 ex:{refr} a math:SymbolReference ; math:hasMathematicalSymbol ex:symR .\n\
                 ex:symL a math:MathematicalSymbol .\n\
                 ex:symR a math:MathematicalSymbol .\n"
        )
    };
    let digest_of = |text: &str, root: &str| {
        let graph = MathGraph::from_turtle(text.as_bytes()).expect("parse");
        let mut dag = TermDag::new();
        let node = lower_math_expression(&mut dag, &graph, root).expect("lowers");
        structural_digest(&dag, node)
    };
    let a = digest_of(
        &ttl("refA0", "refA1", "appA", "sA0", "sA1"),
        "https://example.org/appA",
    );
    let b = digest_of(
        &ttl("refB0", "refB1", "appB", "sB0", "sB1"),
        "https://example.org/appB",
    );
    assert_eq!(
        a, b,
        "two independently authored copies of one expression over the SAME symbols must \
             intern to ONE key; a digest that moves with the occurrence-wrapper IRI is a label, \
             not a content key, and the alpha-equivalence contract is false"
    );

    // Different SYMBOLS must still separate — the fix must not collapse distinct content.
    let other = digest_of(
        &ttl("refA0", "refA1", "appA", "sA0", "sA1").replace("ex:symR", "ex:symZ"),
        "https://example.org/appA",
    );
    assert_ne!(
        a, other,
        "expressions over DIFFERENT symbols must not collide"
    );
}

// ── 2. Injectivity: structurally DISTINCT expressions get DIFFERENT digests ───────

#[test]
fn structurally_distinct_expressions_get_distinct_digests() {
    let mut dag = TermDag::new();

    // f(a, b) vs f(b, a): swapped slot indexes at the same operator — operand order
    // is always identity-bearing for a `math:ApplicationExpression`.
    let fab = MathGraph::from_turtle(application_ttl(&[(0, "ex:a"), (1, "ex:b")]).as_bytes())
        .expect("parse f(a,b)");
    let fba = MathGraph::from_turtle(application_ttl(&[(0, "ex:b"), (1, "ex:a")]).as_bytes())
        .expect("parse f(b,a)");
    let n_fab = lower_math_expression(&mut dag, &fab, "https://example.org/app").expect("f(a,b)");
    let n_fba = lower_math_expression(&mut dag, &fba, "https://example.org/app").expect("f(b,a)");

    // A DIFFERENT operator entirely, over the SAME operands/shape as f(a,b).
    let ttl_g = "@prefix math: <https://blackcatinformatics.ca/math/> .\n\
             @prefix ex: <https://example.org/> .\n\
             ex:app a math:ApplicationExpression ; math:operator ex:g ; \
             math:argumentSlot ex:s0 , ex:s1 .\n\
             ex:s0 a math:ArgumentSlot ; math:slotIndex 0 ; math:slotExpression ex:a .\n\
             ex:s1 a math:ArgumentSlot ; math:slotIndex 1 ; math:slotExpression ex:b .\n";
    let g_graph = MathGraph::from_turtle(ttl_g.as_bytes()).expect("parse g(a,b)");
    let n_g = lower_math_expression(&mut dag, &g_graph, "https://example.org/app").expect("g(a,b)");

    // A different binder sort/domain over an otherwise identical binder shape.
    let untyped = MathGraph::from_turtle(binder_ttl(None).as_bytes()).expect("parse untyped");
    let typed =
        MathGraph::from_turtle(binder_ttl(Some("ex:Reals")).as_bytes()).expect("parse typed");
    let n_untyped = lower_math_expression(&mut dag, &untyped, "https://example.org/binder")
        .expect("untyped binder");
    let n_typed = lower_math_expression(&mut dag, &typed, "https://example.org/binder")
        .expect("typed binder");

    let labeled_digests = [
        ("f(a,b)", structural_digest(&dag, n_fab)),
        ("f(b,a)", structural_digest(&dag, n_fba)),
        ("g(a,b)", structural_digest(&dag, n_g)),
        ("untyped binder", structural_digest(&dag, n_untyped)),
        ("typed binder", structural_digest(&dag, n_typed)),
    ];
    for i in 0..labeled_digests.len() {
        for j in (i + 1)..labeled_digests.len() {
            let (name_i, digest_i) = &labeled_digests[i];
            let (name_j, digest_j) = &labeled_digests[j];
            assert_ne!(
                digest_i, digest_j,
                "{name_i} and {name_j} are structurally distinct and must get distinct \
                     digests"
            );
        }
    }
}

// ── 3. Interning: α-variants of ONE expression add nodes for the distinct ─────────
// ──    structure only, never once per variant ─────────────────────────────────────

#[test]
fn alpha_variants_of_one_expression_intern_to_a_fixed_node_count() {
    let mut dag = TermDag::new();
    let len_before = dag.len();

    // A single one-variable-binder expression is built from exactly 6 distinct
    // constituent nodes (its `forall` op leaf, the untyped-individual sort leaf,
    // `p`'s op leaf, the bound occurrence, the `App` node, the `Binder` node
    // itself) — so this family must be STRICTLY larger than 6 for "grew by far
    // fewer nodes than variants lowered" to be a meaningful (non-vacuous) claim.
    let variants = [
        "Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta", "Eta", "Theta", "Iota", "Kappa",
    ];
    let mut nodes = Vec::new();
    for suffix in variants {
        let ttl = one_var_binder_variant_ttl(suffix);
        let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("variant parses");
        let root = one_var_binder_variant_root(suffix);
        let node = lower_math_expression(&mut dag, &graph, &root).expect("variant lowers");
        nodes.push(node);
    }

    // Every variant interns to the SAME node (hash-consing under alpha-equivalence).
    assert!(
        nodes.windows(2).all(|w| w[0] == w[1]),
        "all α-variants intern to one NodeId: {nodes:?}"
    );

    // The dag grew by the FIXED, small number of distinct constituent nodes a single
    // one-variable-binder expression is built from (its `forall` op leaf, the
    // untyped-individual sort leaf, `p`'s op leaf, the bound occurrence, the App
    // node, the Binder node) — strictly fewer nodes than the number of α-variants
    // lowered, never one new node per variant.
    let distinct_nodes_added = dag.len() - len_before;
    assert!(
        distinct_nodes_added > 0,
        "the dag actually grew by lowering the first variant"
    );
    assert!(
        distinct_nodes_added < variants.len(),
        "{distinct_nodes_added} distinct nodes added for {} α-variants of ONE expression — \
             must intern to far fewer nodes than variants lowered, never grow linearly with N",
        variants.len()
    );

    // Re-lowering the SAME shapes a second time must add ZERO new nodes at all (pure
    // re-interning) — proving the count above was not an accident of some
    // sub-structure not being fully shared.
    let stable_len = dag.len();
    for suffix in variants {
        let ttl = one_var_binder_variant_ttl(suffix);
        let graph = MathGraph::from_turtle(ttl.as_bytes()).expect("variant re-parses");
        let root = one_var_binder_variant_root(suffix);
        let _ = lower_math_expression(&mut dag, &graph, &root).expect("variant re-lowers");
    }
    assert_eq!(
        dag.len(),
        stable_len,
        "re-lowering the same α-variants a second time adds ZERO new nodes"
    );
}
