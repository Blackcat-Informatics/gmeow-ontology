// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original mapping-role observations, kept separate from the asserted ontology.
//! Native alignment extraction preserves the RDF 1.2 reifier tables; consumers
//! receive the extracted GMEOW envelope, never a reparsable corpus substitute.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_logic_compile::ingest::DslView;
use gmeow_logic_compile::projections::{correspondence_frontend, sssom};
use gmeow_validate::grounding_observation::{Cell, Observations, TranspilationOutcome};
use purrdf::{RdfDataset, TermRef};
mod json;

use crate::stages::parse_sources::SourceCatalog;

pub(super) const CHANNEL: &str = "pipeline/grounding-catalog-observations.json";
const LOGIC: &str = "slices/grounding/logic/module.ttl";
const MATH: &str = "slices/grounding/math/module.ttl";
const GUFO: &str = "imports/gufo.ttl";
const BFO: &str = "imports/targets/bfo.ttl";
const QUANTITY: &str = "slices/grounding/math/mappings/quantity-bridges.ttl";
const CATALOGS: [&str; 4] = [
    "slices/grounding/logic/mappings/grounding-bridges.ttl",
    "slices/grounding/logic/mappings/foundation-bridges.ttl",
    QUANTITY,
    "slices/core/observations/mappings/equivalences.ttl",
];
const CONTROLS: [&str; 2] = [
    "slices/grounding/logic/tests/conformance-fixtures/grounding-bridge-wellformed.ttl",
    "slices/grounding/logic/tests/counter-examples/grounding-bridge-missing-preservation.ttl",
];

/// Retain each extracted correspondence coordinate under the shared transport owner.
fn cell(cell: sssom::EquivalenceCell) -> Cell {
    Cell {
        iri: correspondence_frontend::alignment_provenance_iri(
            &cell.subject,
            &cell.predicate,
            &cell.obj,
        ),
        subject: cell.subject,
        predicate: cell.predicate,
        obj: cell.obj,
        source_endpoint: cell.source_endpoint,
        target_endpoint: cell.target_endpoint,
        sssom_file: cell.sssom_file,
        morphism_class: cell.morphism_class,
        morphism_kind: cell.morphism_kind,
        preservation: cell.preservation,
        confidence: cell.confidence,
        grounding: cell.grounding,
    }
}

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    CATALOGS
        .into_iter()
        .chain(CONTROLS)
        .chain([LOGIC, MATH, GUFO, BFO])
        .map(|path| root.join(path))
        .collect()
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let mut observed = Observations {
        sources: BTreeMap::new(),
        catalogs: BTreeMap::new(),
        transpilation: BTreeMap::new(),
        declared_subjects: BTreeMap::new(),
        imported_classes: BTreeMap::new(),
        bfo_labels: BTreeMap::new(),
    };
    // Mapping and negative-fixture roles are deliberately absent from SourceCatalog's
    // asserted module/import inventory. These bounded original-document actions never
    // admit either role into the production object-level base.
    for path in CATALOGS {
        let bytes = std::fs::read(root.join(path)).map_err(fail)?;
        let digest = bytes_digest(&bytes);
        let (cells, transpiled): (Vec<Cell>, Option<Result<usize, gmeow_errors::RecordedDiag>>) =
            super::execution::cached_native_inputs(
                &store,
                "grounding-catalog-v1:original-native-envelope",
                vec![source_input(path, &digest)],
                || {
                    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                        .map_err(gmeow_errors::Diag::from)?;
                    let view = DslView::new(&dataset);
                    let cells = sssom::equivalence_cells(&view)?
                        .into_iter()
                        .map(cell)
                        .collect();
                    let transpiled =
                        (path == QUANTITY).then(|| transpile(&view).map_err(super::record_failure));
                    Ok(json::Catalog((cells, transpiled)))
                },
            )?
            .0;
        observed.sources.insert(path.to_owned(), digest);
        observed.catalogs.insert(path.to_owned(), cells);
        if let Some(result) = transpiled {
            observed.transpilation.insert(path.to_owned(), result);
        }
    }
    for path in CONTROLS {
        let bytes = std::fs::read(root.join(path)).map_err(fail)?;
        let digest = bytes_digest(&bytes);
        let result = super::execution::cached_native_inputs(
            &store,
            "grounding-envelope-control-v1",
            vec![source_input(path, &digest)],
            || {
                let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(gmeow_errors::Diag::from)?;
                Ok(TranspilationOutcome(
                    transpile(&DslView::new(&dataset)).map_err(super::record_failure),
                ))
            },
        )?;
        observed.sources.insert(path.to_owned(), digest);
        observed.transpilation.insert(path.to_owned(), result.0);
    }
    for path in [LOGIC, MATH, GUFO] {
        let digest = catalog.document_digest(path)?;
        let (subjects, classes, _labels) = super::execution::cached_native_inputs(
            &store,
            "grounding-source-declarations-v1",
            vec![source_input(path, digest)],
            || {
                Ok(json::Declaration(declarations(
                    catalog.document(path)?,
                    false,
                )))
            },
        )?
        .0;
        observed.sources.insert(path.to_owned(), digest.to_owned());
        observed.declared_subjects.insert(path.to_owned(), subjects);
        observed.imported_classes.insert(path.to_owned(), classes);
    }
    // The by-reference BFO target is not an asserted SourceCatalog import.
    let bytes = std::fs::read(root.join(BFO)).map_err(fail)?;
    let digest = bytes_digest(&bytes);
    let (_, classes, labels) = super::execution::cached_native_inputs(
        &store,
        "grounding-source-declarations-v1",
        vec![source_input(BFO, &digest)],
        || {
            let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                .map_err(gmeow_errors::Diag::from)?;
            Ok(json::Declaration(declarations(&dataset, true)))
        },
    )?
    .0;
    observed.sources.insert(BFO.to_owned(), digest);
    observed.imported_classes.insert(BFO.to_owned(), classes);
    observed.bfo_labels = labels;
    artifacts.insert(
        CHANNEL.to_owned(),
        purrdf_lex::json::record::to_vec(&observed),
    );
    Ok(())
}

