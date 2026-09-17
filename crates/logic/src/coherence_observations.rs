// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit producer observations for shipped coherence and foundation contracts.
//! Consumers grade the authenticated observations without constructing a corpus.

use crate::foundation::{AntiRigidityPolicy, FoundationQuad, evaluate as foundation_evaluate};
use crate::reason::{DlVerdict, LogicalGraph};
use crate::reasoning_graphs::is_object_level_named_graph;
use crate::store::WorldStore;
use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm, TermRef};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// Authenticated poisoned object-level native consistency observation.
pub const DISJOINT_ARTIFACT: &str = "coherence-disjoint-observation-v1.json";
/// Authenticated clean and poisoned relator-mediation observations.
pub const RELCOMP_ARTIFACT: &str = "coherence-relcomp-observation-v1.json";
/// Authenticated characteristic carriers, closure and violation observations.
pub const CHARACTERISTIC_ARTIFACT: &str = "coherence-characteristic-observation-v1.json";

/// The IRI-only projection selected by the foundation operation, without RDF text.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct IriQuad {
    /// Exact subject IRI.
    pub subject: String,
    /// Exact predicate IRI.
    pub predicate: String,
    /// Exact object IRI.
    pub object: String,
    /// Explicit projection or retained source world.
    pub graph: String,
}
impl IriQuad {
    /// Select one native IRI statement in its explicit operation world.
    pub fn new(subject: &str, predicate: &str, object: &str, graph: &str) -> Self {
        Self {
            subject: subject.into(),
            predicate: predicate.into(),
            object: object.into(),
            graph: graph.into(),
        }
    }
    fn quad(&self) -> RdfQuad {
        RdfQuad::new(
            RdfTerm::iri(self.subject.as_str()),
            self.predicate.as_str(),
            RdfTerm::iri(self.object.as_str()),
        )
        .in_graph(RdfTerm::iri(self.graph.as_str()))
    }
}

/// Native consistency result driven solely by the actual shipped disjoint edge.
#[derive(Debug, Serialize, Deserialize)]
pub struct DisjointObservation {
    /// Exact world where the producer found the shipped edge and injected types.
    pub disjoint_world: LogicalGraph,
    /// Full native verdict including supported clashes and independent boundaries.
    pub verdict: DlVerdict,
}
/// Relator-mediation violations before and after the one-role synthetic injection.
#[derive(Debug, Serialize, Deserialize)]
pub struct RelcompObservation {
    /// Every shipped concrete relator that fails the discipline.
    pub clean_offenders: Vec<String>,
    /// Every failing relator after adding the one-role control.
    pub poisoned_offenders: Vec<String>,
}
/// One retained foundation result row, including its exact derivation ownership.
#[derive(Debug, Serialize, Deserialize)]
pub struct FoundationObservation {
    /// Contextual modal evidence if this row belongs to a modal evaluation.
    pub modal_evaluation: Option<crate::modal::ModalEvaluation>,
    /// Exact execution world.
    pub graph: String,
    /// Exact subject IRI.
    pub subject: String,
    /// Exact predicate IRI.
    pub predicate: String,
    /// Native foundation object representation.
    pub object: String,
    /// Actual producer rule.
    pub rule_iri: String,
    /// Ordered premise reifier identities.
    pub source_quad_ids: Vec<String>,
    /// Exact content-addressed derivation.
    pub derivation_id: String,
}
impl From<FoundationQuad> for FoundationObservation {
    fn from(row: FoundationQuad) -> Self {
        Self {
            modal_evaluation: row.modal_evaluation,
            graph: row.graph,
            subject: row.subject,
            predicate: row.predicate,
            object: row.object,
            rule_iri: row.rule_iri,
            source_quad_ids: row.source_quad_ids,
            derivation_id: row.derivation_id,
        }
    }
}
/// Shipped carrier declarations and the exact clean/poisoned foundation outcomes.
#[derive(Debug, Serialize, Deserialize)]
pub struct CharacteristicObservation {
    /// Marker and record-link projection required by every carrier assertion.
    pub carrier_facts: BTreeSet<IriQuad>,
    /// All clean irreflexivity/asymmetry violations, with no subject filtering.
    pub clean_violations: Vec<(String, String)>,
    /// All clean carrier disagreements, with no subject filtering.
    pub clean_disagreements: Vec<String>,
    /// Complete result rows for the injected control namespace, with provenance.
    pub poisoned: Vec<FoundationObservation>,
}

