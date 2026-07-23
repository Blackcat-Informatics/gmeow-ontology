// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The wired command bodies. Each function marshals its inputs and delegates to
//! an already-native backend, following the console convention: product results
//! → stdout, errors/diagnostics → stderr, and a `0`/`1` exit code.

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::ir::InMemoryPageProvider;
use purrdf::{
    DistanceMetric, FamilyId, MatrixId, PageGeneration, PagedDataset, PagedQueryLimits,
    PrefixPostprocessing, ProjectionId, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm,
    SourceVerificationMode, TargetSetId, TermValue, VectorDtype, VectorSpaceId,
};

use gmeow_cli_core::{Reporter, report_diag};
use gmeow_errors::dag::DagNode;
use gmeow_errors::grade::{Belnap, BoundedLattice};
use gmeow_errors::model::Finding;
use gmeow_errors::{
    Diag, FindingCategory, Grade, ResultExt, Severity, Standpoint, define_diag_kind,
};
use gmeow_logic::annotation::{
    AnnotationContract, AnnotationFactRef, AnnotationQueryClass, AnnotationRequest,
    TupleAnnotationAlgebra,
};
use gmeow_logic::dispatch::{
    RelationAnnotationRequest, RelationQueryError, dispatch_query_annotated_with_relations,
};
use gmeow_logic::external_relation::{
    NeverCancelled, QueryRelationProviders, RelationAnnotationDimension, RelationOrderDirection,
    RelationOrdering, RelationProviderBudget, RelationProviderDescriptor,
    RelationProviderRegistration, RelationTuple, TableRelationProvider,
};
use gmeow_logic::provenance::ZWeightSemiring;
use gmeow_logic::purremb_relation::{
    PurrembBinding, PurrembRetrievalProvider, PurrembSelection, RetrievalPolicy, RetrievalScore,
    SpaceTaggedScore, VectorSpaceScopedAlgebra, purremb_descriptor, purremb_generation_iri,
};
use gmeow_logic::query_ir::{Budget, parse_query_program};
use gmeow_logic::result::PreservationClaim;
use gmeow_logic::runtime::{
    Checkpoint, FragmentDisposition, IncompleteCause, IntegrityFault, OperationOutcome,
    PagedCompositionMetrics, ReasoningSession, RebuildReason, SessionDelta, Suppression,
    UnsupportedFragment, edb_data_generation,
};
use gmeow_logic::seam::{WorldFactSnapshot, WorldSourceIdentity};
use gmeow_logic::store::WorldStore;
use gmeow_logic_compile::result_shape::ColumnKind;
use gmeow_pipeline::diagnostics_reader::{
    FindingIndex, WitnessIndex, WitnessRecord, explain_finding, explain_witness, minimal_fatal_cut,
    read_findings, read_invented_witnesses, render_shared_dag, verdict,
};

use crate::{BUNDLE_GTS, FragmentsFormat, NAMESPACE};

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
    let (modeled_defs, dataset) = match gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(&bytes)
        .and_then(|bundle| Ok((bundle.modeled_def_keys()?, bundle.dataset()?)))
    {
        Ok(parsed) => parsed,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.describe.modeled-defs",
                format!(
                    "cannot read the bundled RDF and JSON Schema for the describe surface: {e}"
                ),
            );
        }
    };
    let (text, status) =
        gmeow_docs::describe_dataset(term, dataset, resolved.as_deref(), format, &modeled_defs);
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

// ── candidate (propose/verify seam) ──────────────────────────────────────────

/// `gmeow candidate submit` — test a candidate `logic:` formula against a KB and, ONLY if the
/// isolated-world verdict CORROBORATES it (admissible), APPEND it to the append-only candidate
/// library. Delegates to the SHARED [`gmeow_pipeline::mcp::run_submit_candidate`] core (the same
/// path the MCP `submit_candidate` tool runs), so there is one implementation, not two. A refuted
/// or open candidate is not admitted (a non-zero exit), and `--dry-run` writes nothing.
#[allow(clippy::too_many_arguments)]
pub fn candidate_submit(
    reporter: &dyn Reporter,
    formula: &Path,
    kb: &Path,
    standpoint: &str,
    math_conjecture: Option<&str>,
    for_slice: Option<&str>,
    for_packet: Option<&str>,
    dry_run: bool,
    max_steps: Option<u64>,
    max_answers: Option<usize>,
) -> i32 {
    let formula_ttl = match std::fs::read_to_string(formula) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.read",
                format!("cannot read {}: {e}", formula.display()),
            );
        }
    };
    let kb_ttl = match std::fs::read_to_string(kb) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.read",
                format!("cannot read {}: {e}", kb.display()),
            );
        }
    };

    let out = match gmeow_pipeline::mcp::run_submit_candidate(
        &gmeow_pipeline::mcp::CandidateSubmitInput {
            formula_ttl: &formula_ttl,
            kb_ttl: &kb_ttl,
            standpoint,
            math_conjecture,
            for_slice,
            for_packet,
            dry_run,
            max_steps,
            max_answers,
        },
    ) {
        Ok(out) => out,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.failed",
                format!("candidate submission failed: {e}"),
            );
        }
    };

    println!("lifecycle {}", out.lifecycle);
    println!("information {}", out.information);
    println!("evaluation {}", out.evaluation);
    println!("completeness {}", out.completeness);
    println!("discharge {}", out.discharge);
    println!("admissible {}", out.admissible);
    println!("candidate {}", out.node_iri);

    // NOT admissible (refuted / open): the write was refused and nothing was appended. Report and
    // fail (exit 1), mirroring the MCP `ok:false` path.
    if let Some(reason) = &out.precondition_unmet {
        return fail(
            reporter,
            "gmeow-cli.candidate.not-admissible",
            format!("submitCandidate precondition unmet (candidate not admissible): {reason}"),
        );
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

/// `gmeow candidate withdraw` — withdraw a persisted candidate (P10 supersession). Delegates to
/// the SHARED [`gmeow_pipeline::mcp::run_withdraw_candidate`] core the MCP tool runs. An unknown
/// or already-withdrawn id is a hard error (a non-zero exit).
pub fn candidate_withdraw(
    reporter: &dyn Reporter,
    candidate_id: &str,
    reason: Option<&str>,
    dry_run: bool,
) -> i32 {
    let body = match gmeow_pipeline::mcp::run_withdraw_candidate(
        candidate_id,
        reason.unwrap_or(""),
        dry_run,
    ) {
        Ok(body) => body,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.withdraw-failed",
                format!("candidate withdrawal failed: {e}"),
            );
        }
    };
    render_candidate_json(reporter, &body, "gmeow-cli.candidate.withdraw")
}

/// `gmeow candidate list` — list admitted candidates with their disposition + provenance.
/// Delegates to the SHARED [`gmeow_pipeline::mcp::run_list_candidates`] core.
pub fn candidate_list(
    reporter: &dyn Reporter,
    slice: Option<&str>,
    disposition: Option<&str>,
) -> i32 {
    let body = match gmeow_pipeline::mcp::run_list_candidates(slice, disposition) {
        Ok(body) => body,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.list-failed",
                format!("candidate list failed: {e}"),
            );
        }
    };
    render_candidate_json(reporter, &body, "gmeow-cli.candidate.list")
}

/// Print a candidate tool's JSON response body verbatim, mapping its `ok` flag to the process
/// exit: an `ok:false` envelope (e.g. an unmet withdrawal precondition) is a hard failure.
fn render_candidate_json(reporter: &dyn Reporter, body: &str, code: &str) -> i32 {
    let value: serde_json::Value = match serde_json::from_str(body) {
        Ok(v) => v,
        Err(e) => return fail(reporter, code, format!("malformed tool response: {e}")),
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string())
    );
    if value.get("ok").and_then(serde_json::Value::as_bool) == Some(false) {
        let msg = value
            .get("error")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("candidate operation failed");
        return fail(reporter, code, msg.to_string());
    }
    0
}

// ── hybrid-query ─────────────────────────────────────────────────────────────

/// The isolated query world every `hybrid-query --facts` document is re-homed
/// into, mirroring `gmeow conjecture test`'s scenario-world isolation: the
/// caller's facts are ordinary asserted RDF, never mutated, and never joined
/// against any other world already resident in the process.
const HYBRID_QUERY_WORLD: &str = "https://blackcatinformatics.ca/gmeow/hybrid-query/world";

/// The semantic profile `hybrid-query` evaluates under: positive Horn Datalog,
/// the same default the `gmeow-dev logic query` developer surface uses.
const HYBRID_QUERY_PROFILE: &str = "PositiveHornProfile";

/// No caller-supplied asserted-RDF annotation source: every asserted fact
/// contributes the multiplicative identity, so an answer's combined annotation
/// is driven entirely by the provider's own ZWeight scores.
fn hybrid_query_unscored(_: AnnotationFactRef<'_>) -> Option<i64> {
    None
}

/// Stable per-site diagnostic code every `--candidates` parse failure lowers to
/// (the Diag substrate replaces a bare `String` error — Phase-6 honest invariant).
const CANDIDATES_DIAG_CODE: &str = "gmeow-cli.hybrid-query.candidates";

/// Parse one non-blank, non-comment `--candidates` line into a provider row.
///
/// The line format is `<arg1-iri> <arg2-iri> annotation order-key`,
/// whitespace-separated (tabs or spaces are both accepted, and repeated
/// whitespace is collapsed): `arg1-iri`/`arg2-iri` MUST be bracketed absolute
/// IRIs (`<https://example.org/x>`); `annotation` is a signed 64-bit ZWeight
/// integer; `order-key` is the provider's own lexical rank token for the
/// pushed-down total order (the final field, taken verbatim — it may not
/// itself contain whitespace).
fn parse_candidate_line(line: &str, line_no: usize) -> Result<RelationTuple<i64>, Diag> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    let [arg1, arg2, annotation, order_key] = fields.as_slice() else {
        return Err(error_diag(
            CANDIDATES_DIAG_CODE,
            format!(
                "line {line_no}: expected 4 whitespace-separated fields \
                 `<arg1-iri> <arg2-iri> annotation order-key`, got {} field(s)",
                fields.len()
            ),
        ));
    };
    let parse_iri = |field: &str| -> Result<String, Diag> {
        let trimmed = field.strip_prefix('<').and_then(|s| s.strip_suffix('>'));
        let Some(trimmed) = trimmed else {
            return Err(error_diag(
                CANDIDATES_DIAG_CODE,
                format!(
                    "line {line_no}: {field:?} must be a bracketed absolute IRI, \
                     e.g. <https://example.org/x>"
                ),
            ));
        };
        purrdf::iri::parse(trimmed).map_err(|e| {
            error_diag(
                CANDIDATES_DIAG_CODE,
                format!("line {line_no}: invalid IRI {trimmed:?}: {e}"),
            )
        })?;
        Ok(trimmed.to_owned())
    };
    let arg1 = parse_iri(arg1)?;
    let arg2 = parse_iri(arg2)?;
    let annotation: i64 = annotation.parse().map_err(|e| {
        error_diag(
            CANDIDATES_DIAG_CODE,
            format!("line {line_no}: invalid annotation integer {annotation:?}: {e}"),
        )
    })?;
    Ok(RelationTuple {
        arguments: vec![TermValue::iri(arg1), TermValue::iri(arg2)],
        annotation,
        order_key: (*order_key).to_owned(),
    })
}

/// Parse a whole `--candidates` file: one tuple per non-blank, non-`#`-comment
/// line (see [`parse_candidate_line`] for the line grammar).
fn parse_candidates_file(text: &str) -> Result<Vec<RelationTuple<i64>>, Diag> {
    let mut rows = Vec::new();
    for (offset, raw_line) in text.lines().enumerate() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        rows.push(parse_candidate_line(line, offset + 1)?);
    }
    Ok(rows)
}

/// Load an RDF facts file (Turtle or N-Triples, chosen by extension) and
/// re-home every triple into the isolated [`HYBRID_QUERY_WORLD`] — the
/// caller's asserted facts join against the provider relation inside one
/// scenario world, never against any other world.
fn load_hybrid_query_facts(reporter: &dyn Reporter, facts: &Path) -> Result<WorldStore, i32> {
    let suffix = facts
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let media = match suffix.as_str() {
        ".ttl" | ".turtle" => "text/turtle",
        ".nt" | ".ntriples" => "application/n-triples",
        _ => {
            return Err(fail(
                reporter,
                "gmeow-cli.hybrid-query.facts-format",
                format!(
                    "--facts {} must be Turtle (.ttl/.turtle) or N-Triples (.nt/.ntriples)",
                    facts.display()
                ),
            ));
        }
    };
    let bytes = read_bytes(reporter, facts)?;
    let parsed = purrdf::parse_dataset(&bytes, media, None).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.hybrid-query.facts-parse",
            format!("cannot parse {}: {e}", facts.display()),
        )
    })?;
    let world = RdfTerm::iri(HYBRID_QUERY_WORLD);
    let mut builder = RdfDatasetBuilder::new();
    for quad in parsed.owned_quads() {
        builder.push_owned_quad(
            &RdfQuad::new(quad.subject, quad.predicate, quad.object).in_graph(world.clone()),
        );
    }
    let dataset = builder.freeze().map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.hybrid-query.facts-rehome",
            format!(
                "cannot re-home {} into the query world: {e}",
                facts.display()
            ),
        )
    })?;
    let store = WorldStore::new();
    store.load_dataset(&dataset).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.hybrid-query.facts-load",
            format!("cannot load {} into the query world: {e}", facts.display()),
        )
    })?;
    Ok(store)
}

