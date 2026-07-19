// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-dev validate` — whole-ontology structural + SHACL + optional
//! signature/deep validation.
//!
//! Validates the committed `gmeow.gts` bundle (or a `--gts` bundle) through the
//! purpose-built whole-bundle orchestration `ValidationRun::run` (merged SHACL,
//! structural + naming + ownership lints, gUFO invariants). When any
//! signature/trust flag is supplied with `--gts`, the embedded GTS signature
//! pre-gate runs first (as part of the same orchestration); with `--deep`, the
//! native Tier-2 reasoning pass folds in.

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use gmeow_validate::lint::{LintConfig, default_annotation_predicates};
use gmeow_validate::validate_all::{SignatureConfig, ValidateOptions, ValidationRun};

use crate::dev_common::{
    NAMESPACE, ONTOLOGY_IRI, emit_report, fail, project_root, write_timings_json,
};

/// Audit the mandatory wire-level compression profile of a GMEOW GTS bundle.
pub fn gts_frame_profile(path: &Path) -> i32 {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return fail(format!("cannot read {}: {error}", path.display())),
    };
    match gmeow_pipeline::validate_mandated_frames(&bytes) {
        Ok(()) => {
            println!("GTS frame profile passed: {}", path.display());
            0
        }
        Err(message) => fail(format!(
            "GTS frame profile failed for {}: {message}",
            path.display()
        )),
    }
}

/// `gmeow-dev validate [--gts --trust-policy --require-signed --trusted-key --deep …]`.
#[allow(clippy::too_many_arguments)]
pub fn validate(
    timings: bool,
    timings_json: Option<&Path>,
    gts: Option<&Path>,
    trust_policy: Option<&Path>,
    require_signed: bool,
    trusted_key: Option<&Path>,
    deep: bool,
) -> i32 {
    let signature_flags = trust_policy.is_some() || require_signed || trusted_key.is_some();
    if signature_flags && gts.is_none() {
        return fail("--trust-policy/--require-signed/--trusted-key require --gts");
    }

    let root = project_root();
    let signature_config = if signature_flags {
        match build_signature_config(trust_policy, require_signed, trusted_key) {
            Ok(c) => Some(c),
            Err(code) => return code,
        }
    } else {
        None
    };

    let lint_config = LintConfig {
        namespace: NAMESPACE.to_owned(),
        ontology_iri: ONTOLOGY_IRI.to_owned(),
        selector_tokens: BTreeSet::new(),
        core_slice_iris: HashSet::new(),
        annotation_predicates: default_annotation_predicates().into_iter().collect(),
    };

    // The merged SHACL shape union (with the canonical exclusions applied by
    // `shape_files`) drives the conformance phase in both modes — the same shape
    // set the `make validate` gate enforces.
    let shapes_ttl = match merged_shapes(&root) {
        Ok(s) => s,
        Err(code) => return code,
    };

    // `--gts PATH`: validate a folded bundle directly (with the optional signature
    // pre-gate). Default: validate the authored repository sources — the `make
    // validate` gate — over the same merged shapes.
    let run = if let Some(path) = gts {
        let gts_bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        };
        let options = ValidateOptions {
            timings,
            gts_bytes: Some(gts_bytes),
            signature_config,
            deep,
            ..ValidateOptions::default()
        };
        ValidationRun::run(&[], &shapes_ttl, "", "", &lint_config, &options)
    } else {
        let source_paths: Vec<String> =
            match gmeow_pipeline::stages::source_load::authored_files(&root) {
                Ok(paths) => paths.iter().map(|p| p.display().to_string()).collect(),
                Err(e) => return fail(format!("cannot list authored sources: {e}")),
            };
        let options = ValidateOptions {
            timings,
            project_root: Some(root.clone()),
            deep,
            ..ValidateOptions::default()
        };
        ValidationRun::run(&source_paths, &shapes_ttl, "", "", &lint_config, &options)
    };
    let run = match run {
        Ok(r) => r,
        Err(e) => return fail(format!("validation error: {e}")),
    };
    let report = run.report;

    emit_report(&report);
    if let Some(path) = timings_json {
        let payload = serde_json::json!({
            "command": "validate",
            "gts": gts.map(|p| p.display().to_string()),
            "deep": deep,
            "ok": report.ok(),
            "errors": report.error_count(),
            "warnings": report.warning_count(),
        });
        let code = write_timings_json(path, &payload);
        if code != 0 {
            return code;
        }
    }

    if report.ok() {
        println!("validation passed");
        0
    } else {
        fail(format!("{} error(s)", report.error_count()))
    }
}

/// The merged SHACL shape union from the repo — the canonical shape file set
/// (`purrdf::shapes::shape_union::shape_files`, with the same exclusions the live
/// validator applies), concatenated into one Turtle document.
fn merged_shapes(root: &Path) -> Result<String, i32> {
    let files = purrdf::shapes::shape_union::shape_files(root)
        .map_err(|e| fail(format!("cannot list shape files: {e}")))?;
    let mut out = String::new();
    for file in files {
        let text = std::fs::read_to_string(&file)
            .map_err(|e| fail(format!("cannot read {}: {e}", file.display())))?;
        out.push_str(&text);
        out.push('\n');
    }
    Ok(out)
}

/// Build the [`SignatureConfig`] from the CLI flags + an optional trust-policy TOML.
fn build_signature_config(
    trust_policy: Option<&Path>,
    require_signed: bool,
    trusted_key: Option<&Path>,
) -> Result<SignatureConfig, i32> {
    let mut config = SignatureConfig {
        trusted_signers: Vec::new(),
        require_signatures: require_signed,
        require_trusted_signer: false,
        trusted_key: None,
    };
    if let Some(policy_path) = trust_policy {
        let text = std::fs::read_to_string(policy_path).map_err(|e| {
            fail(format!(
                "cannot read --trust-policy {}: {e}",
                policy_path.display()
            ))
        })?;
        let policy: toml::Value = toml::from_str(&text).map_err(|e| {
            fail(format!(
                "invalid TOML in --trust-policy {}: {e}",
                policy_path.display()
            ))
        })?;
        if let Some(signers) = policy.get("trusted_signers").and_then(|v| v.as_array()) {
            config.trusted_signers = signers
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect();
        }
        config.require_trusted_signer = policy
            .get("require_trusted_signer")
            .and_then(toml::Value::as_bool)
            .unwrap_or(false);
        if let Some(key) = policy.get("trusted_key").and_then(|v| v.as_str()) {
            let mut key_path = std::path::PathBuf::from(key);
            if key_path.is_relative()
                && let Some(parent) = policy_path.parent()
            {
                key_path = parent.join(key_path);
            }
            let armor = std::fs::read_to_string(&key_path).map_err(|e| {
                fail(format!(
                    "cannot read trusted key {}: {e}",
                    key_path.display()
                ))
            })?;
            config.trusted_key = Some(armor);
        }
    }
    // The CLI --trusted-key wins over any policy trusted_key path.
    if let Some(path) = trusted_key {
        let armor = std::fs::read_to_string(path)
            .map_err(|e| fail(format!("cannot read --trusted-key {}: {e}", path.display())))?;
        config.trusted_key = Some(armor);
    }
    Ok(config)
}