fn failure(detail: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: detail.into(),
    })
}
fn native_dataset(facts: &BTreeSet<IriQuad>) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut builder = RdfDatasetBuilder::new();
    for fact in facts {
        builder.push_owned_quad(&fact.quad());
    }
    builder.freeze().map_err(gmeow_errors::Diag::from)
}

/// Evaluate an explicitly selected IRI projection through the native foundation.
/// # Errors
/// Retains native store, source-shape, modal-frame and reasoning failures.
pub fn evaluate_characteristic_facts(
    facts: &BTreeSet<IriQuad>,
) -> gmeow_errors::Result<Vec<FoundationQuad>> {
    let dataset = native_dataset(facts)?;
    let store = WorldStore::new();
    store.load_dataset(&dataset)?;
    foundation_evaluate(&store, AntiRigidityPolicy::WitnessObligation)
}

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

const LOGIC_DISJOINT_WITH: &str = "https://blackcatinformatics.ca/logic/disjointWith";

const OWL_FUNCTIONAL_PROPERTY: &str = "http://www.w3.org/2002/07/owl#FunctionalProperty";

const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

const LOGIC_SUBCLASS_OF: &str = "https://blackcatinformatics.ca/logic/subClassOf";

const LOGIC_MEDIATES: &str = "https://blackcatinformatics.ca/logic/mediates";

const LOGIC_VIOLATION: &str = "https://blackcatinformatics.ca/logic/violation";

const LOGIC_RELCOMP: &str = "https://blackcatinformatics.ca/logic/RelComp";

const LOGIC_KIND: &str = "https://blackcatinformatics.ca/logic/Kind";

const LOGIC_RELATOR: &str = "https://blackcatinformatics.ca/logic/Relator";

const BUNDLE_WORLD: &str = "https://blackcatinformatics.ca/gmeow/test/relcomp/world";

const OWL_TRANSITIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#TransitiveProperty";

const OWL_SYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#SymmetricProperty";

const OWL_IRREFLEXIVE_PROPERTY: &str = "http://www.w3.org/2002/07/owl#IrreflexiveProperty";

const OWL_ASYMMETRIC_PROPERTY: &str = "http://www.w3.org/2002/07/owl#AsymmetricProperty";

const LOGIC_CHARACTERIZES: &str = "https://blackcatinformatics.ca/logic/characterizes";

const LOGIC_CHARACTERISTIC_SORT: &str = "https://blackcatinformatics.ca/logic/characteristicSort";

const LOGIC_NECESSARILY: &str = "https://blackcatinformatics.ca/logic/necessarily";

const LOGIC_POSSIBLY: &str = "https://blackcatinformatics.ca/logic/possibly";

const LOGIC_OVER_ACCESSIBILITY: &str = "https://blackcatinformatics.ca/logic/overAccessibility";

const LOGIC_MODAL_EVAL_WORLD: &str = "https://blackcatinformatics.ca/logic/modalEvalWorld";

const LOGIC_ATOM_SUBJECT: &str = "https://blackcatinformatics.ca/logic/atomSubject";

const LOGIC_ATOM_PREDICATE: &str = "https://blackcatinformatics.ca/logic/atomPredicate";

const LOGIC_ATOM_OBJECT: &str = "https://blackcatinformatics.ca/logic/atomObject";

