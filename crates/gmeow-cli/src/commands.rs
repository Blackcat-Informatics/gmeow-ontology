// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The wired command bodies. Each function marshals its inputs and delegates to
//! an already-native backend, following the console convention: product results
//! → stdout, errors/diagnostics → stderr, and a `0`/`1` exit code.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use gmeow_cli_core::{Reporter, report_diag};
use gmeow_errors::grade::{Belnap, BoundedLattice};
use gmeow_errors::model::Finding;
use gmeow_errors::{
    Diag, FindingCategory, Grade, ResultExt, Severity, Standpoint, define_diag_kind,
};
use gmeow_pipeline::diagnostics_reader::{
    FindingIndex, explain_finding, minimal_fatal_cut, read_findings, render_shared_dag, verdict,
};

use crate::{BUNDLE_GTS, NAMESPACE};

/// Build an Error-grade CLI diagnostic carrying a per-site stable code — the
/// pre-carrier graded witness a handled `gmeow` failure lowers to (never a bare
/// string). The `code` is interned once (idempotently) and the message carries
/// the specifics.
fn error_diag(code: &str, message: impl Into<String>) -> Diag {
    Diag::new(
        gmeow_errors::code::register_code(code),
        Grade::new(
            Severity::Error,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    )
}

/// Emit an Error-grade CLI diagnostic on the reporter's channel (human text on
/// stderr, an NDJSON `finding` line for agents, dropped by a silent sink).
pub(crate) fn emit_error(reporter: &dyn Reporter, code: &str, message: impl Into<String>) {
    reporter.report(&report_diag(error_diag(code, message), "gmeow"));
}

/// Emit an Error-grade CLI diagnostic through `reporter` and yield the failure
/// exit code `1` — the substrate replacement for the old stderr `fail`.
pub(crate) fn fail(reporter: &dyn Reporter, code: &str, message: impl Into<String>) -> i32 {
    emit_error(reporter, code, message);
    1
}

/// Emit an Error-grade CLI diagnostic through `reporter` and yield an explicit
/// exit code (e.g. `2` for a usage-shaped failure that keeps clap's convention).
pub(crate) fn fail_code(
    reporter: &dyn Reporter,
    code: &str,
    message: impl Into<String>,
    exit: i32,
) -> i32 {
    emit_error(reporter, code, message);
    exit
}

/// The bytes of `file` (read from disk), or the embedded [`BUNDLE_GTS`] when
/// `file` is `None` — the repo-free default every command shares.
fn gts_bytes(reporter: &dyn Reporter, file: Option<&Path>) -> Result<Cow<'static, [u8]>, i32> {
    match file {
        None => Ok(Cow::Borrowed(BUNDLE_GTS)),
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => Ok(Cow::Owned(bytes)),
            Err(e) => Err(fail(
                reporter,
                "gmeow-cli.io.read",
                format!("cannot read {}: {e}", path.display()),
            )),
        },
    }
}

/// Read a file's bytes or fail with a clean CLI error.
fn read_bytes(reporter: &dyn Reporter, path: &Path) -> Result<Vec<u8>, i32> {
    std::fs::read(path).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.io.read",
            format!("cannot read {}: {e}", path.display()),
        )
    })
}

/// Build the internal→BCP-47 language tag map from a snapshot (its default-graph
/// N-Triples projection), for the language selector.
fn bundle_tag_map(bytes: &[u8]) -> gmeow_errors::Result<HashMap<String, String>> {
    let dataset = purrdf::gts::flattened_dataset_from_bytes(bytes).map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("cannot fold snapshot: {e}"),
        })
    })?;
    let nt = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("cannot project snapshot to N-Triples: {e}"),
        })
    })?;
    gmeow_validate::language_tags::load_tag_map(&nt, "n-triples").map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("cannot load language tag map: {e}"),
        })
    })
}

/// Resolve the `--lang` / `GMEOW_LANG` request against a snapshot's tag map into a
/// [`LangSelector`](gmeow_validate::language_tags::LangSelector). The env read
/// happens here (the bin's concern); an explicit `--lang` (incl. `''`) wins.
fn resolve_selector(
    reporter: &dyn Reporter,
    lang: Option<&str>,
    bytes: &[u8],
) -> Result<gmeow_validate::language_tags::LangSelector, i32> {
    let tag_map = bundle_tag_map(bytes)
        .map_err(|e| fail(reporter, "gmeow-cli.lang.tag-map", e.to_string()))?;
    let raw: Option<String> = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok());
    gmeow_validate::language_tags::resolve_lang_input(raw.as_deref(), &tag_map, None).map_err(|u| {
        fail(
            reporter,
            "gmeow-cli.lang.unknown",
            format!(
                "unknown language tag '{}'. Available languages: {}",
                u.tag,
                u.available.join(", ")
            ),
        )
    })
}

// ── version / info ───────────────────────────────────────────────────────────

/// `gmeow version` — print the package version to stdout.
pub fn version() -> i32 {
    println!("{}", env!("CARGO_PKG_VERSION"));
    0
}

/// `gmeow info` — print a count summary of a GTS snapshot.
pub fn info(reporter: &dyn Reporter, file: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(reporter, file) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let graph = purrdf::gts::reader::read(&bytes, true, None);
    let title = file
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "gmeow.gts".to_owned());
    println!("{title}");
    println!("  terms        {}", graph.terms.len());
    println!("  quads        {}", graph.quads.len());
    println!("  reifiers     {}", graph.reifiers.len());
    println!("  annotations  {}", graph.annotations.len());
    println!("  docs blobs   {}", graph.blobs.len());
    println!("  opaque       {}", graph.opaque.len());
    for diag in &graph.diagnostics {
        gmeow_cli_core::note(
            reporter,
            "gmeow",
            "gmeow-cli.info.snapshot-diagnostic",
            format!("{}: {}", diag.code, diag.detail),
        );
    }
    0
}

// ── verify / verify-release-bundle ───────────────────────────────────────────

