// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The hand-built expected formula for "every cat chases a mouse":
/// `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))`.
fn expected_flagship_formula() -> Formula {
    let cat_x = predicate_atom("cat", vec![Term::var("x").unwrap()]).unwrap();
    let mouse_y = predicate_atom("mouse", vec![Term::var("y").unwrap()]).unwrap();
    let chase_xy = predicate_atom(
        "chase",
        vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
    )
    .unwrap();
    let inner = Formula::Exists {
        vars: vec!["y".to_owned()],
        body: Box::new(Formula::And(vec![mouse_y, chase_xy])),
    };
    Formula::Forall {
        vars: vec!["x".to_owned()],
        body: Box::new(Formula::Implies(Box::new(cat_x), Box::new(inner))),
    }
}

#[test]
fn flagship_lowers_to_the_expected_compositional_formula() {
    let lowering = lower_svo(&flagship_svo_sentence()).expect("flagship lowers");
    let expected = expected_flagship_formula();
    assert_eq!(
        lowering.formula.content_key(),
        expected.content_key(),
        "every cat chases a mouse must lower to ∀x(cat(x) → ∃y(mouse(y) ∧ chase(x,y)))"
    );
}

#[test]
fn every_stage_carries_a_preservation_record() {
    let lowering = lower_svo(&flagship_svo_sentence()).expect("flagship lowers");
    // No lowering step is undeclared: exactly the required stages, in order.
    lowering
        .assert_all_stages_declared()
        .expect("all stages declared");
    assert_eq!(lowering.stages.len(), REQUIRED_STAGES.len());
    for stage in &lowering.stages {
        // The modeled fragment lowers exactly.
        assert_eq!(stage.preservation, PreservationKind::Exact);
        assert!(!stage.note.trim().is_empty(), "stage note is present");
    }
    assert_eq!(lowering.stage_names(), REQUIRED_STAGES);
}

#[test]
fn existential_subject_reads_as_and_not_implies() {
    // "some cat chases a mouse" → ∃x(cat(x) ∧ ∃y(mouse(y) ∧ chase(x,y))).
    let mut sentence = flagship_svo_sentence();
    if let Form::Composed { slots, .. } = &mut sentence
        && let Form::Composed { slots: np, .. } = &mut slots[0].form
    {
        np[0].form = lexeme("some", "DET");
    }
    let lowering = lower_svo(&sentence).expect("existential subject lowers");
    let cat_x = predicate_atom("cat", vec![Term::var("x").unwrap()]).unwrap();
    let mouse_y = predicate_atom("mouse", vec![Term::var("y").unwrap()]).unwrap();
    let chase_xy = predicate_atom(
        "chase",
        vec![Term::var("x").unwrap(), Term::var("y").unwrap()],
    )
    .unwrap();
    let inner = Formula::Exists {
        vars: vec!["y".to_owned()],
        body: Box::new(Formula::And(vec![mouse_y, chase_xy])),
    };
    let expected = Formula::Exists {
        vars: vec!["x".to_owned()],
        body: Box::new(Formula::And(vec![cat_x, inner])),
    };
    assert_eq!(lowering.formula.content_key(), expected.content_key());
}

#[test]
fn non_composed_sentence_hard_fails() {
    let err = lower_svo(&lexeme("cat", "NOUN")).expect_err("a bare lexeme is not a sentence");
    assert!(err.construct.contains("not a Composed sentence"), "{err}");
}

#[test]
fn unmodeled_determiner_hard_fails() {
    // "most cats chase a mouse" — 'most' is not a modeled first-order determiner.
    let mut sentence = flagship_svo_sentence();
    if let Form::Composed { slots, .. } = &mut sentence
        && let Form::Composed { slots: np, .. } = &mut slots[0].form
    {
        np[0].form = lexeme("most", "DET");
    }
    let err = lower_svo(&sentence).expect_err("'most' is unmodeled");
    assert!(err.construct.contains("unmodeled determiner"), "{err}");
}