fn transpile(view: &DslView<'_>) -> Result<usize, gmeow_errors::Diag> {
    correspondence_frontend::transpile_correspondences_indexed(view, view, &[])
        .map(|(program, _)| program.correspondences.len())
}

type Declarations = (
    BTreeSet<String>,
    BTreeSet<String>,
    BTreeMap<String, BTreeSet<String>>,
);

/// The former GraphStore flattened source graphs for these three named-term queries.
/// Reading borrowed rows across graphs has the same set semantics without rebuilding it.
fn declarations(dataset: &RdfDataset, include_labels: bool) -> Declarations {
    let mut subjects = BTreeSet::new();
    let mut classes = BTreeSet::new();
    let mut labels: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for quad in dataset.quad_refs() {
        let TermRef::Iri(subject) = quad.s else {
            continue;
        };
        subjects.insert(subject.to_owned());
        match (quad.p, quad.o) {
            (
                TermRef::Iri("http://www.w3.org/1999/02/22-rdf-syntax-ns#type"),
                TermRef::Iri("http://www.w3.org/2002/07/owl#Class"),
            ) => {
                classes.insert(subject.to_owned());
            }
            (
                TermRef::Iri("http://www.w3.org/2000/01/rdf-schema#label"),
                TermRef::Literal { lexical, .. },
            ) if include_labels => {
                labels
                    .entry(subject.to_owned())
                    .or_default()
                    .insert(lexical.to_owned());
            }
            _ => {}
        }
    }
    (subjects, classes, labels)
}

fn source_input(path: &str, digest: &str) -> ActionInput {
    ActionInput::Raw {
        logical_path: path.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest: digest.to_owned(),
    }
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
