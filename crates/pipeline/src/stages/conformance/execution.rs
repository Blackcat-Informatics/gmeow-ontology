// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Producer-owned native case execution, with independent bounded action reuse.
//! The stage embeds every selected result so eviction of a child action cannot
//! strand an authenticated consumer. Tests read these bytes without an engine.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_action_cache::{
    ActionContext, ActionInput, ActionStore, FileKind, ProducerIdentity, STORE_FORMAT_VERSION,
    StoreLimits, bytes_digest,
};
use gmeow_conformance::{discover, run, vendored};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

const CASES: &str = "conformance/logic/cases";
const PREFIX: &str = "pipeline/conformance-cases/";
const INDEX: &str = "pipeline/conformance-case-index.json";
pub(crate) const CONSISTENCY_CHANNEL: &str = "pipeline/native-consistency-observations.json";

/// Required live witnesses for the retained full-DL boundaries. Exhaustive
/// full-divergence grading has a separate producer profile and must not force
/// that breadth into every ordinary pipeline invocation.
const REQUIRED_FULL_DIVERGENCE: &[&str] = &[
    "one-two",
    "webont-description-logic-035",
    "rolechainviolationlumen",
    "webont-description-logic-501",
    "webont-description-logic-502",
];

#[derive(Serialize, Deserialize)]
struct Observation {
    case_id: String,
    outcome: Result<run::CaseOutputs, gmeow_errors::RecordedDiag>,
}

fn fail(message: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&message.to_string())
}

/// Walk all authored inputs deterministically, refusing unreadable entries and
/// symlinks. Includes goldens for stage authentication; case execution keys below
/// exclude goldens so changing an expectation never re-executes the engine.
pub(super) fn input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    collect_files(&root.join(CASES), &mut files)?;
    files.sort();
    Ok(files)
}

pub(super) fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> gmeow_errors::Result<()> {
    for entry in std::fs::read_dir(dir).map_err(fail)? {
        let entry = entry.map_err(fail)?;
        let kind = entry.file_type().map_err(fail)?;
        if kind.is_dir() {
            collect_files(&entry.path(), files)?;
        } else if kind.is_file() {
            files.push(entry.path());
        } else {
            return Err(fail(format!(
                "unsupported conformance input {}",
                entry.path().display()
            )));
        }
    }
    Ok(())
}

fn selected_cases(root: &Path) -> gmeow_errors::Result<Vec<discover::ConformanceCase>> {
    let mut selected = Vec::new();
    for path in input_files(root)? {
        if path.file_name().is_none_or(|name| name != "profile.json") {
            continue;
        }
        let dir = path.parent().expect("file within cases");
        if dir
            .strip_prefix(root.join(CASES))
            .map_err(fail)?
            .starts_with("bench")
        {
            continue;
        }
        // Preserve anatomy checks even for cases routed to a dedicated lane.
        let case = discover::validate_case(dir)?;
        if matches!(
            vendored::lane_for_case(dir)?,
            Some(vendored::Lane::B | vendored::Lane::Divergence | vendored::Lane::NativeProfiled)
        ) {
            continue;
        }
        selected.push(case);
    }
    if selected.is_empty() {
        return Err(fail("no required native conformance cases selected"));
    }
    Ok(selected)
}

fn uses_library(case: &discover::ConformanceCase) -> gmeow_errors::Result<bool> {
    Ok(
        !gmeow_conformance::profile::parse_profile(&case.case_id, &case.profile)?
            .shipped_rules
            .is_empty(),
    )
}

fn context(
    root: &Path,
    case: &discover::ConformanceCase,
    library_digest: &str,
) -> gmeow_errors::Result<ActionContext> {
    let mut files = Vec::new();
    collect_files(&case.case_dir, &mut files)?;
    let mut inputs = Vec::new();
    for file in files {
        if file
            .strip_prefix(&case.case_dir)
            .map_err(fail)?
            .starts_with("expected")
        {
            continue;
        }
        inputs.push(ActionInput::Raw {
            logical_path: file
                .strip_prefix(root)
                .map_err(fail)?
                .to_string_lossy()
                .into_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: bytes_digest(&std::fs::read(&file).map_err(fail)?),
        });
    }
    if uses_library(case)? {
        inputs.push(ActionInput::Upstream {
            producer: "stage-parse-sources".to_owned(),
            entity: Some(crate::stages::compile_logic::SOURCE_PATH.to_owned()),
            receipt_digest: None,
            product_digest: library_digest.to_owned(),
        });
    }
    let build = crate::cache::BuildIdentity::current();
    let producer = ProducerIdentity {
        digest: build.fingerprint,
        toolchain: Some(build.toolchain),
        target: Some(build.target),
        profile: Some(build.profile),
        features: build.features,
    };
    Ok(ActionContext::new(
        "logic-conformance",
        &case.case_id,
        producer,
        "case-observation-json-v2-typed-diagnostics",
        inputs,
    ))
}

