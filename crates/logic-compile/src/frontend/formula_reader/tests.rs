// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Reconstruction reuse must preserve GMEOW ownership, scope and diagnostics.

use std::sync::Arc;

use super::*;
use crate::frontend::{extract_constraints, extract_formulas, extract_reasoning_programs};

fn dataset(source: &str) -> Arc<RdfDataset> {
    purrdf::parse_dataset(
        format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .
         @prefix ex: <https://example.org/> . {source}"
        )
        .as_bytes(),
        "text/turtle",
        None,
    )
    .expect("synthetic logic source")
}

fn subject(local: &str) -> Subject {
    Subject::Iri(format!("https://example.org/{local}"))
}

const OWNED_SOURCE: &str = r#"
    ex:law a logic:Formula ; logic:quantifiedVariable ex:x ; logic:forall ex:implication .
    ex:implication a logic:Formula ; logic:antecedent ex:guard ; logic:consequent ex:claim .
    ex:guard a logic:Formula ; logic:relation logic:instanceOf ; logic:argument ex:x, ex:class .
    ex:claim a logic:Formula ; logic:relation ex:p ; logic:argument ex:x, ex:value .
    ex:x logic:termIndex 0 ; logic:termVariable "x" ; logic:variableSort ex:Thing .
    ex:class logic:termIndex 1 ; logic:termIri ex:Thing .
    ex:value logic:termIndex 1 ; logic:termIri ex:valueConstant .
    ex:constraint a logic:Constraint ; logic:integrity ex:law .
    ex:program a logic:ReasoningProgram ; logic:evaluationMode logic:BackwardEvaluation ;
        logic:clause ex:law ; logic:programQuery ex:claim ; logic:verdictProbe ex:ground .
    ex:ground a logic:Formula ; logic:relation ex:p ; logic:argument
        [ logic:termIndex 0 ; logic:termIri ex:individual ], ex:value .
    ex:correspondence a logic:Correspondence ;
        logic:correspondenceRelation logic:Subsumes ; logic:morphismClass logic:LossyLens ;
        logic:morphismKind logic:InstitutionMorphism ; logic:recoveryCase ex:recovery .
    ex:recovery a logic:RecoveryCase ; logic:recoveryTransform ex:law .
"#;

#[test]
fn every_semantic_owner_reuses_reconstruction_without_becoming_an_assertion() {
    let source = dataset(OWNED_SOURCE);
    let mut reader = FormulaReader::new(source.as_ref());
    let mut diagnostics = Vec::new();
    let extracted = extract_formulas(
        &mut reader,
        &super::super::StructuralSourceGraph::new(&source),
        &mut diagnostics,
    );
    assert!(
        extracted.formulas.is_empty(),
        "owned laws are not global assertions"
    );
    assert!(extracted.malformed.is_empty());
    assert_eq!(
        reader.observer.reconstructions, 5,
        "each shared source formula is reconstructed once"
    );
    let mut lowerings = Vec::new();
    let constraints: Vec<_> = extract_constraints(
        &mut reader,
        &mut diagnostics,
        &extracted.malformed,
        &mut lowerings,
    )
    .into_iter()
    .map(|emission| emission.value)
    .collect();
    let programs: Vec<_> =
        extract_reasoning_programs(&mut reader, &mut diagnostics, &mut lowerings)
            .into_iter()
            .map(|emission| emission.value)
            .collect();
    let (correspondences, errors) =
        crate::projections::correspondence::extract_correspondences_with_reader(&mut reader);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(constraints.len(), 1);
    assert_eq!(programs.len(), 1);
    assert_eq!(correspondences.len(), 1);
    assert_eq!(
        reader.observer.reconstructions, 5,
        "all owners consume the already reconstructed nodes"
    );
    assert!(
        programs[0]
            .variable_sorts
            .iter()
            .any(|(_, name, sort)| name == "x" && sort == "https://example.org/Thing")
    );
}

