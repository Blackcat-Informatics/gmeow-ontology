// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Expression-identity observations over a canonical math-TBox projection and a
//! selected scene. The complete module remains an explicit authenticated input;
//! this action derives the one law it consumes and never admits an example into
//! the production asserted base.

use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld, prepare_reasoning_input,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_errors::Finding;
use gmeow_logic::math_expression::check_math_expression_findings;
use gmeow_logic::reason::reason_all;
use gmeow_logic::verify::{ReasonedGraphOutcome, materialize_reasoned_closure};
use purrdf::{CanonHash, CompositeDatasetView, RdfDataset, RdfDatasetBuilder, RdfTerm, ViewLimits};
use serde::{Deserialize, Serialize};

use crate::cache::BuildIdentity;
use crate::stages::parse_sources::SourceCatalog;

mod controls;

pub(super) const CHANNEL: &str = "pipeline/expression-substrate-observations.json";
pub(super) const MATH: &str = "slices/grounding/math/module.ttl";
pub(super) const REFERENCE: &str = "slices/grounding/math/examples/reference-ast-act.ttl";
pub(super) const TWINS: &str = "slices/grounding/math/examples/alpha-equivalent-twins.ttl";
pub(super) const CLOSED_FORMS: &str = "slices/grounding/math/examples/closed-form-functions.ttl";
pub(super) const WRAPPER_CONTROL: &str = "expression-substrate/independent-wrapper-twins";
pub(super) const OPERATOR_CONTROL: &str = "expression-substrate/operator-less-root";
const ALPHA_CLASS: &str = "https://blackcatinformatics.ca/math/alphaEquivalenceClass";
const APPLICATION_EXPRESSION: &str = "https://blackcatinformatics.ca/math/ApplicationExpression";
const OPERATOR: &str = "https://blackcatinformatics.ca/math/operator";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RESTRICTION: &str = "https://blackcatinformatics.ca/logic/Restriction";
const ON_PROPERTY: &str = "https://blackcatinformatics.ca/logic/onProperty";
const MIN_QUALIFIED_CARDINALITY: &str =
    "https://blackcatinformatics.ca/logic/minQualifiedCardinality";
