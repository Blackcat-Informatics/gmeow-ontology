// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Selected native case observations share one lazy parse per source. Each
//! operation has its own bounded action entry; no dataset survives its source task.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use gmeow_action_cache::{ActionInput, ActionStore, FileKind, bytes_digest};
use gmeow_logic::reason::refute::{ClassDiagnosticOutcome, class_diagnostic};
use gmeow_logic::reason::{
    DomainProfile, LogicalGraph, PreparedReasoningInput, SelectedDomains, SelectedLogicalWorld,
    prepare_reasoning_input,
};
use gmeow_logic::result::NativeExecutionEvidence;
use purrdf::{NativeRdfFormat, RdfDataset, parse_dataset};
use rayon::prelude::*;

pub(crate) const CLASS_ADMISSION_CHANNEL: &str =
    "pipeline/class-source-admission-observations.json";
pub(crate) const CLASS_DIAGNOSTIC_CHANNEL: &str = "pipeline/class-diagnostic-observations.json";
pub(crate) const DETERMINISM_CHANNEL: &str = "pipeline/refutation-determinism-observations.json";
pub(super) const FULL_CORPORA: &[&str] = &[
    "conformance/logic/cases/external/w3c-owl2-full-divergence",
    "conformance/logic/cases/external/w3c-owl2-full-native",
];
pub(super) const DETERMINISM_INPUTS: &[&str] = &[
    "conformance/logic/cases/datatype-value-space/length-facet-empty/input.nq",
    "conformance/logic/cases/external/w3c-owl2-full-native/footnote-not-about-self/input.nq",
];

/// Fast, positive witnesses retained by the ordinary producer. The exhaustive
/// 152-case class-diagnostic sweep is breadth work produced by [`super::heavy`].
/// These five cases are the complete current set whose selected class execution
/// proves a conflict, so the local gate still exercises every positive proof
/// carried by the corpus without paying for its 147 negative controls.
pub(super) const CLASS_DIAGNOSTIC_WITNESSES: &[&str] = &[
    "conformance/logic/cases/external/w3c-owl2-full-native/webont-description-logic-001/input.nq",
    "conformance/logic/cases/external/w3c-owl2-full-native/webont-description-logic-101/input.nq",
    "conformance/logic/cases/external/w3c-owl2-full-native/webont-description-logic-103/input.nq",
    "conformance/logic/cases/external/w3c-owl2-full-native/webont-description-logic-104/input.nq",
    "conformance/logic/cases/external/w3c-owl2-full-native/webont-description-logic-504/input.nq",
];

pub(super) type Consistency = BTreeMap<
    String,
    Result<gmeow_conformance::consistency::Observation, gmeow_errors::RecordedDiag>,
>;
pub(super) type Admissions = BTreeMap<
    String,
    Result<
        gmeow_conformance::native_observation::SourceAdmissionObservation,
        gmeow_errors::RecordedDiag,
    >,
>;

pub(super) struct NativeObservations {
    pub(super) consistency: Consistency,
    pub(super) admissions: Admissions,
}

pub(super) type ClassDiagnostics =
    BTreeMap<String, Result<ClassDiagnosticOutcome, gmeow_errors::RecordedDiag>>;
pub(super) type Determinism =
    BTreeMap<String, Result<[NativeExecutionEvidence; 2], gmeow_errors::RecordedDiag>>;

#[derive(Default)]
struct Selection {
    consistency: bool,
    class_admission: bool,
    class_diagnostic: bool,
    repeat_kernel: bool,
}

/// Immutable source bytes bind independent actions. Native parsing happens only
/// if a selected observation misses; sibling misses borrow that same dataset.
struct NativeInput {
    inputs: Vec<ActionInput>,
    bytes: Vec<u8>,
    dataset: OnceLock<Arc<RdfDataset>>,
}

impl NativeInput {
    fn new(root: &Path, path: &Path) -> gmeow_errors::Result<Self> {
        let fail = |error: String| super::stage_err(&error);
        let bytes = std::fs::read(path).map_err(|error| fail(error.to_string()))?;
        Ok(Self {
            inputs: vec![ActionInput::Raw {
                logical_path: path
                    .strip_prefix(root)
                    .map_err(|error| fail(error.to_string()))?
                    .to_string_lossy()
                    .into_owned(),
                file_kind: FileKind::File,
                executable: false,
                digest: bytes_digest(&bytes),
            }],
            bytes,
            dataset: OnceLock::new(),
        })
    }