/// `gmeow verify` — the native OpenPGP signature check, the blob-DAG integrity
/// law, and the source-free ontology-completeness checks, all folded into ONE
/// unified proof-carrying report that renders identically to `gmeow validate`
/// on `--format human|sarif|json` (every finding carrying its remediation "how
/// to fix" and per-term usage guidance). None of those legs reason, so plain
/// `verify` is fast even over the full shipped bundle.
///
/// The reasoned deep-semantic pass is **`--deep`-gated**, mirroring `validate
/// --deep`: only under `deep` does verify additionally run the Tier-2 native
/// semantic pass and emit real `validate.deep.*` reasoned-quad verdicts (the
/// witnesses the explain-skeleton derivation attaches to).
pub fn verify(
    reporter: &dyn Reporter,
    file: Option<&Path>,
    trusted_key: Option<&Path>,
    allow_unsigned: bool,
    format: &str,
    deep: bool,
) -> i32 {
    let output = format.to_lowercase();
    if !matches!(output.as_str(), "human" | "sarif" | "json") {
        return fail(
            reporter,
            "gmeow-cli.verify.unknown-format",
            format!("unknown --format {output:?}: expected human, sarif, or json"),
        );
    }
    let bytes = match gts_bytes(reporter, file) {
        Ok(b) => b,
        Err(code) => return code,
    };

    // The single unified report every leg folds into, and the artifact under
    // inspection is its OWN `documented_terms` subject (the F3 contract:
    // `subject = bundle`).
    let mut report = gmeow_errors::Report::new("verify");

    // 1. Signature/trust check via the native `purrdf::gts::verify` primitive,
    // in-process — no external `gts` binary. Every signature/trust finding
    // (resolved key/fingerprint, missing signature, untrusted signer, …) folds
    // into the unified report so it renders enriched on every channel; the
    // hard-fail boolean still governs the exit code (a bad signature must fail).
    let config = gmeow_validate::validate_all::SignatureConfig {
        require_signatures: !allow_unsigned,
        trusted_key: trusted_key.map(|p| p.to_string_lossy().into_owned()),
        ..Default::default()
    };
    let (sig_findings, sig_hard_fail) =
        match gmeow_validate::signature::verify_gts_bundle(&bytes, &config) {
            Ok(pair) => pair,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.verify.signature",
                    format!("signature verification failed: {e}"),
                );
            }
        };
    let sig_ok = !sig_hard_fail;
    for finding in sig_findings {
        report.add_finding(finding);
    }

    // 2. Blob-DAG integrity over the folded snapshot (the reusable law from
    // `Bundle::integrity_report`): no dangling content-addressed reference, no
    // orphan blob, no hash-integrity mismatch. A hard-fail gate (never silently
    // accepted).
    let integrity_report = match gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(&bytes)
        .and_then(|bundle| bundle.integrity_report())
    {
        Ok(report) => report,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.verify.integrity",
                format!("blob integrity check failed: {e}"),
            );
        }
    };
    let integrity_ok = integrity_report.is_clean();

    // 3. Source-free ontology-completeness findings over the folded snapshot: one
    // rule-coded, ledger-identified `Finding` per missing label / definition per
    // documented term, each carrying the term IRI as its `documented_terms` join
    // key so the per-term guidance lights up on verify too.
    if let Err(e) = append_ontology_findings(&bytes, &mut report) {
        return fail(
            reporter,
            "gmeow-cli.verify.ontology-checks",
            format!("bundled ontology checks failed: {e}"),
        );
    }

    // 4. The reasoned deep-semantic pass over the bundle, `--deep`-gated (TRUE
    // parity with `validate --deep`): plain `verify` never reasons. When `deep`
    // is set, this calls the SAME public entry the dev bundle pass runs, so
    // verify emits real `validate.deep.*` reasoned-quad verdicts (the witnesses
    // the derivation attaches to). Honors the Task-5 hard-fail: a verdict that
    // cannot be joined to its explain-skeleton derivation propagates as `Err`
    // and is a `Severity::Error` failure here, never swallowed.
    if deep && let Err(e) = gmeow_validate::validate_all::bundle_deep_findings(&bytes, &mut report)
    {
        return fail(
            reporter,
            "gmeow-cli.verify.deep",
            format!("deep semantic pass failed: {e}"),
        );
    }

    // 5. The single proof-carrying enrichment pass over the unified report:
    // rule identity + registry-authored remediation + per-term usage guidance +
    // derivation, so verify renders every enrichment identically to validate. The
    // bundle IS both the rule-catalog graph and the `documented_terms` subject.
    let bundle_ds = match purrdf::import_gts_events(&bytes) {
        Ok(b) => b,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.verify.fold",
                format!("cannot fold snapshot for enrichment: {e}"),
            );
        }
    };
    let subject = bundle_ds.dataset.as_ref();
    gmeow_validate::enrich::enrich_findings(&mut report, subject, subject);

    // 6. Render the unified report on the chosen channel, then compute the exit
    // code from the unified report's error_count PLUS the signature and integrity
    // hard-fails (do not regress the existing verify failure semantics).
    match output.as_str() {
        "sarif" => match gmeow_errors::render::to_sarif(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.verify.render-sarif",
                    format!("cannot render SARIF: {e}"),
                );
            }
        },
        "json" => match gmeow_errors::render::to_json(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.verify.render-json",
                    format!("cannot render JSON: {e}"),
                );
            }
        },
        _ => {
            let text = gmeow_errors::render::to_text(&report);
            if !text.is_empty() {
                println!("{text}");
            }
        }
    }

    let verify_failed = !sig_ok || !integrity_ok || report.error_count() > 0;
    if verify_failed {
        return fail(reporter, "gmeow-cli.verify.failed", "verification failed");
    }
    if output == "human" {
        println!("verification passed");
    }
    0
}

/// Fold one rule-coded, ledger-identified [`Finding`] per ontology-completeness
/// gap — a missing `rdfs:label` or `skos:definition` on a documented class/property
/// — into `report`, routing each through a [`DiagLedger`] so it carries a stable
/// `finding_iri`/anchor identity and its term IRI as `documented_terms` (the Task-4
/// documented-term guidance join key). Non-blocking Warnings: a bundle that passes
/// `gmeow verify` today has none, so the exit contract is preserved.
fn append_ontology_findings(bytes: &[u8], report: &mut gmeow_errors::Report) -> Result<(), Diag> {
    use gmeow_errors::model::Location;
    use gmeow_errors::{DiagLedger, StageId, code::register_code};

    let terms = gmeow_pipeline::cli_ops::confirmations::bundle_term_summaries(bytes)?;
    let stage = StageId::new("verify.ontology");
    let grade = Grade::new(
        Severity::Warning,
        FindingCategory::PolicyWarning,
        Standpoint::Perspectival,
    );
    let mut ledger = DiagLedger::new();
    for term in &terms {
        // The term IRI anchors the finding (a non-trivial source context) and is
        // its `documented_terms` join key.
        let anchor = || Location {
            logical: Some(term.iri.clone()),
            ..Location::default()
        };
        if term.label.is_empty() {
            ledger.attach(
                Diag::new(
                    register_code(gmeow_validate::codes::ONTOLOGY_MISSING_LABEL),
                    grade,
                    format!("term {} carries no rdfs:label", term.curie),
                )
                .with_documented_term(term.iri.clone())
                .with_location(anchor()),
                stage.clone(),
            );
        }
        if term.definition.is_empty() {
            ledger.attach(
                Diag::new(
                    register_code(gmeow_validate::codes::ONTOLOGY_MISSING_DEFINITION),
                    grade,
                    format!("term {} carries no skos:definition", term.curie),
                )
                .with_documented_term(term.iri.clone())
                .with_location(anchor()),
                stage.clone(),
            );
        }
    }
    for finding in ledger.findings("verify") {
        report.add_finding(finding);
    }
    Ok(())
}

/// `gmeow verify-release-bundle` — native COSE + attestation-walk verification.
pub fn verify_release_bundle(
    reporter: &dyn Reporter,
    bundle: &Path,
    public_key: Option<&Path>,
) -> i32 {
    let bundle_bytes = match read_bytes(reporter, bundle) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let armor = match public_key {
        None => None,
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.verify-release.public-key",
                    format!("✗ public key {} is unreadable: {e}", path.display()),
                );
            }
        },
    };
    match gmeow_pipeline::stages::release::verify_release_bundle(&bundle_bytes, armor.as_deref()) {
        Ok(report) => {
            let key_line = report.kid.map(|k| format!(", key {k}")).unwrap_or_default();
            let fp_line = report
                .fingerprint
                .map(|f| format!(", fingerprint {f}"))
                .unwrap_or_default();
            println!(
                "✓ release verified: {} ({}/{} valid signature(s){key_line}{fp_line}, \
                 {} attested artifact(s) present)",
                bundle.display(),
                report.valid,
                report.signed,
                report.artifacts_verified,
            );
            0
        }
        Err(e) => fail(
            reporter,
            "gmeow-cli.verify-release.failed",
            format!("✗ release verification failed: {e}"),
        ),
    }
}

// ── describe ─────────────────────────────────────────────────────────────────

/// `gmeow describe` — render one term card from a GTS snapshot.
pub fn describe(
    reporter: &dyn Reporter,
    term: &str,
    gts: Option<&Path>,
    lang: Option<&str>,
    format: gmeow_docs::card::CardFormat,
) -> i32 {
    let bytes = match gts_bytes(reporter, gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // The env/`--lang` precedence is the bin's concern; the backend does the
    // snapshot-aware language resolution. An explicit `--lang` (incl. `''`) wins
    // over `GMEOW_LANG`.
    let resolved: Option<String> = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok());
    // The JSON Schema `$defs` key set folded into THIS bundle — the
    // model-existence signal `build_card` gates a class's `python_model` link on
    // (a class with no `$defs` entry has no generated Pydantic model, so the link
    // must never be fabricated: issue "Pydantic model surface", finding F3).
    let modeled_defs = match gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(&bytes)
        .and_then(|bundle| bundle.modeled_def_keys())
    {
        Ok(defs) => defs,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.describe.modeled-defs",
                format!("cannot read the bundled JSON Schema for the model-existence gate: {e}"),
            );
        }
    };
    let (text, status) =
        gmeow_docs::describe(term, &bytes, resolved.as_deref(), format, &modeled_defs);
    // Map each backend failure kind to its OWN typed diagnostic code — a resolution
    // miss, a cross-namespace ambiguity, an unknown language, and a bundle-load
    // failure are distinct, greppable codes (the old path lumped them all under
    // `describe.unresolved`).
    use gmeow_docs::DescribeStatus;
    match status {
        DescribeStatus::Ok => {
            println!("{text}");
            0
        }
        DescribeStatus::Unresolved => {
            reporter.report(&report_diag(
                Diag::of_kind(crate::error::DescribeUnresolved { detail: text }),
                "gmeow",
            ));
            status.exit_code()
        }
        DescribeStatus::Ambiguous => {
            reporter.report(&report_diag(
                Diag::of_kind(crate::error::DescribeAmbiguous { detail: text }),
                "gmeow",
            ));
            status.exit_code()
        }
        DescribeStatus::UnknownLanguage => {
            fail_code(reporter, "gmeow-cli.lang.unknown", text, status.exit_code())
        }
        DescribeStatus::LoadFailed => {
            reporter.report(&report_diag(
                Diag::of_kind(crate::error::RdfPipelineFailed { detail: text }),
                "gmeow",
            ));
            status.exit_code()
        }
    }
}