#[test]
fn modal_world_and_depth_are_part_of_reuse_identity() {
    let source = dataset(
        r#"
        ex:atom logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .
        ex:box logic:necessarily ex:atom ; logic:overAccessibility logic:deonticallyIdeal .
    "#,
    );
    let mut reader = FormulaReader::new(source.as_ref());
    let atom = subject("atom");
    let plain = reader.read(&atom).unwrap();
    for (world, depth) in [("w", 1), ("v", 1), ("w", 2)] {
        let context = Translation::AtWorld {
            world: Term::Var(world.to_owned()),
            depth,
        };
        let expanded = reader.expand(&atom, &context, &mut Vec::new()).unwrap();
        assert_ne!(plain, expanded);
        let Formula::Atom { args, .. } = expanded else {
            panic!("expected atom")
        };
        assert_eq!(
            args,
            vec![
                Term::Var(world.to_owned()),
                Term::Iri("https://example.org/a".to_owned())
            ]
        );
        let nested = reader
            .expand(&subject("box"), &context, &mut Vec::new())
            .unwrap();
        let Formula::Forall { vars, .. } = nested else {
            panic!("expected translated necessity")
        };
        assert_eq!(vars, vec![format!("__w{depth}")]);
    }
    let before = reader.observer.reconstructions;
    reader.read(&atom).unwrap();
    assert_eq!(reader.observer.reconstructions, before);
}

#[test]
fn modal_translation_cannot_capture_authored_variables() {
    let source = dataset(
        r#"
        ex:outer logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable "__w0" ] ;
            logic:forall ex:box .
        ex:box logic:necessarily ex:atom ; logic:overAccessibility logic:deonticallyIdeal .
        ex:atom logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termVariable "__w0" ] .
    "#,
    );
    let mut reader = FormulaReader::new(source.as_ref());
    let Formula::Forall { vars, body } = reader.read(&subject("outer")).unwrap() else {
        panic!("authored quantifier")
    };
    assert_eq!(vars, vec!["__w0"]);
    let Formula::Forall { vars: worlds, body } = *body else {
        panic!("modal quantifier")
    };
    assert_ne!(worlds, vars);
    let Formula::Implies(_, atom) = *body else {
        panic!("accessibility guard")
    };
    let Formula::Atom { args, .. } = *atom else {
        panic!("world-indexed predication")
    };
    assert_eq!(
        args,
        vec![Term::Var(worlds[0].clone()), Term::Var("__w0".to_owned())]
    );
}

#[test]
fn sort_walk_includes_modal_bodies_without_merging_occurrence_scopes() {
    let source = dataset(
        r#"
        ex:left logic:necessarily ex:atom ; logic:overAccessibility logic:deonticallyIdeal .
        ex:right logic:necessarily ex:other ; logic:overAccessibility logic:deonticallyIdeal .
        ex:atom logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ; logic:variableSort ex:Left ] .
        ex:other logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termVariable "x" ; logic:variableSort ex:Right ] .
    "#,
    );
    let mut reader = FormulaReader::new(source.as_ref());
    let left = reader.read(&subject("left")).unwrap();
    let right = reader.read(&subject("right")).unwrap();
    assert_eq!(left.content_key(), right.content_key());
    for (root, sort) in [("left", "Left"), ("right", "Right")] {
        let mut sorts = Vec::new();
        crate::frontend::collect_variable_sorts(&source, &subject(root), &mut sorts).unwrap();
        assert_eq!(
            sorts,
            vec![("x".to_owned(), format!("https://example.org/{sort}"))]
        );
    }
}

#[test]
fn disabled_or_evicted_memo_preserves_values_and_cycle_diagnostics() {
    let source = dataset(
        r#"
        ex:atom logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .
        ex:negation logic:not ex:atom .
        ex:a logic:not ex:b . ex:b logic:not ex:c . ex:c logic:not ex:a .
        ex:bad logic:and ex:atom .
    "#,
    );
    let mut reference = FormulaReader::new(source.as_ref());
    reference.max_bytes = 0;
    let roots = ["atom", "negation", "a", "b", "c", "bad", "a", "atom", "bad"];
    let expected: Vec<_> = roots
        .iter()
        .map(|root| reference.expand(&subject(root), &Translation::Plain, &mut Vec::new()))
        .collect();
    for budget in [0, 1024, MAX_MEMO_BYTES] {
        let mut reader = FormulaReader::new(source.as_ref());
        reader.max_bytes = budget;
        for (root, expected) in roots.iter().zip(&expected) {
            assert_eq!(
                &reader.expand(&subject(root), &Translation::Plain, &mut Vec::new()),
                expected,
                "root {root}, budget {budget}"
            );
            assert!(reader.retained_bytes <= budget);
        }
    }
}

