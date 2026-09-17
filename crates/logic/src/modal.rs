// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared typed modal-evaluation kernel.
//!
//! This module owns the bounded Kripke evaluation of `logic:necessarily` / `logic:possibly`
//! over the finite materialized world set. Callers provide fact-like rows through
//! [`ModalFact`]; the kernel resolves typed modal frames, hard-fails malformed frames,
//! and returns typed verdict rows the caller can project onto its own surface.

use std::collections::{BTreeMap, BTreeSet};

pub(crate) mod native;
pub use native::{NativeModalEvidence, NativeModalSupport};

mod evidence;
pub(crate) use evidence::occurrence_id;
pub use evidence::{ModalEvaluation, ModalFrontier, ModalPremise, ModalWorldEvidence};

mod composite;
pub mod contextual;
pub mod journal;

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
    fn object(&self) -> std::borrow::Cow<'_, str>;
}

impl<T: ModalFact + ?Sized> ModalFact for &T {
    fn graph(&self) -> &str {
        <T as ModalFact>::graph(*self)
    }

    fn subject(&self) -> &str {
        <T as ModalFact>::subject(*self)
    }

    fn predicate(&self) -> &str {
        <T as ModalFact>::predicate(*self)
    }

    fn object(&self) -> std::borrow::Cow<'_, str> {
        <T as ModalFact>::object(*self)
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
pub enum ModalOp {
    Box,
    Diamond,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModalFrame {
    pub(crate) context: String,
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
    pub(crate) evaluation: ModalEvaluation,
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
    nec_body: BTreeMap<(String, String), BTreeSet<String>>,
    pos_body: BTreeMap<(String, String), BTreeSet<String>>,
    over: BTreeMap<(String, String), BTreeSet<String>>,
    eval_world: BTreeMap<(String, String), BTreeSet<String>>,
    atom_s: BTreeMap<(String, String), BTreeSet<String>>,
    atom_p: BTreeMap<(String, String), BTreeSet<String>>,
    atom_o: BTreeMap<(String, String), BTreeSet<String>>,
    typed_relations: BTreeSet<&'static str>,
}

pub(crate) fn evaluate<T: ModalFact>(facts: &[T]) -> gmeow_errors::Result<Vec<ModalVerdict>> {
    evaluate_replayable(|| facts.iter())
}

/// Evaluate a replayable typed fact stream without materializing a second full closure.
///
/// The kernel makes three deterministic passes: frame resolution, active accessibility
/// validation, and atom-presence indexing. A production caller can therefore replay a
/// borrowed closure chained with a narrow typed-EDB supplement while retaining the same
/// single evaluator and bounded memory profile as [`evaluate`].
pub(crate) fn evaluate_replayable<T, I, F>(facts: F) -> gmeow_errors::Result<Vec<ModalVerdict>>
where
    T: ModalFact,
    I: Iterator<Item = T>,
    F: Fn() -> I,
{
    let frames = prepare_frames(&facts)?;
    if frames.is_empty() {
        return Ok(Vec::new());
    }

    // A typed relation can also occur as ordinary domain data (notably
    // `gmeow:sharpens`). Validate only the edge rows selected by a resolved modal
    // frame's exact `(asserting context, evaluation world, relation)` key. This keeps unrelated
    // literal/blank/triple-valued domain facts outside the modal seam while an
    // ill-typed endpoint on an edge that the frame actually evaluates remains an
    // atomic failure before any verdict is published.
    let active_access: BTreeSet<(&str, &str, &str)> = frames
        .iter()
        .map(|frame| {
            (
                frame.context.as_str(),
                frame.w0.as_str(),
                frame.relation.as_str(),
            )
        })
        .collect();
    let mut access: BTreeMap<(String, String, String), BTreeSet<String>> = BTreeMap::new();
    for fact in facts() {
        if !TYPED_ACCESSIBILITY.contains(&fact.predicate()) {
            continue;
        }
        let Ok(source) = iri_binding(fact.subject(), "typed accessibility edge source world")
        else {
            continue;
        };
        if !active_access.contains(&(fact.graph(), source.as_str(), fact.predicate())) {
            continue;
        }
        let target = iri_binding(&fact.object(), "typed accessibility edge target world")?;
        access
            .entry((fact.graph().to_owned(), source, fact.predicate().to_owned()))
            .or_default()
            .insert(target);
    }
    drop(active_access);

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
    for fact in facts() {
        let value = fact.object();
        let object = normalize_object(&value);
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
        let worlds: Vec<ModalWorldEvidence> = access
            .get(&(
                frame.context.clone(),
                frame.w0.clone(),
                frame.relation.clone(),
            ))
            .into_iter()
            .flatten()
            .map(|world| ModalWorldEvidence {
                world: world.clone(),
                atom_present: presence.contains(&(
                    world.clone(),
                    frame.atom_s.clone(),
                    frame.atom_p.clone(),
                    frame.atom_o.clone(),
                )),
            })
            .collect();
        verdicts.extend(evaluate_frame(&frame, worlds)?);
    }
    Ok(verdicts)
}

/// Admit original frame ownership once, before any producer can normalize or
/// derive the definition it is being asked to execute.
fn prepare_frames<T, I, F>(facts: &F) -> gmeow_errors::Result<Vec<ModalFrame>>
where
    T: ModalFact,
    I: Iterator<Item = T>,
    F: Fn() -> I,
{
    let mut frame_indexes = ModalFrameIndexes {
        typed_relations: TYPED_ACCESSIBILITY.iter().copied().collect(),
        ..ModalFrameIndexes::default()
    };

    let mut contextual_requests = BTreeSet::new();

    for fact in facts() {
        if fact.predicate() == "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
            && normalize_object(&fact.object())
                == "https://blackcatinformatics.ca/logic/ContextualEvaluationRequest"
        {
            contextual_requests.insert((fact.graph().to_owned(), fact.subject().to_owned()));
        }
        match fact.predicate() {
            NECESSARILY => {
                frame_indexes
                    .nec_body
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert(normalize_object(&fact.object()).to_owned());
            }
            POSSIBLY => {
                frame_indexes
                    .pos_body
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert(normalize_object(&fact.object()).to_owned());
            }
            OVER_ACCESSIBILITY => {
                frame_indexes
                    .over
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert(normalize_object(&fact.object()).to_owned());
            }
            MODAL_EVAL_WORLD => {
                frame_indexes
                    .eval_world
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert(normalize_object(&fact.object()).to_owned());
            }
            ATOM_SUBJECT => {
                frame_indexes
                    .atom_s
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert(normalize_object(&fact.object()).to_owned());
            }
            ATOM_PREDICATE => {
                frame_indexes
                    .atom_p
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert(normalize_object(&fact.object()).to_owned());
            }
            ATOM_OBJECT => {
                frame_indexes
                    .atom_o
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert(normalize_object(&fact.object()).to_owned());
            }
            _ => {}
        }
    }

    let mut contextual_roots: BTreeMap<(String, String), BTreeSet<(String, String)>> =
        BTreeMap::new();
    let mut formula_children: BTreeMap<(String, String), BTreeSet<(String, String)>> =
        BTreeMap::new();
    if !contextual_requests.is_empty() {
        for fact in facts() {
            if fact.predicate() == "https://blackcatinformatics.ca/logic/queryFormula"
                && contextual_requests
                    .contains(&(fact.graph().to_owned(), fact.subject().to_owned()))
            {
                contextual_roots
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert((
                        fact.graph().to_owned(),
                        normalize_object(&fact.object()).to_owned(),
                    ));
            }
            if fact
                .predicate()
                .strip_prefix("https://blackcatinformatics.ca/logic/")
                .is_some_and(|local| {
                    gmeow_logic_compile::frontend::FORMULA_SUBLINKS.contains(&local)
                })
            {
                formula_children
                    .entry((fact.graph().to_owned(), fact.subject().to_owned()))
                    .or_default()
                    .insert((
                        fact.graph().to_owned(),
                        normalize_object(&fact.object()).to_owned(),
                    ));
            }
        }
    }

    // Query-owned expressions are evaluated by the composite kernel. They are
    // not independently requested flat frames, and their inner modal nodes must
    // retain the enclosing context instead of acquiring an implicit root world.
    let mut owned = BTreeSet::new();
    let mut pending = contextual_requests
        .iter()
        .filter_map(|request| contextual_roots.get(request))
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    while let Some(formula) = pending.pop() {
        if !owned.insert(formula.clone()) {
            continue;
        }
        if let Some(children) = formula_children.get(&formula) {
            pending.extend(children.iter().cloned());
        }
    }
    for formula in owned {
        if frame_indexes.eval_world.contains_key(&formula) {
            return Err(modal_err(format!(
                "formula {formula:?} is selected by both a contextual request and a flat modal evaluation world"
            )));
        }
        frame_indexes.nec_body.remove(&formula);
        frame_indexes.pos_body.remove(&formula);
        frame_indexes.over.remove(&formula);
        frame_indexes.eval_world.remove(&formula);
    }
    resolve_frames(&frame_indexes)
}

/// The single verdict calculus, shared by the native scheduled producer and
/// explicit bounded evaluation of a caller-supplied completed frame.
fn evaluate_frame(
    frame: &ModalFrame,
    worlds: Vec<ModalWorldEvidence>,
) -> gmeow_errors::Result<Vec<ModalVerdict>> {
    let mut verdicts = Vec::new();
    let missing = worlds.iter().find(|world| !world.atom_present);
    let predicate = match frame.op {
        ModalOp::Box if worlds.is_empty() && frame.relation == DEONTICALLY_IDEAL => {
            MODAL_NECESSITY_UNDETERMINED
        }
        ModalOp::Box if missing.is_some() => MODAL_NECESSITY_FAILS,
        ModalOp::Box => MODAL_NECESSITY_HOLDS,
        ModalOp::Diamond if worlds.iter().any(|world| world.atom_present) => {
            MODAL_POSSIBILITY_HOLDS
        }
        ModalOp::Diamond => MODAL_POSSIBILITY_FAILS,
    };
    verdicts.push(verdict(
        frame,
        predicate,
        frame.body.clone(),
        worlds.clone(),
    )?);
    if frame.op == ModalOp::Box
        && let Some(witness) = missing
    {
        verdicts.push(verdict(
            frame,
            MODAL_COUNTEREXAMPLE_WORLD,
            witness.world.clone(),
            worlds.clone(),
        )?);
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
    let mut formula_nodes: BTreeSet<(String, String)> = BTreeSet::new();
    formula_nodes.extend(nec_body.keys().cloned());
    formula_nodes.extend(pos_body.keys().cloned());
    formula_nodes.extend(over.keys().cloned());
    formula_nodes.extend(eval_world.keys().cloned());

    for ((context, formula), bodies) in nec_body.iter().chain(pos_body) {
        if let Some(body) = bodies
            .iter()
            .find(|body| formula_nodes.contains(&(context.clone(), (*body).clone())))
        {
            return Err(modal_err(format!(
                "modal body {body} of formula {formula} is itself a modal formula; the modal \
                 body is scoped to a single ground atom, not a nested modal"
            )));
        }
    }

    let mut frames = Vec::new();
    for key @ (context, formula) in &formula_nodes {
        iri_binding(formula, "modal formula")?;
        let has_nec = nec_body.contains_key(key);
        let has_pos = pos_body.contains_key(key);
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
            (ModalOp::Box, &nec_body[key])
        } else {
            (ModalOp::Diamond, &pos_body[key])
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
            &exact_one(over.get(key), || {
                format!(
                    "modal formula {formula} must carry exactly one logic:overAccessibility \
                 relation (found {})",
                    over.get(key).map_or(0, BTreeSet::len)
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
            &exact_one(eval_world.get(key), || {
                format!(
                    "modal formula {formula} must carry exactly one logic:modalEvalWorld \
                 evaluation world (found {})",
                    eval_world.get(key).map_or(0, BTreeSet::len)
                )
            })?,
            "modal evaluation world",
        )?;

        frames.push(ModalFrame {
            context: context.clone(),
            formula: formula.clone(),
            op,
            body: body.clone(),
            relation,
            w0,
            atom_s: single_atom_binding(atom_s, context, &body, ATOM_SUBJECT, formula)?,
            atom_p: single_atom_binding(atom_p, context, &body, ATOM_PREDICATE, formula)?,
            atom_o: single_atom_binding(atom_o, context, &body, ATOM_OBJECT, formula)?,
        });
    }
    Ok(frames)
}

fn single_atom_binding(
    index: &BTreeMap<(String, String), BTreeSet<String>>,
    context: &str,
    body: &str,
    predicate: &str,
    formula: &str,
) -> gmeow_errors::Result<String> {
    match index
        .get(&(context.to_owned(), body.to_owned()))
        .map(|values| (values.len(), values))
    {
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
    worlds: Vec<ModalWorldEvidence>,
) -> gmeow_errors::Result<ModalVerdict> {
    let evaluation = ModalEvaluation::from_frame(frame, worlds, predicate, object.clone());
    evaluation.validate()?;
    let positives = evaluation.positive_premises();
    let sources = positives.iter().map(ModalPremise::occurrence_id).collect();
    let premises = positives
        .into_iter()
        .map(|p| (p.subject, p.predicate, p.object))
        .collect();
    Ok(ModalVerdict {
        derivation_id: evaluation.derivation_id(),
        evaluation,
        graph: frame.context.clone(),
        subject: frame.formula.clone(),
        predicate: predicate.to_owned(),
        object,
        rule_iri: MODAL_RULE_IRI.to_owned(),
        premises,
        source_quad_ids: sources,
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

#[path = "modal.tests.rs"]
#[cfg(test)]
mod tests;