    /// Every production operation also binds the exact authored operation profile.
    fn bind_profile(mut self, root: &Path, path: &Path) -> gmeow_errors::Result<Self> {
        let path = path
            .parent()
            .ok_or_else(|| super::stage_err("native case input has no parent"))?
            .join("profile.json");
        let bytes = std::fs::read(&path).map_err(|error| super::stage_err(&error.to_string()))?;
        self.inputs.push(ActionInput::Raw {
            logical_path: path
                .strip_prefix(root)
                .map_err(|error| super::stage_err(&error.to_string()))?
                .to_string_lossy()
                .into_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: bytes_digest(&bytes),
        });
        Ok(self)
    }

    fn observe<T: serde::Serialize + serde::de::DeserializeOwned>(
        &self,
        store: &ActionStore,
        operation: &str,
        observe: impl Fn(&RdfDataset) -> Result<T, gmeow_errors::Diag>,
    ) -> gmeow_errors::Result<T> {
        super::execution::cached_inputs(store, operation, self.inputs.clone(), || {
            let dataset = self.dataset.get_or_try_init(|| {
                parse_dataset(&self.bytes, NativeRdfFormat::NQuads.media_type(), None)
                    .map_err(gmeow_errors::Diag::from)
            })?;
            observe(dataset)
        })
    }
}

/// Preserve both complete W3C inventories, with hard failures for unreadable or
/// unsupported directory entries. Non-case documentation files are not inputs.
fn profiled_inputs(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut inputs = Vec::new();
    for corpus in FULL_CORPORA {
        for entry in std::fs::read_dir(root.join(corpus))
            .map_err(|error| super::stage_err(&error.to_string()))?
        {
            let entry = entry.map_err(|error| super::stage_err(&error.to_string()))?;
            let kind = entry
                .file_type()
                .map_err(|error| super::stage_err(&error.to_string()))?;
            if kind.is_dir() {
                let input = entry.path().join("input.nq");
                if input.is_file() {
                    inputs.push(input);
                }
            } else if !kind.is_file() {
                return Err(super::stage_err(&format!(
                    "unsupported refutation corpus entry {}",
                    entry.path().display()
                )));
            }
        }
    }
    inputs.sort();
    Ok(inputs)
}

fn is_class_source_admission(path: &Path) -> gmeow_errors::Result<bool> {
    let profile_path = path
        .parent()
        .ok_or_else(|| super::stage_err("native case input has no parent"))?
        .join("profile.json");
    let profile: serde_json::Value = serde_json::from_slice(
        &std::fs::read(&profile_path).map_err(|error| super::stage_err(&error.to_string()))?,
    )
    .map_err(|error| super::stage_err(&error.to_string()))?;
    let profile = gmeow_conformance::profile::parse_profile(&path.display().to_string(), &profile)?;
    Ok(profile.verdict_mode == gmeow_conformance::profile::VerdictMode::ClassSourceAdmission)
}

fn observe_class_diagnostic(
    input: &NativeInput,
    store: &ActionStore,
) -> gmeow_errors::Result<ClassDiagnosticOutcome> {
    input.observe(store, "class-diagnostic-v1", |dataset| {
        let input = prepare_reasoning_input(dataset)?;
        let domains = source_theory_domains(&input, "urn:gmeow:conformance:class-diagnostic-v1")?;
        class_diagnostic(input, &domains, None)
    })
}

/// Produce the complete class-diagnostic inventory for the explicit heavy lane.
/// Each case remains an independently reusable action; the returned aggregate is
/// complete even when the bounded store later evicts an individual child.
pub(super) fn exhaustive_class_diagnostics(
    root: &Path,
    store: &ActionStore,
) -> gmeow_errors::Result<ClassDiagnostics> {
    let results = class_diagnostic_inputs(root)?
        .into_par_iter()
        .map(|path| {
            let key = path
                .strip_prefix(root)
                .map_err(|error| super::stage_err(&error.to_string()))?
                .to_string_lossy()
                .into_owned();
            let input = NativeInput::new(root, &path)?.bind_profile(root, &path)?;
            Ok((
                key,
                observe_class_diagnostic(&input, store).map_err(super::record_failure),
            ))
        })
        .collect::<Vec<gmeow_errors::Result<_>>>();
    results.into_iter().collect()
}

pub(super) fn class_diagnostic_inputs(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    profiled_inputs(root)?
        .into_iter()
        .filter_map(|path| match is_class_source_admission(&path) {
            Ok(true) => None,
            Ok(false) => Some(Ok(path)),
            Err(error) => Some(Err(error)),
        })
        .collect()
}

/// This corpus operation explicitly treats its supplied input as a logical theory.
/// Selection uses the retained ingress inventory and commitment, never another scan.
fn source_theory_domains(
    input: &PreparedReasoningInput,
    authority: &str,
) -> gmeow_errors::Result<SelectedDomains> {
    SelectedDomains::new(
        input
            .source_contexts()
            .values()
            .map(|graph| {
                SelectedLogicalWorld::new(
                    LogicalGraph::from_graph(graph.clone()),
                    DomainProfile::NonemptyObjectDomainV1,
                    authority.to_owned(),
                    *input.ingress_contract(),
                )
            })
            .collect::<gmeow_errors::Result<Vec<_>>>()?,
    )
}