#[test]
fn malformed_roots_reuse_typed_failures_and_keep_their_source_focus() {
    let source = dataset("ex:root logic:not ex:missing .");
    let mut reader = FormulaReader::new(source.as_ref());
    let first = reader.read(&subject("root")).unwrap_err();
    let before = reader.observer.reconstructions;
    let second = reader.read(&subject("root")).unwrap_err();
    assert_eq!(reader.observer.reconstructions, before);
    assert_eq!(first.code(), second.code());
    assert_eq!(first.grade(), second.grade());
    assert_eq!(first.message(), second.message());
    assert_eq!(
        first.inner().source_ctx.focus,
        second.inner().source_ctx.focus
    );
    assert_eq!(
        second.inner().source_ctx.focus.as_ref().unwrap().0,
        "https://example.org/missing"
    );
}

#[test]
fn equal_blank_labels_in_distinct_scopes_do_not_create_cycles_or_merge_owners() {
    let leaf = dataset(
        r#"
        ex:atom a logic:Formula ; logic:relation ex:p ;
            logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .
    "#,
    );
    let mut builder = purrdf::RdfDatasetBuilder::new();
    builder.push_dataset(&leaf);
    let outer = builder.intern_blank("sameLabel", purrdf::BlankScope(1));
    let inner = builder.intern_blank("sameLabel", purrdf::BlankScope(2));
    let atom = builder.intern_iri("https://example.org/atom");
    let negation = builder.intern_iri(&logic_iri("not"));
    let typing = builder.intern_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type");
    let formula = builder.intern_iri(&logic_iri("Formula"));
    builder.push_quad(outer, typing, formula, None);
    builder.push_quad(inner, typing, formula, None);
    builder.push_quad(outer, negation, inner, None);
    builder.push_quad(inner, negation, atom, None);
    let source = builder.freeze().unwrap();
    let mut reader = FormulaReader::new(source.as_ref());
    let mut diagnostics = Vec::new();
    let extracted = extract_formulas(
        &mut reader,
        &super::super::StructuralSourceGraph::new(&source),
        &mut diagnostics,
    );
    assert!(
        diagnostics.is_empty(),
        "distinct scoped nodes are not a cycle: {diagnostics:?}"
    );
    assert_eq!(
        extracted.formulas.len(),
        1,
        "only the outer scoped node is unowned"
    );
    let expected = reader.read(&subject("atom")).unwrap();
    assert_eq!(
        extracted
            .formulas
            .into_iter()
            .map(|entry| entry.formula)
            .collect::<Vec<_>>(),
        vec![Formula::Not(Box::new(Formula::Not(Box::new(expected))))]
    );
    assert_eq!(reader.observer.reconstructions, 3);
}

fn contextual_atom(relation: &str, world: Term, argument: Term) -> Formula {
    Formula::atom(Term::Iri(relation.to_owned()), vec![world, argument]).unwrap()
}