// ── conjecture ─────────────────────────────────────────────────────────────────

/// `gmeow conjecture test` — test a candidate `logic:` formula against a KB in an
/// isolated, standpoint-scoped scenario world, print the engine verdict, and —
/// unless `--dry-run` — APPEND it to the append-only conjecture library. Delegates
/// to the SHARED [`gmeow_pipeline::mcp::run_conjecture_test`] core (the same path
/// the MCP `conjecture_test` tool runs), so there is one implementation, not two.
#[allow(clippy::too_many_arguments)]
pub fn conjecture_test(
    reporter: &dyn Reporter,
    formula: &Path,
    kb: &Path,
    standpoint: &str,
    math_conjecture: Option<&str>,
    dry_run: bool,
    max_steps: Option<u64>,
    max_answers: Option<usize>,
) -> i32 {
    let formula_ttl = match std::fs::read_to_string(formula) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.conjecture.read",
                format!("cannot read {}: {e}", formula.display()),
            );
        }
    };
    let kb_ttl = match std::fs::read_to_string(kb) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.conjecture.read",
                format!("cannot read {}: {e}", kb.display()),
            );
        }
    };

    let out =
        match gmeow_pipeline::mcp::run_conjecture_test(&gmeow_pipeline::mcp::ConjectureRunInput {
            formula_ttl: &formula_ttl,
            kb_ttl: &kb_ttl,
            standpoint,
            math_conjecture,
            dry_run,
            max_steps,
            max_answers,
        }) {
            Ok(out) => out,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.conjecture.failed",
                    format!("conjecture test failed: {e}"),
                );
            }
        };

    // A precondition-unmet TR gate refused the write: report it and fail (exit 1),
    // mirroring the MCP `ok:false` path — the verdict was computed but not persisted.
    if let Some(reason) = &out.precondition_unmet {
        println!("lifecycle {}", out.lifecycle);
        println!("information {}", out.information);
        return fail(
            reporter,
            "gmeow-cli.conjecture.precondition-unmet",
            format!("persistConjecture precondition unmet: {reason}"),
        );
    }

    // Product results → stdout with stable, greppable key prefixes.
    println!("lifecycle {}", out.lifecycle);
    println!("information {}", out.information);
    println!("evaluation {}", out.evaluation);
    println!("completeness {}", out.completeness);
    println!("discharge {}", out.discharge);
    println!("conjecture {}", out.node_iri);
    if let Some(witness) = &out.witness {
        println!("witness-individual {}", witness.individual);
        println!("witness-world {}", witness.world);
        for premise in &witness.premises {
            println!("witness-premise {premise}");
        }
    }
    if out.dry_run {
        println!("persisted dry-run (nothing written)");
    } else if out.committed {
        println!("persisted committed");
    } else {
        println!("persisted no");
    }
    0
}

// ── validate ─────────────────────────────────────────────────────────────────

/// The native RDF format id for a file suffix, mirroring
/// `gmeow_tools.validate_data.format_for_suffix`.
fn rdf_format_for_suffix(suffix: &str) -> Option<&'static str> {
    match suffix {
        ".nq" | ".nquads" => Some("nquads"),
        ".trig" => Some("trig"),
        ".ttl" | ".turtle" => Some("turtle"),
        ".nt" | ".ntriples" => Some("ntriples"),
        ".rdf" | ".owl" => Some("rdf+xml"),
        ".jsonld" => Some("json-ld"),
        _ => None,
    }
}

/// `gmeow validate` — RDF conformance against the bundle, or a JSON/YAML instance
/// against a JSON Schema. The mode is chosen by file type.
pub fn validate(
    reporter: &dyn Reporter,
    instance: &Path,
    schema: Option<&Path>,
    format: &str,
    deep: bool,
) -> i32 {
    let suffix = instance
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let rdf_format = rdf_format_for_suffix(&suffix);
    if deep && (schema.is_some() || rdf_format.is_none()) {
        return fail(
            reporter,
            "gmeow-cli.validate.deep-unsupported",
            "--deep is only supported for RDF validation without --schema",
        );
    }
    if schema.is_none()
        && let Some(fmt) = rdf_format
    {
        return validate_rdf(reporter, instance, fmt, format, deep);
    }
    validate_instance(reporter, instance, schema)
}

/// The repo-free RDF Tier-1 (and opt-in Tier-2) conformance path.
fn validate_rdf(
    reporter: &dyn Reporter,
    instance: &Path,
    fmt: &str,
    output: &str,
    deep: bool,
) -> i32 {
    let output = output.to_lowercase();
    if !matches!(output.as_str(), "human" | "sarif" | "json") {
        return fail(
            reporter,
            "gmeow-cli.validate.unknown-format",
            format!("unknown --format {output:?}: expected human, sarif, or json"),
        );
    }
    let data = match read_bytes(reporter, instance) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let report = match gmeow_validate::data_validate::run(
        &data,
        fmt,
        BUNDLE_GTS,
        NAMESPACE,
        &instance.display().to_string(),
        deep,
    ) {
        Ok(r) => r,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.validate.run",
                format!("validation error: {e}"),
            );
        }
    };

    match output.as_str() {
        "sarif" => match gmeow_errors::render::to_sarif(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.validate.render-sarif",
                    format!("cannot render SARIF: {e}"),
                );
            }
        },
        "json" => match gmeow_errors::render::to_json(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.validate.render-json",
                    format!("cannot render JSON: {e}"),
                );
            }
        },
        _ => {
            // The conformance findings ARE a substrate report: surface them on the
            // reporter's channel (human text on stderr, NDJSON for agents) instead
            // of hand-rendering text to stderr.
            reporter.report(&report);
            if report.error_count() == 0 && report.warning_count() == 0 {
                println!("validation passed");
            }
        }
    }
    if report.error_count() > 0 { 1 } else { 0 }
}

/// The JSON/YAML instance-against-schema path.
fn validate_instance(reporter: &dyn Reporter, instance: &Path, schema: Option<&Path>) -> i32 {
    let suffix = instance
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let fmt = match suffix.as_str() {
        ".json" | ".jsonld" => gmeow_validate::instance::InstanceFormat::Json,
        ".yaml" | ".yml" => gmeow_validate::instance::InstanceFormat::Yaml,
        _ => {
            return fail(
                reporter,
                "gmeow-cli.validate.unknown-instance-format",
                format!(
                    "cannot infer format from {}: expected a .json, .jsonld, .yaml, or .yml \
                     instance for JSON-Schema validation",
                    instance
                        .file_name()
                        .map(|n| n.to_string_lossy())
                        .unwrap_or_default()
                ),
            );
        }
    };
    let instance_bytes = match read_bytes(reporter, instance) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let schema_bytes = match schema {
        Some(path) => match read_bytes(reporter, path) {
            Ok(b) => b,
            Err(code) => return code,
        },
        None => match gmeow_pipeline::bundle_blobs::bundled_schema(BUNDLE_GTS) {
            Ok(Some(b)) => b,
            Ok(None) => {
                return fail(
                    reporter,
                    "gmeow-cli.validate.no-bundled-schema",
                    "no bundled JSON Schema; pass one with --schema",
                );
            }
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.validate.bundled-schema",
                    format!("cannot read bundled JSON Schema: {e}"),
                );
            }
        },
    };
    match gmeow_validate::instance::validate_instance(&instance_bytes, fmt, &schema_bytes) {
        Ok(violations) if violations.is_empty() => {
            println!("validation passed");
            0
        }
        Ok(violations) => {
            for v in &violations {
                emit_error(
                    reporter,
                    "gmeow-cli.validate.instance-violation",
                    v.to_string(),
                );
            }
            1
        }
        Err(e) => fail(
            reporter,
            "gmeow-cli.validate.instance-run",
            format!("validation error: {e}"),
        ),
    }
}