const ON_CLASS: &str = "https://blackcatinformatics.ca/logic/onClass";
const THING: &str = "https://blackcatinformatics.ca/logic/Thing";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(super) enum Profile {
    Reference,
    Twins,
    MultiGraph,
    Control,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Observations {
    pub producer: BuildIdentity,
    pub context_path: String,
    pub context_source_digest: String,
    pub reasoning_projection_digest: String,
    pub scenes: BTreeMap<String, Scene>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Scene {
    pub source_path: String,
    pub source_digest: String,
    pub profile: Profile,
    pub declared_keys: BTreeMap<String, Vec<String>>,
    pub reasoned: Option<Reasoned>,
    pub multi_graph_findings: Option<Vec<Finding>>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Reasoned {
    pub alpha_classes: BTreeMap<String, String>,
    pub operator_less_root_operator_edges: usize,
    pub expression_findings: Option<Vec<Finding>>,
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    [REFERENCE, TWINS, CLOSED_FORMS]
        .into_iter()
        .chain(std::iter::once(MATH))
        .map(|path| root.join(path))
        .collect()
}

fn raw(path: &str, bytes: &str) -> ActionInput {
    ActionInput::Raw {
        logical_path: path.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest: bytes.to_owned(),
    }
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let math = &catalog
        .sources()
        .sources()
        .iter()
        .find(|source| source.relative_path == MATH)
        .ok_or_else(|| fail("native source catalog omitted the selected math TBox"))?
        .ingested
        .dataset;
    let context_source_digest = catalog.document_digest(MATH)?.to_owned();
    let reasoning_context = expression_reasoning_context(math)?;
    let reasoning_projection_digest =
        purrdf::try_flat_digest_view(reasoning_context.as_ref(), CanonHash::Sha256)
            .map_err(fail)?
            .to_hex();
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let mut scenes = BTreeMap::new();
    for (path, profile, inline) in [
        (REFERENCE, Profile::Reference, None),
        (TWINS, Profile::Twins, None),
        (CLOSED_FORMS, Profile::MultiGraph, None),
        (WRAPPER_CONTROL, Profile::Control, Some(controls::TWINS)),
        (
            OPERATOR_CONTROL,
            Profile::Control,
            Some(controls::OPERATOR_LESS),
        ),
    ] {
        let bytes = match inline {
            Some(text) => text.as_bytes().to_vec(),
            None => std::fs::read(root.join(path)).map_err(fail)?,
        };
        let digest = bytes_digest(&bytes);
        let inputs = vec![
            raw(path, &digest),
            raw(
                "expression-substrate/reasoning-context",
                &reasoning_projection_digest,
            ),
        ];
        let observed = super::execution::cached_inputs(
            &store,
            &format!("expression-substrate-v4:{profile:?}"),
            inputs,
            || {
                let example = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(gmeow_errors::Diag::from)?;
                let declared_keys = declared_keys(&example);
                let reasoned = match profile {
                    Profile::MultiGraph => None,
                    Profile::Reference | Profile::Twins | Profile::Control => {
                        // Reason only over the exact canonical TBox fragment this
                        // observation consumes. Feeding the complete math module
                        // here activates unrelated contextual calculi and all native
                        // producers four times without strengthening this contract.
                        let composite = CompositeDatasetView::new(
                            vec![Arc::clone(&reasoning_context), Arc::clone(&example)],
                            ViewLimits {
                                max_sources: 2,
                                ..ViewLimits::default()
                            },
                        )
                        .map_err(gmeow_errors::Diag::from)?;
                        let asserted = composite.materialize().map_err(gmeow_errors::Diag::from)?;
                        Some(observe_reasoned(&asserted, profile == Profile::Reference)?)
                    }
                };
                let multi_graph_findings = match profile {
                    Profile::Control => None,
                    Profile::Reference | Profile::Twins | Profile::MultiGraph => {
                        Some(observe_multi_graph(&example)?)
                    }
                };
                Ok(Scene {
                    source_path: path.to_owned(),
                    source_digest: digest.clone(),
                    profile,
                    declared_keys,
                    reasoned,
                    multi_graph_findings,
                })
            },
        )?;
        scenes.insert(path.to_owned(), observed);
    }
    let observations = Observations {
        producer: BuildIdentity::current(),
        context_path: MATH.to_owned(),
        context_source_digest,
        reasoning_projection_digest,
        scenes,
    };
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observations).map_err(fail)?,
    );
    Ok(())
}

/// Project the one authored law this action consumes from the canonical math
/// module. Structural matching makes a changed, missing, or ambiguous law fail
/// closed; the action never carries a second handwritten copy of the axiom.
fn expression_reasoning_context(math: &RdfDataset) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let quads: Vec<_> = math.owned_quads().collect();
    let roots: Vec<_> = quads
        .iter()
        .filter_map(|quad| {
            (quad.subject == RdfTerm::iri(APPLICATION_EXPRESSION)
                && quad.predicate == RDFS_SUBCLASS_OF)
                .then_some(&quad.object)
                .and_then(|term| match term {
                    RdfTerm::BlankNode(label) => Some(label.clone()),
                    _ => None,
                })
        })
        .filter(|label| is_operator_minimum_restriction(&quads, label))
        .collect();
    let [root] = roots.as_slice() else {
        return Err(fail(format!(
            "canonical math TBox must contain exactly one ApplicationExpression minimum operator restriction; found {}",
            roots.len()
        )));
    };

    let mut selected_blanks = BTreeSet::from([root.clone()]);
    loop {
        let before = selected_blanks.len();
        for quad in &quads {
            if matches!(&quad.subject, RdfTerm::BlankNode(label) if selected_blanks.contains(label))
                && let RdfTerm::BlankNode(label) = &quad.object
            {
                selected_blanks.insert(label.clone());
            }
        }
        if selected_blanks.len() == before {
            break;
        }
    }

    let root_term = RdfTerm::blank_node(root);
    let mut builder = RdfDatasetBuilder::new();
    for quad in &quads {
        let root_edge = quad.subject == RdfTerm::iri(APPLICATION_EXPRESSION)
            && quad.predicate == RDFS_SUBCLASS_OF
            && quad.object == root_term;
        let closure =
            matches!(&quad.subject, RdfTerm::BlankNode(label) if selected_blanks.contains(label));
        if root_edge || closure {
            builder.push_owned_quad(quad);
        }
    }
    builder.freeze().map_err(fail)
}