#[test]
fn selected_context_and_explicit_subtree_context_have_separate_memo_identity() {
    let source = dataset(
        r#"
        ex:root logic:and ex:plain, ex:scoped .
        ex:plain logic:relation ex:p ; logic:argument ex:argument .
        ex:scoped logic:relation ex:q ; logic:argument ex:argument ; logic:inContext ex:inner .
        ex:argument logic:termIndex 0 ; logic:termIri ex:a .
        "#,
    );
    let mut reader = FormulaReader::new(source.as_ref());
    let argument = Term::Iri("https://example.org/a".into());
    for world in ["outer", "other", "outer"] {
        let Formula::And(parts) = reader
            .read_in_context(
                &subject("root"),
                Term::Iri(format!("https://example.org/{world}")),
            )
            .unwrap()
        else {
            panic!("selected conjunction")
        };
        assert_eq!(parts.len(), 2);
        assert!(parts.contains(&contextual_atom(
            "https://example.org/p",
            Term::Iri(format!("https://example.org/{world}")),
            argument.clone()
        )));
        assert!(parts.contains(&contextual_atom(
            "https://example.org/q",
            Term::Iri("https://example.org/inner".into()),
            argument.clone()
        )));
    }
    assert_eq!(
        reader.admitted_sources.len(),
        1,
        "source admission is independent of translation context"
    );
    let Formula::Atom { args, .. } = reader.read(&subject("plain")).unwrap() else {
        panic!("unscoped source atom")
    };
    assert_eq!(
        args,
        vec![argument],
        "contextual reuse must not alter a plain reconstruction"
    );
}

#[test]
fn malformed_explicit_contexts_are_never_ignored() {
    for context in ["ex:left, ex:right", "\"literal context\"", "[]"] {
        let source = dataset(&format!(
            "ex:root logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ; logic:inContext {context} ."
        ));
        let mut reader = FormulaReader::new(source.as_ref());
        for contextual in [false, true] {
            let error = if contextual {
                reader.read_in_context(
                    &subject("root"),
                    Term::Iri("https://example.org/selected".into()),
                )
            } else {
                reader.read(&subject("root"))
            }
            .unwrap_err();
            assert!(error.message().contains("logic:inContext"));
            assert_eq!(
                error.inner().source_ctx.focus.as_ref().unwrap().0,
                "https://example.org/root"
            );
        }
    }
}

#[test]
fn finite_temporal_guards_and_until_interval_share_the_selected_context() {
    use crate::frontend::{FINITE_AT_OR_AFTER, FINITE_NEXT, FINITE_STRICTLY_BEFORE};

    let source = dataset(
        r#"
        ex:next logic:next ex:right .
        ex:eventually logic:eventually ex:right .
        ex:globally logic:globally ex:right .
        ex:until logic:until ex:right ; logic:untilLeft ex:left .
        ex:right logic:relation ex:arrived ; logic:argument ex:argument .
        ex:left logic:relation ex:waiting ; logic:argument ex:argument .
        ex:argument logic:termIndex 0 ; logic:termIri ex:a .
        "#,
    );
    let mut reader = FormulaReader::new(source.as_ref());
    let world = Term::Iri("https://example.org/selected".into());
    let argument = Term::Iri("https://example.org/a".into());
    for operator in ["next", "eventually", "globally", "until"] {
        let actual = reader
            .read_in_context(&subject(operator), world.clone())
            .unwrap();
        let future = Term::Var("__tw0".into());
        let guard = contextual_atom(
            if operator == "next" {
                FINITE_NEXT
            } else {
                FINITE_AT_OR_AFTER
            },
            world.clone(),
            future.clone(),
        );
        let right = contextual_atom(
            "https://example.org/arrived",
            future.clone(),
            argument.clone(),
        );
        let expected = if operator == "globally" {
            Formula::Forall {
                vars: vec!["__tw0".into()],
                body: Box::new(Formula::Implies(Box::new(guard), Box::new(right))),
            }
        } else {
            let body = if operator == "until" {
                let intermediate = Term::Var("__ti0".into());
                Formula::And(vec![
                    right,
                    Formula::Forall {
                        vars: vec!["__ti0".into()],
                        body: Box::new(Formula::Implies(
                            Box::new(Formula::And(vec![
                                contextual_atom(
                                    FINITE_AT_OR_AFTER,
                                    world.clone(),
                                    intermediate.clone(),
                                ),
                                contextual_atom(
                                    FINITE_STRICTLY_BEFORE,
                                    intermediate.clone(),
                                    future,
                                ),
                            ])),
                            Box::new(contextual_atom(
                                "https://example.org/waiting",
                                intermediate,
                                argument.clone(),
                            )),
                        )),
                    },
                ])
            } else {
                right
            };
            Formula::Exists {
                vars: vec!["__tw0".into()],
                body: Box::new(Formula::And(vec![guard, body])),
            }
        };
        assert_eq!(actual, expected, "finite {operator} translation");
    }
}