// ── build ────────────────────────────────────────────────────────────────────

/// `gmeow build` — write derived serializations of a GTS snapshot.
pub fn build(reporter: &dyn Reporter, out: &Path, gts: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(reporter, gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let dataset = match purrdf::gts::flattened_dataset_from_bytes(&bytes) {
        Ok(ds) => ds,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.build.fold",
                format!("cannot fold snapshot: {e}"),
            );
        }
    };
    if let Err(e) = std::fs::create_dir_all(out) {
        return fail(
            reporter,
            "gmeow-cli.io.create-dir",
            format!("cannot create {}: {e}", out.display()),
        );
    }

    // N-Quads (the full RDF-1.2 statement layer).
    let writes: &[(&str, &str)] = &[
        ("gmeow.nq", "application/n-quads"),
        ("gmeow.ttl", "text/turtle"),
        ("gmeow.nt", "application/n-triples"),
    ];
    for (name, media) in writes {
        let selection = if *name == "gmeow.nt" {
            purrdf::SerializeGraph::DefaultGraph
        } else {
            purrdf::SerializeGraph::Dataset
        };
        match purrdf::serialize_dataset(&dataset, media, selection) {
            Ok(data) => {
                let target = out.join(name);
                if let Err(e) = std::fs::write(&target, data) {
                    return fail(
                        reporter,
                        "gmeow-cli.io.write",
                        format!("cannot write {}: {e}", target.display()),
                    );
                }
                println!("wrote {}", target.display());
            }
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.build.serialize",
                    format!("cannot serialize {name}: {e}"),
                );
            }
        }
    }

    // RDF-1.2-star: JSON-LD-star + YAML-LD-star, via the native pipeline serializer.
    match gmeow_pipeline::stages::yaml_ld::serialize_graph(&dataset) {
        Ok(text) => {
            let target = out.join("gmeow.jsonld");
            if let Err(e) = std::fs::write(&target, text) {
                return fail(
                    reporter,
                    "gmeow-cli.io.write",
                    format!("cannot write {}: {e}", target.display()),
                );
            }
            println!("wrote {}", target.display());
        }
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.build.serialize",
                format!("cannot serialize gmeow.jsonld: {e}"),
            );
        }
    }
    match gmeow_pipeline::stages::yaml_ld::serialize_graph_yaml(&dataset, None) {
        Ok(text) => {
            let target = out.join("gmeow.yamlld");
            if let Err(e) = std::fs::write(&target, text) {
                return fail(
                    reporter,
                    "gmeow-cli.io.write",
                    format!("cannot write {}: {e}", target.display()),
                );
            }
            println!("wrote {}", target.display());
        }
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.build.serialize",
                format!("cannot serialize gmeow.yamlld: {e}"),
            );
        }
    }
    0
}

// ── project ──────────────────────────────────────────────────────────────────

/// Re-serialize an N-Triples document as Turtle for the projection output.
fn nt_to_turtle(nt: &str) -> gmeow_errors::Result<Vec<u8>> {
    let dataset =
        purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None).map_err(|e| {
            Diag::of_kind(crate::error::RdfPipelineFailed {
                detail: format!("projected N-Triples parse failed: {e}"),
            })
        })?;
    purrdf::serialize_dataset(
        &dataset,
        "text/turtle",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("Turtle serialization failed: {e}"),
        })
    })
}

/// `gmeow project` — a per-profile CONSTRUCT over a data file, or a view filter
/// over a `.gts` / the bundle.
pub fn project(
    reporter: &dyn Reporter,
    source: Option<&Path>,
    profile: &str,
    out: &Path,
    format: &str,
    lang: Option<&str>,
) -> i32 {
    use gmeow_pipeline::projections::{self, GTS_VIEW_ALL, GTS_VIEW_GMEOW, TagMap};

    let fmt_lower = format.to_lowercase();
    if fmt_lower == "yaml-ld" {
        if source.is_some() {
            return fail(
                reporter,
                "gmeow-cli.project.yaml-ld-source",
                "--format yaml-ld reads the bundled snapshot only; do not pass a source file",
            );
        }
        let yamlld = match gmeow_pipeline::bundle_blobs::bundled_yaml_ld(BUNDLE_GTS) {
            Ok(map) => map.get("gmeow.yamlld").cloned(),
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.project.yaml-ld-read",
                    format!("cannot read bundled YAML-LD: {e}"),
                );
            }
        };
        let Some(yamlld) = yamlld else {
            return fail(
                reporter,
                "gmeow-cli.project.yaml-ld-missing",
                "bundled YAML-LD snapshot not found",
            );
        };
        if let Err(e) = std::fs::create_dir_all(out) {
            return fail(
                reporter,
                "gmeow-cli.io.create-dir",
                format!("cannot create {}: {e}", out.display()),
            );
        }
        let target = out.join("gmeow.yamlld");
        if let Err(e) = std::fs::write(&target, yamlld) {
            return fail(
                reporter,
                "gmeow-cli.io.write",
                format!("cannot write {}: {e}", target.display()),
            );
        }
        println!("wrote {}", target.display());
        return 0;
    }
    if !matches!(fmt_lower.as_str(), "turtle" | "ttl") {
        return fail(
            reporter,
            "gmeow-cli.project.unknown-format",
            format!("unknown --format: {format}"),
        );
    }

    let is_gts = source.is_some_and(|s| {
        s.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gts"))
    });
    let is_data_file = source.is_some() && !is_gts;

    // The language selector is resolved against the target graph (the bundle for
    // a data file, else the supplied snapshot).
    let selector_bytes = match gts_bytes(reporter, if is_data_file { None } else { source }) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // Resolving the selector validates `--lang` (an unknown tag hard-fails); the
    // retag itself uses the full internal→public tag map below.
    let _selector = match resolve_selector(reporter, lang, &selector_bytes) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let tag_map: TagMap = build_project_tag_map(&selector_bytes);

    if let Err(e) = std::fs::create_dir_all(out) {
        return fail(
            reporter,
            "gmeow-cli.io.create-dir",
            format!("cannot create {}: {e}", out.display()),
        );
    }

    if is_data_file {
        let source = source.expect("checked");
        let known = projections::profiles();
        if !known.contains_key(profile) {
            return fail(
                reporter,
                "gmeow-cli.project.unknown-profile",
                format!("unknown projection profile: {profile} (a vocabulary profile)"),
            );
        }
        return project_data_file(reporter, source, profile, out, &tag_map);
    }

    // View filter over a `.gts` / the bundle.
    let known = projections::profiles();
    let valid =
        known.contains_key(profile) || profile == GTS_VIEW_GMEOW || GTS_VIEW_ALL.contains(&profile);
    if !valid {
        return fail(
            reporter,
            "gmeow-cli.project.unknown-view",
            format!("unknown view: {profile} (vocab | gmeow | all | maximal)"),
        );
    }
    let bytes = match gts_bytes(reporter, source) {
        Ok(b) => b,
        Err(code) => return code,
    };
    match projections::project_gts_subset(&bytes, profile, &tag_map) {
        Ok(nt) => match nt_to_turtle(&nt) {
            Ok(ttl) => {
                let target = out.join(format!("{profile}.ttl"));
                if let Err(e) = std::fs::write(&target, ttl) {
                    return fail(
                        reporter,
                        "gmeow-cli.io.write",
                        format!("cannot write {}: {e}", target.display()),
                    );
                }
                println!("wrote {}", target.display());
                0
            }
            Err(e) => fail(reporter, "gmeow-cli.project.turtle", e.to_string()),
        },
        Err(e) => fail(reporter, "gmeow-cli.project.subset", e.to_string()),
    }
}