/// `gmeow hybrid-query` — register one external relation provider (a table of
/// candidate tuples the caller supplies, e.g. lexical or vector-similarity
/// hits) and drive a query-scoped, annotated Datalog query against it END TO
/// END on the shipped `gmeow` binary: load ordinary asserted RDF facts, parse
/// the query program, seal the provider registration + deterministic budget,
/// dispatch through [`dispatch_query_annotated_with_relations`], and print
/// both the resolved answer bindings and the query receipt (every
/// contributing provider's identity, artifact generation, and per-invocation
/// request/response evidence) — so this capability is observably exercised
/// from the consumer CLI, not only from `crates/logic`'s own test binary.
///
/// External relation tuples are DERIVED QUERY INPUTS, never asserted ontology
/// facts: `--candidates` is a plain line-oriented table, deliberately not RDF.
/// A provider failure or declared incompleteness is printed to stderr and
/// yields a non-zero exit — it never renders as an empty completed answer.
#[allow(clippy::too_many_arguments)]
pub fn hybrid_query(
    reporter: &dyn Reporter,
    facts: &Path,
    program: &Path,
    candidates: &Path,
    relation: &str,
    provider_iri: &str,
    model_iri: &str,
    artifact_generation: &str,
    per_call_limit: usize,
    max_calls: u64,
    max_rows: u64,
) -> i32 {
    let store = match load_hybrid_query_facts(reporter, facts) {
        Ok(store) => store,
        Err(code) => return code,
    };
    let source =
        match WorldFactSnapshot::from_world(&store, HYBRID_QUERY_WORLD, HYBRID_QUERY_PROFILE) {
            Ok(source) => source,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.hybrid-query.snapshot",
                    format!("cannot snapshot the query world: {e}"),
                );
            }
        };

    let program_src = match std::fs::read_to_string(program) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.io.read",
                format!("cannot read {}: {e}", program.display()),
            );
        }
    };
    let program = match parse_query_program(&program_src) {
        Ok(program) => program,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.hybrid-query.program",
                format!("cannot parse query program: {e}"),
            );
        }
    };

    let candidates_text = match std::fs::read_to_string(candidates) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.io.read",
                format!("cannot read {}: {e}", candidates.display()),
            );
        }
    };
    let rows = match parse_candidates_file(&candidates_text) {
        Ok(rows) => rows,
        Err(detail) => {
            return fail(
                reporter,
                CANDIDATES_DIAG_CODE,
                format!(
                    "cannot parse {}: {}",
                    candidates.display(),
                    detail.message()
                ),
            );
        }
    };

    let provider = TableRelationProvider::new(artifact_generation, rows);
    let algebra_iri = TupleAnnotationAlgebra::identity(&ZWeightSemiring).to_owned();
    let ordering = match RelationOrdering::new(
        format!("{relation}/order"),
        RelationOrderDirection::Ascending,
    ) {
        Ok(ordering) => ordering,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.hybrid-query.contract",
                format!("invalid provider ordering contract: {e}"),
            );
        }
    };
    let descriptor = match RelationProviderDescriptor::new(
        provider_iri,
        artifact_generation,
        model_iri,
        relation,
        vec![ColumnKind::Iri, ColumnKind::Iri],
        RelationAnnotationDimension::Similarity,
        algebra_iri,
        PreservationClaim::exact(),
        ordering,
    ) {
        Ok(descriptor) => descriptor,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.hybrid-query.contract",
                format!("invalid provider descriptor: {e}"),
            );
        }
    };
    let registration =
        match RelationProviderRegistration::new(descriptor, per_call_limit, &provider) {
            Ok(registration) => registration,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.hybrid-query.contract",
                    format!("invalid provider registration: {e}"),
                );
            }
        };
    let budget = match RelationProviderBudget::new(max_calls, max_rows) {
        Ok(budget) => budget,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.hybrid-query.contract",
                format!("invalid provider budget: {e}"),
            );
        }
    };
    // A named local, not a call-site temporary: `providers` below borrows this for
    // the whole query execution, and a temporary's scope would not outlive the
    // statement that constructs `providers`.
    let never_cancelled = NeverCancelled;
    let providers = match QueryRelationProviders::new(vec![registration], budget, &never_cancelled)
    {
        Ok(providers) => providers,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.hybrid-query.contract",
                format!("invalid provider set: {e}"),
            );
        }
    };

    let contract = AnnotationContract::exact();
    let request = RelationAnnotationRequest::new(
        AnnotationRequest::new(&ZWeightSemiring, &contract, hybrid_query_unscored),
        &providers,
    );
    let result = dispatch_query_annotated_with_relations(
        &source,
        HYBRID_QUERY_WORLD,
        &program,
        HYBRID_QUERY_PROFILE,
        &Budget::default(),
        request,
    );

    match result {
        Ok(result) => {
            for answer in &result.answer.answers {
                // `Binding` is a `BTreeMap`, so this is already deterministically
                // sorted by variable name.
                let rendered: Vec<String> = answer
                    .binding
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect();
                println!(
                    "answer {} annotation={}",
                    if rendered.is_empty() {
                        "(true)".to_owned()
                    } else {
                        rendered.join(", ")
                    },
                    answer.annotation
                );
            }
            println!("status {:?}", result.answer.status);
            println!(
                "receipt query-contract {}",
                result.receipt.query_contract_hash
            );
            println!(
                "receipt provider-manifest {}",
                result.receipt.provider_manifest_hash
            );
            println!("receipt engine {}", result.receipt.engine_descriptor_hash);
            println!("receipt hash {}", result.receipt.receipt_hash);
            for (provider_iri, generation) in &result.receipt.contributing_providers {
                println!("receipt contributing-provider {provider_iri} generation {generation}");
            }
            for invocation in &result.receipt.invocations {
                println!(
                    "receipt invocation relation={} provider={} model={} generation={} \
                     status={:?} request={} response-hash={} delivered={} admitted={} \
                     contributed={}",
                    invocation.relation_iri,
                    invocation.provider_iri,
                    invocation.model_iri,
                    invocation.artifact_generation,
                    invocation.status,
                    invocation.request_iri,
                    invocation.response_hash.as_deref().unwrap_or("-"),
                    invocation.delivered_rows,
                    invocation.admitted_rows,
                    invocation.contributed,
                );
            }
            0
        }
        Err(RelationQueryError::Contract(e)) => fail(
            reporter,
            "gmeow-cli.hybrid-query.contract",
            format!("provider/algebra contract mismatch: {e}"),
        ),
        Err(RelationQueryError::Query {
            diagnostic,
            receipt,
        }) => {
            emit_error(
                reporter,
                "gmeow-cli.hybrid-query.query-failed",
                format!(
                    "query evaluation failed: {diagnostic} (provider calls {}, admitted rows {})",
                    receipt.metrics.provider_calls, receipt.metrics.admitted_rows
                ),
            );
            1
        }
        Err(RelationQueryError::Provider { error, receipt }) => {
            emit_error(
                reporter,
                "gmeow-cli.hybrid-query.provider-failed",
                format!(
                    "external relation provider did not complete: {error} \
                     (provider calls {}, admitted rows {})",
                    receipt.metrics.provider_calls, receipt.metrics.admitted_rows
                ),
            );
            1
        }
    }
}

// ── hybrid-query: verified-PURREMB retrieval mode ─────────────────────────────

/// Stable diagnostic code for a PURREMB binding, selection, or verification
/// failure surfaced through the CLI.
const PURREMB_DIAG_CODE: &str = "gmeow-cli.hybrid-query.purremb";

/// Every declared input of the verified-PURREMB retrieval mode, borrowed for one
/// dispatch. Each field is a mandatory part of the query contract (the mode is
/// fully specified — no half-specified silent degradation); the binding validates
/// each against the opened artifact and fails closed on any disagreement.
pub struct PurrembHybridQuery<'a> {
    /// RDF facts re-homed into the isolated query world.
    pub facts: &'a Path,
    /// Datalog query program referencing the provider relation.
    pub program: &'a Path,
    /// The `.purremb` artifact to open and verify.
    pub purremb: &'a Path,
    /// The exact source pack the artifact is bound to.
    pub source: &'a Path,
    /// Provider relation IRI referenced by the program.
    pub relation: &'a str,
    /// Provider identity IRI.
    pub provider_iri: &'a str,
    /// Model/algorithm identity IRI.
    pub model_iri: &'a str,
    /// Base artifact-generation IRI; the pinned artifact root and the explicit
    /// selection (policy + source mode) are folded into the full generation IRI.
    pub generation_base: &'a str,
    /// Target-set identity (64 hex characters).
    pub target_set: &'a str,
    /// Family identity (64 hex characters).
    pub family: &'a str,
    /// Effective vector-space identity (64 hex characters).
    pub vector_space: &'a str,
    /// Stored-matrix identity (64 hex characters).
    pub matrix: &'a str,
    /// Effective-projection identity (64 hex characters); required only for the
    /// Matryoshka prefix-then-rerank policy.
    pub projection: Option<&'a str>,
    /// Declared distance metric.
    pub metric: &'a str,
    /// Effective (leading-prefix) dimension.
    pub effective_dimension: u32,
    /// Declared stored scalar type.
    pub dtype: &'a str,
    /// Prefix postprocessing for the effective space.
    pub postprocessing: &'a str,
    /// Selected retrieval branch.
    pub retrieval_policy: &'a str,
    /// Source-verification mode.
    pub source_mode: &'a str,
    /// Ordered-prefix row limit pushed into the provider on each call.
    pub per_call_limit: usize,
    /// Deterministic operation-wide provider call budget.
    pub max_calls: u64,
    /// Deterministic operation-wide provider row budget.
    pub max_rows: u64,
}

/// Decode exactly 32 hex bytes into a raw identity array, failing closed on a
/// wrong length or a non-hex character (a bad identity is a hard fail, never a
/// silent reinterpretation).
fn parse_identity_hex(label: &str, hex: &str) -> Result<[u8; 32], String> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(format!(
            "{label} identity must be 64 hex characters (32 bytes), got {}",
            hex.len()
        ));
    }
    let mut out = [0u8; 32];
    for (index, byte) in out.iter_mut().enumerate() {
        let start = index * 2;
        let pair = hex
            .get(start..start + 2)
            .ok_or_else(|| format!("{label} identity is truncated"))?;
        *byte = u8::from_str_radix(pair, 16)
            .map_err(|_| format!("{label} identity has a non-hex character"))?;
    }
    Ok(out)
}

/// Parse the declared distance metric, rejecting anything the retrieval scan has
/// no canonical scoring rule for.
fn parse_metric(value: &str) -> Result<DistanceMetric, String> {
    match value {
        "cosine" => Ok(DistanceMetric::Cosine),
        "negative-dot" => Ok(DistanceMetric::NegativeDot),
        "squared-euclidean" => Ok(DistanceMetric::SquaredEuclidean),
        other => Err(format!(
            "unknown metric '{other}' (expected cosine, negative-dot, or squared-euclidean)"
        )),
    }
}

/// Parse the declared stored scalar type.
fn parse_dtype(value: &str) -> Result<VectorDtype, String> {
    match value {
        "f32" => Ok(VectorDtype::F32),
        "f64" => Ok(VectorDtype::F64),
        other => Err(format!("unknown dtype '{other}' (expected f32 or f64)")),
    }
}

/// Parse the effective-space prefix postprocessing policy.
fn parse_postprocessing(value: &str) -> Result<PrefixPostprocessing, String> {
    match value {
        "none" => Ok(PrefixPostprocessing::None),
        "deterministic-l2" => Ok(PrefixPostprocessing::DeterministicL2),
        other => Err(format!(
            "unknown postprocessing '{other}' (expected none or deterministic-l2)"
        )),
    }
}

/// Parse the selected retrieval branch.
fn parse_retrieval_policy(value: &str) -> Result<RetrievalPolicy, String> {
    match value {
        "exact-full-space" => Ok(RetrievalPolicy::ExactFullSpace),
        "matryoshka-prefix-then-rerank" => Ok(RetrievalPolicy::MatryoshkaPrefixThenRerank),
        other => Err(format!(
            "unknown retrieval policy '{other}' (expected exact-full-space or \
             matryoshka-prefix-then-rerank)"
        )),
    }
}

/// Parse the source-verification mode.
fn parse_source_mode(value: &str) -> Result<SourceVerificationMode, String> {
    match value {
        "exact" => Ok(SourceVerificationMode::Exact),
        "certified" => Ok(SourceVerificationMode::Certified),
        other => Err(format!(
            "unknown source-verification mode '{other}' (expected exact or certified)"
        )),
    }
}

/// Lowercase hex of a 32-byte identity, for annotation/space rendering.
fn purremb_hex32(bytes: &[u8; 32]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(64);
    for byte in bytes {
        let _ = write!(out, "{byte:02x}");
    }
    out
}

/// Build the fully validated [`PurrembSelection`] from the declared hex/enum
/// inputs, failing closed on any malformed identity or unknown code.
fn purremb_selection(args: &PurrembHybridQuery<'_>) -> Result<PurrembSelection, String> {
    let target_set = TargetSetId::from_raw(parse_identity_hex("target-set", args.target_set)?);
    let family = FamilyId::from_raw(parse_identity_hex("family", args.family)?);
    let vector_space =
        VectorSpaceId::from_raw(parse_identity_hex("vector-space", args.vector_space)?);
    let matrix = MatrixId::from_raw(parse_identity_hex("matrix", args.matrix)?);
    let projection = match args.projection {
        None => None,
        Some(hex) => Some(ProjectionId::from_raw(parse_identity_hex(
            "projection",
            hex,
        )?)),
    };
    Ok(PurrembSelection {
        target_set,
        family,
        vector_space,
        matrix,
        projection,
        metric: parse_metric(args.metric)?,
        effective_dimension: args.effective_dimension,
        postprocessing: parse_postprocessing(args.postprocessing)?,
        dtype: parse_dtype(args.dtype)?,
        policy: parse_retrieval_policy(args.retrieval_policy)?,
    })
}

/// The asserted-fact annotation source for the space-tagged algebra: an ordinary
/// asserted RDF fact carries no vector-space score, so it maps to the algebra's
/// multiplicative identity (`None`) and stays neutral in the fold. Only the
/// PURREMB provider's retrieved tuples contribute a space-tagged distance.
fn purremb_unscored(_: AnnotationFactRef<'_>) -> Option<SpaceTaggedScore> {
    None
}