const LOGIC_IRREFLEXIVITY_VIOLATION: &str =
    "https://blackcatinformatics.ca/logic/IrreflexivityViolation";

const LOGIC_ASYMMETRY_VIOLATION: &str = "https://blackcatinformatics.ca/logic/AsymmetryViolation";

const CHAR_WORLD: &str = "https://blackcatinformatics.ca/gmeow/test/characteristic/world";

const GMEOW_SUB_EVENT_OF: &str = "https://blackcatinformatics.ca/gmeow/subEventOf";

const GMEOW_COUNTER_GOAL: &str = "https://blackcatinformatics.ca/gmeow/counterGoal";

const GMEOW_COUNTERPART_OF: &str = "https://blackcatinformatics.ca/gmeow/counterpartOf";

const LOGIC_CARRIER_DISAGREEMENT: &str =
    "https://blackcatinformatics.ca/logic/CharacteristicCarrierDisagreement";

const X: &str = "https://blackcatinformatics.ca/gmeow/test/coherence/x";

const AGENT: &str = "https://blackcatinformatics.ca/gmeow/Agent";

const SOCIAL_OBJECT: &str = "https://blackcatinformatics.ca/gmeow/SocialObject";

fn admitted_reasoning_graph(graph: &Option<RdfTerm>) -> bool {
    match graph {
        None => true,
        Some(RdfTerm::Iri(iri)) => is_object_level_named_graph(iri),
        Some(_) => false,
    }
}

/// Observe the native response to two injected types over the shipped disjoint edge.
/// No disjointness axiom is supplied by the control itself.
/// # Errors
/// Rejects a missing shipped edge or any native ingress/execution failure.
pub fn observe_disjoint_clash(snapshot: &RdfDataset) -> gmeow_errors::Result<DisjointObservation> {
    let disjoint_world = snapshot.owned_quads().find_map(|q| {
        let is_edge = admitted_reasoning_graph(&q.graph_name)
            && (q.predicate == LOGIC_DISJOINT_WITH || q.predicate == OWL_DISJOINT_WITH)
            && matches!((&q.subject, &q.object), (RdfTerm::Iri(s), RdfTerm::Iri(o))
                if (s == AGENT && o == SOCIAL_OBJECT) || (s == SOCIAL_OBJECT && o == AGENT));
        is_edge.then(|| q.graph_name.clone())
    }).ok_or_else(|| failure(
        "the committed gmeow.gts must ship gmeow:Agent logic:disjointWith gmeow:SocialObject (the canonical authored spelling; owl:disjointWith is only its generated projection)",
    ))?;
    let mut builder = RdfDatasetBuilder::new();
    for graph in snapshot.owned_named_graphs() {
        if admitted_reasoning_graph(&Some(graph.clone())) {
            let graph = builder.intern_owned_term(&graph);
            builder.declare_named_graph(graph);
        }
    }
    for quad in snapshot.owned_quads() {
        if admitted_reasoning_graph(&quad.graph_name) {
            builder.push_owned_quad(&quad);
        }
    }
    for reifier in snapshot.owned_reifiers() {
        if admitted_reasoning_graph(&reifier.graph) {
            builder.push_owned_reifier(&reifier);
        }
    }
    for annotation in snapshot.owned_annotations() {
        if admitted_reasoning_graph(&annotation.graph) {
            builder.push_owned_annotation(&annotation);
        }
    }
    for class in [AGENT, SOCIAL_OBJECT] {
        let mut quad = RdfQuad::new(RdfTerm::iri(X), RDF_TYPE, RdfTerm::iri(class));
        quad.graph_name = disjoint_world.clone();
        builder.push_owned_quad(&quad);
    }
    let poisoned = builder.freeze().map_err(gmeow_errors::Diag::from)?;
    let input = crate::reason::prepare_reasoning_input(&poisoned)?;
    let verdict =
        crate::reason::dl_consistency(input, &crate::reasoning_graphs::object_level_domains()?)?;
    let disjoint_world = match disjoint_world {
        None => LogicalGraph::Default,
        Some(RdfTerm::Iri(iri)) => LogicalGraph::Named(purrdf::TermValue::iri(iri)),
        Some(_) => return Err(failure("shipped disjoint edge has an inadmissible world")),
    };
    Ok(DisjointObservation {
        disjoint_world,
        verdict,
    })
}