/// The internal→BCP-47 tag map restricted to the actual retag surface (an empty
/// map is a valid no-op that leaves internal tags in place).
fn build_project_tag_map(bytes: &[u8]) -> gmeow_pipeline::projections::TagMap {
    bundle_tag_map(bytes)
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

/// Run a profile's bundled CONSTRUCT over a user data file merged with the bundle
/// ontology, writing the projected Turtle.
fn project_data_file(
    reporter: &dyn Reporter,
    source: &Path,
    profile: &str,
    out: &Path,
    tag_map: &gmeow_pipeline::projections::TagMap,
) -> i32 {
    // The compiled CONSTRUCT for this profile, from the bundle's query archive.
    let queries = match gmeow_pipeline::bundle_blobs::bundled_queries(BUNDLE_GTS) {
        Ok(q) => q,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.project.queries",
                format!("cannot read bundled queries: {e}"),
            );
        }
    };
    let want = format!("{profile}.rq");
    let query = queries
        .iter()
        .find(|(k, _)| k.ends_with(&want))
        .map(|(_, v)| String::from_utf8_lossy(v).into_owned());
    let Some(query) = query else {
        return fail(
            reporter,
            "gmeow-cli.project.no-query",
            format!("no bundled CONSTRUCT query for profile {profile}"),
        );
    };

    // source_nt = the bundle ontology base graph + the user's instance data.
    let base = match gmeow_pipeline::projections::gts_base_graph(BUNDLE_GTS) {
        Ok(b) => b,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.project.base-graph",
                format!("cannot read bundle base graph: {e}"),
            );
        }
    };
    let ontology_nt = match quads_to_nt(&base) {
        Ok(nt) => nt,
        Err(e) => return fail(reporter, "gmeow-cli.project.ntriples", e.to_string()),
    };
    let instance_bytes = match read_bytes(reporter, source) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let instance_ds = match purrdf::parse_dataset(&instance_bytes, "text/turtle", None) {
        Ok(ds) => ds,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.project.parse-instance",
                format!("cannot parse {}: {e}", source.display()),
            );
        }
    };
    let instance_nt = match purrdf::serialize_dataset(
        &instance_ds,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    ) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.project.ntriples",
                format!("cannot project instance to N-Triples: {e}"),
            );
        }
    };
    let source_nt = format!("{ontology_nt}\n{instance_nt}");

    match gmeow_pipeline::projections::project_graph(&source_nt, &query, tag_map) {
        Ok(nt) => match nt_to_turtle(&nt) {
            Ok(ttl) => {
                let target = out.join(format!("{profile}.ttl"));
                if let Err(e) = std::fs::write(&target, ttl) {
                    return fail(
                        reporter,
                        "gmeow-cli.io.write",
                        format!("cannot write {}: {e}", target.display()),
                    );
                }
                println!("wrote {}", target.display());
                0
            }
            Err(e) => fail(reporter, "gmeow-cli.project.turtle", e.to_string()),
        },
        Err(e) => fail(reporter, "gmeow-cli.project.graph", e.to_string()),
    }
}

/// Serialize a flat default-graph quad stream to canonical N-Triples.
fn quads_to_nt(quads: &[purrdf::RdfQuad]) -> gmeow_errors::Result<String> {
    let flat = purrdf::flat_dataset_from_quads(quads).map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("N-Triples flatten failed: {e}"),
        })
    })?;
    let bytes = purrdf::serialize_dataset(
        &flat,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("N-Triples serialization failed: {e}"),
        })
    })?;
    String::from_utf8(bytes).map_err(|e| {
        Diag::of_kind(crate::error::OutputEncodingFailed {
            detail: format!("N-Triples output is not UTF-8: {e}"),
        })
    })
}

// ── transpile ────────────────────────────────────────────────────────────────

/// `gmeow transpile` — consumer RDF → pure GMEOW → MAXIMAL multi-vocab, or an OKF
/// bundle directory routed through the OKF lift lane.
pub fn transpile(
    reporter: &dyn Reporter,
    source: &Path,
    out: Option<&Path>,
    profiles: &str,
    lang: Option<&str>,
) -> i32 {
    use gmeow_pipeline::projections::{self, TagMap};

    let selector_bytes: Cow<'static, [u8]> = Cow::Borrowed(BUNDLE_GTS);
    let _selector = match resolve_selector(reporter, lang, &selector_bytes) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let tag_map: TagMap = build_project_tag_map(&selector_bytes);

    // Validate any requested profile names against the registry.
    let known = projections::profiles();
    if profiles != "all" {
        let unknown: Vec<&str> = profiles
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty() && !known.contains_key(*p))
            .collect();
        if !unknown.is_empty() {
            return fail(
                reporter,
                "gmeow-cli.transpile.unknown-profile",
                format!("unknown projection profile(s): {}", unknown.join(", ")),
            );
        }
    }

    // Assemble the lawful up-projection + maximal inputs from the embedded bundle.
    let (up_inputs, maximal_inputs) = match assemble_transpile_inputs() {
        Ok(pair) => pair,
        Err(e) => return fail(reporter, "gmeow-cli.transpile.inputs", e.to_string()),
    };

    // An OKF bundle directory routes through the OKF lift lane.
    if source.is_dir() {
        let report = match gmeow_pipeline::cli_ops::okf_import::transpile_okf(
            source,
            &maximal_inputs,
            &tag_map,
            None,
            None,
        ) {
            Ok(r) => r,
            Err(e) => return fail(reporter, "gmeow-cli.transpile.okf", e.to_string()),
        };
        gmeow_cli_core::note(
            reporter,
            "gmeow",
            "gmeow-cli.transpile.progress",
            format!(
                "lifted {} okf facts · retained {} annotation(s) · subjects {}",
                report.lift.lifted, report.lift.retained, report.lift.subjects
            ),
        );
        return write_transpile_outputs(reporter, out, source, &report.draft_nt, &report.transform);
    }

    // A source RDF file (Turtle) or stdin (`-`).
    let (source_nt, stem) = match load_transpile_source(source) {
        Ok(pair) => pair,
        Err(e) => return fail(reporter, "gmeow-cli.transpile.source", e.to_string()),
    };
    match projections::transpile_graph(&source_nt, &stem, &up_inputs, &maximal_inputs, &tag_map) {
        Ok(report) => {
            gmeow_cli_core::note(
                reporter,
                "gmeow",
                "gmeow-cli.transpile.progress",
                format!(
                    "lifted {} facts · claimed {} inferred · gap {}",
                    report.lifted, report.claimed, report.gap_terms
                ),
            );
            gmeow_cli_core::note(
                reporter,
                "gmeow",
                "gmeow-cli.transpile.progress",
                format!(
                    "maximal asserted {} · saturated {} · projected {}",
                    report.transform.asserted,
                    report.transform.saturated,
                    report.transform.projected
                ),
            );
            write_transpile_outputs(reporter, out, source, &report.draft_nt, &report.transform)
        }
        Err(e) => fail(reporter, "gmeow-cli.transpile.graph", e.to_string()),
    }
}

/// Read a transpile source: Turtle from a file, or Turtle from stdin (`-`).
fn load_transpile_source(source: &Path) -> gmeow_errors::Result<(String, String)> {
    let is_stdin = source.as_os_str() == "-";
    let bytes = if is_stdin {
        use std::io::Read;
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .ctx("cannot read stdin")?;
        buf
    } else {
        std::fs::read(source).with_ctx(|| format!("cannot read {}", source.display()))?
    };
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        Diag::of_kind(crate::error::SourceReadFailed {
            detail: format!("cannot parse Turtle source: {e}"),
        })
    })?;
    let nt = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("cannot project source to N-Triples: {e}"),
        })
    })?;
    let stem = if is_stdin {
        "stdin".to_owned()
    } else {
        source
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "source".to_owned())
    };
    Ok((String::from_utf8_lossy(&nt).into_owned(), stem))
}

