// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit producer and read-only admission for exhaustive divergence evidence.

use std::collections::BTreeMap;
use std::path::Path;

use gmeow_action_cache::selection::SelectedAction;
use gmeow_action_cache::{
    ActionContext, ActionInput, ActionStore, FileKind, ProducerIdentity, STORE_FORMAT_VERSION,
    StoreLimits, bytes_digest,
};
use serde::{Deserialize, Serialize};

const DIVERGENCE_CORPUS: &str = "conformance/logic/cases/external/w3c-owl2-full-divergence";
const CORPORA: &[&str] = super::native_cases::FULL_CORPORA;
const ACTION: &str = "full-native-conformance-observations";
const CODEC: &str = "full-native-conformance-json-v2";

/// Complete native consistency outcomes for the divergence inventory.
pub type ConsistencyObservations = BTreeMap<
    String,
    Result<gmeow_conformance::consistency::Observation, gmeow_errors::RecordedDiag>,
>;

/// Exhaustive products selected by the explicit heavy producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observations {
    pub consistency: ConsistencyObservations,
    pub class_diagnostics: super::native_cases::ClassDiagnostics,
}

fn fail(detail: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&detail.to_string())
}

/// Explicit optimized producer. Each completed native action is reusable even if
/// later work fails. The aggregate embeds all observations, independent of eviction.
pub fn produce(root: &Path) -> gmeow_errors::Result<SelectedAction> {
    if crate::cache::PRODUCER_BUILD_CONTRACT.is_empty() {
        return Err(fail(
            "exhaustive conformance requires the optimized producer",
        ));
    }
    let inputs = corpus_inputs(root)?;
    let build = crate::cache::BuildIdentity::current();
    let context = ActionContext::new(
        "logic-conformance",
        ACTION,
        ProducerIdentity {
            digest: build.fingerprint,
            toolchain: Some(build.toolchain),
            target: Some(build.target),
            profile: Some(build.profile),
            features: build.features,
        },
        CODEC,
        inputs,
    );
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let hit = store
        .coordinate::<_, gmeow_errors::Diag, _, _>(
            &context.key(),
            || store.get::<()>(&context).map_err(fail),
            || {
                // Divergence consistency is serial: hard cases must not multiply the
                // live native memory footprint. Native failures stay errors, never
                // semantic gaps. Independent class diagnostics use their bounded pool.
                let mut consistency = ConsistencyObservations::new();
                for input in &context.inputs {
                    let ActionInput::Raw { logical_path, .. } = input else {
                        unreachable!()
                    };
                    if logical_path.starts_with(DIVERGENCE_CORPUS)
                        && Path::new(logical_path)
                            .file_name()
                            .is_some_and(|name| name == "input.nq")
                    {
                        let observation = super::execution::cached_consistency(
                            root,
                            &root.join(logical_path),
                            &store,
                        )
                        .map_err(super::record_failure);
                        consistency.insert(logical_path.clone(), observation);
                    }
                }
                let class_diagnostics =
                    super::native_cases::exhaustive_class_diagnostics(root, &store)?;
                let observations = Observations {
                    consistency,
                    class_diagnostics,
                };
                let bytes = serde_json::to_vec(&observations)?;
                let receipt = store
                    .publish(&context, bytes_digest(&bytes), (), &bytes)
                    .map_err(fail)?;
                Ok(gmeow_action_cache::VerifiedEntry { receipt, bytes })
            },
        )
        .map_err(fail)?;
    Ok(SelectedAction::from_receipt(&hit.value.receipt))
}

/// Authenticate the producer-selected action across Cargo profiles. A miss, stale
/// input or malformed selector fails without executing any corpus operation.
pub fn load(root: &Path) -> gmeow_errors::Result<Observations> {
    #[derive(Deserialize)]
    struct Manifest {
        conformance_heavy: SelectedAction,
    }
    let manifest: Manifest = gmeow_action_cache::selection::load_manifest(root).map_err(fail)?;
    admit(root, &manifest.conformance_heavy)
}

fn admit(root: &Path, selected: &SelectedAction) -> gmeow_errors::Result<Observations> {
    if selected.context.domain != "logic-conformance"
        || selected.context.action != ACTION
        || selected.context.codec != CODEC
        || selected.context.inputs != corpus_inputs(root)?
    {
        return Err(fail(
            "exhaustive conformance selection has stale inputs or the wrong operation",
        ));
    }
    let store = ActionStore::open_existing_read_only(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let hit = store
        .get::<()>(&selected.context)
        .map_err(fail)?
        .ok_or_else(|| fail("selected exhaustive conformance observation is absent"))?;
    selected.verify(&hit.receipt).map_err(fail)?;
    let observations: Observations = serde_json::from_slice(&hit.bytes).map_err(fail)?;
    let expected_consistency: std::collections::BTreeSet<_> = selected
        .context
        .inputs
        .iter()
        .filter_map(|input| {
            if let ActionInput::Raw { logical_path, .. } = input
                && logical_path.starts_with(DIVERGENCE_CORPUS)
                && Path::new(logical_path)
                    .file_name()
                    .is_some_and(|name| name == "input.nq")
            {
                Some(logical_path.as_str())
            } else {
                None
            }
        })
        .collect();
    if observations
        .consistency
        .keys()
        .map(String::as_str)
        .collect::<std::collections::BTreeSet<_>>()
        != expected_consistency
    {
        return Err(fail(
            "exhaustive consistency observations do not cover the selected divergence inventory exactly",
        ));
    }
    let expected_diagnostics = super::native_cases::class_diagnostic_inputs(root)?
        .into_iter()
        .map(|path| {
            path.strip_prefix(root)
                .map(|path| path.to_string_lossy().into_owned())
                .map_err(fail)
        })
        .collect::<gmeow_errors::Result<std::collections::BTreeSet<_>>>()?;
    if observations
        .class_diagnostics
        .keys()
        .cloned()
        .collect::<std::collections::BTreeSet<_>>()
        != expected_diagnostics
    {
        return Err(fail(
            "exhaustive class diagnostics do not cover the selected full-corpus inventory exactly",
        ));
    }
    Ok(observations)
}

/// Read-only inventory and identity verification; never parse, lower or reason.
fn corpus_inputs(root: &Path) -> gmeow_errors::Result<Vec<ActionInput>> {
    let mut inputs = Vec::new();
    let mut files = Vec::new();
    for corpus in CORPORA {
        super::execution::collect_files(&root.join(corpus), &mut files)?;
    }
    for path in files {
        let relative = path.strip_prefix(root).map_err(fail)?;
        if !CORPORA.iter().any(|corpus| relative.starts_with(corpus)) {
            continue;
        }
        // Include expectations/provenance so readers cannot combine an observation
        // with a different corpus generation, while child actions key only execution.
        inputs.push(ActionInput::Raw {
            logical_path: relative.to_string_lossy().into_owned(),
            file_kind: FileKind::File,
            executable: false,
            digest: bytes_digest(&std::fs::read(&path).map_err(fail)?),
        });
    }
    if !inputs.iter().any(|input| {
        matches!(input, ActionInput::Raw { logical_path, .. }
        if Path::new(logical_path).file_name().is_some_and(|name| name == "input.nq"))
    }) {
        return Err(fail("exhaustive divergence corpus has no inputs"));
    }
    inputs.sort();
    Ok(inputs)
}

#[path = "heavy.tests.rs"]
#[cfg(test)]
mod tests;