#[test]
fn intransitive_clause_hard_fails() {
    // A subject + verb with no object is outside the transitive-SVO fragment.
    let sentence = Form::Composed {
        sign_system: "en".to_owned(),
        level: "sentence".to_owned(),
        analysis: None,
        head: Some(1),
        slots: vec![
            Slot {
                index: 0,
                role: Some("subject".to_owned()),
                dep_relation: Some("nsubj".to_owned()),
                depends_on: Some(1),
                form: noun_phrase("every", "cat"),
            },
            Slot {
                index: 1,
                role: Some("predicate".to_owned()),
                dep_relation: Some("root".to_owned()),
                depends_on: None,
                form: lexeme("sleep", "VERB"),
            },
        ],
    };
    let err = lower_svo(&sentence).expect_err("intransitive clause is unmodeled");
    assert!(err.construct.contains("object constituent"), "{err}");
}

#[test]
fn bare_nominal_subject_hard_fails() {
    // "cats chase a mouse" — a determiner-less subject is refused, never read as an
    // implicit existential.
    let mut sentence = flagship_svo_sentence();
    if let Form::Composed { slots, .. } = &mut sentence {
        slots[0].form = lexeme("cat", "NOUN");
    }
    let err = lower_svo(&sentence).expect_err("bare nominal subject is unmodeled");
    assert!(err.construct.contains("bare nominal"), "{err}");
}

#[test]
fn grammar_rule_lowers_to_span_indexed_derivation() {
    // S → NP VP  ⇒  S(i0,i2) :- NP(i0,i1), VP(i1,i2).
    let s_rule = &svo_grammar().rules[0];
    let derivation = grammar_rule_to_derivation(s_rule).expect("S production lowers");
    assert_eq!(derivation.nonterminal, "S");

    let expected_head = nonterminal_atom("S", 0, 2).expect("S head atom");
    let expected_body = vec![
        nonterminal_atom("NP", 0, 1).expect("NP body atom"),
        nonterminal_atom("VP", 1, 2).expect("VP body atom"),
    ];
    assert_eq!(derivation.head.content_key(), expected_head.content_key());
    assert_eq!(derivation.body.len(), 2);
    assert_eq!(
        derivation.body[0].content_key(),
        expected_body[0].content_key()
    );
    assert_eq!(
        derivation.body[1].content_key(),
        expected_body[1].content_key()
    );

    // The rule as one implication `(NP(i0,i1) ∧ VP(i1,i2)) → S(i0,i2)`.
    let expected_formula = Formula::Implies(
        Box::new(Formula::And(expected_body)),
        Box::new(expected_head),
    );
    assert_eq!(
        derivation.to_formula().content_key(),
        expected_formula.content_key()
    );
}

#[test]
fn whole_svo_grammar_lowers_to_three_chart_rules() {
    let rules = grammar_to_derivation_rules(&svo_grammar()).expect("SVO grammar lowers");
    assert_eq!(rules.len(), 3);
    let names: Vec<&str> = rules.iter().map(|r| r.nonterminal.as_str()).collect();
    assert_eq!(names, vec!["S", "VP", "NP"]);
    // Every production here is a two-symbol concatenation → head span (0,2), two body atoms.
    for rule in &rules {
        assert_eq!(rule.body.len(), 2);
        assert_eq!(
            rule.head.content_key(),
            nonterminal_atom(&rule.nonterminal, 0, 2)
                .expect("head atom")
                .content_key()
        );
    }
}

#[test]
fn ntriples_emission_records_every_stage_preservation() {
    let lowering = lower_svo(&flagship_svo_sentence()).expect("flagship lowers");
    let bytes = lowering.to_ntriples("http://example.org/lang/lowering/flagship");
    let text = String::from_utf8(bytes).expect("UTF-8 N-Triples");
    // One preservationKind triple per stage, all Exact for the modeled fragment.
    let exact = PreservationKind::Exact.iri();
    let count = text
        .lines()
        .filter(|l| l.contains("preservationKind") && l.contains(&exact))
        .count();
    assert_eq!(count, REQUIRED_STAGES.len());
    assert!(text.contains("CompositionalLowering"));
    assert!(text.contains("LoweringStage"));
}
