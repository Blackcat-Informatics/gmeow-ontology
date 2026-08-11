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

use gmeow_errors::Report;
use gmeow_validate::lint::{LintConfig, default_annotation_predicates};
use gmeow_validate::validate_all::{MergedShacl, SignatureConfig, ValidateOptions, ValidationRun};

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

    // The whole-corpus merged-SHACL verdict is CONSUMED from `stage-validate`'s
    // recorded product rather than computed a second time over the same inputs.
    //
    // Reading a record is only sound once the record is proven current, so the source
    // is resolved by `recorded_merged_shacl`, which recomputes `stage-validate`'s input
    // digest — over every authored source AND every member of the committed shape
    // union, `generated/shapes/*.ttl` included — and HARD-FAILS on an absent, stale, or
    // digest-less record. That digest comparison is what carries forward the one thing
    // the duplicate run uniquely caught: `stage-validate` structurally never reads
    // `generated/shapes` off disk, so committed-shape drift showed up only here. It
    // still does, now as a digest mismatch instead of a second corpus-wide SHACL pass.
    //
    // `--gts` validates a bundle that is NOT the authored working tree, so no record of
    // the pipeline's describes it; that path keeps its own live pass.
    let merged_shacl = if gts.is_some() {
        gmeow_validate::validate_all::MergedShacl::Live
    } else {
        match recorded_merged_shacl(&root) {
            Ok(source) => source,
            Err(code) => return code,
        }
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
            shape_union_root: Some(root.clone()),
            merged_shacl,
            ..ValidateOptions::default()
        };
        ValidationRun::run(&[], "", "", "", &lint_config, &options)
    } else {
        // The authored-source invocation — source paths + the DSL SHACL wiring — is
        // assembled in ONE testable place (`authored_source_invocation`) so the
        // wiring cannot silently regress to the historical defect of empty DSL
        // args. `authored_source_invocation_wires_every_dsl_surface` binds it.
        let inv = match authored_source_invocation(&root, timings, deep, merged_shacl) {
            Ok(inv) => inv,
            Err(code) => return code,
        };
        ValidationRun::run(
            &inv.source_paths,
            "",
            &inv.mapping_dsl_dir,
            &inv.statement_dsl_dir,
            &lint_config,
            &inv.options,
        )
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

/// The fully-assembled authored-source invocation for `gmeow-dev validate` (the
/// `make validate` gate): the source paths, the mapping/statement DSL directories
/// passed positionally to [`ValidationRun::run`], and the [`ValidateOptions`] that
/// carry the central-DSL SHACL surfaces (the three `*_shapes_ttl` and `test_dsl_dir`).
///
/// It exists so the DSL wiring lives in ONE testable place: the exact args whose
/// prior absence was the historical original defect are built here, and
/// `authored_source_invocation_wires_every_dsl_surface` binds them.
pub(crate) struct AuthoredSourceInvocation {
    pub source_paths: Vec<String>,
    pub mapping_dsl_dir: String,
    pub statement_dsl_dir: String,
    pub options: ValidateOptions,
}

/// Assemble the authored-source invocation onto the `make validate` gate.
///
/// Wires the committed central-DSL SHACL surfaces (mapping/statement/test) resolved
/// by [`gmeow_validate::dsl_coverage::authored_dsl_shacl_inputs`]; a missing DSL
/// input is a HARD FAIL there (no-optionality), never a silent skip. `slices_dir` is
/// deliberately left UNSET so per-example SHACL (owned by the `example_sweep`
/// rust-test) and slice-local test DSL (owned by `slicetest`) are not re-run as a
/// duplicate whole-corpus pass — see `docs/DSL-VALIDATION-COVERAGE.md` and the
/// `VALIDATE_PHASE_COVERAGE` registry.
///
/// `merged_shacl` is a PARAMETER (not resolved here) so this assembly — and its
/// test — never depend on a materialized `generated/` tree.
///
/// # Errors
/// Returns the CLI failure code if the authored source set or the DSL inputs cannot
/// be resolved.
pub(crate) fn authored_source_invocation(
    root: &Path,
    timings: bool,
    deep: bool,
    merged_shacl: MergedShacl,
) -> Result<AuthoredSourceInvocation, i32> {
    let source_paths: Vec<String> = match gmeow_pipeline::stages::source_load::authored_files(root)
    {
        Ok(paths) => paths.iter().map(|p| p.display().to_string()).collect(),
        Err(e) => return Err(fail(format!("cannot list authored sources: {e}"))),
    };
    let dsl = match gmeow_validate::dsl_coverage::authored_dsl_shacl_inputs(root) {
        Ok(dsl) => dsl,
        Err(e) => return Err(fail(format!("cannot resolve DSL SHACL inputs: {e}"))),
    };
    let options = ValidateOptions {
        timings,
        project_root: Some(root.to_path_buf()),
        deep,
        shape_union_root: Some(root.to_path_buf()),
        merged_shacl,
        mapping_shapes_ttl: Some(dsl.mapping_shapes),
        statement_shapes_ttl: Some(dsl.statement_shapes),
        test_dsl_shapes_ttl: Some(dsl.test_shapes),
        test_dsl_dir: Some(dsl.test_dir),
        ..ValidateOptions::default()
    };
    Ok(AuthoredSourceInvocation {
        source_paths,
        mapping_dsl_dir: dsl.mapping_dir,
        statement_dsl_dir: dsl.statement_dir,
        options,
    })
}

/// `stage-validate`'s recorded whole-corpus merged-SHACL verdict, admitted ONLY after
/// it is proven to describe the current working tree.
///
/// The proof has TWO halves, because a record is only admissible if it was produced from
/// this tree AND is what the producer wrote:
///
/// * **Freshness** — the input digest `stage-validate` stamps into `shacl.json`'s
///   metadata: a content fold over every authored source file AND every member of the
///   shape union it validated with. This recomputes that digest from disk — including
///   `generated/shapes/*.ttl`, which `stage-validate` structurally never reads — so a
///   committed shape file that has drifted from the bytes the pipeline produced and
///   validated with makes the digests differ.
/// * **Integrity** — the record digest, a fold over the verdict's OWN findings, rules,
///   and metadata. The input digest cannot carry this: hand-deleting a violation from
///   `shacl.json` leaves every validated input byte-identical, so freshness still holds
///   while the verdict the gate reads is fabricated.
///
/// Every failure here is HARD. An absent `shacl.json` is not "nothing to check"; a
/// record with no digest is a record of unknowable vintage; a mismatch is a record of
/// different bytes; a record failing its own content digest is a record someone edited.
/// None of them is a skip, and none of them is a pass: a caller that cannot obtain a
/// proven-current verdict has not validated, and says so.
fn recorded_merged_shacl(root: &Path) -> Result<MergedShacl, i32> {
    let expected = gmeow_pipeline::stages::validate::on_disk_shacl_input_digest(root)
        .map_err(|e| fail(format!("cannot digest the SHACL input set: {e}")))?;
    let path = root.join(gmeow_pipeline::stages::validate::SHACL_JSON_PATH);
    let bytes = std::fs::read(&path).map_err(|e| {
        fail(format!(
            "cannot read the recorded SHACL verdict at {}: {e}. It is a pipeline product \
             — run `make check`; its absence is never a reason to pass validation",
            path.display()
        ))
    })?;
    let recorded: Report = serde_json::from_slice(&bytes).map_err(|e| {
        fail(format!(
            "the recorded SHACL verdict at {} is not a diagnostics report: {e}",
            path.display()
        ))
    })?;
    let digest = recorded
        .metadata
        .get(gmeow_pipeline::stages::validate::SHACL_INPUT_DIGEST_KEY)
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            fail(format!(
                "the recorded SHACL verdict at {} carries no {} metadata, so the inputs it \
                 validated cannot be identified — regenerate it (`make check`)",
                path.display(),
                gmeow_pipeline::stages::validate::SHACL_INPUT_DIGEST_KEY,
            ))
        })?;
    if digest != expected {
        return Err(fail(format!(
            "the recorded SHACL verdict at {} is STALE: it validated inputs digesting \
             {digest}, but the authored sources and the committed shape union \
             (generated/shapes included) now digest {expected}. Regenerate it (`make check`) \
             — a stale verdict is never accepted as current",
            path.display(),
        )));
    }
    // INTEGRITY, the half the input digest structurally cannot carry. The check above
    // proves the record was produced from THESE bytes; it says nothing about the record.
    // Hand-deleting a violation from `shacl.json` changes no validated input, so the input
    // digest still matches exactly — and the gate would then pass on a verdict nobody
    // produced. Recompute the verdict's fold over its own content and refuse a mismatch.
    gmeow_pipeline::stages::diag_render::verify_record_digest(
        &recorded,
        gmeow_pipeline::stages::validate::SHACL_RECORD_DIGEST_KEY,
        &path.display().to_string(),
    )
    .map_err(|e| fail(e.to_string()))?;
    // The recorded findings are exactly `stage-validate`'s post-advisory-split set:
    // Info-severity results (advisory-constraint matches) were lifted out there and
    // re-projected as Notes, and Info NEVER contributed to an error count on either
    // side — `finding_from_shacl` maps `sh:Violation` to Error, `sh:Warning` to
    // Warning, and `sh:Info` to Info. So the ERROR set this run gates on is identical
    // to the one a live pass here would produce.
    Ok(MergedShacl::Recorded(recorded.findings))
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

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_validate::validate_all::MergedShacl;

    /// Repo root: this crate's manifest is `<repo>/crates/gmeow-dev-cli`.
    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .expect("repo root must resolve from crates/gmeow-dev-cli")
    }

    /// The regression guard that binds the PRODUCTION call site. The historical
    /// original defect was `dev_validate` passing empty DSL dir/shape arguments to
    /// `ValidationRun::run`, so the mapping/statement/test DSL SHACL phases never
    /// executed on `make validate`. The other guards check the resolver, the engine,
    /// and the help-text — but NOT the args `validate()` actually assembles. This one
    /// does: it drives the real assembly ([`authored_source_invocation`], the single
    /// place those args are built) against the real repository and asserts every DSL
    /// surface is wired. Reverting any surface to an empty dir or a `None`/empty
    /// shapes text makes this FAIL, on every `make check`, with no `generated/`
    /// dependency — so the exact original regression can no longer pass green.
    #[test]
    fn authored_source_invocation_wires_every_dsl_surface() {
        let root = repo_root();
        // `MergedShacl::Live` is a stand-in so the assembly does not touch
        // `generated/`; the DSL wiring under test is independent of it.
        let inv = authored_source_invocation(&root, false, false, MergedShacl::Live)
            .expect("authored-source invocation must assemble on the real repo");

        assert!(
            !inv.source_paths.is_empty(),
            "the authored source set must be non-empty"
        );
        // Positional args 3 and 4 to ValidationRun::run — the mapping/statement DSL
        // directories. Empty here IS the historical original defect.
        assert!(
            !inv.mapping_dsl_dir.is_empty(),
            "mapping DSL dir (ValidationRun::run arg 3) must be wired, not empty"
        );
        assert!(
            !inv.statement_dsl_dir.is_empty(),
            "statement DSL dir (ValidationRun::run arg 4) must be wired, not empty"
        );
        // The three committed DSL shape texts + the test DSL dir carried on options.
        for (label, value) in [
            ("mapping_shapes_ttl", &inv.options.mapping_shapes_ttl),
            ("statement_shapes_ttl", &inv.options.statement_shapes_ttl),
            ("test_dsl_shapes_ttl", &inv.options.test_dsl_shapes_ttl),
        ] {
            assert!(
                value.as_deref().is_some_and(|s| !s.trim().is_empty()),
                "options.{label} must be wired with non-empty committed shapes text"
            );
        }
        assert!(
            inv.options
                .test_dsl_dir
                .as_deref()
                .is_some_and(|d| !d.is_empty()),
            "options.test_dsl_dir must be wired, not None/empty"
        );
    }
}
