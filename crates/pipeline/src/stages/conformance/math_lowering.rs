// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Authenticated source-only math lowering observations. Required rejection and
//! structural-identity evidence is produced before tests, without a reasoned union.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{ActionStore, STORE_FORMAT_VERSION, StoreLimits, bytes_digest};
use gmeow_logic::math_expression::analysis::{self, MathLoweringError};
use purrdf::{RdfDataset, TermRef};
use serde::{Deserialize, Serialize};

pub(crate) const CHANNEL: &str = "pipeline/math-lowering-observations.json";
pub(super) const MODULE: &str = "slices/grounding/math/module.ttl";
pub(super) const REFERENCE: &str = "slices/grounding/math/examples/reference-ast-act.ttl";
pub(super) const COUNTER_EXAMPLES: &str = "slices/grounding/math/tests/counter-examples";
pub(super) const ALPHA_A: &str =
    "slices/grounding/math/tests/conformance-fixtures/alpha-equivalent-pair-a.ttl";
pub(super) const ALPHA_B: &str =
    "slices/grounding/math/tests/conformance-fixtures/alpha-equivalent-pair-b.ttl";
pub(super) const SHADOW: &str =
    "slices/grounding/math/tests/conformance-fixtures/alpha-equivalent-shadowing.ttl";
const STRUCTURAL_KEY: &str = "https://blackcatinformatics.ca/math/structuralKey";

#[derive(Serialize, Deserialize)]
pub(super) struct SourceObservation {
    /// All values remain visible so duplicate or malformed source fields fail grading.
    pub finite_value_spaces: BTreeMap<String, FiniteValueSpace>,
    pub source_path: String,
    pub subjects: BTreeSet<String>,
    pub authored_keys: BTreeMap<String, Vec<String>>,
    pub keys: BTreeMap<String, Result<String, MathLoweringError>>,
    pub application_nodes: usize,
    /// Only the shadowing fixture requests an independent repeated lowering.
    pub repeated_shadow: Option<Result<String, MathLoweringError>>,
    /// Selected native lint work shares this source action's original parse.
    /// Moved into its own compact published channel before this artifact is emitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_lint: Option<super::source_lint::SourceRecord>,
}

#[derive(Default, Serialize, Deserialize)]
pub(super) struct FiniteValueSpace {
    pub datatypes: BTreeSet<String>,
    pub counts: BTreeSet<String>,
}

#[derive(Serialize, Deserialize)]
pub(super) struct Observations {
    pub depth_limit: usize,
    pub source_digests: BTreeMap<String, String>,
    pub sources: BTreeMap<String, SourceObservation>,
}

pub(super) fn input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut inputs = Vec::new();
    super::execution::collect_files(&root.join(COUNTER_EXAMPLES), &mut inputs)?;
    inputs.retain(|path| path.extension().is_some_and(|ext| ext == "ttl"));
    inputs.extend(
        [MODULE, REFERENCE, ALPHA_A, ALPHA_B, SHADOW]
            .into_iter()
            .map(|path| root.join(path)),
    );
    inputs.sort();
    Ok(inputs)
}

/// Reuse the grounding producer's original native module/example parse.
pub(super) fn observe(path: &str, dataset: &RdfDataset) -> Option<SourceObservation> {
    matches!(path, MODULE | REFERENCE).then(|| summarize(path, dataset))
}