/// Assemble the lawful up-projection + maximal inputs from the embedded bundle:
/// the SSSOM lift maps, the projection/EDOAL TTLs, the ontology base graph, the
/// per-profile CONSTRUCT queries, and the saturation refusal set.
fn assemble_transpile_inputs() -> gmeow_errors::Result<(
    gmeow_pipeline::projections::UpProjectionInputs,
    gmeow_pipeline::projections::MaximalInputs,
)> {
    use gmeow_pipeline::bundle_blobs;

    let sssom_texts: Vec<String> = bundle_blobs::bundled_sssom(BUNDLE_GTS)
        .map_err(|e| {
            Diag::of_kind(crate::error::BundleReadFailed {
                detail: format!("cannot read bundled SSSOM: {e}"),
            })
        })?
        .into_values()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .collect();
    // The authored `gmeow:ProjectionMapping` cells live in the CELLS archive (the mappings
    // archive holds only the SSSOM surface). Reading REP_CELLS is what puts the EDOAL `=` cells
    // in front of the lawful-lift program; the old REP_MAPPINGS read folded an EMPTY `.ttl` set.
    let projection_ttls: Vec<String> = bundle_blobs::Bundle::from_snapshot(BUNDLE_GTS)
        .map_err(|e| {
            Diag::of_kind(crate::error::RdfPipelineFailed {
                detail: format!("cannot fold bundle: {e}"),
            })
        })?
        .archive(bundle_blobs::REP_CELLS)
        .map_err(|e| {
            Diag::of_kind(crate::error::BundleReadFailed {
                detail: format!("cannot read bundled cells: {e}"),
            })
        })?
        .into_iter()
        .filter(|(k, _)| k.ends_with(".ttl"))
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .collect();
    // The A→B authorization channel: the discharged mnemomorphic `=` cells (Deliverable A),
    // read from the bundle's `graph/correspondence-laws`.
    let discharged_section_cells =
        gmeow_pipeline::projections::discharged_section_cells_from_bundle(BUNDLE_GTS).map_err(
            |e| {
                Diag::of_kind(crate::error::BundleReadFailed {
                    detail: format!("cannot read discharged section cells: {e}"),
                })
            },
        )?;
    let base = gmeow_pipeline::projections::gts_base_graph(BUNDLE_GTS).map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("cannot read bundled base graph: {e}"),
        })
    })?;
    let ontology_nt = quads_to_nt(&base)?;

    let projection_queries: Vec<(String, String)> = bundle_blobs::bundled_queries(BUNDLE_GTS)
        .map_err(|e| {
            Diag::of_kind(crate::error::BundleReadFailed {
                detail: format!("cannot read bundled queries: {e}"),
            })
        })?
        .into_iter()
        .filter(|(k, _)| k.ends_with(".rq"))
        .map(|(k, v)| {
            let stem = Path::new(&k)
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or(k);
            (stem, String::from_utf8_lossy(&v).into_owned())
        })
        .collect();
    let denied = bundle_blobs::bundled_denied_cells(BUNDLE_GTS)
        .map_err(|e| {
            Diag::of_kind(crate::error::BundleReadFailed {
                detail: format!("cannot read denied cells: {e}"),
            })
        })?
        .unwrap_or_default();

    let up_inputs = gmeow_pipeline::projections::UpProjectionInputs {
        sssom_texts,
        projection_ttls,
        ontology_nt: ontology_nt.clone(),
        discharged_section_cells,
    };
    let maximal_inputs = gmeow_pipeline::projections::MaximalInputs {
        ontology_nt,
        cells: Vec::new(),
        denied,
        projection_queries,
    };
    Ok((up_inputs, maximal_inputs))
}

/// Write the transpile draft + maximal artifacts under the output directory.
fn write_transpile_outputs(
    reporter: &dyn Reporter,
    out: Option<&Path>,
    source: &Path,
    draft_nt: &str,
    transform: &gmeow_pipeline::transform::TransformReportNative,
) -> i32 {
    let stem = source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "source".to_owned());
    let out_dir = match out {
        Some(p) => p.to_path_buf(),
        None => Path::new("dist").join("transpile").join(&stem),
    };
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return fail(
            reporter,
            "gmeow-cli.io.create-dir",
            format!("cannot create {}: {e}", out_dir.display()),
        );
    }
    let draft_path = out_dir.join(format!("{stem}.gmeow.nt"));
    if let Err(e) = std::fs::write(&draft_path, draft_nt) {
        return fail(
            reporter,
            "gmeow-cli.io.write",
            format!("cannot write {}: {e}", draft_path.display()),
        );
    }
    println!("wrote {}", draft_path.display());
    let gts_path = out_dir.join(format!("{stem}.gts"));
    if let Err(e) = std::fs::write(&gts_path, &transform.gts_bytes) {
        return fail(
            reporter,
            "gmeow-cli.io.write",
            format!("cannot write {}: {e}", gts_path.display()),
        );
    }
    println!("wrote {}", gts_path.display());
    0
}

// ── export ───────────────────────────────────────────────────────────────────

/// `gmeow export` — write every flat consumer view from a GTS snapshot.
pub fn export(reporter: &dyn Reporter, out: &Path, gts: Option<&Path>, lang: Option<&str>) -> i32 {
    let bytes = match gts_bytes(reporter, gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let selector = match resolve_selector(reporter, lang, &bytes) {
        Ok(s) => s,
        Err(code) => return code,
    };
    match gmeow_pipeline::cli_ops::confirmations::export_views(&bytes, out, &selector.requested) {
        Ok(written) => {
            for path in &written {
                println!("wrote {path}");
            }
            0
        }
        Err(e) => fail(reporter, "gmeow-cli.export.views", e.to_string()),
    }
}

// ── convert ──────────────────────────────────────────────────────────────────

/// `gmeow convert` — transcode any RDF-1.2 syntax/projection to any other,
/// recording loss.
pub fn convert(
    reporter: &dyn Reporter,
    source: &str,
    from: &str,
    to: &str,
    out: Option<&Path>,
    loss_report: Option<&Path>,
    base: Option<&str>,
) -> i32 {
    use gmeow_pipeline::transcode::{Codec, realized_loss_json, transcode as run_transcode};

    let data: Vec<u8> = if source == "-" {
        use std::io::Read;
        let mut buf = Vec::new();
        if let Err(e) = std::io::stdin().read_to_end(&mut buf) {
            return fail(
                reporter,
                "gmeow-cli.io.stdin",
                format!("cannot read stdin: {e}"),
            );
        }
        buf
    } else {
        match std::fs::read(source) {
            Ok(b) => b,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.io.read",
                    format!("cannot read {source}: {e}"),
                );
            }
        }
    };

    let from_codec = match Codec::from_cli_str(from) {
        Ok(c) => c,
        Err(e) => return fail(reporter, "gmeow-cli.convert.from-codec", e.to_string()),
    };
    let to_codec = match Codec::from_cli_str(to) {
        Ok(c) => c,
        Err(e) => return fail(reporter, "gmeow-cli.convert.to-codec", e.to_string()),
    };
    let output = match run_transcode(&data, from_codec, to_codec, base) {
        Ok(o) => o,
        Err(e) => return fail(reporter, "gmeow-cli.convert.transcode", e.to_string()),
    };

    match out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &output.bytes) {
                return fail(
                    reporter,
                    "gmeow-cli.io.write",
                    format!("cannot write {}: {e}", path.display()),
                );
            }
            println!("wrote {}", path.display());
        }
        None => {
            use std::io::Write;
            let _ = std::io::stdout().write_all(&output.bytes);
        }
    }

    let loss_json = realized_loss_json(&output.realized);
    match loss_report {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &loss_json) {
                return fail(
                    reporter,
                    "gmeow-cli.io.write",
                    format!("cannot write {}: {e}", path.display()),
                );
            }
            gmeow_cli_core::note(
                reporter,
                "gmeow",
                "gmeow-cli.convert.loss",
                format!("loss {}", path.display()),
            );
        }
        None => {
            let trimmed = loss_json.trim();
            if !trimmed.is_empty() && trimmed != "[]" {
                gmeow_cli_core::note(
                    reporter,
                    "gmeow",
                    "gmeow-cli.convert.loss",
                    format!("loss {loss_json}"),
                );
            }
        }
    }
    0
}

// ── crossref ─────────────────────────────────────────────────────────────────