fn project_relator_facts(onto: &RdfDataset) -> BTreeSet<IriQuad> {
    onto.owned_quads()
        .filter_map(|q| {
            let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) else {
                return None;
            };
            let predicate = match q.predicate.as_str() {
                RDF_TYPE if o.starts_with(LOGIC_NS) || o == OWL_FUNCTIONAL_PROPERTY => RDF_TYPE,
                RDFS_SUBCLASS_OF | LOGIC_SUBCLASS_OF => LOGIC_SUBCLASS_OF,
                LOGIC_MEDIATES => LOGIC_MEDIATES,
                _ => return None,
            };
            Some(IriQuad::new(s, predicate, o, BUNDLE_WORLD))
        })
        .collect()
}

fn relcomp_offenders(facts: &BTreeSet<IriQuad>) -> gmeow_errors::Result<Vec<String>> {
    let target = format!("<{LOGIC_RELCOMP}>");
    Ok(evaluate_characteristic_facts(facts)?
        .into_iter()
        .filter(|row| row.predicate == LOGIC_VIOLATION && row.object == target)
        .map(|row| row.subject)
        .collect())
}

/// Observe the clean relator discipline and its single functional-role control.
/// # Errors
/// Retains native projection and foundation execution failures.
pub fn observe_relcomp(snapshot: &RdfDataset) -> gmeow_errors::Result<RelcompObservation> {
    let mut facts = project_relator_facts(snapshot);
    let clean_offenders = relcomp_offenders(&facts)?;
    let bad = "https://blackcatinformatics.ca/gmeow/test/relcomp/DegenerateRelator";
    let role = "https://blackcatinformatics.ca/gmeow/test/relcomp/soleRole";
    for (subject, predicate, object) in [
        (bad, RDF_TYPE, LOGIC_KIND),
        (bad, LOGIC_SUBCLASS_OF, LOGIC_RELATOR),
        (bad, LOGIC_MEDIATES, role),
        (role, RDF_TYPE, OWL_FUNCTIONAL_PROPERTY),
    ] {
        facts.insert(IriQuad::new(subject, predicate, object, BUNDLE_WORLD));
    }
    let poisoned_offenders = relcomp_offenders(&facts)?;
    Ok(RelcompObservation {
        clean_offenders,
        poisoned_offenders,
    })
}

