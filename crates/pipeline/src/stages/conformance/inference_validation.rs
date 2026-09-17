// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer observations for GMEOW commitment, rivalry and self-attack constraints.
//! The two selected controls share one native module and prepared shape model.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use purrdf::shapes::engine::{PreparedShapes, parse_shapes};
use purrdf::{RdfDataset, RdfDatasetBuilder};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

mod controls;

pub(crate) const CHANNEL: &str = "pipeline/inference-validation-observations.json";
const MODULE: &str = "slices/core/inference/module.ttl";
const SHAPES: &str = "slices/core/inference/shapes.ttl";
const PROFILE: &str = "inference-slice-plus-constraint-shapes:flattened-default:v1";

#[derive(Serialize, Deserialize)]
struct Observations {
    profile: String,
    sources: BTreeMap<String, String>,
    controls: BTreeMap<String, Control>,
}

#[derive(Serialize, Deserialize)]
struct Control {
    input_digest: String,
    results: Vec<ResultMessage>,
}

#[derive(Serialize, Deserialize)]
struct ResultMessage {
    severity: String,
    message: Option<String>,
}

pub(crate) fn input_files(root: &Path) -> Vec<PathBuf> {
    [MODULE, SHAPES]
        .into_iter()
        .map(|path| root.join(path))
        .collect()
}

/// `constraint_shapes` must be the consumed export product of this producer run.
/// Never reads the previous generated tree or rerenders the selected projection.
pub(crate) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    constraint_shapes: &[u8],
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let source_shapes = std::fs::read(root.join(SHAPES)).map_err(fail)?;
    let sources = BTreeMap::from([
        (
            MODULE.to_owned(),
            catalog.document_digest(MODULE)?.to_owned(),
        ),
        (SHAPES.to_owned(), bytes_digest(&source_shapes)),
        (
            crate::stages::constraint_shapes::CONSTRAINT_SHAPES_PATH.to_owned(),
            bytes_digest(constraint_shapes),
        ),
    ]);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let prepared: OnceLock<(Arc<RdfDataset>, PreparedShapes)> = OnceLock::new();
    let mut observations = Observations {
        profile: PROFILE.to_owned(),
        sources,
        controls: BTreeMap::new(),
    };
    for (name, body) in [
        ("wellformed", controls::WELLFORMED),
        ("malformed", controls::MALFORMED),
    ] {
        let text = format!("{}{body}", controls::PRELUDE);
        let input_digest = bytes_digest(text.as_bytes());
        let mut inputs: Vec<_> = observations
            .sources
            .iter()
            .map(|(path, digest)| ActionInput::Raw {
                logical_path: path.clone(),
                file_kind: FileKind::File,
                executable: false,
                digest: digest.clone(),
            })
            .collect();
        inputs.push(ActionInput::Raw {
            logical_path: format!("inference-control/{name}"),
            file_kind: FileKind::Aggregate,
            executable: false,
            digest: input_digest.clone(),
        });
        let observed = super::execution::cached_inputs(&store, PROFILE, inputs, || {
            let (module, shapes) = prepared.get_or_try_init(|| -> gmeow_errors::Result<_> {
                let module = flatten(catalog.document(MODULE)?)?;
                // Both sources are already serialization-boundary inputs. Parse their
                // ordered document once; no module serialization or N-Triples reparse.
                let authored =
                    std::str::from_utf8(&source_shapes).map_err(gmeow_errors::Diag::from)?;
                let generated =
                    std::str::from_utf8(constraint_shapes).map_err(gmeow_errors::Diag::from)?;
                let shape_text = format!("{authored}\n{generated}");
                let shapes = parse_shapes(&shape_text, None)
                    .map_err(|message| super::stage_err(&message))?;
                Ok((module, PreparedShapes::new(Arc::new(shapes))))
            })?;
            let instance = purrdf::parse_dataset(text.as_bytes(), "text/turtle", None)
                .map_err(gmeow_errors::Diag::from)?;
            let instance = flatten(&instance)?;
            let mut builder = RdfDatasetBuilder::new();
            builder.push_dataset(module);
            builder.push_dataset(&instance);
            let data = builder.freeze().map_err(gmeow_errors::Diag::from)?;
            let report = shapes
                .bind_projected_dataset(data)
                .and_then(|validator| validator.validate())
                .map_err(|message| super::stage_err(&message))?;
            let mut results: Vec<_> = report
                .results
                .into_iter()
                .map(|result| ResultMessage {
                    severity: result.severity.iri().to_owned(),
                    message: result.message,
                })
                .collect();
            results.sort_by(|a, b| (&a.severity, &a.message).cmp(&(&b.severity, &b.message)));
            Ok(Control {
                input_digest: input_digest.clone(),
                results,
            })
        })?;
        observations.controls.insert(name.to_owned(), observed);
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&observations).map_err(fail)?,
    );
    Ok(())
}

/// Preserve the original tests' explicit flattened SHACL data role in native rows.
fn flatten(source: &RdfDataset) -> Result<Arc<RdfDataset>, gmeow_errors::Diag> {
    let mut quads = purrdf::flat_rdf_quads_from_dataset(source);
    for quad in &mut quads {
        quad.graph_name = None;
    }
    purrdf::flat_dataset_from_quads(&quads).map_err(|message| super::stage_err(&message))
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}