#[test]
fn finite_temporal_operands_remain_mandatory_and_exclusive() {
    for declaration in [
        "logic:next ex:atom, ex:other",
        "logic:eventually \"not a formula\"",
        "logic:globally ex:atom ; logic:not ex:atom",
        "logic:until ex:atom",
        "logic:untilLeft ex:atom",
        "logic:until ex:atom ; logic:untilLeft ex:atom, ex:other",
    ] {
        let source = dataset(&format!(
            "ex:root {declaration} . ex:atom logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ."
        ));
        let error = FormulaReader::new(source.as_ref())
            .read(&subject("root"))
            .unwrap_err();
        assert_eq!(
            error.code(),
            crate::error::Frontend::register(),
            "{declaration}"
        );
        assert_eq!(
            error.inner().source_ctx.focus.as_ref().unwrap().0,
            "https://example.org/root"
        );
    }
}

#[test]
fn temporal_future_and_interval_variables_cannot_capture_authored_names() {
    let source = dataset(
        r#"
        ex:root logic:until ex:right ; logic:untilLeft ex:left .
        ex:right logic:relation ex:arrived ; logic:argument ex:x .
        ex:left logic:relation ex:waiting ; logic:argument ex:y .
        ex:x logic:termIndex 0 ; logic:termVariable "__tw0" .
        ex:y logic:termIndex 0 ; logic:termVariable "__ti0" .
        "#,
    );
    let Formula::Exists { vars, body } = FormulaReader::new(source.as_ref())
        .read(&subject("root"))
        .unwrap()
    else {
        panic!("temporal witness")
    };
    assert_ne!(vars, vec!["__tw0"]);
    let Formula::And(outer) = *body else {
        panic!("witness guard")
    };
    let Formula::And(inner) = &outer[1] else {
        panic!("until obligations")
    };
    assert_eq!(
        inner[0],
        contextual_atom(
            "https://example.org/arrived",
            Term::Var(vars[0].clone()),
            Term::Var("__tw0".into())
        )
    );
    let Formula::Forall {
        vars: interval,
        body,
    } = &inner[1]
    else {
        panic!("maintained interval")
    };
    assert_ne!(interval, &vec!["__ti0"]);
    assert_ne!(interval, &vars);
    let Formula::Implies(_, maintained) = body.as_ref() else {
        panic!("interval guard")
    };
    assert_eq!(
        maintained.as_ref(),
        &contextual_atom(
            "https://example.org/waiting",
            Term::Var(interval[0].clone()),
            Term::Var("__ti0".into())
        )
    );
}

#[test]
fn temporal_and_modal_nesting_never_resets_the_selected_context() {
    use crate::frontend::{FINITE_AT_OR_AFTER, FINITE_NEXT};

    let source = dataset(
        r#"
        ex:root logic:next ex:box .
        ex:box logic:necessarily ex:future ; logic:overAccessibility logic:deonticallyIdeal .
        ex:future logic:eventually ex:atom .
        ex:atom logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .
    "#,
    );
    let context = Term::Iri("https://example.org/context".into());
    let actual = FormulaReader::new(source.as_ref())
        .read_in_context(&subject("root"), context.clone())
        .unwrap();
    let first = Term::Var("__tw0".into());
    let ideal = Term::Var("__w1".into());
    let future = Term::Var("__tw2".into());
    let expected = Formula::Exists {
        vars: vec!["__tw0".into()],
        body: Box::new(Formula::And(vec![
            contextual_atom(FINITE_NEXT, context, first.clone()),
            Formula::Forall {
                vars: vec!["__w1".into()],
                body: Box::new(Formula::Implies(
                    Box::new(contextual_atom(
                        &logic_iri("deonticallyIdeal"),
                        first,
                        ideal.clone(),
                    )),
                    Box::new(Formula::Exists {
                        vars: vec!["__tw2".into()],
                        body: Box::new(Formula::And(vec![
                            contextual_atom(FINITE_AT_OR_AFTER, ideal, future.clone()),
                            contextual_atom(
                                "https://example.org/p",
                                future,
                                Term::Iri("https://example.org/a".into()),
                            ),
                        ])),
                    }),
                )),
            },
        ])),
    };
    assert_eq!(actual, expected);
}