/// Observe characteristic carriers and clean/poisoned foundation consequences.
/// Records only the carrier statements and control result rows the grader needs;
/// no serialized intermediate RDF projection is constructed or reparsed.
/// # Errors
/// Retains native projection, contextual-frame and foundation execution failures.
pub fn observe_characteristics(
    snapshot: &RdfDataset,
) -> gmeow_errors::Result<CharacteristicObservation> {
    let mut facts = project_characteristic_facts(snapshot);
    let carrier_facts = facts
        .iter()
        .filter(|fact| {
            matches!(
                fact.predicate.as_str(),
                RDF_TYPE | LOGIC_CHARACTERIZES | LOGIC_CHARACTERISTIC_SORT
            )
        })
        .cloned()
        .collect();
    let clean = evaluate_characteristic_facts(&facts)?;
    let violations = [
        format!("<{LOGIC_IRREFLEXIVITY_VIOLATION}>"),
        format!("<{LOGIC_ASYMMETRY_VIOLATION}>"),
    ];
    let clean_violations = clean
        .iter()
        .filter(|row| row.predicate == LOGIC_VIOLATION && violations.contains(&row.object))
        .map(|row| (row.subject.clone(), row.object.clone()))
        .collect();
    let disagreement = format!("<{LOGIC_CARRIER_DISAGREEMENT}>");
    let clean_disagreements = clean
        .iter()
        .filter(|row| row.predicate == LOGIC_VIOLATION && row.object == disagreement)
        .map(|row| row.subject.clone())
        .collect();
    let t = "https://blackcatinformatics.ca/gmeow/test/characteristic";
    let irr_prop = format!("{t}/strictlyContains");
    let asym_prop = format!("{t}/strictlyBefore");
    for (subject, predicate, object) in [
        (
            format!("{t}/A"),
            GMEOW_SUB_EVENT_OF.to_owned(),
            format!("{t}/B"),
        ),
        (
            format!("{t}/B"),
            GMEOW_SUB_EVENT_OF.to_owned(),
            format!("{t}/C"),
        ),
        (
            format!("{t}/M"),
            GMEOW_COUNTER_GOAL.to_owned(),
            format!("{t}/N"),
        ),
        (
            format!("{t}/X"),
            GMEOW_COUNTERPART_OF.to_owned(),
            format!("{t}/Y"),
        ),
        (
            format!("{t}/Y"),
            GMEOW_COUNTERPART_OF.to_owned(),
            format!("{t}/Z"),
        ),
        (
            irr_prop.clone(),
            RDF_TYPE.to_owned(),
            OWL_IRREFLEXIVE_PROPERTY.to_owned(),
        ),
        (format!("{t}/self"), irr_prop, format!("{t}/self")),
        (
            asym_prop.clone(),
            RDF_TYPE.to_owned(),
            OWL_ASYMMETRIC_PROPERTY.to_owned(),
        ),
        (format!("{t}/P"), asym_prop.clone(), format!("{t}/Q")),
        (format!("{t}/Q"), asym_prop, format!("{t}/P")),
        (
            format!("{t}/driftRec"),
            LOGIC_CHARACTERIZES.to_owned(),
            format!("{t}/driftProp"),
        ),
        (
            format!("{t}/driftRec"),
            LOGIC_CHARACTERISTIC_SORT.to_owned(),
            format!("{LOGIC_NS}transitiveProperty"),
        ),
    ] {
        facts.insert(IriQuad::new(&subject, &predicate, &object, CHAR_WORLD));
    }
    let injected_namespace = format!("{t}/");
    let poisoned = evaluate_characteristic_facts(&facts)?
        .into_iter()
        .filter(|row| row.subject.starts_with(&injected_namespace))
        .map(FoundationObservation::from)
        .collect();
    Ok(CharacteristicObservation {
        carrier_facts,
        clean_violations,
        clean_disagreements,
        poisoned,
    })
}

fn is_characteristic_marker(iri: &str) -> bool {
    matches!(
        iri,
        OWL_TRANSITIVE_PROPERTY
            | OWL_SYMMETRIC_PROPERTY
            | OWL_IRREFLEXIVE_PROPERTY
            | OWL_ASYMMETRIC_PROPERTY
            | OWL_FUNCTIONAL_PROPERTY
    ) || matches!(
        iri.strip_prefix(LOGIC_NS),
        Some("transitiveProperty")
            | Some("symmetricProperty")
            | Some("irreflexiveProperty")
            | Some("asymmetricProperty")
            | Some("functionalProperty")
    )
}

fn is_modal_frame_predicate(predicate: &str) -> bool {
    matches!(
        predicate,
        LOGIC_NECESSARILY | LOGIC_POSSIBLY | LOGIC_OVER_ACCESSIBILITY | LOGIC_MODAL_EVAL_WORLD
    )
}

