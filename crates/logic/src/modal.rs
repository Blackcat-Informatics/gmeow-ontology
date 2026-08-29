// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared typed modal-evaluation kernel.
//!
//! This module owns the bounded Kripke evaluation of `logic:necessarily` / `logic:possibly`
//! over the finite materialized world set. Callers provide fact-like rows through
//! [`ModalFact`]; the kernel resolves typed modal frames, hard-fails malformed frames,
//! and returns typed verdict rows the caller can project onto its own surface.

use std::collections::{BTreeMap, BTreeSet};

use crate::provenance::mint_derivation_id;

fn modal_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason { detail })
}

pub(crate) const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
pub(crate) const MODAL_RULE_IRI: &str =
    "https://blackcatinformatics.ca/logic/rule/modal-evaluation";
pub(crate) const NECESSARILY: &str = "https://blackcatinformatics.ca/logic/necessarily";
pub(crate) const POSSIBLY: &str = "https://blackcatinformatics.ca/logic/possibly";
pub(crate) const OVER_ACCESSIBILITY: &str =
    "https://blackcatinformatics.ca/logic/overAccessibility";
pub(crate) const MODAL_EVAL_WORLD: &str = "https://blackcatinformatics.ca/logic/modalEvalWorld";
pub(crate) const ATOM_SUBJECT: &str = "https://blackcatinformatics.ca/logic/atomSubject";
pub(crate) const ATOM_PREDICATE: &str = "https://blackcatinformatics.ca/logic/atomPredicate";
pub(crate) const ATOM_OBJECT: &str = "https://blackcatinformatics.ca/logic/atomObject";
pub(crate) const ACCESSIBLE_FROM: &str = "https://blackcatinformatics.ca/logic/accessibleFrom";
pub(crate) const TYPED_ACCESSIBILITY: [&str; 6] = [
    "https://blackcatinformatics.ca/logic/epistemicallyPossible",
    "https://blackcatinformatics.ca/logic/doxasticallyAccessible",
    "https://blackcatinformatics.ca/logic/deonticallyIdeal",
    "https://blackcatinformatics.ca/logic/temporallySucceeds",
    "https://blackcatinformatics.ca/logic/counterfactuallyCloser",
    "https://blackcatinformatics.ca/gmeow/sharpens",
];
pub(crate) const DEONTICALLY_IDEAL: &str = "https://blackcatinformatics.ca/logic/deonticallyIdeal";
pub(crate) const MODAL_NECESSITY_HOLDS: &str =
    "https://blackcatinformatics.ca/logic/modalNecessityHolds";
pub(crate) const MODAL_NECESSITY_FAILS: &str =
    "https://blackcatinformatics.ca/logic/modalNecessityFails";
pub(crate) const MODAL_NECESSITY_UNDETERMINED: &str =
    "https://blackcatinformatics.ca/logic/modalNecessityUndetermined";
pub(crate) const MODAL_POSSIBILITY_HOLDS: &str =
    "https://blackcatinformatics.ca/logic/modalPossibilityHolds";
pub(crate) const MODAL_POSSIBILITY_FAILS: &str =
    "https://blackcatinformatics.ca/logic/modalPossibilityFails";
pub(crate) const MODAL_COUNTEREXAMPLE_WORLD: &str =
    "https://blackcatinformatics.ca/logic/modalCounterexampleWorld";