pub(super) fn record(
    root: &Path,
    catalog: &SourceCatalog,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<super::native_cases::Admissions> {
    let cases = selected_cases(root)?;
    let source = crate::stages::compile_logic::SOURCE_PATH;
    let library_digest = catalog.document_digest(source)?;
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let mut misses = Vec::new();
    let mut index = BTreeSet::new();
    for case in cases {
        let context = context(root, &case, library_digest)?;
        index.insert(case.case_id.clone());
        if let Some(hit) = store.get::<()>(&context).map_err(fail)? {
            artifacts.insert(format!("{PREFIX}{}.json", case.case_id), hit.bytes);
        } else {
            misses.push((case, context));
        }
    }
    let needs_library = misses
        .iter()
        .map(|(case, _)| uses_library(case))
        .collect::<gmeow_errors::Result<Vec<_>>>()?
        .into_iter()
        .any(|needed| needed);
    // The selected native observations and the standalone shipped-rule lowering
    // are independent. Start both together so one serial compiler invocation
    // cannot leave the producer's worker pool idle. SourceCatalog coordinates the
    // exact canonical compilation with compile-logic and later conformance
    // consumers, while each native observation retains its own action identity.
    let consistency_inputs = required_consistency_inputs(root)?;
    let (observations, compiled) = rayon::join(
        || super::native_cases::record(root, consistency_inputs, &store, artifacts),
        || {
            if needs_library {
                catalog
                    .compiled_document(
                        source,
                        Some(crate::stages::compile_logic::SOURCE_IRI.to_owned()),
                    )
                    .map(Some)
            } else {
                Ok(None)
            }
        },
    );
    // Preserve the established native-observation error priority if both
    // independent branches fail.
    let observations = observations?;
    let compiled = compiled?;
    let library = match &compiled {
        Some(theory) => {
            if let Some(error) = theory
                .diagnostics()
                .iter()
                .find(|d| d.severity == gmeow_logic_compile::frontend::Severity::Error)
            {
                return Err(fail(format!(
                    "shipped rule library {}: {}",
                    error.code, error.message
                )));
            }
            run::RuleLibrary::new(theory.program())?
        }
        None => run::RuleLibrary::default(),
    };
    let results: Vec<_> = misses
        .par_iter()
        .map(|(case, context)| {
            let result = store
                .coordinate::<_, gmeow_action_cache::ActionCacheError, _, _>(
                    &context.key(),
                    || store.get::<()>(context),
                    || {
                        let observation = Observation {
                            case_id: case.case_id.clone(),
                            outcome: run::run_case(&case.case_dir, &library, &|path, operation| {
                                let key = path.strip_prefix(root).map_err(fail)?.to_string_lossy();
                                use gmeow_conformance::native_observation::{
                                    NativeCaseObservation, NativeCaseOperation,
                                };
                                match operation {
                                    NativeCaseOperation::Consistency => observations
                                        .consistency
                                        .get(key.as_ref())
                                        .ok_or_else(|| {
                                            fail(format!(
                                                "no selected consistency observation for {key}"
                                            ))
                                        })?
                                        .clone()
                                        .map(NativeCaseObservation::Consistency)
                                        .map_err(fail),
                                    NativeCaseOperation::ClassSourceAdmission => observations
                                        .admissions
                                        .get(key.as_ref())
                                        .ok_or_else(|| {
                                            fail(format!(
                                                "no selected source-admission observation for {key}"
                                            ))
                                        })?
                                        .clone()
                                        .map(NativeCaseObservation::ClassSourceAdmission)
                                        .map_err(fail),
                                }
                            })
                            .map_err(super::record_failure),
                        };
                        let bytes = serde_json::to_vec(&observation)?;
                        let receipt = store.publish(context, bytes_digest(&bytes), (), &bytes)?;
                        Ok(gmeow_action_cache::VerifiedEntry { receipt, bytes })
                    },
                )
                .map_err(fail)?;
            Ok((format!("{PREFIX}{}.json", case.case_id), result.value.bytes))
        })
        .collect::<Vec<gmeow_errors::Result<_>>>();
    // Each completed case was already published, including before another case
    // fails. This aggregate owns its result bytes independently of child eviction.
    for result in results {
        let (path, bytes) = result?;
        artifacts.insert(path, bytes);
    }
    artifacts.insert(INDEX.to_owned(), serde_json::to_vec(&index).map_err(fail)?);
    artifacts.insert(
        CONSISTENCY_CHANNEL.to_owned(),
        serde_json::to_vec(&observations.consistency).map_err(fail)?,
    );
    Ok(observations.admissions)
}

fn required_consistency_inputs(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut inputs = Vec::new();
    for path in input_files(root)? {
        if path.file_name().is_none_or(|name| name != "profile.json") {
            continue;
        }
        let dir = path.parent().expect("case profile parent");
        let rel = dir.strip_prefix(root.join(CASES)).map_err(fail)?;
        if rel.starts_with("bench")
            || matches!(vendored::lane_for_case(dir)?, Some(vendored::Lane::B))
        {
            continue;
        }
        if rel.starts_with("external/w3c-owl2-full-divergence")
            && !REQUIRED_FULL_DIVERGENCE
                .iter()
                .any(|slug| dir.file_name().is_some_and(|name| name == *slug))
        {
            continue;
        }
        let case = discover::validate_case(dir)?;
        let profile = gmeow_conformance::profile::parse_profile(&case.case_id, &case.profile)?;
        if profile.verdict_mode == gmeow_conformance::profile::VerdictMode::Consistency {
            let input = dir.join("input.nq");
            if !input.is_file() {
                return Err(fail(format!("consistency case has no {}", input.display())));
            }
            inputs.push(input);
        }
    }
    Ok(inputs)
}

/// One native consistency action serves generic case output and every dedicated
/// verifier of the same exact input. Typed verdicts retain witnesses, coverage,
/// gaps and ledger-identified boundary findings; execution failure remains Err.
pub(super) fn cached_consistency(
    root: &Path,
    path: &Path,
    store: &ActionStore,
) -> gmeow_errors::Result<gmeow_conformance::consistency::Observation> {
    cached_observation(root, path, store, "native-consistency", |bytes| {
        let dataset =
            purrdf::parse_dataset(bytes, purrdf::NativeRdfFormat::NQuads.media_type(), None)
                .map_err(gmeow_errors::Diag::from)?;
        gmeow_conformance::consistency::observe(&dataset)
    })
}

/// Cache one typed native observation; the operation is part of its action and
/// codec identity. Expectations are authenticated by the enclosing stage.
pub(super) fn cached_observation<T: serde::Serialize + serde::de::DeserializeOwned>(
    root: &Path,
    path: &Path,
    store: &ActionStore,
    operation: &str,
    observe: impl Fn(&[u8]) -> Result<T, gmeow_errors::Diag>,
) -> gmeow_errors::Result<T> {
    let bytes = std::fs::read(path).map_err(fail)?;
    cached_inputs(
        store,
        operation,
        vec![ActionInput::Raw {
            logical_path: path
                .strip_prefix(root)
                .map_err(fail)?
                .to_string_lossy()
                .into_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: bytes_digest(&bytes),
        }],
        || observe(&bytes),
    )
}

/// Reuse an observation whose operation reads several independently authenticated inputs.
/// The callback runs only on a miss and never runs in an authenticated consumer.
/// Cache completed observations, including explicit semantic refusals inside them.
/// Initialization failures propagate their live diagnostic without publishing an action.
pub(super) fn cached_inputs<T: serde::Serialize + serde::de::DeserializeOwned>(
    store: &ActionStore,
    operation: &str,
    inputs: Vec<ActionInput>,
    observe: impl Fn() -> Result<T, gmeow_errors::Diag>,
) -> gmeow_errors::Result<T> {
    let build = crate::cache::BuildIdentity::current();
    let context = ActionContext::new(
        "logic-conformance",
        operation,
        ProducerIdentity {
            digest: build.fingerprint,
            toolchain: Some(build.toolchain),
            target: Some(build.target),
            profile: Some(build.profile),
            features: build.features,
        },
        format!("native-observation-json-v2-typed-diagnostics:{operation}"),
        inputs,
    );
    let hit = store.coordinate::<_, gmeow_errors::Diag, _, _>(
        &context.key(),
        || store.get::<()>(&context).map_err(gmeow_errors::Diag::from),
        || {
            let observed = observe()?;
            let bytes = serde_json::to_vec(&observed)?;
            let receipt = store.publish(&context, bytes_digest(&bytes), (), &bytes)?;
            Ok(gmeow_action_cache::VerifiedEntry { receipt, bytes })
        },
    )?;
    serde_json::from_slice::<T>(&hit.value.bytes).map_err(gmeow_errors::Diag::from)
}

#[path = "execution.tests.rs"]
#[cfg(test)]
mod tests;