/// Project characterized IRI facts while retaining complete modal grammar,
/// contextual formula ownership and the original worlds of ground-atom evidence.
#[must_use]
pub fn project_characteristic_facts(onto: &purrdf::RdfDataset) -> BTreeSet<IriQuad> {
    // Pass 1: which predicates carry a characteristic (a marker on the property itself, or
    // a property named by a central record)?
    let mut characterized: BTreeSet<String> = BTreeSet::new();
    for q in onto.owned_quads() {
        if let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) {
            if q.predicate == RDF_TYPE && is_characteristic_marker(o) {
                characterized.insert(s.clone());
            } else if q.predicate == LOGIC_CHARACTERIZES {
                characterized.insert(o.clone());
            }
        }
    }
    // Pass 2: emit the markers, the record links, and the edges of characterized predicates.
    // Remember any modal formula pulled into that projection so pass 3 can retain its whole
    // bounded-Kripke frame rather than manufacturing a partial one.
    let mut lines: BTreeSet<IriQuad> = BTreeSet::new();
    let mut modal_formulas: BTreeSet<String> = BTreeSet::new();
    for q in onto.owned_quads() {
        let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) else {
            continue;
        };
        let emit = match q.predicate.as_str() {
            RDF_TYPE => is_characteristic_marker(o),
            LOGIC_CHARACTERIZES | LOGIC_CHARACTERISTIC_SORT => true,
            pred => characterized.contains(pred),
        };
        if emit {
            lines.insert(IriQuad::new(s, &q.predicate, o, CHAR_WORLD));
            if is_modal_frame_predicate(&q.predicate) {
                modal_formulas.insert(s.clone());
            }
        }
    }

    if modal_formulas.is_empty() {
        return lines;
    }

    // Contextual formulas must retain their query ownership through this lossy
    // projection. Otherwise their inner modal nodes become orphan flat requests.
    preserve_contextual_formula_ownership(onto, &mut lines);

    // Pass 3a: retain every operator/frame binding for each selected modal formula and
    // collect the bodies, evaluation worlds, and typed relations whose closure is needed.
    let mut modal_bodies: BTreeSet<String> = BTreeSet::new();
    let mut modal_worlds: BTreeSet<String> = BTreeSet::new();
    let mut modal_relations: BTreeSet<String> = BTreeSet::new();
    for q in onto.owned_quads() {
        let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) else {
            continue;
        };
        if !modal_formulas.contains(s) || !is_modal_frame_predicate(&q.predicate) {
            continue;
        }
        lines.insert(IriQuad::new(s, &q.predicate, o, CHAR_WORLD));
        match q.predicate.as_str() {
            LOGIC_NECESSARILY | LOGIC_POSSIBLY => {
                modal_bodies.insert(o.clone());
            }
            LOGIC_OVER_ACCESSIBILITY => {
                modal_relations.insert(o.clone());
            }
            LOGIC_MODAL_EVAL_WORLD => {
                modal_worlds.insert(o.clone());
            }
            _ => unreachable!("modal-frame predicate was matched above"),
        }
    }

    // Pass 3b: retain each body's ground-atom bindings as ONE complete (subject, predicate,
    // object) triple per body, so pass 3c admits a ground-atom presence quad only when its
    // FULL triple matches a single body — never a cross-body mix of an atomSubject from one
    // body with an atomPredicate/atomObject from another. The modal kernel will still reject
    // missing, repeated, non-IRI, or otherwise malformed bindings atomically.
    let mut body_atom_s: BTreeMap<String, String> = BTreeMap::new();
    let mut body_atom_p: BTreeMap<String, String> = BTreeMap::new();
    let mut body_atom_o: BTreeMap<String, String> = BTreeMap::new();
    for q in onto.owned_quads() {
        let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) else {
            continue;
        };
        if !modal_bodies.contains(s) {
            continue;
        }
        match q.predicate.as_str() {
            LOGIC_ATOM_SUBJECT => {
                body_atom_s.insert(s.clone(), o.clone());
            }
            LOGIC_ATOM_PREDICATE => {
                body_atom_p.insert(s.clone(), o.clone());
            }
            LOGIC_ATOM_OBJECT => {
                body_atom_o.insert(s.clone(), o.clone());
            }
            _ => continue,
        }
        lines.insert(IriQuad::new(s, &q.predicate, o, CHAR_WORLD));
    }
    // The exact (subject, predicate, object) ground atom of each body that carries all three.
    let atom_bindings: BTreeSet<(String, String, String)> = modal_bodies
        .iter()
        .filter_map(|body| {
            Some((
                body_atom_s.get(body)?.clone(),
                body_atom_p.get(body)?.clone(),
                body_atom_o.get(body)?.clone(),
            ))
        })
        .collect();

    // Pass 3c: retain accessibility edges and ground-atom truth in their ORIGINAL named
    // worlds. Collapsing atom presence into CHAR_WORLD would silently change a modal
    // verdict; preserving the source graph keeps evaluation-world identity exact.
    for q in onto.owned_quads() {
        let (RdfTerm::Iri(s), RdfTerm::Iri(o)) = (&q.subject, &q.object) else {
            continue;
        };
        if modal_worlds.contains(s) && modal_relations.contains(&q.predicate) {
            lines.insert(IriQuad::new(s, &q.predicate, o, CHAR_WORLD));
        }
        if atom_bindings.contains(&(s.clone(), q.predicate.clone(), o.clone()))
            && let Some(RdfTerm::Iri(graph)) = &q.graph_name
        {
            lines.insert(IriQuad::new(s, &q.predicate, o, graph));
        }
    }
    lines
}

