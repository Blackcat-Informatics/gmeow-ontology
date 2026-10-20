// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Admit only declared import owners and their native RDF structure. References
//! to unrelated named terms do not copy the surrounding bundle into a foreign score.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use gmeow_errors::ResultExt;
use gmeow_logic_compile::frontend::{SourceEdgeRole, StructuralSourceGraph};
use gmeow_logic_compile::lists::{RDF_FIRST, RDF_NIL, RDF_REST, constructor};
use purrdf::{DatasetView, GraphMatch, RdfDataset, RdfDatasetBuilder, TermId, TermRef};

const IMPORTS: [&str; 3] = [
    "https://blackcatinformatics.ca/logic/imports",
    "http://www.w3.org/2002/07/owl#imports",
    "https://blackcatinformatics.ca/gmeow/sliceDependsOn",
];
const OWNER_PROPERTIES: [&str; 2] = [
    "http://www.w3.org/2000/01/rdf-schema#isDefinedBy",
    "https://blackcatinformatics.ca/logic/inModule",
];

fn declared_owner(ds: &RdfDataset, owner: &str) -> Option<TermId> {
    let subject = ds.term_id_by_iri(owner)?;
    let typing = ds.term_id_by_iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type")?;
    ds.quads_for_pattern(Some(subject), Some(typing), None, GraphMatch::Any)
        .any(|quad| {
            matches!(
                ds.resolve(quad.o),
                TermRef::Iri(
                    "https://blackcatinformatics.ca/logic/Ontology"
                        | "https://blackcatinformatics.ca/logic/Module"
                        | "http://www.w3.org/2002/07/owl#Ontology"
                        | "https://blackcatinformatics.ca/gmeow/Slice"
                        | "https://blackcatinformatics.ca/gmeow/GroundingSlice"
                )
            )
        })
        .then_some(subject)
}

fn declarations(
    ds: &RdfDataset,
    subject: Option<TermId>,
) -> gmeow_errors::Result<BTreeSet<String>> {
    let mut imports = BTreeSet::new();
    for iri in IMPORTS {
        if let Some(predicate) = ds.term_id_by_iri(iri) {
            for quad in ds.quads_for_pattern(subject, Some(predicate), None, GraphMatch::Any) {
                let TermRef::Iri(owner) = ds.resolve(quad.o) else {
                    return Err(super::invalid(format!(
                        "declared quality import {iri} requires an ontology IRI"
                    )));
                };
                imports.insert(owner.to_owned());
            }
        }
    }
    Ok(imports)
}

/// Resolve declared imports from the bundle or explicit foreign import documents.
/// Missing owners fail; no network fetch, checkout discovery or weaker context is used.
pub(super) fn assemble(
    data: &RdfDataset,
    manifest: &RdfDataset,
    sources: &[&RdfDataset],
) -> gmeow_errors::Result<(Arc<RdfDataset>, Vec<String>)> {
    let mut pending = declarations(data, None)?;
    pending.extend(declarations(manifest, None)?);
    let mut admitted = BTreeSet::new();
    let mut selected = vec![BTreeSet::new(); sources.len()];
    while let Some(owner) = pending.pop_first() {
        if !admitted.insert(owner.clone()) {
            continue;
        }
        let mut found = if let Some(subject) = declared_owner(data, &owner) {
            pending.extend(declarations(data, Some(subject))?);
            true
        } else {
            false
        };
        for (index, source) in sources.iter().enumerate() {
            if let Some(subject) = declared_owner(source, &owner) {
                selected[index].insert(subject);
                pending.extend(declarations(source, Some(subject))?);
                found = true;
                for owned_by in OWNER_PROPERTIES
                    .iter()
                    .filter_map(|iri| source.term_id_by_iri(iri))
                {
                    for quad in source.quads_for_pattern(
                        None,
                        Some(owned_by),
                        Some(subject),
                        GraphMatch::Any,
                    ) {
                        selected[index].insert(quad.s);
                        found = true;
                    }
                }
            }
        }
        if !found {
            return Err(super::invalid(format!(
                "declared quality import <{owner}> is absent from the bundle and supplied import documents"
            )));
        }
    }
    let mut builder = RdfDatasetBuilder::new();
    builder.push_dataset(data)?;
    for (source, subjects) in sources.iter().zip(selected) {
        if !subjects.is_empty() {
            let imported = subset(source, subjects)?;
            builder.push_dataset(imported.as_ref())?;
        }
    }
    Ok((builder.freeze()?, admitted.into_iter().collect()))
}