/// Independent determinism executions retain the complete same-run native evidence.
fn full_native_observation(dataset: &RdfDataset) -> gmeow_errors::Result<NativeExecutionEvidence> {
    let input = prepare_reasoning_input(dataset)?;
    let domains = source_theory_domains(&input, "urn:gmeow:conformance:native-determinism-v2")?;
    let result = gmeow_logic::reason::reason_all(input, &domains)?;
    result.validate_native_closure()?;
    Ok(result.native_execution()?.clone())
}

pub(super) fn record(
    root: &Path,
    consistency_inputs: Vec<PathBuf>,
    store: &ActionStore,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<NativeObservations> {
    let mut selected = BTreeMap::<PathBuf, Selection>::new();
    for path in consistency_inputs {
        selected.entry(path).or_default().consistency = true;
    }
    for path in profiled_inputs(root)? {
        let class_source_admission = is_class_source_admission(&path)?;
        let class_diagnostic = CLASS_DIAGNOSTIC_WITNESSES
            .iter()
            .any(|witness| root.join(witness) == path);
        if class_source_admission {
            selected.entry(path).or_default().class_admission = true;
        } else if class_diagnostic {
            selected.entry(path).or_default().class_diagnostic = true;
        }
    }
    for path in DETERMINISM_INPUTS {
        selected.entry(root.join(path)).or_default().repeat_kernel = true;
    }
    let observations = selected
        .par_iter()
        .map(|(path, selected)| {
            let input = NativeInput::new(root, path)?.bind_profile(root, path)?;
            let key = path
                .strip_prefix(root)
                .map_err(|error| super::stage_err(&error.to_string()))?
                .to_string_lossy()
                .into_owned();
            let consistency = selected.consistency.then(|| {
                input.observe(store, "native-consistency", |dataset| {
                    gmeow_conformance::consistency::observe(dataset)
                })
            });
            let admission = selected.class_admission.then(|| {
                input.observe(store, "class-source-admission-v1", |dataset| {
                    let observed = gmeow_conformance::native_observation::observe(dataset, *blake3::hash(&input.bytes).as_bytes(),
                        gmeow_conformance::native_observation::NativeCaseOperation::ClassSourceAdmission)?;
                    let gmeow_conformance::native_observation::NativeCaseObservation::ClassSourceAdmission(observed) = observed else {
                        return Err(super::stage_err("source admission returned another native operation"));
                    };
                    Ok(observed)
                })
            });
            let class_diagnostic = selected.class_diagnostic.then(|| {
                observe_class_diagnostic(&input, store)
            });
            let repeat = selected.repeat_kernel.then(|| {
                input.observe(store, "native-execution-determinism-v2", |dataset| {
                    Ok([full_native_observation(dataset)?, full_native_observation(dataset)?])
                })
            });
            Ok((key, consistency, admission, class_diagnostic, repeat))
        })
        .collect::<Vec<gmeow_errors::Result<_>>>();
    let mut consistency = Consistency::new();
    let mut admissions = Admissions::new();
    let mut diagnostics = ClassDiagnostics::new();
    let mut determinism = Determinism::new();
    for result in observations {
        let (key, value, admission, class_diagnostic, repeat) = result?;
        if let Some(value) = value {
            consistency.insert(key.clone(), value.map_err(super::record_failure));
        }
        if let Some(value) = admission {
            admissions.insert(key.clone(), value.map_err(super::record_failure));
        }
        if let Some(value) = class_diagnostic {
            diagnostics.insert(key.clone(), value.map_err(super::record_failure));
        }
        if let Some(value) = repeat {
            determinism.insert(key, value.map_err(super::record_failure));
        }
    }
    artifacts.insert(
        CLASS_DIAGNOSTIC_CHANNEL.to_owned(),
        serde_json::to_vec(&diagnostics).map_err(|error| super::stage_err(&error.to_string()))?,
    );
    artifacts.insert(
        DETERMINISM_CHANNEL.to_owned(),
        serde_json::to_vec(&determinism).map_err(|error| super::stage_err(&error.to_string()))?,
    );
    artifacts.insert(
        CLASS_ADMISSION_CHANNEL.to_owned(),
        serde_json::to_vec(&admissions).map_err(|error| super::stage_err(&error.to_string()))?,
    );
    Ok(NativeObservations {
        consistency,
        admissions,
    })
}

#[path = "native_cases.tests.rs"]
#[cfg(test)]
mod tests;