fn preserve_contextual_formula_ownership(onto: &purrdf::RdfDataset, lines: &mut BTreeSet<IriQuad>) {
    const REQUEST: &str = "https://blackcatinformatics.ca/logic/ContextualEvaluationRequest";
    const QUERY: &str = "https://blackcatinformatics.ca/logic/queryFormula";
    let (Some(rdf_type), Some(request_type), Some(query)) = (
        onto.term_id_by_iri(RDF_TYPE),
        onto.term_id_by_iri(REQUEST),
        onto.term_id_by_iri(QUERY),
    ) else {
        return;
    };
    let sublinks: Vec<_> = gmeow_logic_compile::frontend::FORMULA_SUBLINKS
        .iter()
        .filter_map(|local| onto.term_id_by_iri(&format!("{LOGIC_NS}{local}")))
        .collect();
    let mut pending = Vec::new();
    for request in onto.quads_for_pattern(None, Some(rdf_type), Some(request_type), GraphMatch::Any)
    {
        let TermRef::Iri(subject) = onto.resolve(request.s) else {
            continue;
        };
        lines.insert(IriQuad::new(subject, RDF_TYPE, REQUEST, CHAR_WORLD));
        for binding in onto.quads_for_pattern(Some(request.s), Some(query), None, GraphMatch::Any) {
            if let TermRef::Iri(formula) = onto.resolve(binding.o) {
                lines.insert(IriQuad::new(subject, QUERY, formula, CHAR_WORLD));
                pending.push(binding.o);
            }
        }
    }
    let mut seen = BTreeSet::new();
    while let Some(formula) = pending.pop() {
        if !seen.insert(formula) {
            continue;
        }
        let TermRef::Iri(subject) = onto.resolve(formula) else {
            continue;
        };
        for predicate in &sublinks {
            let TermRef::Iri(predicate_iri) = onto.resolve(*predicate) else {
                unreachable!("IRI index");
            };
            for binding in
                onto.quads_for_pattern(Some(formula), Some(*predicate), None, GraphMatch::Any)
            {
                if let TermRef::Iri(child) = onto.resolve(binding.o) {
                    lines.insert(IriQuad::new(subject, predicate_iri, child, CHAR_WORLD));
                    pending.push(binding.o);
                }
            }
        }
    }
}
