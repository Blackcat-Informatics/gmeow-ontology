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

/// Audit a distribution bundle's wire against BOTH halves of the codec rule.
///
/// The two halves are separate checks because they have different domains, and
/// collapsing them would force one of them to lie:
///
/// * [`gmeow_pipeline::validate_mandated_frames`] is the UNIVERSAL Rule 6 rule — one
///   `zstd-rsyncable` transform at level 12 on every payload-bearing frame — and it
///   holds for every GMEOW-authored artifact, including the many that carry no medium
///   registry at all (the feedback / music / math bundles, `convert --to gts` output,
///   the runtime stores);
/// * [`gmeow_pipeline::validate_dist_bundle_media`] is the DECLARED-MEDIA audit: it
///   resolves the medium this bundle's own ontology says its producer writes through
///   and holds the wire to it — every frame's rep primed with the dictionary its
///   registered `gmeow:PayloadSchema` names, that dictionary pinned in band, and the
///   fold free of opaque nodes.
///
/// Making the universal rule registry-dependent instead would leave two escapes — a
/// red gate, or "a registry-less bundle skips the medium check" — and the second is
/// the silent degradation the medium axis exists to forbid.
pub fn gts_frame_profile(path: &Path) -> i32 {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return fail(format!("cannot read {}: {error}", path.display())),
    };
    if let Err(message) = gmeow_pipeline::validate_mandated_frames(&bytes) {
        return fail(format!(
            "GTS frame profile failed for {}: {message}",
            path.display()
        ));
    }
    match gmeow_pipeline::validate_dist_bundle_media(&bytes) {
        Ok(()) => {
            println!(
                "GTS frame profile and declared media passed: {}",
                path.display()
            );
            0
        }
        Err(message) => fail(format!(
            "GTS declared-media audit failed for {}: {message}",
            path.display()
        )),
    }
}

/// Audit the WHOLE medium axis of one GMEOW-authored GTS artifact.
///
/// [`gts_frame_profile`] audits the dist bundle's wire; this audits any artifact's
/// medium end to end, and it accepts a RUNTIME STORE path on purpose. A `~/.gmeow/*.gts`
/// agent-memory or conjecture library is not a build artifact, so no `generated/` gate
/// ever reaches it — yet it is written through a declared `gmeow:Medium`, primed with a
/// dictionary the shipped bundle owns, and is exactly as capable of silently losing that
/// priming as anything the build emits. An axis whose gate could only run on build output
/// would be leaving its most reachable artifacts unchecked.
///
/// The clauses, in the order they decide:
///
/// 1. the UNIVERSAL Rule 6 codec check, on every payload frame of every segment;
/// 2. the DECLARED-MEDIA check for the branch the artifact's own
///    `gmeow:mediumSourceKind` selects — per-rep for the dist bundle, header-dict for a
///    runtime store, same-entry-matches-declaration for a whole-artifact producer;
/// 3. every payload frame DECODED through its declared chain and its in-band digest
///    re-derived, plus the zero-opaque-node / zero-reader-diagnostic clause;
/// 4. every `gmeow:MediumEnvelope` opened against the bytes at hand, including the
///    self-referential snapshot envelope's stratum recomputation;
/// 5. where the artifact carries a registry of its own: the measured MDL win gate, the
///    registry-completeness check (every declared dictionary realized AND pinned, with
///    the pinned bytes matching the recorded digest and length), and the
///    declared-vs-actual reader-capability comparison.
pub fn medium_gate(path: &Path, registry: &Path) -> i32 {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) => return fail(format!("cannot read {}: {error}", path.display())),
    };
    // The priming bundle is read only when it is a DIFFERENT artifact, and it is folded
    // only if the subject turns out to carry no registry of its own: a self-describing
    // bundle is audited against the registry it already carries, so re-reading thirty-odd
    // megabytes to build a second copy of it would be work with no claim attached.
    //
    // A read failure is therefore carried rather than raised. The default `--registry` is
    // a REPO-RELATIVE path, so auditing a `~/.gmeow/*.gts` store from outside a checkout
    // will not find it — and refusing there would refuse the self-describing bundle too,
    // which needs no priming bundle at all. `PrimingBundle::Absent` keeps the reason and
    // hands it to the one branch that genuinely needs the bundle, where it is a HARD FAIL:
    // an absent priming bundle can never become a skipped dictionary resolution.
    let (registry_bytes, absent) = if registry == path {
        (Vec::new(), None)
    } else {
        match std::fs::read(registry) {
            Ok(bytes) => (bytes, None),
            Err(error) => (
                Vec::new(),
                Some(format!("cannot read {}: {error}", registry.display())),
            ),
        }
    };
    let priming = match (&absent, registry == path) {
        (Some(why), _) => gmeow_pipeline::medium::inspect::PrimingBundle::Absent(why),
        (None, true) => gmeow_pipeline::medium::inspect::PrimingBundle::Bytes(&bytes),
        (None, false) => gmeow_pipeline::medium::inspect::PrimingBundle::Bytes(&registry_bytes),
    };
    match gmeow_pipeline::medium::inspect::gate(&bytes, priming) {
        Ok(report) => {
            println!(
                "medium gate passed: {} — {:?} under <{}>, {} payload frame(s) decoded, {} \
                 envelope(s) re-derived, dictionaries {:?}, reader capabilities {:?}",
                path.display(),
                report.class,
                report.medium,
                report.frames.len(),
                report.envelopes_verified,
                report.dictionaries,
                report.declared_capabilities
            );
            0
        }
        Err(diag) => fail(format!(
            "medium gate failed for {}: [{}] {diag}",
            path.display(),
            gmeow_errors::code::code_str(diag.code())
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
        let source_paths: Vec<String> =
            match gmeow_pipeline::stages::source_load::authored_files(&root) {
                Ok(paths) => paths.iter().map(|p| p.display().to_string()).collect(),
                Err(e) => return fail(format!("cannot list authored sources: {e}")),
            };
        let options = ValidateOptions {
            timings,
            project_root: Some(root.clone()),
            deep,
            shape_union_root: Some(root.clone()),
            merged_shacl,
            ..ValidateOptions::default()
        };
        ValidationRun::run(&source_paths, "", "", "", &lint_config, &options)
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
