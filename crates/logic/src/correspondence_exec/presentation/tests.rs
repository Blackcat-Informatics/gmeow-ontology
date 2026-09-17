// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::ir::{Formula, Term};

fn context(standpoint: Option<&str>) -> PresentationContext {
    PresentationContext {
        answer: ResultContext {
            world: "urn:world".into(),
            standpoint: standpoint.map(str::to_owned),
            attributed: None,
            time: None,
            path: None,
        },
        module: None,
        modality: LogicModality::None,
    }
}

fn evidence(name: &str) -> Arc<PresentationEvidence> {
    let native = purrdf::parse_dataset(
        br#"
        @prefix ex: <urn:example:> .
        @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
        ex:empty {}
        ex:r rdf:reifies <<(ex:a ex:p ex:b)>> ; ex:source ex:editor .
    "#,
        "application/trig",
        None,
    )
    .unwrap();
    PresentationEvidence::new(
        TermValue::Iri(name.into()),
        Arc::clone(&native),
        native,
        PreservationClaim::unsupported_with(["urn:unprojected-constraint"]),
    )
    .unwrap()
}

fn symbol(name: &str, role: SymbolRole) -> PresentationSymbol {
    PresentationSymbol {
        context: 0,
        names: BTreeSet::from([TermValue::Iri(name.into())]),
        roles: BTreeSet::from([role]),
    }
}

fn body() -> Arc<SentenceBody> {
    SentenceBody::new(
        Arc::new(Formula::Atom {
            relation: Term::Iri("urn:p".into()),
            args: vec![Term::Iri("urn:x".into())],
        }),
        PresentationLimits::default(),
    )
    .unwrap()
}

fn sentence(
    body: &Arc<SentenceBody>,
    evidence: &Arc<PresentationEvidence>,
    sign: SentenceSign,
    kind: SentenceKind,
) -> PresentationSentence {
    PresentationSentence {
        body: Arc::clone(body),
        bindings: vec![0, 1],
        context: 0,
        kind,
        sign,
        evidence: Arc::clone(evidence),
    }
}

fn presentation(
    symbols: Vec<PresentationSymbol>,
    sentences: Vec<PresentationSentence>,
) -> Arc<FinitePresentation> {
    FinitePresentation::new(
        vec![context(None)],
        symbols,
        sentences,
        Arc::new(ReasoningContract::new()),
        PresentationLimits::default(),
    )
    .unwrap()
}

struct Span {
    left: PresentationMap,
    right: PresentationMap,
}

fn span() -> Span {
    let shared_body = body();
    let base = sentence(
        &shared_body,
        &evidence("urn:apex-claim"),
        SentenceSign::Positive,
        SentenceKind::Axiom,
    );
    let apex = presentation(
        vec![
            symbol("urn:p", SymbolRole::Relation(1)),
            symbol("urn:x", SymbolRole::Individual),
        ],
        vec![base.clone()],
    );
    let mut signature = apex.symbols.clone();
    // The same name occurs on two PRIVATE generators. It is not an apex map.
    signature.push(symbol("urn:private", SymbolRole::Individual));
    let a = presentation(
        signature.clone(),
        vec![
            base.clone(),
            sentence(
                &shared_body,
                &evidence("urn:left-claim"),
                SentenceSign::Positive,
                SentenceKind::Axiom,
            ),
            sentence(
                &shared_body,
                &evidence("urn:left-caveat"),
                SentenceSign::Negative,
                SentenceKind::Caveat,
            ),
        ],
    );
    let b = presentation(
        signature,
        vec![
            base,
            sentence(
                &shared_body,
                &evidence("urn:right-claim"),
                SentenceSign::Negative,
                SentenceKind::Axiom,
            ),
        ],
    );
    Span {
        left: PresentationMap::new(Arc::clone(&apex), a, vec![0, 1]).unwrap(),
        right: PresentationMap::new(apex, b, vec![0, 1]).unwrap(),
    }
}

fn pushout() -> CheckedPushout {
    let span = span();
    CheckedPushout::build(span.left, span.right, PresentationLimits::default()).unwrap()
}

fn reconstitute(
    base: &Arc<FinitePresentation>,
    contexts: Vec<PresentationContext>,
    symbols: Vec<PresentationSymbol>,
    sentences: Vec<PresentationSentence>,
) -> Arc<FinitePresentation> {
    FinitePresentation::new(
        contexts,
        symbols,
        sentences,
        Arc::clone(&base.contract),
        PresentationLimits::default(),
    )
    .unwrap()
}