/// `gmeow hybrid-query --purremb ... --source ...` — the verified-PURREMB
/// retrieval mode. Opens and fully verifies a `.purremb` artifact against its
/// exact source pack (verify-once + source-pack certify), validates the declared
/// selection, registers a QUERY-SCOPED nearest-neighbour relation provider whose
/// retrieved rows are RDF 1.2 identities, and drives the same annotated Datalog
/// dispatch as the table mode — but under the space-tagged retrieval algebra
/// ([`VectorSpaceScopedAlgebra`], element [`SpaceTaggedScore`]) that carries the
/// metric distance in its own annotation dimension and refuses a cross-space
/// conjunction without a licensing correspondence.
///
/// Prints every resolved answer binding (with its space-tagged distance), the
/// standard query receipt (contributing provider identities + per-invocation
/// evidence), AND the full PURREMB retrieval receipt naming every contributing
/// PURREMB identity. A verification, selection, profile, or provider failure —
/// or a declared incompleteness — prints a diagnostic to stderr and returns a
/// NON-ZERO exit; it never renders as an empty completed answer.
pub fn hybrid_query_purremb(reporter: &dyn Reporter, args: &PurrembHybridQuery<'_>) -> i32 {
    let selection = match purremb_selection(args) {
        Ok(selection) => selection,
        Err(detail) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("invalid PURREMB selection: {detail}"),
            );
        }
    };
    let source_verification = match parse_source_mode(args.source_mode) {
        Ok(mode) => mode,
        Err(detail) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("invalid PURREMB selection: {detail}"),
            );
        }
    };
    let policy = selection.policy;

    // The caller owns these byte buffers for the whole dispatch: the binding
    // borrows them, and the provider borrows the binding, so both locals must
    // outlive `providers` and the dispatch call below.
    let artifact_bytes = match read_bytes(reporter, args.purremb) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    let source_bytes = match read_bytes(reporter, args.source) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };

    let binding = match PurrembBinding::open(
        &artifact_bytes,
        &source_bytes,
        selection,
        source_verification,
    ) {
        Ok(binding) => binding,
        Err(error) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("PURREMB artifact did not verify: {error}"),
            );
        }
    };

    // The generation IRI folds the pinned artifact root and the explicit
    // selection so a differing policy/source-mode is never cache-aliased.
    let generation_iri = purremb_generation_iri(
        args.generation_base,
        binding.artifact_root_hex(),
        policy,
        source_verification,
    );

    let algebra = VectorSpaceScopedAlgebra::with_cross_space_refusal(BTreeSet::new());
    let algebra_iri = TupleAnnotationAlgebra::identity(&algebra).to_owned();
    let ordering = match RelationOrdering::new(
        format!("{}/order", args.relation),
        RelationOrderDirection::Ascending,
    ) {
        Ok(ordering) => ordering,
        Err(e) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("invalid provider ordering contract: {e}"),
            );
        }
    };
    let descriptor = match purremb_descriptor(
        args.provider_iri,
        generation_iri,
        args.model_iri,
        args.relation,
        vec![ColumnKind::Iri, ColumnKind::Iri],
        RelationAnnotationDimension::Distance,
        algebra_iri,
        ordering,
    ) {
        Ok(descriptor) => descriptor,
        Err(e) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("invalid PURREMB provider descriptor: {e}"),
            );
        }
    };

    // Each computed retrieval score becomes a single-space score element in the
    // query algebra: the metric distance tagged with the effective vector space
    // it was computed in and the metric it was computed under.
    let annotate = Box::new(|score: RetrievalScore| {
        SpaceTaggedScore::single(
            score.distance,
            VectorSpaceId::from_raw(score.vector_space),
            score.metric_code,
        )
    });
    let provider = match PurrembRetrievalProvider::new(binding, descriptor, annotate) {
        Ok(provider) => provider,
        Err(e) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("PURREMB provider contract rejected: {e}"),
            );
        }
    };

    let registration = match RelationProviderRegistration::new(
        provider.descriptor().clone(),
        args.per_call_limit,
        &provider,
    ) {
        Ok(registration) => registration,
        Err(e) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("invalid PURREMB provider registration: {e}"),
            );
        }
    };
    let budget = match RelationProviderBudget::new(args.max_calls, args.max_rows) {
        Ok(budget) => budget,
        Err(e) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("invalid provider budget: {e}"),
            );
        }
    };
    let never_cancelled = NeverCancelled;
    let providers = match QueryRelationProviders::new(vec![registration], budget, &never_cancelled)
    {
        Ok(providers) => providers,
        Err(e) => {
            return fail(
                reporter,
                PURREMB_DIAG_CODE,
                format!("invalid provider set: {e}"),
            );
        }
    };

    // The ordinary asserted RDF facts + the query program, exactly as the table
    // mode loads them.
    let store = match load_hybrid_query_facts(reporter, args.facts) {
        Ok(store) => store,
        Err(code) => return code,
    };
    let source =
        match WorldFactSnapshot::from_world(&store, HYBRID_QUERY_WORLD, HYBRID_QUERY_PROFILE) {
            Ok(source) => source,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.hybrid-query.snapshot",
                    format!("cannot snapshot the query world: {e}"),
                );
            }
        };
    let program_src = match std::fs::read_to_string(args.program) {
        Ok(text) => text,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.io.read",
                format!("cannot read {}: {e}", args.program.display()),
            );
        }
    };
    let program = match parse_query_program(&program_src) {
        Ok(program) => program,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.hybrid-query.program",
                format!("cannot parse query program: {e}"),
            );
        }
    };

    // The space-tagged algebra makes `⊗` partial (cross-space refusal), so its
    // admission contract discloses that declared deviation, certified across
    // every structural query class the program could inhabit.
    let contract = algebra.annotation_contract([
        AnnotationQueryClass::PositiveAcyclic,
        AnnotationQueryClass::PositiveRecursive,
        AnnotationQueryClass::PositiveNaryAcyclic,
        AnnotationQueryClass::PositiveNaryRecursive,
        AnnotationQueryClass::StratifiedNaf,
        AnnotationQueryClass::WellFounded,
        AnnotationQueryClass::StableModel,
        AnnotationQueryClass::ExistentialChase,
    ]);
    let request = RelationAnnotationRequest::new(
        AnnotationRequest::new(&algebra, &contract, purremb_unscored),
        &providers,
    );
    let result = dispatch_query_annotated_with_relations(
        &source,
        HYBRID_QUERY_WORLD,
        &program,
        HYBRID_QUERY_PROFILE,
        &Budget::default(),
        request,
    );

    match result {
        Ok(result) => {
            for answer in &result.answer.answers {
                let rendered: Vec<String> = answer
                    .binding
                    .iter()
                    .map(|(k, v)| format!("{k}={v}"))
                    .collect();
                let annotation = &answer.annotation;
                let spaces: Vec<String> = annotation.spaces.iter().map(purremb_hex32).collect();
                println!(
                    "answer {} annotation-distance={} metric-code={} spaces={}",
                    if rendered.is_empty() {
                        "(true)".to_owned()
                    } else {
                        rendered.join(", ")
                    },
                    annotation.score.distance(),
                    annotation.metric_code,
                    if spaces.is_empty() {
                        "(none)".to_owned()
                    } else {
                        spaces.join(",")
                    },
                );
            }
            println!("status {:?}", result.answer.status);
            println!(
                "receipt query-contract {}",
                result.receipt.query_contract_hash
            );
            println!(
                "receipt provider-manifest {}",
                result.receipt.provider_manifest_hash
            );
            println!("receipt engine {}", result.receipt.engine_descriptor_hash);
            println!("receipt hash {}", result.receipt.receipt_hash);
            for (provider_iri, generation) in &result.receipt.contributing_providers {
                println!("receipt contributing-provider {provider_iri} generation {generation}");
            }
            for invocation in &result.receipt.invocations {
                println!(
                    "receipt invocation relation={} provider={} model={} generation={} \
                     status={:?} request={} response-hash={} delivered={} admitted={} \
                     contributed={}",
                    invocation.relation_iri,
                    invocation.provider_iri,
                    invocation.model_iri,
                    invocation.artifact_generation,
                    invocation.status,
                    invocation.request_iri,
                    invocation.response_hash.as_deref().unwrap_or("-"),
                    invocation.delivered_rows,
                    invocation.admitted_rows,
                    invocation.contributed,
                );
            }
            // The PURREMB retrieval receipt: every contributing PURREMB identity.
            let receipt = provider.retrieval_receipt();
            println!(
                "purremb-receipt artifact-root={} source-digest={} certified-rdf={} \
                 source-mode={} target-set={} matrix={} projection={} vector-space={} \
                 family={} metric={} metric-code={} dimension={} postprocessing={} \
                 policy={} recall={} loss={} index-guard={}",
                receipt.artifact_root,
                receipt.source_exact_digest,
                receipt.certified_rdf_digest,
                receipt.source_verification_mode,
                receipt.target_set,
                receipt.matrix,
                receipt.projection.as_deref().unwrap_or("-"),
                receipt.vector_space,
                receipt.family,
                receipt.metric_name,
                receipt.metric_code,
                receipt.effective_dimension,
                receipt.postprocessing,
                receipt.retrieval_policy,
                receipt
                    .recall
                    .map(|r| r.to_string())
                    .unwrap_or_else(|| "-".to_owned()),
                receipt.loss,
                receipt.index_guard.as_deref().unwrap_or("-"),
            );
            0
        }
        Err(RelationQueryError::Contract(e)) => fail(
            reporter,
            PURREMB_DIAG_CODE,
            format!("provider/algebra contract mismatch: {e}"),
        ),
        Err(RelationQueryError::Query {
            diagnostic,
            receipt,
        }) => {
            emit_error(
                reporter,
                "gmeow-cli.hybrid-query.query-failed",
                format!(
                    "query evaluation failed: {diagnostic} (provider calls {}, admitted rows {})",
                    receipt.metrics.provider_calls, receipt.metrics.admitted_rows
                ),
            );
            1
        }
        Err(RelationQueryError::Provider { error, receipt }) => {
            emit_error(
                reporter,
                "gmeow-cli.hybrid-query.provider-failed",
                format!(
                    "external relation provider did not complete: {error} \
                     (provider calls {}, admitted rows {})",
                    receipt.metrics.provider_calls, receipt.metrics.admitted_rows
                ),
            );
            1
        }
    }
}

// ── entails ──────────────────────────────────────────────────────────────────

/// Parse an RDF file into a dataset, inferring the syntax from its extension. A
/// `file://` base IRI is derived from the path so RDF/XML relative IRIs resolve.
/// Returns the failure exit code on read / parse error (a hard fail, never a
/// degraded empty graph).
fn parse_rdf_file(
    reporter: &dyn Reporter,
    path: &Path,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, i32> {
    let bytes = read_bytes(reporter, path)?;
    let media = match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => ext.to_lowercase(),
        None => {
            return Err(fail(
                reporter,
                "gmeow-cli.entails.unknown-syntax",
                format!(
                    "cannot infer RDF syntax for {} (no extension); expected one of \
                     .ttl/.nt/.nq/.rdf/.owl/.xml/.trig",
                    path.display()
                ),
            ));
        }
    };
    let base = std::path::absolute(path)
        .ok()
        .map(|abs| format!("file://{}", abs.display()));
    purrdf::parse_dataset(&bytes, &media, base.as_deref()).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.entails.parse",
            format!("cannot parse {} as {media}: {e}", path.display()),
        )
    })
}

/// `gmeow entails` — decide whether the premise graph entails the conclusion
/// (`A ⊨ C`) natively, by refutation over the DL consistency calculus
/// ([`gmeow_logic::entail::dl_entails`]). Prints a stable, greppable verdict:
/// `verdict entailed` / `verdict not-entailed` / `verdict gap` (plus `gap-shape` /
/// `gap-detail` for a gap). A malformed / unparsable input is a hard fail (exit 1);
/// an honest capability gap is a successful, decided answer (exit 0).
pub fn entails(reporter: &dyn Reporter, premise: &Path, conclusion: &Path) -> i32 {
    let premise_ds = match parse_rdf_file(reporter, premise) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let conclusion_ds = match parse_rdf_file(reporter, conclusion) {
        Ok(ds) => ds,
        Err(code) => return code,
    };

    let verdict = match gmeow_logic::entail::dl_entails(premise_ds.as_ref(), conclusion_ds.as_ref())
    {
        Ok(v) => v,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.entails.failed",
                format!("entailment check failed: {e}"),
            );
        }
    };

    println!("verdict {}", verdict.as_token());
    if let gmeow_logic::entail::EntailmentVerdict::Gap(gap) = &verdict {
        println!("gap-shape {}", gap.shape.as_token());
        println!("gap-detail {}", gap.detail);
    }
    0
}

// ── logic fragments (the shipped decidability-surface query) ──────────────────

/// The `logic:` grounding namespace the decidability manifest is authored in.
const LOGIC_FRAGMENTS_NS: &str = "https://blackcatinformatics.ca/logic/";

/// `rdf:type` and `rdfs:label` — the two external predicates the manifest query keys on.
const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// One decided construct family read off the shipped `logic:DecidedFragment`
/// manifest: its stable id (the individual's local name), the
/// `logic:RefutationPattern` local name it closes under, its human label, and its
/// technical `logic:fragmentCompletenessBound`.
struct DecidedFragmentRow {
    id: String,
    pattern: String,
    label: String,
    bound: String,
}

/// One retained withhold read off the shipped `logic:expressivenessBoundary`
/// records: its stable id (local name), its human label, and its technical
/// `logic:fragmentBoundaryReason`.
struct RetainedBoundaryRow {
    id: String,
    label: String,
    reason: String,
}

/// The decidability surface extracted from a graph source.
struct DecidabilitySurface {
    decided: Vec<DecidedFragmentRow>,
    boundaries: Vec<RetainedBoundaryRow>,
}