fn subset(
    ds: &RdfDataset,
    mut subjects: BTreeSet<TermId>,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut by_statement_subject: BTreeMap<TermId, BTreeSet<TermId>> = BTreeMap::new();
    let mut metadata_objects: BTreeMap<TermId, Vec<(TermId, TermId)>> = BTreeMap::new();
    for (reifier, predicate, object, _) in ds.annotations_with_graph() {
        metadata_objects
            .entry(reifier)
            .or_default()
            .push((predicate, object));
    }
    for (reifier, triple) in ds.reifiers() {
        if let TermRef::Triple { s, .. } = ds.resolve(triple) {
            by_statement_subject.entry(s).or_default().insert(reifier);
        }
    }
    let mut pending: Vec<_> = subjects.iter().copied().collect();
    let mut syntax_nodes = BTreeSet::new();
    let mut required_lists = BTreeSet::new();
    let mut expanded_lists = BTreeSet::new();
    let mut expanded_syntax_lists = BTreeSet::new();
    while let Some(subject) = pending.pop() {
        let owns_members = syntax_nodes.contains(&subject);
        if (!expanded_lists.contains(&subject)
            || (owns_members && !expanded_syntax_lists.contains(&subject)))
            && (required_lists.contains(&subject)
                || [RDF_FIRST, RDF_REST].iter().any(|iri| {
                    ds.term_id_by_iri(iri).is_some_and(|predicate| {
                        ds.quads_for_pattern(Some(subject), Some(predicate), None, GraphMatch::Any)
                            .next()
                            .is_some()
                    })
                }))
        {
            let first = ds.term_id_by_iri(RDF_FIRST);
            let rest = ds.term_id_by_iri(RDF_REST);
            let mut cells = Vec::new();
            // The native walker owns list validity and cycle handling. Retain
            // its visited cells as well as members, without another list walk.
            purrdf_core::walk_rdf_list(
                subject,
                ds.term_id_by_iri(RDF_NIL),
                |cell| {
                    let value = DatasetView::sole_object(ds, cell, first, GraphMatch::Any);
                    if let purrdf_core::SoleObject::One(member) = value {
                        cells.push((cell, member));
                    }
                    value
                },
                |cell| DatasetView::sole_object(ds, cell, rest, GraphMatch::Any),
            )
            .ctx("declared quality import RDF list")?;
            for (cell, member) in cells {
                expanded_lists.insert(cell);
                if subjects.insert(cell) {
                    pending.push(cell);
                }
                if owns_members {
                    expanded_syntax_lists.insert(cell);
                    if matches!(ds.resolve(member), TermRef::Iri(_) | TermRef::Blank { .. }) {
                        let newly_owned = syntax_nodes.insert(member);
                        if subjects.insert(member) || newly_owned {
                            pending.push(member);
                        }
                    }
                }
            }
            expanded_lists.insert(subject);
            if owns_members {
                expanded_syntax_lists.insert(subject);
            }
        }
        if let Some(reifiers) = by_statement_subject.get(&subject) {
            for &reifier in reifiers {
                if subjects.insert(reifier) {
                    pending.push(reifier);
                }
            }
        }
        let outgoing = ds
            .quads_for_pattern(Some(subject), None, None, GraphMatch::Any)
            .map(|quad| (quad.p, quad.o))
            .chain(
                metadata_objects
                    .get(&subject)
                    .into_iter()
                    .flatten()
                    .copied(),
            );
        for (predicate_id, object) in outgoing {
            let TermRef::Iri(predicate) = ds.resolve(predicate_id) else {
                continue;
            };
            let owns_structure = matches!(
                StructuralSourceGraph::predicate_role(predicate),
                Some(
                    SourceEdgeRole::FormulaComponent
                        | SourceEdgeRole::TermComponent
                        | SourceEdgeRole::ExclusiveFormulaUse
                        | SourceEdgeRole::RecoveryOwnership
                        | SourceEdgeRole::RuleComponent
                        | SourceEdgeRole::ExplicitFormulaSelection
                )
            );
            if owns_structure && syntax_nodes.insert(object) && subjects.contains(&object) {
                // A shared list may have been visited first through a reference.
                // Revisit it when a real ownership edge reaches it.
                pending.push(object);
            }
            let list_head = constructor(predicate).is_some();
            if list_head
                && required_lists.insert(object)
                && subjects.contains(&object)
                && !expanded_lists.contains(&object)
            {
                pending.push(object);
            }
            if (list_head || (owns_structure && matches!(ds.resolve(object), TermRef::Iri(_))))
                && subjects.insert(object)
            {
                pending.push(object);
            }
            let mut structure = vec![object];
            let mut visited = BTreeSet::new();
            while let Some(term) = structure.pop() {
                if !visited.insert(term) {
                    continue;
                }
                match ds.resolve(term) {
                    TermRef::Blank { .. } if subjects.insert(term) => pending.push(term),
                    TermRef::Triple { s, p, o } => structure.extend([s, p, o]),
                    _ => {}
                }
            }
        }
    }
    let mut builder = RdfDatasetBuilder::new();
    for (index, quad) in ds.quads().enumerate() {
        if subjects.contains(&quad.s) {
            builder.push_owned_quad(&ds.to_owned_quad(index, quad))?;
        }
    }
    for (reifier, triple, graph) in ds.reifiers_with_graph() {
        if subjects.contains(&reifier) {
            builder.push_owned_reifier(&ds.to_owned_reifier(reifier, triple, graph))?;
        }
    }
    for (reifier, predicate, object, graph) in ds.annotations_with_graph() {
        if subjects.contains(&reifier) {
            builder.push_owned_annotation(
                &ds.to_owned_annotation(reifier, predicate, object, graph),
            )?;
        }
    }
    Ok(builder.freeze()?)
}