#[test]
fn indexed_pushout_retains_contested_claims_caveats_and_native_complements() {
    let span = span();
    let originals: Vec<_> = span
        .left
        .target
        .sentences
        .iter()
        .chain(&span.right.target.sentences)
        .map(|sentence| Arc::clone(&sentence.evidence))
        .collect();
    let result =
        CheckedPushout::build(span.left, span.right, PresentationLimits::default()).unwrap();
    assert_eq!(result.output().symbols.len(), 4);
    assert_eq!(result.output().sentences.len(), 4);
    assert_ne!(
        result.left_injection().images()[2],
        result.right_injection().images()[2]
    );
    assert_eq!(
        result.output().symbols[2].names,
        result.output().symbols[3].names
    );
    for original in originals {
        let retained = &result
            .output()
            .sentences
            .iter()
            .find(|sentence| Arc::ptr_eq(&sentence.evidence, &original))
            .unwrap()
            .evidence;
        assert!(
            retained
                .provenance()
                .same_publication(original.provenance())
        );
        assert!(
            retained
                .complement()
                .same_publication(original.complement())
        );
        assert_eq!(retained.loss(), original.loss());
        assert_eq!(retained.origin(), original.origin());
    }
    assert!(
        result
            .output()
            .sentences
            .iter()
            .any(|s| s.sign == SentenceSign::Negative && s.kind == SentenceKind::Axiom)
    );
    assert!(
        result
            .output()
            .sentences
            .iter()
            .any(|s| s.sign == SentenceSign::Positive && s.kind == SentenceKind::Axiom)
    );
    assert!(
        result
            .output()
            .sentences
            .iter()
            .all(|s| Arc::ptr_eq(&s.body, &result.output().sentences[0].body))
    );
    assert_eq!(
        result.engine_descriptor(),
        crate::runtime::EngineContract::current().descriptor_hash
    );
}

#[test]
fn induced_factorization_is_total_unique_and_can_identify_private_generators() {
    let result = pushout();
    let identity = result
        .factor(result.left_injection(), result.right_injection())
        .unwrap();
    assert!(identity.agrees_with(&PresentationMap::identity(Arc::clone(result.output()))));
    let target = reconstitute(
        result.output(),
        result.output().contexts.clone(),
        result.output().symbols[..3].to_vec(),
        result.output().sentences.clone(),
    );
    let first = PresentationMap::new(
        Arc::clone(&result.left_embedding().target),
        Arc::clone(&target),
        vec![0, 1, 2],
    )
    .unwrap();
    let second = PresentationMap::new(
        Arc::clone(&result.right_embedding().target),
        target,
        vec![0, 1, 2],
    )
    .unwrap();
    let factor = result.factor(&first, &second).unwrap();
    assert_eq!(factor.images(), &[0, 1, 2, 2]);
    assert!(!factor.is_embedding());
    result
        .check_factorization(&first, &second, &factor)
        .unwrap();
    assert!(
        result
            .left_injection()
            .then(&factor)
            .unwrap()
            .agrees_with(&first)
    );
    assert!(
        result
            .right_injection()
            .then(&factor)
            .unwrap()
            .agrees_with(&second)
    );
    // An admitted automorphism of P is not a mediator for the original cocone.
    let swapped = PresentationMap::new(
        Arc::clone(result.output()),
        Arc::clone(result.output()),
        vec![0, 1, 3, 2],
    )
    .unwrap();
    assert!(
        result
            .check_factorization(result.left_injection(), result.right_injection(), &swapped)
            .is_err()
    );
}

#[test]
fn commuting_square_with_extra_identifications_is_not_a_pushout() {
    let result = pushout();
    let target = reconstitute(
        result.output(),
        result.output().contexts.clone(),
        result.output().symbols[..3].to_vec(),
        result.output().sentences.clone(),
    );
    let first = PresentationMap::new(
        Arc::clone(&result.left_embedding().target),
        Arc::clone(&target),
        vec![0, 1, 2],
    )
    .unwrap();
    let second = PresentationMap::new(
        Arc::clone(&result.right_embedding().target),
        target,
        vec![0, 1, 2],
    )
    .unwrap();
    let error = CheckedPushout::check(
        result.left_embedding().clone(),
        result.right_embedding().clone(),
        first,
        second,
    )
    .unwrap_err();
    assert!(
        error.to_string().contains("outside the declared apex"),
        "{error}"
    );
}