fn summarize(path: &str, dataset: &RdfDataset) -> SourceObservation {
    let mut subjects = BTreeSet::new();
    let mut finite_subjects = BTreeSet::new();
    let mut finite_fields = BTreeMap::<String, FiniteValueSpace>::new();
    let mut application_subjects = BTreeSet::new();
    let mut authored_keys = BTreeMap::<String, Vec<String>>::new();
    for quad in dataset.quads().filter(|quad| quad.g.is_none()) {
        let TermRef::Iri(subject) = dataset.resolve(quad.s) else {
            continue;
        };
        subjects.insert(subject.to_owned());
        if let TermRef::Iri(predicate) = dataset.resolve(quad.p) {
            match predicate {
                "https://blackcatinformatics.ca/math/hasCardinality"
                    if matches!(
                        dataset.resolve(quad.o),
                        TermRef::Iri("https://blackcatinformatics.ca/math/cardinalityFinite")
                    ) =>
                {
                    finite_subjects.insert(subject.to_owned());
                }
                "http://www.w3.org/2000/01/rdf-schema#seeAlso" => {
                    let datatype = match dataset.resolve(quad.o) {
                        TermRef::Iri(datatype) => datatype.to_owned(),
                        other => format!("invalid datatype field: {other:?}"),
                    };
                    finite_fields
                        .entry(subject.to_owned())
                        .or_default()
                        .datatypes
                        .insert(datatype);
                }
                "https://blackcatinformatics.ca/math/quantityValue" => {
                    let count = match dataset.resolve(quad.o) {
                        TermRef::Literal { lexical, .. } => lexical.to_owned(),
                        other => format!("invalid count field: {other:?}"),
                    };
                    finite_fields
                        .entry(subject.to_owned())
                        .or_default()
                        .counts
                        .insert(count);
                }
                _ => {}
            }
            if predicate == STRUCTURAL_KEY
                && let TermRef::Literal { lexical, .. } = dataset.resolve(quad.o)
            {
                authored_keys
                    .entry(subject.to_owned())
                    .or_default()
                    .push(lexical.to_owned());
            }
            if matches!(
                predicate,
                "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
                    | "https://blackcatinformatics.ca/logic/instanceOf"
            ) && matches!(
                dataset.resolve(quad.o),
                TermRef::Iri("https://blackcatinformatics.ca/math/ApplicationExpression")
            ) {
                application_subjects.insert(subject.to_owned());
            }
        }
    }
    SourceObservation {
        finite_value_spaces: finite_subjects
            .into_iter()
            .map(|subject| {
                let fields = finite_fields.remove(&subject).unwrap_or_default();
                (subject, fields)
            })
            .collect(),
        source_path: path.to_owned(),
        subjects,
        authored_keys,
        keys: if path == MODULE {
            BTreeMap::new()
        } else {
            analysis::structural_keys(dataset)
        },
        application_nodes: application_subjects.len(),
        repeated_shadow: (path == SHADOW)
            .then(|| analysis::structural_key_at(dataset, "http://example.org/math/outerSum")),
        source_lint: None,
    }
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<BTreeMap<String, super::source_lint::SourceRecord>> {
    let fail = |error: String| super::stage_err(&error);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(error.to_string()))?;
    let mut sources = BTreeMap::new();
    let mut source_digests = BTreeMap::new();
    let mut source_lints = BTreeMap::new();
    for path in input_files(root)? {
        let relative = path
            .strip_prefix(root)
            .map_err(|error| fail(error.to_string()))?
            .to_str()
            .ok_or_else(|| fail("non-UTF-8 math lowering source path".to_owned()))?;
        // Those two sources already have original native grounding observations.
        if matches!(relative, MODULE | REFERENCE) {
            continue;
        }
        let bytes = std::fs::read(&path).map_err(|error| fail(error.to_string()))?;
        let digest = bytes_digest(&bytes);
        let mut inputs = vec![gmeow_action_cache::ActionInput::Raw {
            logical_path: relative.to_owned(),
            file_kind: gmeow_action_cache::FileKind::File,
            executable: false,
            digest: digest.clone(),
        }];
        if super::source_lint::selected(relative) {
            inputs.push(super::source_lint::configuration_input()?);
        }
        let mut observed = super::execution::cached_inputs(
            &store,
            "source-only-math-lowering-v3-datatype-inventory",
            inputs,
            || {
                let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
                    .map_err(gmeow_errors::Diag::from)?;
                let mut observed = summarize(relative, &dataset);
                observed.source_lint = super::source_lint::observe(relative, &digest, &dataset);
                Ok(observed)
            },
        )?;
        if let Some(lint) = observed.source_lint.take() {
            source_lints.insert(relative.to_owned(), lint);
        }
        sources.insert(relative.to_owned(), observed);
        source_digests.insert(relative.to_owned(), digest);
    }
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&Observations {
            depth_limit: analysis::depth_limit(),
            source_digests,
            sources,
        })
        .map_err(|error| fail(error.to_string()))?,
    );
    Ok(source_lints)
}

#[cfg(test)]
#[path = "math_lowering_test_support.rs"]
mod test_support;
#[cfg(test)]
pub(super) use test_support::DEPTH;