/// `gmeow crossref` — generate CrossRef DOI deposit XML from self-description.
pub fn crossref(reporter: &dyn Reporter, out: &Path, gts: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(reporter, gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let dataset = match purrdf::gts::flattened_dataset_from_bytes(&bytes) {
        Ok(ds) => ds,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.crossref.fold",
                format!("cannot fold snapshot: {e}"),
            );
        }
    };
    let meta = match gmeow_validate::self_desc::load_self_description_from_dataset(&dataset) {
        Ok(m) => m,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.crossref.self-description",
                format!("self-description unavailable in GTS snapshot: {e}"),
            );
        }
    };
    let lint_json = match gmeow_validate::self_desc::lint_input_json(&meta, None, None) {
        Ok(j) => j,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.crossref.lint-input",
                format!("cannot assemble lint input: {e}"),
            );
        }
    };
    match gmeow_validate::crossref::lint_deposit(&lint_json) {
        Ok(problems) if problems.is_empty() => {}
        Ok(problems) => {
            for p in &problems {
                emit_error(
                    reporter,
                    "gmeow-cli.crossref.doi-lint",
                    format!("doi-lint {p}"),
                );
            }
            return fail(
                reporter,
                "gmeow-cli.crossref.doi-lint-summary",
                format!(
                    "✗ {} doi-lint problem(s) — fix metadata/gmeow-self.ttl",
                    problems.len()
                ),
            );
        }
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.crossref.doi-lint-failed",
                format!("doi-lint failed: {e}"),
            );
        }
    }
    let (ts, batch) = gmeow_validate::self_desc::live_stamp(&meta);
    let deposit_json = match gmeow_validate::self_desc::deposit_input_json(&meta) {
        Ok(j) => j,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.crossref.deposit-input",
                format!("cannot assemble deposit input: {e}"),
            );
        }
    };
    let xml = match gmeow_validate::crossref::build_deposit_xml(&deposit_json, &ts, &batch) {
        Ok(x) => x,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.crossref.build-xml",
                format!("cannot build deposit XML: {e}"),
            );
        }
    };
    if let Some(parent) = out.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return fail(
            reporter,
            "gmeow-cli.io.create-dir",
            format!("cannot create {}: {e}", parent.display()),
        );
    }
    if let Err(e) = std::fs::write(out, format!("{xml}\n")) {
        return fail(
            reporter,
            "gmeow-cli.io.write",
            format!("cannot write {}: {e}", out.display()),
        );
    }
    println!("wrote {} (DOI {})", out.display(), meta.doi());
    0
}

// ── mcp ──────────────────────────────────────────────────────────────────────

/// `gmeow mcp` — serve the native, bundle-only MCP consumer surface over stdio.
///
/// The embedded [`BUNDLE_GTS`] snapshot is the sole ontology source (repo-free);
/// `root = None` so no repo-reading dev tools are exposed. Blocks on the stdio
/// JSON-RPC loop until EOF, then exits `0`; a construction or I/O error maps to a
/// nonzero exit.
pub fn mcp(reporter: &dyn Reporter) -> i32 {
    use gmeow_pipeline::mcp::{McpMode, McpServer};
    let server = match McpServer::from_snapshot(BUNDLE_GTS, None, McpMode::Consumer) {
        Ok(server) => server,
        Err(e) => return fail(reporter, "gmeow-cli.mcp.construct", format!("mcp: {e}")),
    };
    match server.run_stdio() {
        Ok(()) => 0,
        Err(e) => fail(reporter, "gmeow-cli.mcp.run", format!("mcp: {e}")),
    }
}

// ── explain ──────────────────────────────────────────────────────────────────

define_diag_kind! {
    /// The `explain` target is neither a known finding fingerprint IRI nor a known
    /// anchor IRI in the bundle's `graph/diagnostics` — a hard fail, never an empty
    /// DAG rendered as a success.
    pub struct UnknownExplainTarget { target: String }
    code = "gmeow-cli.explain.unknown-target";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "unknown explain target `{}`: not a finding fingerprint IRI or an anchor IRI in graph/diagnostics", target;
}

define_diag_kind! {
    /// The provenance-DAG walk from a resolved finding failed — an unresolved
    /// antecedent or a cycle in the carried subset. A hard fail the DAG engine owns.
    pub struct ExplainWalkFailed { target: String, detail: String }
    code = "gmeow-cli.explain.walk-failed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "cannot walk provenance DAG for `{}`: {}", target, detail;
}

/// Rehydrate the finding index from a GTS snapshot's `graph/diagnostics` named
/// graph. Reads the segments into a dataset that PRESERVES named graphs
/// (`dataset_from_gts_graph`, not the flattening loader), which the reader then
/// projects — a flattened dataset would drop the graph label and read empty.
fn finding_index(bytes: &[u8]) -> gmeow_errors::Result<FindingIndex> {
    let graph = purrdf::gts::read_all_segments(bytes).map_err(|e| {
        Diag::of_kind(crate::error::SourceReadFailed {
            detail: format!("cannot read GTS segments: {e}"),
        })
    })?;
    let dataset = purrdf::gts::dataset_from_gts_graph(&graph).map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("cannot fold GTS dataset: {e}"),
        })
    })?;
    read_findings(&dataset).ctx("cannot read graph/diagnostics")
}

/// The always-emitted substrate algebra: the ledger [`verdict`] and the
/// [`minimal_fatal_cut`] (the fingerprint IRIs whose fix flips the gate to pass).
fn render_algebra(index: &FindingIndex) -> String {
    let mut out = String::new();
    out.push_str(&format!("gate verdict: {:?}\n", verdict(index)));
    let cut = minimal_fatal_cut(index);
    out.push_str(&format!(
        "minimal fatal cut ({}): fix these and the gate passes\n",
        cut.len()
    ));
    for iri in &cut {
        match index.get(iri) {
            Some(f) => out.push_str(&format!("  · {iri} [{}] {}\n", f.code, f.message)),
            None => out.push_str(&format!("  · {iri}\n")),
        }
    }
    out
}

/// The anchor cluster (findings sharing `anchor`) and any Belnap glut it carries.
/// A glut is cleanly derivable from the carried subset: the `⊑_k`-join of the
/// members' category polarities is [`Belnap::Both`] exactly when the cluster holds
/// both Supported and Opposed evidence at one anchor — the opposing pair is then
/// listed. `exclude` marks the finding the explanation is centered on.
fn render_anchor_section(
    index: &FindingIndex,
    anchor: Option<&str>,
    exclude: Option<&str>,
    out: &mut String,
) {
    let Some(anchor) = anchor else {
        out.push_str("anchor cluster: (this finding carries no anchor)\n");
        return;
    };
    let members: Vec<&Finding> = index
        .findings
        .values()
        .filter(|f| f.anchor_iri.as_deref() == Some(anchor))
        .collect();
    out.push_str(&format!(
        "anchor cluster {anchor} ({} member(s)):\n",
        members.len()
    ));
    for f in &members {
        let iri = f.finding_iri.as_deref().unwrap_or("<no-iri>");
        let mark = if Some(iri) == exclude {
            " (this finding)"
        } else {
            ""
        };
        out.push_str(&format!(
            "  · {iri} [{}] {}{}\n",
            f.severity.as_str(),
            f.code,
            mark
        ));
    }
    let joined = members.iter().fold(Belnap::Neither, |acc, f| {
        acc.join(
            f.category
                .map(FindingCategory::polarity)
                .unwrap_or(Belnap::Neither),
        )
    });
    if joined.is_glut() {
        out.push_str("gluts: this anchor carries CONTRADICTORY evidence (Belnap glut):\n");
        for f in &members {
            let pol = f
                .category
                .map(FindingCategory::polarity)
                .unwrap_or(Belnap::Neither);
            if matches!(pol, Belnap::Supported | Belnap::Opposed) {
                let iri = f.finding_iri.as_deref().unwrap_or("<no-iri>");
                out.push_str(&format!("  · {iri} polarity {pol:?} ({:?})\n", f.category));
            }
        }
    } else {
        out.push_str(
            "gluts: none at this anchor (gluts are surfaced via the anchor cluster above)\n",
        );
    }
}