#[test]
fn named_annotation_formula_fields_preserve_source_selection_and_literal_facets() {
    use crate::frontend::{reconstruct_formula_in_context, reconstruct_formula_in_named_context};

    for duplicate in [false, true] {
        let mut builder = purrdf::RdfDatasetBuilder::new();
        let graph = builder.intern_iri("https://example.org/source");
        let root = builder.intern_iri("https://example.org/root");
        let relation = builder.intern_iri(&logic_iri("relation"));
        let predicate = builder.intern_iri("https://example.org/p");
        let argument = builder.intern_blank("argument", purrdf::BlankScope(31));
        let argument_link = builder.intern_iri(&logic_iri("argument"));
        let index = builder.intern_iri(&logic_iri("termIndex"));
        let zero = builder.intern_literal(purrdf::RdfLiteral::simple("0"));
        let value_link = builder.intern_iri(&logic_iri("termLiteral"));
        let literal = purrdf::RdfLiteral {
            lexical_form: "مرحبا".into(),
            datatype: None,
            language: Some("ar".into()),
            direction: Some(purrdf::RdfTextDirection::Rtl),
        };
        let value = builder.intern_literal(literal.clone());
        for (s, p, o) in [
            (root, relation, predicate),
            (root, argument_link, argument),
            (argument, index, zero),
            (argument, value_link, value),
        ] {
            builder.push_annotation_in_graph(s, p, o, Some(graph));
            if duplicate {
                builder.push_quad(s, p, o, Some(graph));
            }
        }
        let quoted = builder.intern_iri("https://example.org/quoted");
        let quoted_statement = builder.intern_triple(quoted, relation, predicate);
        let reifier = builder.intern_iri("https://example.org/quotation");
        builder.push_reifier_in_graph(reifier, quoted_statement, Some(graph));
        let negation = builder.intern_iri(&logic_iri("not"));
        builder.push_quad(root, negation, root, None);
        let source = builder.freeze().unwrap();
        let formula = reconstruct_formula_in_named_context(
            &source,
            "https://example.org/source",
            "https://example.org/root",
            "https://example.org/selected",
        )
        .unwrap();
        assert_eq!(
            formula,
            contextual_atom(
                "https://example.org/p",
                Term::Iri("https://example.org/selected".into()),
                Term::rdf_literal(literal).unwrap()
            )
        );
        assert!(
            reconstruct_formula_in_context(
                &source,
                "https://example.org/root",
                "https://example.org/selected"
            )
            .is_err(),
            "named annotations cannot satisfy a default-source request"
        );
        assert!(
            reconstruct_formula_in_named_context(
                &source,
                "https://example.org/source",
                "https://example.org/quoted",
                "https://example.org/selected"
            )
            .is_err(),
            "quoted formula structure is never asserted"
        );
    }
}