#[test]
fn commuting_square_with_extra_sentences_names_or_contexts_is_not_a_pushout() {
    for mutation in ["sentence", "name", "context", "generator"] {
        let result = pushout();
        let output = result.output();
        let mut contexts = output.contexts.clone();
        let mut symbols = output.symbols.clone();
        let mut sentences = output.sentences.clone();
        match mutation {
            "sentence" => {
                let mut extra = sentences[0].clone();
                extra.evidence = evidence("urn:unsupported-extra-axiom");
                sentences.push(extra);
            }
            "name" => {
                symbols[0]
                    .names
                    .insert(TermValue::Iri("urn:unwitnessed-alias".into()));
            }
            "context" => contexts.push(context(Some("urn:unwitnessed-standpoint"))),
            "generator" => {
                symbols.push(symbol("urn:unwitnessed-generator", SymbolRole::Individual))
            }
            _ => unreachable!(),
        }
        let target = reconstitute(output, contexts, symbols, sentences);
        let first = PresentationMap::new(
            Arc::clone(&result.left_embedding().target),
            Arc::clone(&target),
            result.left_injection().symbols.clone(),
        )
        .unwrap();
        let second = PresentationMap::new(
            Arc::clone(&result.right_embedding().target),
            target,
            result.right_injection().symbols.clone(),
        )
        .unwrap();
        assert!(
            CheckedPushout::check(
                result.left_embedding().clone(),
                result.right_embedding().clone(),
                first,
                second
            )
            .is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn maps_refuse_lost_signed_claims_or_replaced_evidence() {
    let result = pushout();
    let output = result.output();
    for mutation in ["delete", "sign", "kind", "evidence"] {
        let mut sentences = output.sentences.clone();
        match mutation {
            "delete" => {
                sentences.remove(0);
            }
            "sign" => sentences[0].sign = SentenceSign::Negative,
            "kind" => sentences[0].kind = SentenceKind::Caveat,
            "evidence" => sentences[0].evidence = evidence("urn:apex-claim"),
            _ => unreachable!(),
        }
        let target = reconstitute(
            output,
            output.contexts.clone(),
            output.symbols.clone(),
            sentences,
        );
        assert!(
            PresentationMap::new(Arc::clone(output), target, vec![0, 1, 2, 3]).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn empty_apex_keeps_standpoints_and_declared_empty_indices_separate() {
    let contract = Arc::new(ReasoningContract::new());
    let limits = PresentationLimits::default();
    let apex =
        FinitePresentation::new(vec![], vec![], vec![], Arc::clone(&contract), limits).unwrap();
    let mut a_contexts = vec![context(None), context(Some("urn:empty-index"))];
    let a = FinitePresentation::new(
        a_contexts.clone(),
        vec![symbol("urn:same-name", SymbolRole::Individual)],
        vec![],
        Arc::clone(&contract),
        limits,
    )
    .unwrap();
    let b = FinitePresentation::new(
        vec![context(Some("urn:observer"))],
        vec![symbol("urn:same-name", SymbolRole::Individual)],
        vec![],
        contract,
        limits,
    )
    .unwrap();
    assert!(PresentationMap::new(Arc::clone(&a), Arc::clone(&b), vec![0]).is_err());
    let result = CheckedPushout::build(
        PresentationMap::new(Arc::clone(&apex), Arc::clone(&a), vec![]).unwrap(),
        PresentationMap::new(apex, Arc::clone(&b), vec![]).unwrap(),
        limits,
    )
    .unwrap();
    a_contexts.push(context(Some("urn:observer")));
    assert_eq!(result.output().contexts, a_contexts);
    assert_eq!(result.output().symbols.len(), 2);
    assert_ne!(
        result.output().symbols[0].context,
        result.output().symbols[1].context
    );
}

fn renamed_formula(suffix: &str, variable: &str, lexical: &str) -> Formula {
    let atom = Formula::Atom {
        relation: Term::Iri(format!("urn:r{suffix}")),
        args: vec![
            Term::App {
                symbol: format!("urn:f{suffix}"),
                args: vec![Term::Var(variable.into())],
            },
            Term::Literal(purrdf::RdfLiteral::typed(
                lexical,
                "http://www.w3.org/2001/XMLSchema#integer",
            )),
        ],
    };
    Formula::Forall {
        vars: vec![variable.into()],
        body: Box::new(Formula::Iff(
            Box::new(atom.clone()),
            Box::new(Formula::Not(Box::new(atom))),
        )),
    }
}

#[test]
fn sentence_transport_uses_canonical_alpha_identity_without_rewriting_bodies_or_literals() {
    let limits = PresentationLimits::default();
    let original = SentenceBody::new(Arc::new(renamed_formula("", "x", "01")), limits).unwrap();
    let renamed =
        SentenceBody::new(Arc::new(renamed_formula("2", "renamed", "01")), limits).unwrap();
    let wrong = SentenceBody::new(Arc::new(renamed_formula("2", "renamed", "1")), limits).unwrap();
    assert_eq!(original.key(&[0, 1]), renamed.key(&[0, 1]));
    assert_ne!(original.key(&[0, 1]), wrong.key(&[0, 1]));
    assert_ne!(original.key(&[0, 1]), original.key(&[1, 0]));
    let signature = original
        .symbols
        .iter()
        .map(|(name, roles)| PresentationSymbol {
            context: 0,
            names: BTreeSet::from([TermValue::Iri(name.clone())]),
            roles: roles.clone(),
        })
        .collect::<Vec<_>>();
    let claim = sentence(
        &original,
        &evidence("urn:quantified-law"),
        SentenceSign::Positive,
        SentenceKind::Axiom,
    );
    let source = presentation(signature.clone(), vec![claim.clone()]);
    let mut translated = claim;
    translated.body = renamed;
    let target = presentation(signature, vec![translated]);
    PresentationMap::new(source, target, vec![0, 1]).unwrap();
}

#[test]
fn selected_contract_signature_roles_and_exact_publications_are_mandatory() {
    let result = pushout();
    let output = result.output();
    let clone = reconstitute(
        output,
        output.contexts.clone(),
        output.symbols.clone(),
        output.sentences.clone(),
    );
    assert!(
        PresentationMap::identity(Arc::clone(output))
            .then(&PresentationMap::identity(clone))
            .is_err()
    );
    let mut contract = (*output.contract).clone();
    contract.truth_algebra = Some("urn:other-truth-algebra".into());
    let changed = FinitePresentation::new(
        output.contexts.clone(),
        output.symbols.clone(),
        output.sentences.clone(),
        Arc::new(contract),
        PresentationLimits::default(),
    )
    .unwrap();
    assert!(PresentationMap::new(Arc::clone(output), changed, vec![0, 1, 2, 3]).is_err());
    let mut symbols = output.symbols.clone();
    symbols[2].roles.insert(SymbolRole::Relation(3));
    let changed = reconstitute(
        output,
        output.contexts.clone(),
        symbols,
        output.sentences.clone(),
    );
    assert!(PresentationMap::new(Arc::clone(output), changed, vec![0, 1, 2, 3]).is_err());
    assert!(PresentationMap::new(Arc::clone(output), Arc::clone(output), vec![0, 1]).is_err());
    assert!(
        PresentationMap::new(Arc::clone(output), Arc::clone(output), vec![0, 1, 2, 4]).is_err()
    );
}

#[test]
fn bounds_and_closed_sentence_admission_refuse_without_partial_publication() {
    let span = span();
    let a = Arc::clone(&span.left.target);
    let limits = PresentationLimits {
        max_symbols: 1,
        ..PresentationLimits::default()
    };
    assert!(CheckedPushout::build(span.left, span.right, limits).is_err());
    assert_eq!(a.symbols.len(), 3);
    assert_eq!(a.sentences.len(), 3);
    let free = Formula::Atom {
        relation: Term::Iri("urn:p".into()),
        args: vec![Term::Var("unbound".into())],
    };
    assert!(SentenceBody::new(Arc::new(free), PresentationLimits::default()).is_err());
    assert!(
        SentenceBody::new(
            Arc::clone(body().formula()),
            PresentationLimits {
                max_formula_nodes: 1,
                ..PresentationLimits::default()
            }
        )
        .is_err()
    );
    let deep = Formula::Not(Box::new(Formula::Not(Box::new(
        body().formula().as_ref().clone(),
    ))));
    assert!(
        SentenceBody::new(
            Arc::new(deep),
            PresentationLimits {
                max_formula_depth: 1,
                ..PresentationLimits::default()
            }
        )
        .is_err()
    );
}

#[test]
fn reversing_the_span_induces_inverse_maps_between_pushouts() {
    let span = span();
    let limits = PresentationLimits::default();
    let forward = CheckedPushout::build(span.left.clone(), span.right.clone(), limits).unwrap();
    let reverse = CheckedPushout::build(span.right, span.left, limits).unwrap();
    let to_reverse = forward
        .factor(reverse.right_injection(), reverse.left_injection())
        .unwrap();
    let to_forward = reverse
        .factor(forward.right_injection(), forward.left_injection())
        .unwrap();
    assert!(
        to_reverse
            .then(&to_forward)
            .unwrap()
            .agrees_with(&PresentationMap::identity(Arc::clone(forward.output())))
    );
    assert!(
        to_forward
            .then(&to_reverse)
            .unwrap()
            .agrees_with(&PresentationMap::identity(Arc::clone(reverse.output())))
    );
}

#[test]
fn noncommuting_cocones_and_nonembedding_spans_are_refused() {
    let generator = symbol("urn:shared-name", SymbolRole::Individual);
    let apex = presentation(vec![generator.clone()], vec![]);
    let input = presentation(vec![generator.clone(), generator.clone()], vec![]);
    let left = PresentationMap::new(Arc::clone(&apex), Arc::clone(&input), vec![0]).unwrap();
    let right = left.clone();
    let first = PresentationMap::identity(Arc::clone(&input));
    let second = PresentationMap::new(Arc::clone(&input), Arc::clone(&input), vec![1, 0]).unwrap();
    let result =
        CheckedPushout::build(left.clone(), right.clone(), PresentationLimits::default()).unwrap();
    assert!(
        result
            .factor(&first, &second)
            .unwrap_err()
            .to_string()
            .contains("disagrees on the common apex")
    );
    assert!(
        CheckedPushout::check(left, right, first, second)
            .unwrap_err()
            .to_string()
            .contains("does not commute")
    );
    let quotient = PresentationMap::new(Arc::clone(&input), Arc::clone(&apex), vec![0, 0]).unwrap();
    assert!(
        CheckedPushout::build(quotient.clone(), quotient, PresentationLimits::default())
            .unwrap_err()
            .to_string()
            .contains("typed embeddings")
    );
    let different_apex = presentation(vec![generator], vec![]);
    let left = PresentationMap::new(apex, Arc::clone(&input), vec![0]).unwrap();
    let right = PresentationMap::new(different_apex, input, vec![0]).unwrap();
    assert!(
        CheckedPushout::build(left, right, PresentationLimits::default())
            .unwrap_err()
            .to_string()
            .contains("exact common apex")
    );
}

#[test]
fn every_context_coordinate_and_literal_evidence_survives_transport() {
    let base = presentation(vec![symbol("urn:x", SymbolRole::Individual)], vec![]);
    for coordinate in [
        "world",
        "standpoint",
        "attributed",
        "time",
        "path",
        "module",
        "modality",
    ] {
        let mut changed = base.contexts.clone();
        match coordinate {
            "world" => changed[0].answer.world = "urn:another-world".into(),
            "standpoint" => changed[0].answer.standpoint = Some("urn:another-standpoint".into()),
            "attributed" => changed[0].answer.attributed = Some("urn:attributed-context".into()),
            "time" => changed[0].answer.time = Some("2026-09-10".into()),
            "path" => changed[0].answer.path = Some("urn:another-path".into()),
            "module" => changed[0].module = Some("urn:another-module".into()),
            "modality" => changed[0].modality = LogicModality::Deontic,
            _ => unreachable!(),
        }
        let target = reconstitute(&base, changed, base.symbols.clone(), vec![]);
        assert!(
            PresentationMap::new(Arc::clone(&base), target, vec![0]).is_err(),
            "{coordinate}"
        );
    }
    let formula = renamed_formula("", "x", "01");
    assert_eq!(
        formula.content_key(),
        formula.content_key_with_symbols(&BTreeMap::new())
    );
    let map = BTreeMap::from([(
        "http://www.w3.org/2001/XMLSchema#integer",
        "urn:another-datatype".into(),
    )]);
    assert_eq!(
        formula.content_key(),
        formula.content_key_with_symbols(&map),
        "a signature map must not rename literal datatypes"
    );
}