/// Load the graph source the `logic fragments` verb queries: the embedded
/// `gmeow.gts` bundle by default, or a `--bundle` override folded as a `.gts`
/// snapshot / parsed as RDF (chosen by file extension). Both routes yield a frozen
/// [`purrdf::RdfDataset`], never a degraded empty graph.
fn fragments_graph_source(
    reporter: &dyn Reporter,
    bundle: Option<&Path>,
) -> Result<Arc<purrdf::RdfDataset>, i32> {
    match bundle {
        None => purrdf::gts::flattened_dataset_from_bytes(BUNDLE_GTS).map_err(|e| {
            fail(
                reporter,
                "gmeow-cli.logic-fragments.fold-bundle",
                format!("cannot fold the embedded gmeow.gts bundle: {e}"),
            )
        }),
        Some(path) => {
            let is_gts = path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(|ext| ext.eq_ignore_ascii_case("gts"));
            if is_gts {
                let bytes = read_bytes(reporter, path)?;
                purrdf::gts::flattened_dataset_from_bytes(&bytes).map_err(|e| {
                    fail(
                        reporter,
                        "gmeow-cli.logic-fragments.fold-bundle",
                        format!("cannot fold {}: {e}", path.display()),
                    )
                })
            } else {
                parse_rdf_file(reporter, path)
            }
        }
    }
}

/// Extract the decidability surface from `dataset` by graph queries over the shipped
/// `logic:DecidedFragment` / `logic:RefutationPattern` / `logic:expressivenessBoundary`
/// manifest — the SAME projection the kernel's
/// `module_ttl_projects_the_kernel_registry` agreement test asserts is exactly the
/// Rust `decided_fragments()` / `retained_boundaries()` registry. Rows are returned
/// sorted by id (deterministic).
fn extract_decidability_surface(dataset: &purrdf::RdfDataset) -> DecidabilitySurface {
    let decided_class = format!("{LOGIC_FRAGMENTS_NS}DecidedFragment");
    let decides_under = format!("{LOGIC_FRAGMENTS_NS}decidesUnderPattern");
    let completeness_bound = format!("{LOGIC_FRAGMENTS_NS}fragmentCompletenessBound");
    let boundary_reason = format!("{LOGIC_FRAGMENTS_NS}fragmentBoundaryReason");

    let local = |iri: &str| iri.strip_prefix(LOGIC_FRAGMENTS_NS).map(str::to_owned);

    // A decided fragment is a subject typed logic:DecidedFragment carrying a deciding
    // pattern + completeness bound; a retained boundary is a subject carrying a
    // logic:fragmentBoundaryReason. Both are keyed by their local name; labels are
    // collected for every logic: subject and joined in at render time.
    let mut decided_ids: BTreeSet<String> = BTreeSet::new();
    let mut pattern_of: BTreeMap<String, String> = BTreeMap::new();
    let mut bound_of: BTreeMap<String, String> = BTreeMap::new();
    let mut reason_of: BTreeMap<String, String> = BTreeMap::new();
    let mut label_of: BTreeMap<String, String> = BTreeMap::new();

    for quad in dataset.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        let Some(subj) = local(subject) else {
            continue;
        };
        match quad.predicate.as_str() {
            RDF_TYPE_IRI => {
                if let RdfTerm::Iri(o) = &quad.object
                    && *o == decided_class
                {
                    decided_ids.insert(subj);
                }
            }
            RDFS_LABEL_IRI => {
                if let RdfTerm::Literal(l) = &quad.object {
                    label_of.insert(subj, l.lexical_form.clone());
                }
            }
            p if p == decides_under => {
                if let RdfTerm::Iri(o) = &quad.object
                    && let Some(pl) = local(o)
                {
                    pattern_of.insert(subj, pl);
                }
            }
            p if p == completeness_bound => {
                if let RdfTerm::Literal(l) = &quad.object {
                    bound_of.insert(subj, l.lexical_form.clone());
                }
            }
            p if p == boundary_reason => {
                if let RdfTerm::Literal(l) = &quad.object {
                    reason_of.insert(subj, l.lexical_form.clone());
                }
            }
            _ => {}
        }
    }

    let decided = decided_ids
        .into_iter()
        .map(|id| DecidedFragmentRow {
            pattern: pattern_of.get(&id).cloned().unwrap_or_default(),
            label: label_of.get(&id).cloned().unwrap_or_default(),
            bound: bound_of.get(&id).cloned().unwrap_or_default(),
            id,
        })
        .collect();
    let boundaries = reason_of
        .into_iter()
        .map(|(id, reason)| RetainedBoundaryRow {
            label: label_of.get(&id).cloned().unwrap_or_default(),
            id,
            reason,
        })
        .collect();
    DecidabilitySurface {
        decided,
        boundaries,
    }
}

/// Render the decidability surface as deterministic, greppable human text.
fn render_fragments_text(surface: &DecidabilitySurface) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "decided-fragments {}", surface.decided.len());
    for f in &surface.decided {
        let _ = writeln!(out, "fragment {}", f.id);
        let _ = writeln!(out, "  label {}", f.label);
        let _ = writeln!(out, "  pattern {}", f.pattern);
        let _ = writeln!(out, "  bound {}", f.bound);
    }
    let _ = writeln!(out, "retained-boundaries {}", surface.boundaries.len());
    for b in &surface.boundaries {
        let _ = writeln!(out, "boundary {}", b.id);
        let _ = writeln!(out, "  label {}", b.label);
        let _ = writeln!(out, "  reason {}", b.reason);
    }
    out
}

/// Render the decidability surface as pretty, deterministic JSON.
fn render_fragments_json(surface: &DecidabilitySurface) -> Result<String, serde_json::Error> {
    let decided: Vec<serde_json::Value> = surface
        .decided
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "label": f.label,
                "pattern": f.pattern,
                "bound": f.bound,
            })
        })
        .collect();
    let boundaries: Vec<serde_json::Value> = surface
        .boundaries
        .iter()
        .map(|b| {
            serde_json::json!({
                "id": b.id,
                "label": b.label,
                "reason": b.reason,
            })
        })
        .collect();
    let doc = serde_json::json!({
        "decided_fragments": decided,
        "retained_boundaries": boundaries,
    });
    serde_json::to_string_pretty(&doc)
}

/// `gmeow logic fragments` — query the shipped fragment-certified decidability
/// surface and list (1) the construct families the native refutation kernel
/// natively DECIDES, each with its `logic:RefutationPattern` and completeness bound,
/// and (2) their dual, the retained `logic:expressivenessBoundary` records the
/// kernel deliberately WITHHOLDS, with their technical reasons.
///
/// The surface is read by graph queries over a graph source — the embedded
/// `gmeow.gts` bundle by default, or a `--bundle` override (a `.gts` snapshot, or an
/// RDF file such as the `logic` slice `module.ttl`, chosen by extension) — dogfooding
/// the shipped manifest rather than re-authoring a static table. Output is
/// deterministic (sorted by id). A graph source carrying NO decidability manifest is
/// a hard fail (never a silent empty success): the manifest is materialized into the
/// bundle by `make sync`, so an empty read points the user at that.
pub fn logic_fragments(
    reporter: &dyn Reporter,
    bundle: Option<&Path>,
    format: FragmentsFormat,
) -> i32 {
    let dataset = match fragments_graph_source(reporter, bundle) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let surface = extract_decidability_surface(dataset.as_ref());
    if surface.decided.is_empty() && surface.boundaries.is_empty() {
        return fail(
            reporter,
            "gmeow-cli.logic-fragments.empty-surface",
            "the graph source carries no logic:DecidedFragment / logic:expressivenessBoundary \
             decidability manifest; the embedded bundle is materialized by `make sync`, or pass \
             --bundle pointing at a graph source that ships the manifest (e.g. the logic slice \
             module.ttl)",
        );
    }
    let rendered = match format {
        FragmentsFormat::Text => Ok(render_fragments_text(&surface)),
        FragmentsFormat::Json => render_fragments_json(&surface)
            .map(|mut s| {
                s.push('\n');
                s
            })
            .map_err(|e| format!("cannot render decidability surface as JSON: {e}")),
    };
    match rendered {
        Ok(text) => {
            print!("{text}");
            0
        }
        Err(msg) => fail(reporter, "gmeow-cli.logic-fragments.render", msg),
    }
}

// ── logic backward ───────────────────────────────────────────────────────────

/// `rdfs:subClassOf` — the covering-edge predicate `stage-goal-directed` filters
/// its reasoned closure on (`crates/pipeline/src/stages/goal_directed.rs`), read
/// here directly off a parsed source graph instead of a full pipeline reasoning
/// pass.
const RDFS_SUBCLASS_OF: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";

/// The TOLD `rdfs:subClassOf` covering edges `(sub IRI, super IRI)` in `dataset`
/// — an IRI-to-IRI triple on that predicate — deduplicated and sorted.
/// `crate::physical::unify::SortOrder::from_subclass_edges` (inside
/// `gmeow_logic`) computes its OWN reflexive-transitive closure over whatever
/// covering edges it is handed, so passing the told edges of a simple subsort
/// chain (e.g. `math:Integer ⊑ math:RationalNumber ⊑ math:RealNumber`) is
/// sufficient for the engine to accept `math:Integer ⊑ math:RealNumber` —
/// there is no need (and this command makes no attempt) to pre-compute a
/// reasoned closure.
fn collect_subclass_edges(dataset: &RdfDataset) -> Vec<(String, String)> {
    let mut edges: Vec<(String, String)> = dataset
        .owned_quads()
        .filter(|q| q.predicate == RDFS_SUBCLASS_OF)
        .filter_map(|q| match (&q.subject, &q.object) {
            (RdfTerm::Iri(s), RdfTerm::Iri(o)) => Some((s.clone(), o.clone())),
            _ => None,
        })
        .collect();
    edges.sort();
    edges.dedup();
    edges
}

/// Parse a Turtle file into a raw [`RdfDataset`] for `rdfs:subClassOf` edge
/// extraction — deliberately independent of the `logic:` compiler frontend
/// (which never surfaces a told `rdfs:subClassOf` triple as `LogicAxiom`/
/// `Formula` data an engine caller can read back), so this reads the file's own
/// triples directly.
fn parse_turtle_dataset(reporter: &dyn Reporter, path: &Path) -> Result<Arc<RdfDataset>, i32> {
    let bytes = read_bytes(reporter, path)?;
    purrdf::parse_dataset(&bytes, "text/turtle", None).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-backward.subsort-parse",
            format!("cannot parse {} as Turtle: {e}", path.display()),
        )
    })
}

/// `gmeow logic backward` — evaluate one or more authored `logic:ReasoningProgram`
/// cells through the native proof-carrying SLG-WFS backward engine
/// ([`gmeow_logic::goal_directed::evaluate_reasoning_programs`]) — the SAME
/// production path `stage-goal-directed` folds into `gmeow.gts`'s
/// `graph/goal-directed`. Never a reimplementation: this command lowers the
/// authored cell via `gmeow_logic_compile::frontend::parse_logic_path`,
/// collects `rdfs:subClassOf` covering edges straight off the parsed source
/// (see [`collect_subclass_edges`]), and hands both to the SAME engine entry
/// point the pipeline stage calls.
///
/// Hard-fails (exit 1, never a silent empty success) on: a missing
/// `--program-file`, an unparsable file (or one carrying error-grade parse
/// diagnostics), a file with zero `logic:ReasoningProgram` individuals, or a
/// `--program-iri` naming no program in the file.
pub fn logic_backward(
    reporter: &dyn Reporter,
    program_file: &Path,
    program_iri: Option<&str>,
    subsort_source: Option<&Path>,
) -> i32 {
    if !program_file.exists() {
        return fail(
            reporter,
            "gmeow-cli.logic-backward.missing-file",
            format!("--program-file {} does not exist", program_file.display()),
        );
    }
    let (program, diagnostics) =
        match gmeow_logic_compile::frontend::parse_logic_path(program_file, None) {
            Ok(parsed) => parsed,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.logic-backward.parse",
                    format!("cannot parse {}: {e}", program_file.display()),
                );
            }
        };
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == gmeow_logic_compile::frontend::Severity::Error)
        .map(|d| format!("{} {}: {}", d.severity.as_str(), d.code, d.message))
        .collect();
    if !errors.is_empty() {
        return fail(
            reporter,
            "gmeow-cli.logic-backward.malformed",
            format!(
                "{} carries {} error-grade parse diagnostic(s): {}",
                program_file.display(),
                errors.len(),
                errors.join("; ")
            ),
        );
    }
    if program.reasoning_programs.is_empty() {
        return fail(
            reporter,
            "gmeow-cli.logic-backward.no-programs",
            format!(
                "{} carries zero logic:ReasoningProgram individuals",
                program_file.display()
            ),
        );
    }

    let selected: Vec<gmeow_logic_compile::ir::ReasoningProgramIr> = match program_iri {
        None => program.reasoning_programs,
        Some(iri) => {
            let mut programs = program.reasoning_programs;
            let Some(pos) = programs.iter().position(|p| p.iri == iri) else {
                let known: Vec<&str> = programs.iter().map(|p| p.iri.as_str()).collect();
                return fail(
                    reporter,
                    "gmeow-cli.logic-backward.unknown-program",
                    format!(
                        "--program-iri {iri:?} names no logic:ReasoningProgram in {}; known: {}",
                        program_file.display(),
                        known.join(", ")
                    ),
                );
            };
            vec![programs.swap_remove(pos)]
        }
    };

    // Collect `rdfs:subClassOf` covering edges directly off the parsed source
    // graph(s) — never a hardcoded subsort tower (see `collect_subclass_edges`).
    let program_dataset = match parse_turtle_dataset(reporter, program_file) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let mut subsort_edges = collect_subclass_edges(&program_dataset);
    if let Some(source) = subsort_source {
        let extra_dataset = match parse_turtle_dataset(reporter, source) {
            Ok(ds) => ds,
            Err(code) => return code,
        };
        subsort_edges.extend(collect_subclass_edges(&extra_dataset));
        subsort_edges.sort();
        subsort_edges.dedup();
    }

    // The SAME production entry point `stage-goal-directed` calls — no second
    // engine, no reimplementation.
    let evals =
        match gmeow_logic::goal_directed::evaluate_reasoning_programs(&selected, &subsort_edges) {
            Ok(evals) => evals,
            Err(e) => {
                return fail(
                    reporter,
                    "gmeow-cli.logic-backward.evaluate",
                    format!("backward evaluation failed: {e}"),
                );
            }
        };

    // Deterministic ordering throughout: `evaluate_reasoning_programs` already
    // sorts programs by name, answers by `(atom, bindings, derivation_iri)`, and
    // verdicts by `(atom, verdict)` (G12).
    for eval in &evals {
        println!("program {}", eval.name);
        println!("  description: {}", eval.description);
        println!("  goal: {}", eval.goal);
        println!("  status: {}", eval.status);
        for ans in &eval.answers {
            println!(
                "  answer atom={} proof-checked={} derivation={}",
                ans.atom, ans.proof_checks, ans.derivation_iri
            );
            for (var, surface) in &ans.bindings {
                println!("    binding {var} = {surface}");
            }
        }
        for v in &eval.verdicts {
            println!("  verdict atom={} verdict={}", v.atom, v.verdict);
        }
    }
    0
}