#[test]
fn contextual_requests_own_valid_and_malformed_formula_subtrees() {
    use crate::frontend::{FormulaDisposition, SourceNode, SourceUnitKind, StructuralSourceGraph};

    for (valid, typing) in [
        (false, "logic:instanceOf"),
        (true, "logic:instanceOf"),
        (false, "a"),
        (true, "a"),
    ] {
        let body = if valid {
            "ex:atom logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] ."
        } else {
            "ex:atom logic:relation ex:p ."
        };
        let source = dataset(&format!(
            r#"
            ex:request {typing} logic:ContextualEvaluationRequest ; logic:queryFormula ex:root ; logic:queryContext ex:selected .
            ex:root logic:instanceOf logic:Formula ; logic:next ex:atom .
            ex:atom logic:instanceOf logic:Formula . {body}
        "#
        ));
        let ownership = StructuralSourceGraph::new(&source);
        let root = SourceNode {
            term: subject_id(&source, &subject("root")).unwrap(),
            graph: None,
        };
        let atom = SourceNode {
            term: subject_id(&source, &subject("atom")).unwrap(),
            graph: None,
        };
        let request = SourceNode {
            term: subject_id(&source, &subject("request")).unwrap(),
            graph: None,
        };
        assert!(ownership.declares(request, SourceUnitKind::ContextualEvaluationRequest));
        assert!(ownership.formula_is_owned(root));
        assert!(ownership.formula_is_owned(atom));
        assert!(ownership.ownership_diagnostics(&source).is_empty());
        let mut reader = FormulaReader::new(source.as_ref());
        let mut diagnostics = Vec::new();
        let extracted = extract_formulas(&mut reader, &ownership, &mut diagnostics);
        assert!(
            extracted.formulas.is_empty(),
            "an owned question cannot become a universal assertion"
        );
        for node in [root, atom] {
            let lowered = extracted
                .lowerings
                .iter()
                .find(|entry| entry.source == node)
                .unwrap();
            if valid {
                assert_eq!(lowered.disposition, FormulaDisposition::ReadForOwner);
            } else {
                let FormulaDisposition::Malformed { diagnostic } = lowered.disposition else {
                    panic!("owned malformed syntax is retained")
                };
                assert_eq!(diagnostics[diagnostic].code, "MALFORMED_FORMULA");
            }
        }
        let (program, _) = crate::frontend::parse_logic_dataset(&source, None).unwrap();
        assert!(program.formulas.is_empty());
        assert!(
            program.axioms.is_empty(),
            "request control edges must remain source metadata"
        );
    }
}

#[test]
fn malformed_request_ownership_fails_without_promoting_its_question() {
    use crate::frontend::StructuralSourceGraph;

    let source = dataset(
        r#"
        ex:undeclared logic:queryFormula ex:root .
        ex:bad logic:instanceOf logic:ContextualEvaluationRequest ; logic:queryFormula "not a root" .
        ex:root logic:instanceOf logic:Formula ; logic:relation ex:p ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .
    "#,
    );
    let ownership = StructuralSourceGraph::new(&source);
    let diagnostics = ownership.ownership_diagnostics(&source);
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code == "UNDECLARED_SEMANTIC_OWNER")
    );
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code == "MALFORMED_SEMANTIC_TARGET")
    );
    let mut reader = FormulaReader::new(source.as_ref());
    assert!(
        extract_formulas(&mut reader, &ownership, &mut Vec::new())
            .formulas
            .is_empty()
    );
}

#[test]
fn a_named_request_cannot_take_ownership_of_a_default_formula_with_the_same_iri() {
    use crate::frontend::{SourceNode, StructuralSourceGraph};

    let source = purrdf::parse_dataset(
        br#"
        @prefix logic: <https://blackcatinformatics.ca/logic/> .
        @prefix ex: <https://example.org/> .
        ex:root logic:instanceOf logic:Formula ; logic:relation ex:default ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:a ] .
        ex:named {
            ex:request logic:instanceOf logic:ContextualEvaluationRequest ; logic:queryFormula ex:root ; logic:queryContext ex:selected .
            ex:root logic:instanceOf logic:Formula ; logic:relation ex:named ; logic:argument [ logic:termIndex 0 ; logic:termIri ex:b ] .
        }
        "#,
        "application/trig", None,
    ).unwrap();
    let ownership = StructuralSourceGraph::new(&source);
    let term = subject_id(&source, &subject("root")).unwrap();
    let graph = source.term_id_by_iri("https://example.org/named").unwrap();
    assert!(!ownership.formula_is_owned(SourceNode { term, graph: None }));
    assert!(ownership.formula_is_owned(SourceNode {
        term,
        graph: Some(graph)
    }));
    let mut reader = FormulaReader::new(source.as_ref());
    let extracted = extract_formulas(&mut reader, &ownership, &mut Vec::new());
    assert_eq!(extracted.formulas.len(), 1);
    assert_eq!(
        extracted.formulas[0].source,
        SourceNode { term, graph: None }
    );
    assert_eq!(
        extracted.formulas[0].formula,
        Formula::atom(
            Term::Iri("https://example.org/default".into()),
            vec![Term::Iri("https://example.org/a".into())]
        )
        .unwrap()
    );
}