/// Render the full explanation of an `explain` target — the production rendering
/// path `explain` prints. A finding fingerprint IRI walks its provenance DAG; an
/// anchor IRI resolves the cluster and walks each member. BOTH always append the
/// substrate algebra. An unknown/malformed target is a hard [`Diag`] fail — never
/// an empty DAG returned as success.
fn render_explanation(index: &FindingIndex, target: &str) -> Result<String, Diag> {
    let is_finding = index.get(target).is_some();
    let cluster: Vec<String> = index
        .findings
        .iter()
        .filter(|(_, f)| f.anchor_iri.as_deref() == Some(target))
        .map(|(iri, _)| iri.clone())
        .collect();
    let is_anchor = !cluster.is_empty();
    if !is_finding && !is_anchor {
        return Err(Diag::of_kind(UnknownExplainTarget {
            target: target.to_owned(),
        }));
    }

    let mut out = String::new();
    if is_finding {
        let f = index.get(target).expect("finding present");
        out.push_str(&format!("finding {target}\n"));
        out.push_str(&format!("  code     {}\n", f.code));
        out.push_str(&format!("  severity {}\n", f.severity.as_str()));
        out.push_str(&format!("  message  {}\n", f.message));
        out.push_str("provenance DAG:\n");
        let dag = explain_finding(index, target).map_err(|e| {
            Diag::of_kind(ExplainWalkFailed {
                target: target.to_owned(),
                detail: e.to_string(),
            })
        })?;
        out.push_str(&render_shared_dag(&dag));
        render_anchor_section(index, f.anchor_iri.as_deref(), Some(target), &mut out);
    } else {
        out.push_str(&format!("anchor cluster {target}\n"));
        out.push_str(&format!(
            "  {} finding(s) share this anchor\n",
            cluster.len()
        ));
        for iri in &cluster {
            let f = index.get(iri).expect("cluster member present");
            out.push_str(&format!(
                "  · {iri} [{}] {} — {}\n",
                f.severity.as_str(),
                f.code,
                f.message
            ));
            out.push_str("    provenance DAG:\n");
            let dag = explain_finding(index, iri).map_err(|e| {
                Diag::of_kind(ExplainWalkFailed {
                    target: iri.clone(),
                    detail: e.to_string(),
                })
            })?;
            for line in render_shared_dag(&dag).lines() {
                out.push_str("    ");
                out.push_str(line);
                out.push('\n');
            }
        }
        render_anchor_section(index, Some(target), None, &mut out);
    }
    out.push_str(&render_algebra(index));
    Ok(out)
}

/// `gmeow explain <target_iri>` — address a diagnostic witness by its fingerprint
/// IRI (a finding) or anchor IRI (a cluster) in a snapshot's `graph/diagnostics`,
/// and print its provenance DAG plus the substrate algebra. An unknown target is a
/// hard fail routed through the console error rail.
pub fn explain(reporter: &dyn Reporter, target_iri: String, file: Option<PathBuf>) -> i32 {
    let bytes = match gts_bytes(reporter, file.as_deref()) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let index = match finding_index(&bytes) {
        Ok(i) => i,
        Err(msg) => {
            return fail(
                reporter,
                "gmeow-cli.explain.read-diagnostics",
                msg.to_string(),
            );
        }
    };
    match render_explanation(&index, &target_iri) {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(diag) => gmeow_cli_core::emit_and_exit(reporter, diag, "gmeow"),
    }
}

// ── slice quality ────────────────────────────────────────────────────────────

/// `gmeow slice quality` — score an EXTERNAL slice directory against the embedded
/// `gmeow.gts` bundle's rubric (no repo checkout, no generator inputs, no
/// network): the wheel-shippable consumer runtime entry point for
/// [`gmeow_slice_quality::score_external_slice_bytes`].
pub fn slice_quality(reporter: &dyn Reporter, dir: &Path, format: &str) -> i32 {
    let report = match gmeow_slice_quality::score_external_slice_bytes(BUNDLE_GTS, dir) {
        Ok(r) => r,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.quality.score",
                format!("cannot score {}: {e}", dir.display()),
            );
        }
    };
    let rendered = match format {
        "human" => Ok(report.render_text()),
        "json" => gmeow_errors::render::to_json(&report.to_report()),
        "sarif" => gmeow_errors::render::to_sarif(&report.to_report()),
        other => {
            return fail(
                reporter,
                "gmeow-cli.slice.quality.unknown-format",
                format!("unknown --format {other:?}: expected human, json, or sarif"),
            );
        }
    };
    match rendered {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(e) => fail(
            reporter,
            "gmeow-cli.slice.quality.render",
            format!("cannot render slice-quality report: {e}"),
        ),
    }
}

// ── slice brief ──────────────────────────────────────────────────────────────

/// `gmeow slice brief` — assemble and render a `gmeow:AuthoringPacket` for a slice
/// directory, computed over the slice's OWN sources (module.ttl, mappings/, i18n/).
///
/// The per-term exemplar tiers come from the SINGLE canonical library tiering
/// [`gmeow_slice_brief::exemplar_tiers`] — the same function the `slice_brief`
/// pipeline stage uses, gated by SHACL per-term conformance against the SAME repo
/// shape union — so an in-repo slice's live CLI brief and its committed
/// `generated/briefs/authoring-packets.nt` projection tier terms identically. The repo
/// root (holding `generated/shapes/`) is resolved by walking up from the slice dir. A
/// `--batch` out of range returns a typed hard failure through [`fail`] (a non-zero
/// exit), never a panic or an empty packet.
pub fn slice_brief(
    reporter: &dyn Reporter,
    dir: &Path,
    axis: Option<&str>,
    batch: Option<u32>,
    format: &str,
) -> i32 {
    // Resolve the repo root and load the SHACL shape union the pipeline gates against,
    // so the CLI's exemplar tiering matches the committed projection in a checkout.
    let repo_root = match gmeow_slice_brief::resolve_repo_root(dir) {
        Ok(r) => r,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.brief.repo-root",
                format!("cannot resolve repo root for {}: {e}", dir.display()),
            );
        }
    };
    let shapes = match gmeow_slice_brief::load_shape_union(&repo_root) {
        Ok(s) => s,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.brief.shapes",
                format!(
                    "cannot load SHACL shape union from {}: {e}",
                    repo_root.display()
                ),
            );
        }
    };
    let tiers = match gmeow_slice_brief::exemplar_tiers(dir, &shapes) {
        Ok(t) => t,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.brief.tiers",
                format!("cannot tier {}: {e}", dir.display()),
            );
        }
    };
    let packet = match gmeow_slice_brief::assemble_packet(&gmeow_slice_brief::BriefInputs {
        slice_dir: dir,
        axis,
        batch,
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    }) {
        Ok(p) => p,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.brief.assemble",
                format!(
                    "cannot assemble authoring packet for {}: {e}",
                    dir.display()
                ),
            );
        }
    };
    let rendered = match format {
        "human" => packet.render_text(),
        "json" => packet.to_json(),
        "turtle" => packet.to_turtle(),
        other => {
            return fail(
                reporter,
                "gmeow-cli.slice.brief.unknown-format",
                format!("unknown --format {other:?}: expected human, json, or turtle"),
            );
        }
    };
    print!("{rendered}");
    0
}

#[cfg(test)]
mod explain_tests {
    use gmeow_cli_core::{ConsoleMode, reporter_for};

    use super::*;
    use crate::BUNDLE_GTS;

    #[test]
    fn explain_walks_a_real_shipped_finding_and_hard_fails_unknown() {
        // The shipped bundle's graph/diagnostics must carry real findings (the
        // loss-ledger / projection-loss witnesses populate it even when clean). If
        // it is empty, explain has nothing to walk on the shippable surface — a
        // blocker to surface, not to paper over.
        let index = finding_index(BUNDLE_GTS).expect("read diagnostics from shipped bundle");
        assert!(
            !index.is_empty(),
            "shipped bundle graph/diagnostics carries NO findings — explain has no real witness to walk"
        );

        // Pick the first real fingerprint IRI from the index.
        let real_iri = index
            .findings
            .keys()
            .next()
            .expect("at least one finding")
            .clone();
        let real_code = index.get(&real_iri).expect("finding present").code.clone();

        // The production rendering path returns the finding's IRI + code + verdict.
        let text = render_explanation(&index, &real_iri).expect("render a real finding");
        assert!(text.contains(&real_iri), "output names the finding IRI");
        assert!(text.contains(&real_code), "output names the finding code");
        assert!(
            text.contains("gate verdict:"),
            "output carries a verdict line"
        );
        assert!(
            text.contains("minimal fatal cut"),
            "output carries the minimal fatal cut"
        );

        // A real finding explains with exit 0 through the i32 surface.
        let reporter = reporter_for(ConsoleMode::Text);
        assert_eq!(
            explain(reporter.as_ref(), real_iri, None),
            0,
            "a real finding explains successfully"
        );

        // An unknown target is a hard fail: Err from the renderer AND a non-zero
        // exit through the command surface — never an empty DAG returned as 0.
        assert!(
            render_explanation(&index, "not-a-real-iri").is_err(),
            "an unknown target is a hard fail"
        );
        assert_ne!(
            explain(reporter.as_ref(), "not-a-real-iri".to_owned(), None),
            0,
            "an unknown target exits non-zero"
        );
    }
}