// ── logic session (the operational ReasoningSession consumer) ──────────────────

/// The single named-graph world IRI the session EDB/additions are re-homed into.
/// The incremental maintenance fragment is single-world, so every fact the façade
/// folds lives here (an ordinary Turtle EDB, whose triples land in the default
/// graph, is deterministically re-homed into this world).
const SESSION_WORLD_DEFAULT: &str = "https://blackcatinformatics.ca/gmeow/logic/session/world";

/// The graph IRI the printed `SessionIdentity` N-Quads are scoped to.
const SESSION_IDENTITY_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/logic/session/identity";

/// Load an authored `logic:`-vocabulary program Turtle into a canonical
/// [`gmeow_logic_compile::ir::LogicProgram`] via the SAME production frontend
/// `gmeow logic backward` uses. Hard-fails (never a silent empty program) on a
/// missing file, an unparsable file, or one carrying error-grade parse diagnostics.
fn session_load_program(
    reporter: &dyn Reporter,
    program_file: &Path,
) -> Result<gmeow_logic_compile::ir::LogicProgram, i32> {
    if !program_file.exists() {
        return Err(fail(
            reporter,
            "gmeow-cli.logic-session.missing-program",
            format!("--program {} does not exist", program_file.display()),
        ));
    }
    let (program, diagnostics) =
        gmeow_logic_compile::frontend::parse_logic_path(program_file, None).map_err(|e| {
            fail(
                reporter,
                "gmeow-cli.logic-session.program-parse",
                format!("cannot parse {}: {e}", program_file.display()),
            )
        })?;
    let errors: Vec<String> = diagnostics
        .iter()
        .filter(|d| d.severity == gmeow_logic_compile::frontend::Severity::Error)
        .map(|d| format!("{} {}: {}", d.severity.as_str(), d.code, d.message))
        .collect();
    if !errors.is_empty() {
        return Err(fail(
            reporter,
            "gmeow-cli.logic-session.program-malformed",
            format!(
                "{} carries {} error-grade parse diagnostic(s): {}",
                program_file.display(),
                errors.len(),
                errors.join("; ")
            ),
        ));
    }
    if program.rules.is_empty() {
        return Err(fail(
            reporter,
            "gmeow-cli.logic-session.no-rules",
            format!(
                "{} carries zero logic:Rule individuals",
                program_file.display()
            ),
        ));
    }
    Ok(program)
}

/// Load an RDF file (Turtle/N-Triples/N-Quads/TriG, syntax inferred from the
/// extension) and re-home every quad into the single named-graph `world`, so the
/// façade sees exactly one world regardless of the source serialization's graph
/// structure. A hard fail (never a degraded empty world) on read/parse/freeze error.
fn session_load_world_dataset(
    reporter: &dyn Reporter,
    path: &Path,
    world: &str,
) -> Result<RdfDataset, i32> {
    let parsed = parse_rdf_file(reporter, path)?;
    let graph = RdfTerm::iri(world.to_owned());
    let mut builder = RdfDatasetBuilder::new();
    for quad in parsed.owned_quads() {
        builder.push_owned_quad(
            &RdfQuad::new(quad.subject, quad.predicate, quad.object).in_graph(graph.clone()),
        );
    }
    let dataset = builder.freeze().map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-session.edb-rehome",
            format!(
                "cannot re-home {} into the session world: {e}",
                path.display()
            ),
        )
    })?;
    // The façade takes an owned `RdfDataset` for deltas/suppressions; the builder
    // hands back a fresh single-reference `Arc`, so unwrap it into the owned world.
    Arc::try_unwrap(dataset).map_err(|_| {
        fail(
            reporter,
            "gmeow-cli.logic-session.edb-own",
            "internal: a freshly-frozen session dataset was unexpectedly shared",
        )
    })
}

/// An owned, empty single-world dataset — the additions slot of a suppression-only
/// committed delta (`checkpoint --retract` without `--apply`). Never a degraded
/// success: a freeze/own failure is a hard CLI fail.
fn session_empty_world_dataset(reporter: &dyn Reporter) -> Result<RdfDataset, i32> {
    let dataset = RdfDatasetBuilder::new().freeze().map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-session.empty-additions",
            format!("cannot build an empty additions dataset: {e}"),
        )
    })?;
    Arc::try_unwrap(dataset).map_err(|_| {
        fail(
            reporter,
            "gmeow-cli.logic-session.empty-own",
            "internal: a freshly-frozen empty session dataset was unexpectedly shared",
        )
    })
}

/// Open a session over `edb`/`program` under the default contract and the exact
/// annotation semiring, mapping a façade open error to a hard CLI fail.
fn session_open(
    reporter: &dyn Reporter,
    edb: &RdfDataset,
    program: &gmeow_logic_compile::ir::LogicProgram,
) -> Result<ReasoningSession, i32> {
    let contract = gmeow_logic_compile::ir::ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    ReasoningSession::open(edb, program, &contract, &annotation).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-session.open",
            format!("cannot open reasoning session: {e}"),
        )
    })
}

/// The source-contract identity the CLI's demand-paged world source is named under,
/// distinct from the resident authorized-EDB generation contract. It records that the
/// data-generation was paged in through the in-memory `PagedDataset` provider.
const SESSION_PAGED_SOURCE_CONTRACT: &str =
    "https://blackcatinformatics.ca/logic/session/paged-in-memory-provider-v1";

/// Compose a session over a demand-paged world-source that pages the SAME authorized
/// world facts back in through a `PagedDataset`, exercising the `open_paged` composition
/// surface on the production CLI.
///
/// `page_size` chunks the EDB quads into pages of that many quads (each a single-world
/// frozen `RdfDataset`) so the page-fault accounting is non-trivial; `None` or a size
/// `>=` the quad count pages the whole world as one page. The paged
/// [`WorldSourceIdentity`] is DETERMINISTIC — its generation reuses the canonical EDB
/// content-address ([`edb_data_generation`]) under the paged source contract — so the
/// printed identity is stable and reproducible. Any paged freeze/seal/open error is a
/// hard CLI fail (never a silent resident fallback).
fn session_open_paged(
    reporter: &dyn Reporter,
    edb: &RdfDataset,
    program: &gmeow_logic_compile::ir::LogicProgram,
    world: &str,
    page_size: Option<usize>,
) -> Result<ReasoningSession, i32> {
    let contract = gmeow_logic_compile::ir::ReasoningContract::new();
    let annotation = AnnotationContract::exact();

    // Deterministic, content-addressed paged identity: the generation is the canonical
    // EDB content-address (identical to the resident data-generation), named under the
    // paged provider's own source contract.
    let identity: WorldSourceIdentity = edb_data_generation(edb, SESSION_PAGED_SOURCE_CONTRACT)
        .map_err(|e| {
            fail(
                reporter,
                "gmeow-cli.logic-session.paged-generation",
                format!("cannot content-address the paged EDB generation: {e}"),
            )
        })?;

    // Page the authorized world quads (already re-homed into `world`) into one or more
    // single-world frozen pages. Chunking makes the page-fault accounting non-trivial;
    // the chunks are quad-disjoint, so the paged seal admits them.
    let quads: Vec<RdfQuad> = edb.owned_quads().collect();
    let total = quads.len();
    let chunk = match page_size {
        Some(size) if size > 0 && size < total => size,
        // `None`, `0`, or a size that would not split the world → one whole-world page.
        _ => total.max(1),
    };
    let mut pages: Vec<Arc<RdfDataset>> = Vec::new();
    for window in quads.chunks(chunk) {
        let mut builder = RdfDatasetBuilder::new();
        for quad in window {
            builder.push_owned_quad(quad);
        }
        let page = builder.freeze().map_err(|e| {
            fail(
                reporter,
                "gmeow-cli.logic-session.paged-page-freeze",
                format!("cannot freeze a paged world page: {e}"),
            )
        })?;
        pages.push(page);
    }
    // An empty EDB pages one empty world page, so the provider always has ≥1 page.
    if pages.is_empty() {
        let page = RdfDatasetBuilder::new().freeze().map_err(|e| {
            fail(
                reporter,
                "gmeow-cli.logic-session.paged-page-freeze",
                format!("cannot freeze the empty paged world page: {e}"),
            )
        })?;
        pages.push(page);
    }

    let provider = Arc::new(InMemoryPageProvider::with_generation(
        pages,
        PageGeneration(0),
    ));
    let paged = PagedDataset::from_provider(provider).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-session.paged-seal",
            format!("cannot seal the paged world source: {e}"),
        )
    })?;

    ReasoningSession::open_paged(
        &paged,
        identity,
        world,
        program,
        &contract,
        &annotation,
        PagedQueryLimits::UNBOUNDED,
    )
    .map_err(|outcome| {
        fail(
            reporter,
            "gmeow-cli.logic-session.open-paged",
            format!("cannot open paged reasoning session: {outcome:?}"),
        )
    })
}

/// Print the page-fault / source-access accounting of a paged-composed session, so the
/// `open_paged` composition is observable/dogfooded on the CLI. Every line is a stable,
/// greppable `paged-*` key.
fn print_paged_metrics(metrics: &PagedCompositionMetrics) {
    let source = &metrics.source;
    let backend = &metrics.backend;
    println!("paged-source-delivered-quads {}", source.delivered_quads());
    println!("paged-source-primary-quads {}", source.primary_quads);
    println!("paged-source-pattern-probes {}", source.pattern_probes);
    println!("paged-backend-generation {}", backend.generation.0);
    println!(
        "paged-backend-requested-pages {}",
        backend.requested_pages.len()
    );
    println!("paged-backend-consumed-pages {}", backend.consumed_pages);
    println!("paged-backend-consumed-bytes {}", backend.consumed_bytes);
}

/// A stable, greppable label for a program's fragment disposition.
fn render_fragment_disposition(disposition: &FragmentDisposition) -> String {
    match disposition {
        FragmentDisposition::Incremental => "incremental".to_owned(),
        FragmentDisposition::RequiresFullRebuild(reason) => {
            format!("requires-full-rebuild {}", render_rebuild_reason(reason))
        }
        FragmentDisposition::Unsupported(kind) => {
            format!("unsupported {}", render_unsupported_fragment(kind))
        }
        // `FragmentDisposition` is `#[non_exhaustive]`: a future tier still prints.
        _ => "unknown".to_owned(),
    }
}

/// A stable label for a rebuild reason.
fn render_rebuild_reason(reason: &RebuildReason) -> &'static str {
    match reason {
        RebuildReason::BoundedRetractionUnsupported => "bounded-retraction-unsupported",
        RebuildReason::AdditionsOutsideIncrementalFragment => {
            "additions-outside-incremental-fragment"
        }
        RebuildReason::ContractOrEngineDriftSinceCheckpoint => {
            "contract-or-engine-drift-since-checkpoint"
        }
        // `RebuildReason` is `#[non_exhaustive]`: a future reason still prints.
        _ => "unknown",
    }
}

/// A stable label for an unsupported-fragment kind.
fn render_unsupported_fragment(kind: &UnsupportedFragment) -> &'static str {
    match kind {
        UnsupportedFragment::NonStratifiable => "non-stratifiable",
        UnsupportedFragment::Cut => "cut",
        UnsupportedFragment::Arithmetic => "arithmetic",
        UnsupportedFragment::NonBinaryAtom => "non-binary-atom",
        UnsupportedFragment::Floundering => "floundering",
        UnsupportedFragment::NonTerminatingExistential => "non-terminating-existential",
        UnsupportedFragment::NonTerminatingArithmetic => "non-terminating-arithmetic",
        UnsupportedFragment::ClauseBodyTooWide => "clause-body-too-wide",
        // `UnsupportedFragment` is `#[non_exhaustive]`: a future kind still prints.
        _ => "unknown",
    }
}

/// A stable label for an incomplete-operation cause.
fn render_incomplete_cause(cause: &IncompleteCause) -> &'static str {
    match cause {
        IncompleteCause::StepBudget => "step-budget",
        IncompleteCause::Cancelled => "cancelled",
        IncompleteCause::Deadline => "deadline",
        IncompleteCause::SourceBudgetExhausted => "source-budget-exhausted",
        _ => "unknown",
    }
}

/// A stable label for an integrity fault.
fn render_integrity_fault(fault: &IntegrityFault) -> String {
    match fault {
        IntegrityFault::PreconditionMismatch {
            expected_state_hash,
            delta_base,
        } => format!(
            "PreconditionMismatch expected-state-hash={expected_state_hash} delta-anchor={delta_base}"
        ),
        IntegrityFault::IdentityMismatch { expected, found } => {
            format!("IdentityMismatch expected={expected} found={found}")
        }
        IntegrityFault::CorruptCheckpoint {
            expected_address,
            computed_address,
        } => format!(
            "CorruptCheckpoint stored-address={expected_address} computed-address={computed_address}"
        ),
        IntegrityFault::IllegalSignedTransaction { detail } => {
            format!("IllegalSignedTransaction {detail}")
        }
        IntegrityFault::CheckpointReplayDivergence {
            expected_head,
            replayed_head,
        } => format!(
            "CheckpointReplayDivergence expected-head={expected_head} replayed-head={replayed_head}"
        ),
        _ => "unknown".to_owned(),
    }
}