pub(crate) trait ModalFact {
    fn graph(&self) -> &str;
    fn subject(&self) -> &str;
    fn predicate(&self) -> &str;
    fn object(&self) -> &str;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModalOp {
    Box,
    Diamond,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModalFrame {
    pub(crate) formula: String,
    pub(crate) op: ModalOp,
    pub(crate) body: String,
    pub(crate) relation: String,
    pub(crate) w0: String,
    pub(crate) atom_s: String,
    pub(crate) atom_p: String,
    pub(crate) atom_o: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModalVerdict {
    pub(crate) graph: String,
    pub(crate) subject: String,
    pub(crate) predicate: String,
    pub(crate) object: String,
    pub(crate) rule_iri: String,
    pub(crate) premises: Vec<(String, String, String)>,
    pub(crate) source_quad_ids: Vec<String>,
    pub(crate) derivation_id: String,
}

#[derive(Debug, Default)]
struct ModalFrameIndexes {
    nec_body: BTreeMap<String, BTreeSet<String>>,
    pos_body: BTreeMap<String, BTreeSet<String>>,
    over: BTreeMap<String, BTreeSet<String>>,
    eval_world: BTreeMap<String, BTreeSet<String>>,
    atom_s: BTreeMap<String, BTreeSet<String>>,
    atom_p: BTreeMap<String, BTreeSet<String>>,
    atom_o: BTreeMap<String, BTreeSet<String>>,
    typed_relations: BTreeSet<&'static str>,
}

pub(crate) fn evaluate<T: ModalFact>(facts: &[T]) -> gmeow_errors::Result<Vec<ModalVerdict>> {
    let mut frame_indexes = ModalFrameIndexes {
        typed_relations: TYPED_ACCESSIBILITY.iter().copied().collect(),
        ..ModalFrameIndexes::default()
    };
    let mut access: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();

    for fact in facts {
        match fact.predicate() {
            NECESSARILY => {
                frame_indexes
                    .nec_body
                    .entry(fact.subject().to_owned())
                    .or_default()
                    .insert(normalize_object(fact.object()).to_owned());
            }
            POSSIBLY => {
                frame_indexes
                    .pos_body
                    .entry(fact.subject().to_owned())
                    .or_default()
                    .insert(normalize_object(fact.object()).to_owned());
            }
            OVER_ACCESSIBILITY => {
                frame_indexes
                    .over
                    .entry(fact.subject().to_owned())
                    .or_default()
                    .insert(normalize_object(fact.object()).to_owned());
            }
            MODAL_EVAL_WORLD => {
                frame_indexes
                    .eval_world
                    .entry(fact.subject().to_owned())
                    .or_default()
                    .insert(normalize_object(fact.object()).to_owned());
            }
            ATOM_SUBJECT => {
                frame_indexes
                    .atom_s
                    .entry(fact.subject().to_owned())
                    .or_default()
                    .insert(normalize_object(fact.object()).to_owned());
            }
            ATOM_PREDICATE => {
                frame_indexes
                    .atom_p
                    .entry(fact.subject().to_owned())
                    .or_default()
                    .insert(normalize_object(fact.object()).to_owned());
            }
            ATOM_OBJECT => {
                frame_indexes
                    .atom_o
                    .entry(fact.subject().to_owned())
                    .or_default()
                    .insert(normalize_object(fact.object()).to_owned());
            }
            predicate if frame_indexes.typed_relations.contains(predicate) => {
                let source = iri_binding(fact.subject(), "typed accessibility edge source world")?;
                let target = iri_binding(fact.object(), "typed accessibility edge target world")?;
                access
                    .entry((source, fact.predicate().to_owned()))
                    .or_default()
                    .insert(target);
            }
            _ => {}
        }
    }

    let frames = resolve_frames(&frame_indexes)?;
    if frames.is_empty() {
        return Ok(Vec::new());
    }

    // The production closure can contain millions of rows. Retain only the facts
    // whose ground atom is named by a resolved modal frame instead of cloning the
    // complete closure into a second ordered set. The frame set is fully validated
    // before this scan, so malformed input still publishes no partial verdicts.
    let required_atoms: BTreeSet<(&str, &str, &str)> = frames
        .iter()
        .map(|frame| {
            (
                frame.atom_s.as_str(),
                frame.atom_p.as_str(),
                frame.atom_o.as_str(),
            )
        })
        .collect();
    let mut presence: BTreeSet<(String, String, String, String)> = BTreeSet::new();
    for fact in facts {
        let object = normalize_object(fact.object());
        if required_atoms.contains(&(fact.subject(), fact.predicate(), object)) {
            presence.insert((
                fact.graph().to_owned(),
                fact.subject().to_owned(),
                fact.predicate().to_owned(),
                object.to_owned(),
            ));
        }
    }
    drop(required_atoms);

    let mut verdicts = Vec::new();
    for frame in frames {
        let accessible: Vec<String> = access
            .get(&(frame.w0.clone(), frame.relation.clone()))
            .map(|worlds| worlds.iter().cloned().collect())
            .unwrap_or_default();
        let atom_present = |world: &str| {
            presence.contains(&(
                world.to_owned(),
                frame.atom_s.clone(),
                frame.atom_p.clone(),
                frame.atom_o.clone(),
            ))
        };
        let body_premise = (
            frame.atom_s.clone(),
            frame.atom_p.clone(),
            n3(&frame.atom_o),
        );
        let body_reifier = triple_reifier(&body_premise.0, &body_premise.1, &frame.atom_o)?;

        match frame.op {
            ModalOp::Box => {
                if accessible.is_empty() {
                    let predicate = if frame.relation == DEONTICALLY_IDEAL {
                        MODAL_NECESSITY_UNDETERMINED
                    } else {
                        MODAL_NECESSITY_HOLDS
                    };
                    verdicts.push(verdict(
                        &frame,
                        predicate,
                        frame.body.clone(),
                        vec![body_premise],
                        vec![body_reifier],
                    )?);
                } else if let Some(witness) = accessible.iter().find(|world| !atom_present(world)) {
                    verdicts.push(verdict(
                        &frame,
                        MODAL_NECESSITY_FAILS,
                        frame.body.clone(),
                        vec![body_premise.clone()],
                        vec![body_reifier.clone()],
                    )?);
                    let access_premise = (frame.w0.clone(), frame.relation.clone(), n3(witness));
                    let access_reifier =
                        triple_reifier(&frame.w0, &frame.relation, witness.as_str())?;
                    verdicts.push(verdict(
                        &frame,
                        MODAL_COUNTEREXAMPLE_WORLD,
                        witness.clone(),
                        vec![body_premise, access_premise],
                        vec![body_reifier, access_reifier],
                    )?);
                } else {
                    verdicts.push(verdict(
                        &frame,
                        MODAL_NECESSITY_HOLDS,
                        frame.body.clone(),
                        vec![body_premise],
                        vec![body_reifier],
                    )?);
                }
            }
            ModalOp::Diamond => {
                let predicate = if accessible.iter().any(|world| atom_present(world)) {
                    MODAL_POSSIBILITY_HOLDS
                } else {
                    MODAL_POSSIBILITY_FAILS
                };
                verdicts.push(verdict(
                    &frame,
                    predicate,
                    frame.body.clone(),
                    vec![body_premise],
                    vec![body_reifier],
                )?);
            }
        }
    }
    Ok(verdicts)
}

fn resolve_frames(indexes: &ModalFrameIndexes) -> gmeow_errors::Result<Vec<ModalFrame>> {
    let ModalFrameIndexes {
        nec_body,
        pos_body,
        over,
        eval_world,
        atom_s,
        atom_p,
        atom_o,
        typed_relations,
    } = indexes;
    let mut formula_nodes: BTreeSet<String> = BTreeSet::new();
    formula_nodes.extend(nec_body.keys().cloned());
    formula_nodes.extend(pos_body.keys().cloned());
    formula_nodes.extend(over.keys().cloned());
    formula_nodes.extend(eval_world.keys().cloned());

    for (formula, bodies) in nec_body.iter().chain(pos_body) {
        if let Some(body) = bodies.iter().find(|body| formula_nodes.contains(*body)) {
            return Err(modal_err(format!(
                "modal body {body} of formula {formula} is itself a modal formula; the modal \
                 body is scoped to a single ground atom, not a nested modal"
            )));
        }
    }

    let mut frames = Vec::new();
    for formula in formula_nodes {
        iri_binding(&formula, "modal formula")?;
        let has_nec = nec_body.contains_key(&formula);
        let has_pos = pos_body.contains_key(&formula);
        if has_nec && has_pos {
            return Err(modal_err(format!(
                "modal formula {formula} carries both logic:necessarily (□) and \
                 logic:possibly (◇); a modal node pins exactly one operator"
            )));
        }
        if !has_nec && !has_pos {
            return Err(modal_err(format!(
                "modal formula {formula} carries frame metadata but no logic:necessarily (□) \
                 or logic:possibly (◇) operator"
            )));
        }
        let (op, bodies) = if has_nec {
            (ModalOp::Box, &nec_body[&formula])
        } else {
            (ModalOp::Diamond, &pos_body[&formula])
        };
        if bodies.len() != 1 {
            return Err(modal_err(format!(
                "modal formula {formula} carries {} body formulae; a modal node pins exactly \
                 one body",
                bodies.len()
            )));
        }
        let body = iri_binding(
            bodies.iter().next().expect("one body"),
            "modal body formula",
        )?;

        let relation = iri_binding(
            &exact_one(over.get(&formula), || {
                format!(
                    "modal formula {formula} must carry exactly one logic:overAccessibility \
                 relation (found {})",
                    over.get(&formula).map_or(0, BTreeSet::len)
                )
            })?,
            "modal accessibility relation",
        )?;
        if !typed_relations.contains(relation.as_str()) {
            let why = if relation == ACCESSIBLE_FROM {
                " (the bare logic:accessibleFrom superproperty is prose-only and licenses \
                 no modal translation)"
            } else if relation.starts_with(&format!("{GMEOW_NS}modalForce")) {
                " (a gmeow:modalForce* term is a claim's modal force, not an accessibility \
                 relation)"
            } else {
                ""
            };
            return Err(modal_err(format!(
                "modal formula {formula} is translated over {relation}, which is not one of the \
                 six typed accessibility relations{why}"
            )));
        }

        let w0 = iri_binding(
            &exact_one(eval_world.get(&formula), || {
                format!(
                    "modal formula {formula} must carry exactly one logic:modalEvalWorld \
                 evaluation world (found {})",
                    eval_world.get(&formula).map_or(0, BTreeSet::len)
                )
            })?,
            "modal evaluation world",
        )?;

        frames.push(ModalFrame {
            formula: formula.clone(),
            op,
            body: body.clone(),
            relation,
            w0,
            atom_s: single_atom_binding(atom_s, &body, ATOM_SUBJECT, &formula)?,
            atom_p: single_atom_binding(atom_p, &body, ATOM_PREDICATE, &formula)?,
            atom_o: single_atom_binding(atom_o, &body, ATOM_OBJECT, &formula)?,
        });
    }
    Ok(frames)
}

fn single_atom_binding(
    index: &BTreeMap<String, BTreeSet<String>>,
    body: &str,
    predicate: &str,
    formula: &str,
) -> gmeow_errors::Result<String> {
    match index.get(body).map(|values| (values.len(), values)) {
        Some((1, values)) => iri_binding(
            values.iter().next().expect("one binding"),
            "modal ground-atom binding",
        ),
        other => Err(modal_err(format!(
            "modal body {body} of formula {formula} must carry exactly one {predicate} \
             ground-atom binding (found {})",
            other.map_or(0, |(count, _)| count)
        ))),
    }
}

fn exact_one<F>(values: Option<&BTreeSet<String>>, err: F) -> gmeow_errors::Result<String>
where
    F: FnOnce() -> String,
{
    match values {
        Some(set) if set.len() == 1 => Ok(set.iter().next().expect("one value").clone()),
        _ => Err(modal_err(err())),
    }
}

fn verdict(
    frame: &ModalFrame,
    predicate: &str,
    object: String,
    premises: Vec<(String, String, String)>,
    sources: Vec<String>,
) -> gmeow_errors::Result<ModalVerdict> {
    let refs: Vec<&str> = sources.iter().map(String::as_str).collect();
    let derivation_id = mint_derivation_id(MODAL_RULE_IRI, &refs);
    Ok(ModalVerdict {
        graph: frame.w0.clone(),
        subject: frame.formula.clone(),
        predicate: predicate.to_owned(),
        object,
        rule_iri: MODAL_RULE_IRI.to_owned(),
        premises,
        source_quad_ids: sources,
        derivation_id,
    })
}

fn normalize_object(object: &str) -> &str {
    object
        .strip_prefix('<')
        .and_then(|inner| inner.strip_suffix('>'))
        .unwrap_or(object)
}

fn iri_binding(value: &str, role: &str) -> gmeow_errors::Result<String> {
    let iri = normalize_object(value);
    if iri.is_empty()
        || !iri.contains(':')
        || iri.starts_with('"')
        || iri.starts_with("_:")
        || iri.starts_with("<<")
        || iri.chars().any(char::is_whitespace)
    {
        return Err(modal_err(format!(
            "{role} must be an IRI in the bounded modal frame, found {value}"
        )));
    }
    Ok(iri.to_owned())
}

fn n3(iri: &str) -> String {
    format!("<{iri}>")
}

fn triple_reifier(subject: &str, predicate: &str, object: &str) -> gmeow_errors::Result<String> {
    let subject = purrdf::TermValue::iri(subject);
    let object = purrdf::TermValue::iri(object);
    crate::provenance::mint_reifier(&subject, predicate, &object)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone)]
    struct Fact {
        graph: String,
        subject: String,
        predicate: String,
        object: String,
    }

    impl ModalFact for Fact {
        fn graph(&self) -> &str {
            &self.graph
        }

        fn subject(&self) -> &str {
            &self.subject
        }

        fn predicate(&self) -> &str {
            &self.predicate
        }

        fn object(&self) -> &str {
            &self.object
        }
    }

    fn fact(graph: &str, subject: &str, predicate: &str, object: &str) -> Fact {
        Fact {
            graph: graph.to_owned(),
            subject: subject.to_owned(),
            predicate: predicate.to_owned(),
            object: object.to_owned(),
        }
    }

    fn modal_frame_at(base: &str, op: &str, relation: &str, atom_worlds: &[&str]) -> Vec<Fact> {
        let frame = format!("{base}/frame");
        let mut facts = vec![
            fact(
                &frame,
                &format!("{base}/F"),
                &format!("https://blackcatinformatics.ca/logic/{op}"),
                &format!("{base}/B"),
            ),
            fact(&frame, &format!("{base}/F"), OVER_ACCESSIBILITY, relation),
            fact(
                &frame,
                &format!("{base}/F"),
                MODAL_EVAL_WORLD,
                &format!("{base}/w0"),
            ),
            fact(
                &frame,
                &format!("{base}/B"),
                ATOM_SUBJECT,
                &format!("{base}/a"),
            ),
            fact(
                &frame,
                &format!("{base}/B"),
                ATOM_PREDICATE,
                &format!("{base}/knows"),
            ),
            fact(
                &frame,
                &format!("{base}/B"),
                ATOM_OBJECT,
                &format!("{base}/b"),
            ),
            fact(
                &frame,
                &format!("{base}/w0"),
                relation,
                &format!("{base}/w1"),
            ),
            fact(
                &frame,
                &format!("{base}/w0"),
                relation,
                &format!("{base}/w2"),
            ),
        ];
        for world in atom_worlds {
            facts.push(fact(
                &format!("{base}/{world}"),
                &format!("{base}/a"),
                &format!("{base}/knows"),
                &format!("{base}/b"),
            ));
        }
        facts
    }

    fn modal_frame(op: &str, relation: &str, atom_worlds: &[&str]) -> Vec<Fact> {
        modal_frame_at("https://example.org/modal", op, relation, atom_worlds)
    }

    fn without_access_edges(mut facts: Vec<Fact>, relation: &str) -> Vec<Fact> {
        facts.retain(|fact| {
            fact.subject != "https://example.org/modal/w0" || fact.predicate != relation
        });
        facts
    }

    fn verdict_with<'a>(verdicts: &'a [ModalVerdict], predicate: &str) -> &'a ModalVerdict {
        verdicts
            .iter()
            .find(|verdict| verdict.predicate == predicate)
            .expect("expected modal verdict")
    }

    #[test]
    fn all_six_typed_relations_drive_both_modal_operators() {
        for relation in TYPED_ACCESSIBILITY {
            let box_verdicts = evaluate(&modal_frame("necessarily", relation, &["w1", "w2"]))
                .expect("typed necessity evaluation");
            assert_eq!(
                verdict_with(&box_verdicts, MODAL_NECESSITY_HOLDS).object,
                "https://example.org/modal/B",
                "necessity must use {relation}"
            );

            let diamond_verdicts = evaluate(&modal_frame("possibly", relation, &["w1"]))
                .expect("typed possibility evaluation");
            assert_eq!(
                verdict_with(&diamond_verdicts, MODAL_POSSIBILITY_HOLDS).object,
                "https://example.org/modal/B",
                "possibility must use {relation}"
            );
        }
    }

    #[test]
    fn box_holds_when_every_accessible_world_has_the_atom() {
        let verdicts = evaluate(&modal_frame(
            "necessarily",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            &["w1", "w2"],
        ))
        .expect("modal evaluation");
        assert!(verdicts.iter().any(|verdict| {
            verdict.predicate == MODAL_NECESSITY_HOLDS
                && verdict.graph == "https://example.org/modal/w0"
                && verdict.object == "https://example.org/modal/B"
        }));
    }

    #[test]
    fn box_failure_emits_a_counterexample_world() {
        let verdicts = evaluate(&modal_frame(
            "necessarily",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            &["w1"],
        ))
        .expect("modal evaluation");
        assert!(
            verdicts
                .iter()
                .any(|verdict| verdict.predicate == MODAL_NECESSITY_FAILS)
        );
        let counterexample = verdict_with(&verdicts, MODAL_COUNTEREXAMPLE_WORLD);
        assert_eq!(counterexample.object, "https://example.org/modal/w2");
        assert_eq!(counterexample.premises.len(), 2);
        assert_eq!(counterexample.source_quad_ids.len(), 2);
    }

    #[test]
    fn deontic_empty_accessible_set_is_undetermined() {
        let facts = without_access_edges(
            modal_frame("necessarily", DEONTICALLY_IDEAL, &[]),
            DEONTICALLY_IDEAL,
        );
        let verdicts = evaluate(&facts).expect("modal evaluation");
        assert!(
            verdicts
                .iter()
                .any(|verdict| verdict.predicate == MODAL_NECESSITY_UNDETERMINED)
        );
    }

    #[test]
    fn non_deontic_empty_accessible_set_is_vacuously_true() {
        let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
        let facts = without_access_edges(modal_frame("necessarily", relation, &[]), relation);
        let verdicts = evaluate(&facts).expect("modal evaluation");
        assert!(
            verdicts
                .iter()
                .any(|verdict| verdict.predicate == MODAL_NECESSITY_HOLDS)
        );
    }

    #[test]
    fn diamond_fails_when_no_accessible_world_has_the_atom() {
        let verdicts = evaluate(&modal_frame(
            "possibly",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            &[],
        ))
        .expect("modal evaluation");
        assert!(
            verdicts
                .iter()
                .any(|verdict| verdict.predicate == MODAL_POSSIBILITY_FAILS)
        );
    }

    #[test]
    fn verdict_identity_carries_the_exact_rule_and_ordered_reifier_recipe() {
        let verdicts = evaluate(&modal_frame(
            "necessarily",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            &["w1"],
        ))
        .expect("modal evaluation");
        let verdict = verdict_with(&verdicts, MODAL_COUNTEREXAMPLE_WORLD);
        let body = triple_reifier(
            "https://example.org/modal/a",
            "https://example.org/modal/knows",
            "https://example.org/modal/b",
        )
        .expect("body reifier");
        let access = triple_reifier(
            "https://example.org/modal/w0",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            "https://example.org/modal/w2",
        )
        .expect("access reifier");
        assert_eq!(verdict.rule_iri, MODAL_RULE_IRI);
        assert_eq!(verdict.source_quad_ids, vec![body, access]);
        let sources: Vec<&str> = verdict.source_quad_ids.iter().map(String::as_str).collect();
        assert_eq!(
            verdict.derivation_id,
            mint_derivation_id(MODAL_RULE_IRI, &sources)
        );
        assert_eq!(verdict.graph, "https://example.org/modal/w0");
        assert_eq!(verdict.subject, "https://example.org/modal/F");
    }

    #[test]
    fn malformed_frame_hard_fails_on_bare_accessible_from() {
        let err = evaluate(&modal_frame("necessarily", ACCESSIBLE_FROM, &[])).unwrap_err();
        assert!(err.message().contains("prose-only"), "got: {err}");
    }

    #[test]
    fn malformed_frame_hard_fails_on_claim_modal_force_relation() {
        let err = evaluate(&modal_frame(
            "necessarily",
            "https://blackcatinformatics.ca/gmeow/modalForceNecessary",
            &[],
        ))
        .unwrap_err();
        assert!(err.message().contains("modal force"), "got: {err}");
    }

    #[test]
    fn malformed_frame_hard_fails_on_missing_or_duplicate_slots() {
        let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
        let base = modal_frame("necessarily", relation, &["w1", "w2"]);

        for (missing_predicate, expected) in [
            (OVER_ACCESSIBILITY, "overAccessibility"),
            (MODAL_EVAL_WORLD, "modalEvalWorld"),
            (ATOM_SUBJECT, ATOM_SUBJECT),
            (ATOM_PREDICATE, ATOM_PREDICATE),
            (ATOM_OBJECT, ATOM_OBJECT),
        ] {
            let mut facts = base.clone();
            facts.retain(|fact| fact.predicate != missing_predicate);
            let err = evaluate(&facts).unwrap_err();
            assert!(err.message().contains(expected), "got: {err}");
        }

        let mut duplicate_body = base.clone();
        duplicate_body.push(fact(
            "https://example.org/modal/frame",
            "https://example.org/modal/F",
            NECESSARILY,
            "https://example.org/modal/B2",
        ));
        let err = evaluate(&duplicate_body).unwrap_err();
        assert!(err.message().contains("2 body"), "got: {err}");

        let mut duplicate_relation = base;
        duplicate_relation.push(fact(
            "https://example.org/modal/frame",
            "https://example.org/modal/F",
            OVER_ACCESSIBILITY,
            "https://blackcatinformatics.ca/logic/doxasticallyAccessible",
        ));
        let err = evaluate(&duplicate_relation).unwrap_err();
        assert!(err.message().contains("found 2"), "got: {err}");
    }

    #[test]
    fn malformed_frame_hard_fails_without_exactly_one_operator() {
        let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
        let mut no_operator = modal_frame("necessarily", relation, &["w1", "w2"]);
        no_operator.retain(|fact| fact.predicate != NECESSARILY);
        let err = evaluate(&no_operator).unwrap_err();
        assert!(err.message().contains("no logic:necessarily"), "got: {err}");

        let mut both = modal_frame("necessarily", relation, &["w1", "w2"]);
        both.push(fact(
            "https://example.org/modal/frame",
            "https://example.org/modal/F",
            POSSIBLY,
            "https://example.org/modal/B",
        ));
        let err = evaluate(&both).unwrap_err();
        assert!(
            err.message().contains("both logic:necessarily"),
            "got: {err}"
        );
    }

    #[test]
    fn malformed_frame_hard_fails_on_non_iri_ground_atom_binding() {
        let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
        let mut facts = modal_frame("necessarily", relation, &["w1", "w2"]);
        facts.retain(|fact| fact.predicate != ATOM_OBJECT);
        facts.push(fact(
            "https://example.org/modal/frame",
            "https://example.org/modal/B",
            ATOM_OBJECT,
            "\"not-an-iri\"",
        ));
        let err = evaluate(&facts).unwrap_err();
        assert!(err.message().contains("must be an IRI"), "got: {err}");
    }

    #[test]
    fn one_malformed_frame_aborts_the_complete_evaluation() {
        let relation = "https://blackcatinformatics.ca/logic/epistemicallyPossible";
        let mut facts = modal_frame_at(
            "https://example.org/valid-modal",
            "necessarily",
            relation,
            &["w1", "w2"],
        );
        let mut malformed = modal_frame_at(
            "https://example.org/malformed-modal",
            "possibly",
            relation,
            &["w1"],
        );
        malformed.retain(|fact| fact.predicate != ATOM_PREDICATE);
        facts.extend(malformed);
        assert!(evaluate(&facts).is_err());
    }

    #[test]
    fn malformed_frame_hard_fails_on_nested_modal_body() {
        let mut facts = modal_frame(
            "necessarily",
            "https://blackcatinformatics.ca/logic/epistemicallyPossible",
            &["w1"],
        );
        facts.push(fact(
            "https://example.org/modal/frame",
            "https://example.org/modal/B",
            NECESSARILY,
            "https://example.org/modal/C",
        ));
        let err = evaluate(&facts).unwrap_err();
        assert!(err.message().contains("nested modal"), "got: {err}");
    }
}
