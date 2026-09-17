// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Original MCP policy preparation, isolated from the ontology's asserted base.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_action_cache::{
    ActionInput, ActionStore, FileKind, STORE_FORMAT_VERSION, StoreLimits, bytes_digest,
};
use gmeow_logic_compile::action_policy::{self, PreparedActionPolicy};
use purrdf::{RdfQuad, RdfTerm};

pub(crate) const CONTROLS: &str = "pipeline/mcp-action-policy-controls.json";

pub(super) fn input_files(root: &Path) -> Vec<PathBuf> {
    vec![root.join(action_policy::SOURCE_PATH)]
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let fail = |error: String| super::stage_err(&error);
    let source = std::fs::read(root.join(action_policy::SOURCE_PATH))
        .map_err(|error| fail(error.to_string()))?;
    let digest = bytes_digest(&source);
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(|error| fail(error.to_string()))?;
    let inputs = vec![ActionInput::Raw {
        logical_path: action_policy::SOURCE_PATH.to_owned(),
        file_kind: FileKind::File,
        executable: false,
        digest: digest.clone(),
    }];
    let native = OnceLock::new();
    let dataset = || {
        native.get_or_try_init(|| {
            purrdf::parse_dataset(&source, "text/turtle", None).map_err(gmeow_errors::Diag::from)
        })
    };
    let policy: PreparedActionPolicy = super::execution::cached_inputs(
        &store,
        "mcp-action-policy-native-v1",
        inputs.clone(),
        || PreparedActionPolicy::from_dataset(dataset()?.as_ref(), &digest),
    )?;
    policy.validate()?;
    artifacts.insert(
        action_policy::SOURCE_ARTIFACT.to_owned(),
        serde_json::to_vec(&policy).map_err(|error| fail(error.to_string()))?,
    );
    let controls: BTreeMap<String, String> = super::execution::cached_inputs(
        &store,
        "mcp-action-policy-bijection-controls-v1",
        inputs,
        || {
            let original = purrdf::flat_rdf_quads_from_dataset(dataset()?.as_ref());
            let mut missing = original.clone();
            missing.retain(|quad| !(quad.predicate == action_policy::TOOL_NAME &&
            matches!(&quad.object, RdfTerm::Literal(value) if value.lexical_form == "store_conjecture")));
            if missing.len() + 1 != original.len() {
                return Err(super::stage_err(
                    "expected exactly one store_conjecture name to remove",
                ));
            }
            let missing = purrdf::flat_dataset_from_quads(&missing)
                .map_err(|message| super::stage_err(&message))?;
            let mut orphan = original;
            let subject = RdfTerm::iri(format!(
                "{}teleportOntology",
                action_policy::POLICY_NAMESPACE
            ));
            for (predicate, object) in [
                (
                    "http://www.w3.org/1999/02/22-rdf-syntax-ns#type",
                    RdfTerm::iri("https://blackcatinformatics.ca/logic/ActionSchema"),
                ),
                (
                    action_policy::TOOL_NAME,
                    RdfTerm::Literal(purrdf::RdfLiteral::simple("teleport_ontology")),
                ),
                (
                    "https://blackcatinformatics.ca/logic/capability",
                    RdfTerm::iri(format!(
                        "{}bundleReadCapability",
                        action_policy::POLICY_NAMESPACE
                    )),
                ),
                (
                    "https://blackcatinformatics.ca/logic/precondition",
                    RdfTerm::iri(format!(
                        "{}actionTheoryPresent",
                        action_policy::POLICY_NAMESPACE
                    )),
                ),
            ] {
                orphan.push(RdfQuad::new(subject.clone(), predicate, object));
            }
            let orphan = purrdf::flat_dataset_from_quads(&orphan)
                .map_err(|message| super::stage_err(&message))?;
            Ok(BTreeMap::from([
                (
                    "missing_store_conjecture".to_owned(),
                    action_policy::project_nquads(&missing)?,
                ),
                (
                    "orphan_teleport".to_owned(),
                    action_policy::project_nquads(&orphan)?,
                ),
            ]))
        },
    )?;
    artifacts.insert(
        CONTROLS.to_owned(),
        serde_json::to_vec(&controls).map_err(|error| fail(error.to_string()))?,
    );
    Ok(())
}