/// Print a typed [`OperationOutcome`] to stdout (the product stream) in a stable,
/// greppable, diffable shape, and return the CLI exit code. A typed refusal/route
/// (`RequiresFullRebuild`, `UnsupportedFragment`, `Incomplete`, `Invalid`) is a
/// DECIDED answer — the observable proof the façade classifies rather than silently
/// approximates — so it exits `0`, like an honest `entails` gap. Only a genuine
/// `EngineFailure` is a hard fail (exit `1`).
fn render_outcome(reporter: &dyn Reporter, outcome: &OperationOutcome) -> i32 {
    match outcome {
        OperationOutcome::Applied {
            run,
            new_state_hash,
        } => {
            println!("outcome Applied");
            println!("  new-head {new_state_hash}");
            println!("  consumed-steps {}", run.consumed_steps);
            println!("  derived-count {}", run.derived_count);
            println!("  signed-changes {}", run.changes.len());
            println!("  derivations {}", run.derivations.len());
            0
        }
        OperationOutcome::RequiresFullRebuild { reason } => {
            println!("outcome RequiresFullRebuild");
            println!("  reason {}", render_rebuild_reason(reason));
            0
        }
        OperationOutcome::UnsupportedFragment { kind } => {
            println!("outcome UnsupportedFragment");
            println!("  kind {}", render_unsupported_fragment(kind));
            0
        }
        OperationOutcome::Incomplete { status, cause } => {
            println!("outcome Incomplete");
            println!("  status {status}");
            println!("  cause {}", render_incomplete_cause(cause));
            0
        }
        OperationOutcome::Invalid { fault } => {
            println!("outcome Invalid");
            println!("  fault {}", render_integrity_fault(fault));
            0
        }
        OperationOutcome::EngineFailure { diagnostic } => fail(
            reporter,
            "gmeow-cli.logic-session.engine-failure",
            format!("reasoning-session engine failure: {}", diagnostic.message()),
        ),
        // `OperationOutcome` is `#[non_exhaustive]`: an unknown future variant is a
        // hard fail (never a silent success).
        _ => fail(
            reporter,
            "gmeow-cli.logic-session.unknown-outcome",
            "reasoning-session returned an unrecognized outcome variant",
        ),
    }
}

/// Print the maintained derived closure with per-fact proof provenance, in a
/// deterministic, diffable order. This is the production reader that makes the
/// incrementally-maintained answer set (and its annotations) observable.
fn render_facts_and_provenance(session: &ReasoningSession) {
    let facts = session.facts();
    println!("facts {}", facts.len());
    for row in &facts.rows {
        let args: Vec<String> = row
            .args
            .iter()
            .map(gmeow_logic::provenance::term_display)
            .collect();
        println!("fact {} {}", row.predicate, args.join(" "));
    }

    // The maintained closure's per-fact derivation witnesses are the reasoning
    // OUTPUT (firing rule + premises + signed Z-weight). Sort a copy on the full
    // tuple so the reader is byte-diffable across runs.
    let mut derivations = session.provenance().to_vec();
    derivations.sort_by(|a, b| {
        (&a.subject, &a.predicate, &a.object, &a.rule_iri, a.weight).cmp(&(
            &b.subject,
            &b.predicate,
            &b.object,
            &b.rule_iri,
            b.weight,
        ))
    });
    println!("provenance {}", derivations.len());
    for prov in &derivations {
        println!(
            "derivation subject={} predicate={} object={} rule={} weight={} proof-height={}",
            prov.subject,
            prov.predicate,
            prov.object,
            prov.rule_iri,
            prov.weight,
            prov.proof_height
        );
        let mut premises = prov.premises.clone();
        premises.sort();
        for (subject, predicate, object) in &premises {
            println!("  premise {subject} {predicate} {object}");
        }
    }
}

/// `gmeow logic session open` — open a session and print its seven-axis identity,
/// genesis head, and fragment disposition.
pub fn logic_session_open(
    reporter: &dyn Reporter,
    edb: &Path,
    program: &Path,
    world: Option<&str>,
    paged: bool,
    page_size: Option<usize>,
) -> i32 {
    let world = world.unwrap_or(SESSION_WORLD_DEFAULT);
    let program = match session_load_program(reporter, program) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let edb = match session_load_world_dataset(reporter, edb, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    // `--page-size` implies the paged composition path.
    let paged = paged || page_size.is_some();
    let session = if paged {
        match session_open_paged(reporter, &edb, &program, world, page_size) {
            Ok(s) => s,
            Err(code) => return code,
        }
    } else {
        match session_open(reporter, &edb, &program) {
            Ok(s) => s,
            Err(code) => return code,
        }
    };
    print!("{}", session.identity().to_nquads(SESSION_IDENTITY_GRAPH));
    println!("genesis-head {}", session.head());
    println!(
        "fragment-disposition {}",
        render_fragment_disposition(session.fragment_disposition())
    );
    if let Some(metrics) = session.paged_metrics() {
        print_paged_metrics(metrics);
    }
    0
}

/// `gmeow logic session apply` — build a content-addressed delta anchored on the
/// session's own data-generation and current head, apply it, and print the typed
/// outcome plus the advanced head.
pub fn logic_session_apply(
    reporter: &dyn Reporter,
    edb: &Path,
    program: &Path,
    additions: &Path,
    retract: Option<&Path>,
    max_steps: Option<u64>,
) -> i32 {
    let world = SESSION_WORLD_DEFAULT;
    let program = match session_load_program(reporter, program) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let edb = match session_load_world_dataset(reporter, edb, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let additions = match session_load_world_dataset(reporter, additions, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let mut retirements = Vec::new();
    if let Some(retract) = retract {
        let row = match session_load_world_dataset(reporter, retract, world) {
            Ok(ds) => ds,
            Err(code) => return code,
        };
        retirements.push(Suppression::new(row));
    }

    let mut session = match session_open(reporter, &edb, &program) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let base_commit = session.identity().data_generation.clone();
    let expected_head = session.head().to_owned();
    let delta = match SessionDelta::new(
        base_commit,
        expected_head,
        additions,
        retirements,
        max_steps,
    ) {
        Ok(delta) => delta,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.logic-session.delta",
                format!("cannot build session delta: {e}"),
            );
        }
    };
    let outcome = session.apply(&delta);
    println!("delta-identity {}", delta.delta_identity);
    let code = render_outcome(reporter, &outcome);
    println!("head {}", session.head());
    code
}

/// `gmeow logic session facts` — open (optionally applying a delta first) and READ
/// BACK the maintained derived closure with proof provenance. The anti-DARK reader.
pub fn logic_session_facts(
    reporter: &dyn Reporter,
    edb: &Path,
    program: &Path,
    apply: Option<&Path>,
    retract: Option<&Path>,
    paged: bool,
    page_size: Option<usize>,
) -> i32 {
    let world = SESSION_WORLD_DEFAULT;
    let program = match session_load_program(reporter, program) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let edb = match session_load_world_dataset(reporter, edb, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    // `--page-size` implies the paged composition path; the maintained closure read
    // back is identical to the resident open.
    let paged = paged || page_size.is_some();
    let mut session = if paged {
        match session_open_paged(reporter, &edb, &program, world, page_size) {
            Ok(s) => s,
            Err(code) => return code,
        }
    } else {
        match session_open(reporter, &edb, &program) {
            Ok(s) => s,
            Err(code) => return code,
        }
    };

    // Apply a committed delta before reading the closure back whenever either
    // additions (`--apply`) or suppressions (`--retract`) are supplied. The retirement
    // rows are re-homed into the session world and wrapped in `Suppression::new`
    // exactly as `logic_session_checkpoint` / `logic_session_apply` do (the identical
    // suppression-building path — NOT a second one), so a NON-EMPTY suppression is
    // folded into the applied delta and the read-back closure + per-fact proof heights
    // reflect the retraction (a surviving fact's min-proof-height RISES when its
    // shortest proof is retired).
    if apply.is_some() || retract.is_some() {
        let additions = match apply {
            Some(apply) => match session_load_world_dataset(reporter, apply, world) {
                Ok(ds) => ds,
                Err(code) => return code,
            },
            None => match session_empty_world_dataset(reporter) {
                Ok(ds) => ds,
                Err(code) => return code,
            },
        };
        let mut retirements = Vec::new();
        if let Some(retract) = retract {
            let row = match session_load_world_dataset(reporter, retract, world) {
                Ok(ds) => ds,
                Err(code) => return code,
            };
            retirements.push(Suppression::new(row));
        }
        let base_commit = session.identity().data_generation.clone();
        let expected_head = session.head().to_owned();
        let delta =
            match SessionDelta::new(base_commit, expected_head, additions, retirements, None) {
                Ok(delta) => delta,
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.logic-session.delta",
                        format!("cannot build session delta: {e}"),
                    );
                }
            };
        let outcome = session.apply(&delta);
        // Surface the apply classification, then STOP before reading a
        // non-advanced closure back if the engine genuinely failed.
        let code = render_outcome(reporter, &outcome);
        if code != 0 {
            return code;
        }
    }

    println!("head {}", session.head());
    render_facts_and_provenance(&session);
    if let Some(metrics) = session.paged_metrics() {
        print_paged_metrics(metrics);
    }
    0
}

/// `gmeow logic session checkpoint` — open (optionally applying a delta first), mint
/// a content-addressed checkpoint, and write it to disk as JSON.
pub fn logic_session_checkpoint(
    reporter: &dyn Reporter,
    edb: &Path,
    program: &Path,
    apply: Option<&Path>,
    retract: Option<&Path>,
    out: &Path,
) -> i32 {
    let world = SESSION_WORLD_DEFAULT;
    let program = match session_load_program(reporter, program) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let edb = match session_load_world_dataset(reporter, edb, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let mut session = match session_open(reporter, &edb, &program) {
        Ok(s) => s,
        Err(code) => return code,
    };

    // Apply a committed delta before checkpointing whenever either additions
    // (`--apply`) or suppressions (`--retract`) are supplied. The retirement rows are
    // re-homed into the session world exactly as the additions are (the identical path
    // `logic_session_apply` uses), so the checkpoint persists — and `restore` replays —
    // a delta carrying a NON-EMPTY suppression.
    if apply.is_some() || retract.is_some() {
        let additions = match apply {
            Some(apply) => match session_load_world_dataset(reporter, apply, world) {
                Ok(ds) => ds,
                Err(code) => return code,
            },
            None => match session_empty_world_dataset(reporter) {
                Ok(ds) => ds,
                Err(code) => return code,
            },
        };
        let mut retirements = Vec::new();
        if let Some(retract) = retract {
            let row = match session_load_world_dataset(reporter, retract, world) {
                Ok(ds) => ds,
                Err(code) => return code,
            };
            retirements.push(Suppression::new(row));
        }
        let base_commit = session.identity().data_generation.clone();
        let expected_head = session.head().to_owned();
        let delta =
            match SessionDelta::new(base_commit, expected_head, additions, retirements, None) {
                Ok(delta) => delta,
                Err(e) => {
                    return fail(
                        reporter,
                        "gmeow-cli.logic-session.delta",
                        format!("cannot build session delta: {e}"),
                    );
                }
            };
        let code = render_outcome(reporter, &session.apply(&delta));
        if code != 0 {
            return code;
        }
    }

    let checkpoint = session.checkpoint();
    let json = match serde_json::to_string_pretty(&checkpoint) {
        Ok(json) => json,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.logic-session.checkpoint-serialize",
                format!("cannot serialize checkpoint: {e}"),
            );
        }
    };
    if let Err(e) = std::fs::write(out, format!("{json}\n")) {
        return fail(
            reporter,
            "gmeow-cli.logic-session.checkpoint-write",
            format!("cannot write checkpoint to {}: {e}", out.display()),
        );
    }
    println!("checkpoint-written {}", out.display());
    println!("content-address {}", checkpoint.content_address);
    println!("journal-head {}", checkpoint.journal_head);
    println!("edb-generation {}", checkpoint.edb_generation);
    0
}

/// Load a checkpoint from disk EXACTLY as stored — via serde, NOT `Checkpoint::new`
/// (which would recompute `content_address` and hide tampering). The stored
/// `content_address` survives the round-trip, so `Checkpoint::verify` detects a
/// tampered field.
fn session_load_checkpoint(reporter: &dyn Reporter, path: &Path) -> Result<Checkpoint, i32> {
    let bytes = read_bytes(reporter, path)?;
    serde_json::from_slice::<Checkpoint>(&bytes).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-session.checkpoint-parse",
            format!("cannot parse checkpoint {}: {e}", path.display()),
        )
    })
}