fn is_operator_minimum_restriction(quads: &[purrdf::RdfQuad], label: &str) -> bool {
    let subject = RdfTerm::blank_node(label);
    let has = |predicate: &str, object: &RdfTerm| {
        quads.iter().any(|quad| {
            quad.subject == subject && quad.predicate == predicate && &quad.object == object
        })
    };
    has(RDF_TYPE, &RdfTerm::iri(RESTRICTION))
        && has(ON_PROPERTY, &RdfTerm::iri(OPERATOR))
        && has(ON_CLASS, &RdfTerm::iri(THING))
        && quads.iter().any(|quad| {
            quad.subject == subject
                && quad.predicate == MIN_QUALIFIED_CARDINALITY
                && matches!(&quad.object, RdfTerm::Literal(literal)
                    if literal.lexical_form == "1"
                        && literal.datatype.as_deref() == Some(XSD_INTEGER))
        })
}

fn declared_keys(example: &RdfDataset) -> BTreeMap<String, Vec<String>> {
    let mut keys: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for quad in example.owned_quads() {
        if quad.predicate == "https://blackcatinformatics.ca/math/structuralKey"
            && let (RdfTerm::Iri(root), RdfTerm::Literal(literal)) = (quad.subject, quad.object)
        {
            keys.entry(root).or_default().push(literal.lexical_form);
        }
    }
    keys
}

fn observe_reasoned(
    asserted: &RdfDataset,
    include_findings: bool,
) -> Result<Reasoned, gmeow_errors::Diag> {
    let reasoning_input = prepare_reasoning_input(asserted)?;
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        "gmeow.pipeline.expression-substrate.v1".to_owned(),
        *reasoning_input.ingress_contract(),
    )?])?;
    let result = reason_all(reasoning_input, &domains)?;
    let graph = match materialize_reasoned_closure(asserted, &result)? {
        ReasonedGraphOutcome::Ready(graph) => graph,
        ReasonedGraphOutcome::IncompleteClosure(findings) => {
            return Err(super::stage_err(&format!(
                "expression acceptance scene must close completely: {findings:?}"
            )));
        }
    };
    let mut alpha_classes = BTreeMap::new();
    let mut operator_less_root_operator_edges = 0;
    for quad in graph.dataset.flat_default_graph_quads() {
        if quad.p.as_iri() == Some(ALPHA_CLASS) {
            let root = quad
                .s
                .as_iri()
                .ok_or_else(|| super::stage_err("materialized expression root must be an IRI"))?;
            let class = quad.o.as_iri().ok_or_else(|| {
                super::stage_err("materialized alpha-equivalence class must be an IRI")
            })?;
            alpha_classes.insert(root.to_owned(), class.to_owned());
        }
        if quad.s.as_iri() == Some("http://example.org/math/refuted/noOperator")
            && quad.p.as_iri() == Some("https://blackcatinformatics.ca/math/operator")
        {
            operator_less_root_operator_edges += 1;
        }
    }
    let expression_findings =
        include_findings.then(|| check_math_expression_findings(asserted, &graph.dataset));
    Ok(Reasoned {
        alpha_classes,
        operator_less_root_operator_edges,
        expression_findings,
    })
}

fn observe_multi_graph(single: &Arc<RdfDataset>) -> Result<Vec<Finding>, gmeow_errors::Diag> {
    let multi = probe_graph_dataset(single)?;
    Ok(check_math_expression_findings(&multi, &multi))
}

/// Observe the already-materialized assertion product in its original roles and
/// in one explicit probe role. Both placements share the same native blank
/// identities and complete statement layers; original declarations remain, and
/// the probe is declared even for empty input. `record` supplies the preceding
/// composite's logical product, which carries no parser row locations.
fn probe_graph_dataset(single: &Arc<RdfDataset>) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let source = purrdf::CompositeSource::new(Arc::clone(single));
    let probe = source
        .clone()
        .with_graph_placement(purrdf::GraphPlacement::Named(purrdf::TermValue::iri(
            "https://blackcatinformatics.ca/gmeow/graph/probe",
        )));
    let view =
        CompositeDatasetView::from_shared_sources(vec![source, probe], ViewLimits::default())?;
    Ok(view.materialize()?)
}

#[cfg(test)]
mod routing_tests;

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