/// `gmeow logic session restore` — load a checkpoint and restore by
/// re-materialization, printing the typed outcome (including the identity-gated /
/// tamper-detecting rejections).
pub fn logic_session_restore(
    reporter: &dyn Reporter,
    input: &Path,
    edb: &Path,
    program: &Path,
) -> i32 {
    let world = SESSION_WORLD_DEFAULT;
    let checkpoint = match session_load_checkpoint(reporter, input) {
        Ok(cp) => cp,
        Err(code) => return code,
    };
    let program = match session_load_program(reporter, program) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let edb = match session_load_world_dataset(reporter, edb, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let contract = gmeow_logic_compile::ir::ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    match ReasoningSession::restore(&checkpoint, &edb, &program, &contract, &annotation) {
        Ok(session) => {
            println!("outcome Restored");
            println!("  head {}", session.head());
            println!(
                "  fragment-disposition {}",
                render_fragment_disposition(session.fragment_disposition())
            );
            0
        }
        Err(outcome) => render_outcome(reporter, &outcome),
    }
}

/// `gmeow logic session restart` — restart from a checkpoint and resume at its
/// durable journal head. With `--reapply`, re-submit an already-committed delta
/// (anchored on the STALE genesis head) to demonstrate the structural double-apply
/// refusal surviving a persist→restore boundary.
pub fn logic_session_restart(
    reporter: &dyn Reporter,
    input: &Path,
    edb: &Path,
    program: &Path,
    reapply: Option<&Path>,
) -> i32 {
    let world = SESSION_WORLD_DEFAULT;
    let checkpoint = match session_load_checkpoint(reporter, input) {
        Ok(cp) => cp,
        Err(code) => return code,
    };
    let program = match session_load_program(reporter, program) {
        Ok(p) => p,
        Err(code) => return code,
    };
    let edb = match session_load_world_dataset(reporter, edb, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let contract = gmeow_logic_compile::ir::ReasoningContract::new();
    let annotation = AnnotationContract::exact();
    let mut session =
        match ReasoningSession::restart(&checkpoint, &edb, &program, &contract, &annotation) {
            Ok(session) => session,
            Err(outcome) => return render_outcome(reporter, &outcome),
        };
    println!("outcome Restarted");
    println!("  head {}", session.head());

    let Some(reapply) = reapply else {
        return 0;
    };
    let additions = match session_load_world_dataset(reporter, reapply, world) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    // Anchor the re-submitted delta on the GENESIS head (the session identity's
    // descriptor hash) — the precondition the delta carried when it was first
    // committed BEFORE the checkpoint. The restarted head is the durable
    // post-commit `journal_head`, so this stale-anchored re-submission fails the
    // transition precondition → `Invalid{PreconditionMismatch}` (the no-double-apply
    // guard surviving a real persist→restore boundary).
    let base_commit = session.identity().data_generation.clone();
    let genesis_head = session.identity().descriptor_hash.clone();
    let delta = match SessionDelta::new(base_commit, genesis_head, additions, Vec::new(), None) {
        Ok(delta) => delta,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.logic-session.delta",
                format!("cannot build session delta: {e}"),
            );
        }
    };
    println!("reapply-delta-identity {}", delta.delta_identity);
    render_outcome(reporter, &session.apply(&delta))
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
/// Fold the GTS snapshot bytes into the graph-preserving dataset the diagnostics
/// readers project. Shared by the finding index and the invented-witness index so a
/// single `explain` reads both off one dataset.
fn diagnostics_dataset(bytes: &[u8]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let graph = purrdf::gts::read_all_segments(bytes).map_err(|e| {
        Diag::of_kind(crate::error::SourceReadFailed {
            detail: format!("cannot read GTS segments: {e}"),
        })
    })?;
    purrdf::gts::dataset_from_gts_graph(&graph).map_err(|e| {
        Diag::of_kind(crate::error::RdfPipelineFailed {
            detail: format!("cannot fold GTS dataset: {e}"),
        })
    })
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

/// The bounded proof height of a witness derivation tree — its min-proof-height over
/// the invented-null sub-forest: a leaf null (no frontier binding that is itself a
/// null) is height 1; a null is `1 + max(child heights)`.
fn witness_height(node: &DagNode<String, WitnessRecord>) -> u32 {
    1 + node.children.iter().map(witness_height).max().unwrap_or(0)
}

/// Render a reconstructed invented-null derivation tree, printing a shared antecedent
/// IN FULL the first time and as a `↑ see <iri>` back-reference on every subsequent
/// visit (mirroring [`render_shared_dag`] for the witness plane).
fn render_witness_dag(root: &DagNode<String, WitnessRecord>) -> String {
    fn render_node(
        node: &DagNode<String, WitnessRecord>,
        visited: &mut std::collections::BTreeSet<String>,
        out: &mut String,
    ) {
        let indent = "  ".repeat(node.depth as usize + 1);
        if visited.contains(&node.key) {
            out.push_str(&format!("{indent}↑ see {}\n", node.key));
            return;
        }
        visited.insert(node.key.clone());
        out.push_str(&format!(
            "{indent}{} [via {}] ordinal {}\n",
            node.key, node.payload.rule_iri, node.payload.ordinal
        ));
        for child in &node.children {
            render_node(child, visited, out);
        }
    }
    let mut out = String::new();
    let mut visited = std::collections::BTreeSet::new();
    render_node(root, &mut visited, &mut out);
    out
}

/// Render the explanation of a chase-invented null (Skolem witness): its firing rule,
/// existential ordinal, frontier binding(s), and bounded proof height, plus the
/// derivation tree re-descended over the SHARED [`gmeow_errors::dag::walk`] engine.
fn render_witness(
    witnesses: &WitnessIndex,
    target: &str,
    record: &WitnessRecord,
) -> Result<String, Diag> {
    let mut out = String::new();
    out.push_str(&format!("invented witness {target}\n"));
    out.push_str(&format!("  firing rule  {}\n", record.rule_iri));
    out.push_str(&format!("  ordinal      {}\n", record.ordinal));
    out.push_str(&format!("  predicate    {}\n", record.predicate));
    let frontier = record.frontier.join(", ");
    out.push_str(&format!(
        "  frontier     {}\n",
        if frontier.is_empty() {
            "(none)"
        } else {
            &frontier
        }
    ));
    let dag = explain_witness(witnesses, target).map_err(|e| {
        Diag::of_kind(ExplainWalkFailed {
            target: target.to_owned(),
            detail: e.to_string(),
        })
    })?;
    out.push_str(&format!(
        "  proof height {} (bounded min-proof-height over the invented-null sub-derivation)\n",
        witness_height(&dag)
    ));
    out.push_str("witness derivation DAG:\n");
    out.push_str(&render_witness_dag(&dag));
    Ok(out)
}

/// Render the full explanation of an `explain` target — the production rendering
/// path `explain` prints. A finding fingerprint IRI walks its provenance DAG; an
/// anchor IRI resolves the cluster and walks each member; a chase-invented null
/// (skolem IRI) decomposes its Skolem recipe. Finding/anchor always append the
/// substrate algebra. An unknown/malformed target is a hard [`Diag`] fail — never
/// an empty DAG returned as success.
fn render_explanation(
    index: &FindingIndex,
    witnesses: &WitnessIndex,
    target: &str,
) -> Result<String, Diag> {
    // A chase-invented null resolves to the invented-witness plane; a skolem IRI with
    // no record falls through to the finding/anchor dispatch and its hard fail below.
    if let Some(record) = witnesses.get(target) {
        return render_witness(witnesses, target, record);
    }
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
    let dataset = match diagnostics_dataset(&bytes) {
        Ok(d) => d,
        Err(msg) => {
            return fail(
                reporter,
                "gmeow-cli.explain.read-diagnostics",
                msg.to_string(),
            );
        }
    };
    let index = match read_findings(&dataset).ctx("cannot read graph/diagnostics") {
        Ok(i) => i,
        Err(msg) => {
            return fail(
                reporter,
                "gmeow-cli.explain.read-diagnostics",
                msg.to_string(),
            );
        }
    };
    let witnesses = match read_invented_witnesses(&dataset).ctx("cannot read invented witnesses") {
        Ok(w) => w,
        Err(msg) => {
            return fail(
                reporter,
                "gmeow-cli.explain.read-diagnostics",
                msg.to_string(),
            );
        }
    };
    match render_explanation(&index, &witnesses, &target_iri) {
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

/// `gmeow slice lint` — the checkout-free tier-domination gate over an external
/// slice directory scored against the embedded `gmeow.gts` bundle. PASS (exit
/// `0`) iff the measured roll-up tier meets the effective bar: the higher-rank
/// of the slice's OWN declared `gmeow:sliceQualityTier` claim and any explicit
/// `--min-tier`; `1` when below the bar; `2` on an operational hard fail
/// (unscorable dir, unknown `--min-tier`, unreadable declared claim, or unknown
/// `--format`). Advisories are always emitted (graded `Error`/`Warning` relative
/// to the bar) but never gate — see [`gmeow_slice_quality::lint_report`].
pub fn slice_lint(
    reporter: &dyn Reporter,
    dir: &Path,
    min_tier: Option<&str>,
    format: &str,
) -> i32 {
    let report = match gmeow_slice_quality::score_external_slice_bytes(BUNDLE_GTS, dir) {
        Ok(r) => r,
        Err(e) => {
            return fail_code(
                reporter,
                "gmeow-cli.slice.lint.score",
                format!("cannot score {}: {e}", dir.display()),
                2,
            );
        }
    };
    let required = match min_tier {
        None => None,
        Some(name) => match gmeow_slice_quality::resolve_min_tier(&report.standard, name) {
            Ok(t) => Some(t.clone()),
            Err(e) => {
                return fail_code(
                    reporter,
                    "gmeow-cli.slice.lint.unknown-tier",
                    format!("{e}"),
                    2,
                );
            }
        },
    };
    let declared = match gmeow_slice_quality::declared_quality_tier(dir, &report.standard) {
        Ok(t) => t,
        Err(e) => {
            return fail_code(
                reporter,
                "gmeow-cli.slice.lint.declared-tier",
                format!("cannot read declared tier for {}: {e}", dir.display()),
                2,
            );
        }
    };
    let outcome = gmeow_slice_quality::lint_report(&report, declared.as_ref(), required.as_ref());
    let rendered = match format {
        "human" => Ok(outcome.render_text(&report)),
        "json" => gmeow_errors::render::to_json(&outcome.findings),
        "sarif" => gmeow_errors::render::to_sarif(&outcome.findings),
        other => {
            return fail_code(
                reporter,
                "gmeow-cli.slice.lint.unknown-format",
                format!("unknown --format {other:?}: expected human, json, or sarif"),
                2,
            );
        }
    };
    match rendered {
        Ok(text) => {
            print!("{text}");
            if outcome.passed { 0 } else { 1 }
        }
        Err(e) => fail_code(
            reporter,
            "gmeow-cli.slice.lint.render",
            format!("cannot render slice-lint report: {e}"),
            2,
        ),
    }
}

// ── slice brief ──────────────────────────────────────────────────────────────

/// `gmeow slice brief` — render a slice's `gmeow:AuthoringPacket`(s) in one of two
/// explicit modes (exactly one source, both/neither is a hard error):
///
/// * a slice `dir` → LIVE re-assembly over the slice's OWN sources (module.ttl,
///   mappings/, i18n/), gated by SHACL per-term conformance against the repo shape
///   union (needs a checkout with `generated/shapes/`) — see [`slice_brief_live`].
/// * `--from-bundle <slice>` → serve the PRE-ASSEMBLED packet(s) straight from the
///   embedded gmeow.gts bundle, checkout-free — see [`slice_brief_from_bundle`].
///
/// The live path's per-term exemplar tiers come from the SINGLE canonical library
/// tiering [`gmeow_slice_brief::exemplar_tiers`] — the same function the `slice_brief`
/// pipeline stage uses — so an in-repo slice's live CLI brief and its committed
/// `generated/briefs/authoring-packets.nt` projection tier terms identically. The
/// bundle path runs the SAME [`gmeow_pipeline::mcp::extract_authoring_packets`] core the
/// MCP `slice_brief` tool serves. A `--batch` out of range is a typed hard failure
/// through [`fail`] (a non-zero exit) on both paths, never a panic or an empty packet.
pub fn slice_brief(
    reporter: &dyn Reporter,
    dir: Option<&Path>,
    from_bundle: Option<&str>,
    axis: Option<&str>,
    batch: Option<u32>,
    format: &str,
) -> i32 {
    // Exactly one source: LIVE re-assembly from a slice `dir`, or the pre-assembled
    // packet served `--from-bundle`. Both/neither is a hard error (explicit selection,
    // no silent default).
    match (dir, from_bundle) {
        (Some(_), Some(_)) => fail(
            reporter,
            "gmeow-cli.slice.brief.ambiguous-source",
            "pass EITHER a slice directory OR --from-bundle <slice>, not both".to_string(),
        ),
        (None, None) => fail(
            reporter,
            "gmeow-cli.slice.brief.missing-source",
            "pass a slice directory (live re-assembly) or --from-bundle <slice> (bundle serve)"
                .to_string(),
        ),
        (None, Some(slice)) => slice_brief_from_bundle(reporter, slice, axis, batch, format),
        (Some(dir), None) => slice_brief_live(reporter, dir, axis, batch, format),
    }
}

/// `gmeow slice brief --from-bundle <slice>` — serve the pre-assembled authoring
/// packet(s) for a slice straight from the embedded gmeow.gts bundle, via the SAME
/// [`gmeow_pipeline::mcp::extract_authoring_packets`] core the MCP `slice_brief` tool
/// runs (one implementation, not two). Checkout-free: no repo root, no SHACL shape union.
fn slice_brief_from_bundle(
    reporter: &dyn Reporter,
    slice: &str,
    axis: Option<&str>,
    batch: Option<u32>,
    format: &str,
) -> i32 {
    let out = match gmeow_pipeline::mcp::slice_brief_from_bundle(
        BUNDLE_GTS,
        slice,
        axis,
        batch.map(u64::from),
    ) {
        Ok(v) => v,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.brief.bundle",
                format!("cannot serve packet for slice {slice:?}: {e}"),
            );
        }
    };
    let rendered = match format {
        "json" => serde_json::to_string_pretty(&out).unwrap_or_else(|_| out.to_string()),
        "turtle" => out
            .get("turtle")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        "human" => render_bundle_brief_human(&out),
        other => {
            return fail(
                reporter,
                "gmeow-cli.slice.brief.unknown-format",
                format!("unknown --format {other:?}: expected human, json, or turtle"),
            );
        }
    };
    println!("{rendered}");
    0
}

/// A compact human summary of a bundle-served authoring brief: per-packet identity,
/// term/exemplar counts, coverage margins, and the covered-term IRIs (whose full
/// definitions resolve via `gmeow lookup`).
fn render_bundle_brief_human(out: &serde_json::Value) -> String {
    // Write directly into the buffer (infallible into a `String`) rather than allocating a fresh
    // `format!` string per line/term.
    use std::fmt::Write as _;
    let mut s = String::new();
    let slice = out.get("slice").and_then(|v| v.as_str()).unwrap_or("?");
    let count = out
        .get("packet_count")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let _ = writeln!(s, "slice {slice} — {count} packet(s)");
    if let Some(packets) = out.get("packets").and_then(|v| v.as_array()) {
        for p in packets {
            let iri = p.get("packet_iri").and_then(|v| v.as_str()).unwrap_or("?");
            let terms = p.get("term_count").and_then(|v| v.as_i64()).unwrap_or(0);
            let axis = p.get("axis").and_then(|v| v.as_str()).unwrap_or("?");
            let batch = p.get("batch").and_then(|v| v.as_i64()).unwrap_or(0);
            let grounding = p
                .get("grounding")
                .and_then(|v| v.as_array())
                .map_or(0, Vec::len);
            let _ = writeln!(
                s,
                "\n{iri}\n  axis {axis}, batch {batch}, {terms} term(s), {grounding} grounding cell(s)"
            );
            if let Some(covers) = p.get("covers_terms").and_then(|v| v.as_array()) {
                for t in covers {
                    if let Some(t) = t.as_str() {
                        let _ = writeln!(s, "  - {t}");
                    }
                }
            }
        }
    }
    s
}

/// `gmeow slice brief <dir>` — the live, checkout-anchored re-assembly path.
fn slice_brief_live(
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

/// `gmeow slice projection-ceilings` — surface the committed projection-vocabulary
/// ratchet (the guarded registry + the per-(slice, vocabulary) ceilings) straight from
/// the embedded `gmeow.gts` bundle, dogfooding Principle 17 from the shippable
/// deliverable. This is the COMMITMENTS view: the resident individuals, not the live
/// measured residue (which needs a repo checkout to scan — that stays on `gmeow-dev`).
pub fn slice_projection_ceilings(reporter: &dyn Reporter, format: &str) -> i32 {
    let floors = match gmeow_slice_quality::ceilings_from_gts(BUNDLE_GTS) {
        Ok(f) => f,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.projection-ceilings.load",
                format!("cannot load projection ceilings from bundle: {e}"),
            );
        }
    };
    let mut vocabs = floors.vocabularies;
    vocabs.sort_by(|a, b| a.prefix.cmp(&b.prefix));
    let mut ceilings = floors.ceilings;
    ceilings.sort_by(|a, b| {
        (a.slice.as_str(), a.vocab_prefix.as_str())
            .cmp(&(b.slice.as_str(), b.vocab_prefix.as_str()))
    });
    match format {
        "human" => {
            println!("Guarded projection vocabularies ({}):", vocabs.len());
            for v in &vocabs {
                println!(
                    "  {:<10} owner={:<66} {:<24} default-ceiling {}",
                    v.prefix,
                    v.owner,
                    v.count_kind.as_local(),
                    v.default_ceiling
                );
            }
            println!("\nCommitted projection ceilings ({}):", ceilings.len());
            for c in &ceilings {
                println!(
                    "  {:<70} {:<10} ceiling {}",
                    c.slice, c.vocab_prefix, c.count
                );
            }
            0
        }
        "tsv" => {
            for v in &vocabs {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    v.prefix,
                    v.namespaces.join(","),
                    v.count_kind.as_local(),
                    v.default_ceiling,
                    v.owner
                );
            }
            for c in &ceilings {
                println!("{}\t{}\t{}", c.slice, c.vocab_prefix, c.count);
            }
            0
        }
        other => fail(
            reporter,
            "gmeow-cli.slice.projection-ceilings.unknown-format",
            format!("unknown --format {other:?}: expected human or tsv"),
        ),
    }
}

// ── docs ─────────────────────────────────────────────────────────────────────────

/// `gmeow docs matrix` — resolve the per-format consumer-need matrix by QUERYING the
/// meta-level distribution-catalog named graph shipped inside the embedded
/// `gmeow.gts` bundle (AC2), dogfooding the distribution-catalog ontology content
/// rather than re-deriving a static table. Prints a deterministic table (slug |
/// family | consumers | media-type | dropped-capabilities) to stdout.
pub fn docs_matrix(reporter: &dyn Reporter) -> i32 {
    let rows = match gmeow_pipeline::docs_distribution::read_distribution_matrix(BUNDLE_GTS) {
        Ok(rows) => rows,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.docs.matrix.read",
                format!("cannot read the distribution catalog matrix from the bundle: {e}"),
            );
        }
    };
    println!(
        "{:<10} {:<14} {:<40} {:<24} dropped-capabilities",
        "slug", "family", "consumers", "media-type"
    );
    for row in &rows {
        println!(
            "{:<10} {:<14} {:<40} {:<24} {}",
            row.slug,
            row.family,
            row.consumers.join(","),
            row.media_type,
            if row.dropped_capabilities.is_empty() {
                "-".to_string()
            } else {
                row.dropped_capabilities.join(",")
            }
        );
    }
    0
}

/// `gmeow docs verify [--dir <path>] [--format <slug>]` — verify a materialized
/// documentation distribution's blake3 content digests against its DCAT manifest
/// (`<dir>/manifest/docs-manifest.ttl`). Exit `0` iff every verdict is `ok`; a
/// single digest mismatch — or a hard read/parse failure — is a failure, with the
/// mismatches reported as diagnostics on stderr.
pub fn docs_verify(reporter: &dyn Reporter, dir: &Path, format: Option<&str>) -> i32 {
    let verdicts = match gmeow_pipeline::docs_distribution::verify_docs_distribution(dir, format) {
        Ok(v) => v,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.docs.verify.read",
                format!(
                    "cannot verify the docs distribution under {}: {e}",
                    dir.display()
                ),
            );
        }
    };
    let mut all_ok = true;
    for verdict in &verdicts {
        if verdict.ok {
            println!("ok        {}  {}", verdict.slug, verdict.declared);
        } else {
            all_ok = false;
            println!(
                "MISMATCH  {}  declared={} actual={}",
                verdict.slug, verdict.declared, verdict.actual
            );
        }
    }
    if all_ok {
        return 0;
    }
    let mismatched: Vec<&str> = verdicts
        .iter()
        .filter(|v| !v.ok)
        .map(|v| v.slug.as_str())
        .collect();
    fail(
        reporter,
        "gmeow-cli.docs.verify.mismatch",
        format!(
            "blake3 digest mismatch for distribution(s): {}",
            mismatched.join(", ")
        ),
    )
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
        let dataset = diagnostics_dataset(BUNDLE_GTS).expect("fold shipped bundle diagnostics");
        let index = read_findings(&dataset).expect("read diagnostics from shipped bundle");
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
        let witnesses = WitnessIndex::default();
        let text =
            render_explanation(&index, &witnesses, &real_iri).expect("render a real finding");
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
            render_explanation(&index, &witnesses, "not-a-real-iri").is_err(),
            "an unknown target is a hard fail"
        );
        assert_ne!(
            explain(reporter.as_ref(), "not-a-real-iri".to_owned(), None),
            0,
            "an unknown target exits non-zero"
        );
    }

    #[test]
    fn explain_decomposes_an_invented_witness_and_hard_fails_unknown_skolem() {
        use std::collections::BTreeMap;

        // A two-level invented-null derivation: `outer` is invented on a frontier
        // that is ITSELF the invented null `inner` — the recursive descent edge.
        let outer = "https://blackcatinformatics.ca/gmeow/skolem/outer";
        let inner = "https://blackcatinformatics.ca/gmeow/skolem/inner";
        let mut map: BTreeMap<String, WitnessRecord> = BTreeMap::new();
        map.insert(
            inner.to_owned(),
            WitnessRecord {
                witness: inner.to_owned(),
                rule_iri: "https://blackcatinformatics.ca/gmeow/rule/inner".to_owned(),
                ordinal: 0,
                predicate: "https://blackcatinformatics.ca/logic/demonstratesChaseWitness"
                    .to_owned(),
                frontier: vec!["https://example.org/seed".to_owned()],
            },
        );
        map.insert(
            outer.to_owned(),
            WitnessRecord {
                witness: outer.to_owned(),
                rule_iri: "https://blackcatinformatics.ca/gmeow/rule/outer".to_owned(),
                ordinal: 1,
                predicate: "https://blackcatinformatics.ca/logic/demonstratesChaseWitness"
                    .to_owned(),
                frontier: vec![inner.to_owned()],
            },
        );
        let witnesses = WitnessIndex { witnesses: map };
        let findings = FindingIndex::default();

        // The witness branch decomposes the recipe: rule, ordinal, frontier binding,
        // and the bounded proof height over the invented-null sub-derivation (2, since
        // `outer` descends into `inner`).
        let text =
            render_explanation(&findings, &witnesses, outer).expect("explain the invented witness");
        assert!(
            text.contains("invented witness"),
            "labels the witness: {text}"
        );
        assert!(
            text.contains("rule/outer"),
            "prints the firing rule: {text}"
        );
        assert!(
            text.contains("ordinal      1"),
            "prints the ordinal: {text}"
        );
        assert!(text.contains(inner), "prints the frontier binding: {text}");
        assert!(
            text.contains("proof height 2"),
            "bounded proof height over the sub-derivation: {text}"
        );
        assert!(
            text.contains("witness derivation DAG:"),
            "renders the derivation tree: {text}"
        );

        // A skolem IRI with NO record falls through to the finding/anchor dispatch and
        // its hard fail — never an empty derivation returned as success (AC2).
        assert!(
            render_explanation(
                &findings,
                &witnesses,
                "https://blackcatinformatics.ca/gmeow/skolem/missing"
            )
            .is_err(),
            "an unknown skolem IRI is a hard fail"
        );
    }

    #[test]
    fn explain_decomposes_a_chase_invented_null_in_the_shipped_bundle() {
        // AC2 on the PRODUCTION surface: the shipped gmeow.gts carries chase-invented
        // nulls; `gmeow explain <skolem-iri>` decomposes one over the real bundle,
        // reading its skolem IRI FROM the bundle (never hand-built).
        let dataset = diagnostics_dataset(BUNDLE_GTS).expect("fold shipped bundle diagnostics");
        let witnesses = read_invented_witnesses(&dataset).expect("read shipped invented witnesses");
        assert!(
            !witnesses.is_empty(),
            "the shipped bundle carries NO chase-invented null — explain(witness) has \
             nothing to decompose on the production surface"
        );
        let iri = witnesses
            .witnesses
            .keys()
            .next()
            .expect("a shipped invented null")
            .clone();
        let findings = read_findings(&dataset).expect("read shipped findings");

        let text = render_explanation(&findings, &witnesses, &iri)
            .expect("explain a shipped invented null");
        assert!(
            text.contains("invented witness"),
            "labels the witness: {text}"
        );
        assert!(
            text.contains("firing rule"),
            "prints the firing rule: {text}"
        );
        assert!(text.contains("ordinal"), "prints the ordinal: {text}");
        assert!(
            text.contains("frontier"),
            "prints the frontier binding: {text}"
        );
        assert!(
            text.contains("proof height"),
            "prints the bounded proof height: {text}"
        );

        // The shipped invented null explains with exit 0 through the i32 command surface.
        let reporter = reporter_for(ConsoleMode::Text);
        assert_eq!(
            explain(reporter.as_ref(), iri, None),
            0,
            "a shipped chase-invented null explains successfully"
        );
    }
}

#[cfg(test)]
mod entails_tests {
    use gmeow_cli_core::{ConsoleMode, reporter_for};

    use super::*;

    /// Drive the REAL `gmeow entails` production surface (parse files → dl_entails →
    /// exit code) over a premise `x∈A, A⊑B` and three conclusions: an entailed
    /// membership, a non-entailed membership, and a role-assertion gap; plus a hard
    /// fail on an unreadable file.
    #[test]
    fn entails_decides_positive_negative_gap_and_hard_fails_missing() {
        let dir = tempfile::tempdir().expect("temp dir");
        let premise = dir.path().join("premise.ttl");
        std::fs::write(
            &premise,
            "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix ex: <http://ex/> .\n\
             ex:x rdf:type ex:A .\n\
             ex:A rdfs:subClassOf ex:B .\n",
        )
        .unwrap();

        let concl_pos = dir.path().join("pos.ttl");
        std::fs::write(
            &concl_pos,
            "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix ex: <http://ex/> .\n\
             ex:x rdf:type ex:B .\n",
        )
        .unwrap();

        let concl_neg = dir.path().join("neg.ttl");
        std::fs::write(
            &concl_neg,
            "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
             @prefix ex: <http://ex/> .\n\
             ex:x rdf:type ex:C .\n",
        )
        .unwrap();

        let concl_gap = dir.path().join("gap.ttl");
        std::fs::write(
            &concl_gap,
            "@prefix ex: <http://ex/> .\nex:x ex:knows ex:y .\n",
        )
        .unwrap();

        let reporter = reporter_for(ConsoleMode::Silent);
        let r = reporter.as_ref();

        // The production handler returns exit 0 for every decided verdict (entailed,
        // not-entailed, and an honest gap) — the verdict itself is on stdout.
        assert_eq!(entails(r, &premise, &concl_pos), 0, "entailed exits 0");
        assert_eq!(entails(r, &premise, &concl_neg), 0, "not-entailed exits 0");
        assert_eq!(entails(r, &premise, &concl_gap), 0, "an honest gap exits 0");

        // The underlying verdicts are correct on the real datasets (parsed exactly as
        // the CLI does).
        let prem = parse_rdf_file(r, &premise).expect("premise parses");
        let pos = parse_rdf_file(r, &concl_pos).expect("pos parses");
        let neg = parse_rdf_file(r, &concl_neg).expect("neg parses");
        let gap = parse_rdf_file(r, &concl_gap).expect("gap parses");
        use gmeow_logic::entail::{EntailmentVerdict, GapShape, dl_entails};
        assert_eq!(
            dl_entails(prem.as_ref(), pos.as_ref()).unwrap(),
            EntailmentVerdict::Entailed
        );
        assert_eq!(
            dl_entails(prem.as_ref(), neg.as_ref()).unwrap(),
            EntailmentVerdict::NotEntailed
        );
        assert!(matches!(
            dl_entails(prem.as_ref(), gap.as_ref()).unwrap(),
            EntailmentVerdict::Gap(g) if g.shape == GapShape::RoleAssertion
        ));

        // A missing conclusion file is a hard fail (exit 1), never a degraded verdict.
        let missing = dir.path().join("nope.ttl");
        assert_eq!(
            entails(r, &premise, &missing),
            1,
            "missing input hard-fails"
        );
    }
}
