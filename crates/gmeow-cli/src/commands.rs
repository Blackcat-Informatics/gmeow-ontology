// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The wired command bodies. Each function marshals its inputs and delegates to
//! an already-native backend, following the console convention: product results
//! → stdout, errors/diagnostics → stderr, and a `0`/`1` exit code.

mod knowledge;
#[cfg(test)]
use knowledge::collect_subclass_edges;
pub use knowledge::{
    PurrembHybridQuery, build, candidate_list, candidate_submit, candidate_withdraw, classify,
    conjecture_test, consistency, describe, entails, hybrid_query, hybrid_query_purremb,
    logic_backward, logic_evaluate, logic_fragments, logic_session_apply, logic_session_checkpoint,
    logic_session_facts, logic_session_open, logic_session_restart, logic_session_restore, module,
    profile, realize, validate,
};
use knowledge::{RDF_TYPE_IRI, parse_rdf_file};
mod prove;
pub(crate) use prove::{ProveRequest, prove};
mod correspondence;
pub(crate) use correspondence::{
    correspondence_compose, correspondence_execute, correspondence_explain, correspondence_inspect,
    correspondence_merge, correspondence_project_plan, correspondence_recover_plan,
};
mod reasoning_input;
mod reasoning_report;
pub(crate) use reasoning_input::ReasoningInputFailure;
use reasoning_input::load_reasoning_input;

use reasoning_report::{
    DecisionClass, EngineIdentity, EvidenceGrade, GateAdmission, InputIdentity,
    NativeReasoningReport, PreservationReport, ProgramIdentity, ReasoningBoundary,
    ReasoningMetrics, ReasoningOperation, ReasoningPayload, ReasoningReportCore, ReportDiagnostic,
    TermPair,
};

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use purrdf::ir::InMemoryPageProvider;
use purrdf::{
    DistanceMetric, FamilyId, MatrixId, PageGeneration, PagedDataset, PagedQueryLimits,
    PrefixPostprocessing, ProjectionId, RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm,
    TargetSetId, TermValue, VectorDtype, VectorSpaceId,
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

use crate::{BUNDLE_GTS, NAMESPACE, OutputFormat};

/// Render a reasoning term for line-oriented CLI output.
/// Use the term's debug representation when it has no IRI.
fn render_term(term: &TermValue) -> String {
    term.as_iri()
        .map_or_else(|| format!("{term:?}"), str::to_owned)
}

/// Build an Error-grade CLI diagnostic carrying a per-site stable code — the
/// pre-carrier graded witness a handled `gmeow` failure lowers to (never a bare
/// string). The `code` is interned once (idempotently) and the message carries
/// the specifics.
pub(crate) fn error_diag(code: &str, message: impl Into<String>) -> Diag {
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

/// Emit a Warning-grade CLI diagnostic on the reporter's channel — a report the run
/// OWES the caller without the run itself having failed.
///
/// Distinct from [`emit_error`] because the two are different claims: an Error says the
/// command could not do what it was asked, a Warning says it did, and here is something
/// about the result you must know. Grading a non-fatal report Error would make the
/// severity axis useless for exactly the readers (agents consuming NDJSON) it exists for.
pub(crate) fn emit_warning(reporter: &dyn Reporter, code: &str, message: impl Into<String>) {
    let diag = Diag::new(
        gmeow_errors::code::register_code(code),
        Grade::new(
            Severity::Warning,
            FindingCategory::ModelingDisciplineViolation,
            Standpoint::Binding,
        ),
        message,
    );
    reporter.report(&report_diag(diag, "gmeow"));
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
pub(crate) fn gts_bytes(
    reporter: &dyn Reporter,
    file: Option<&Path>,
) -> Result<Cow<'static, [u8]>, i32> {
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

/// `gmeow version` — print the package version; `--verbose` prints the build identity.
///
/// The bundle and codebook digests are computed here from the bytes this binary
/// embeds, never stamped at build time, so the report cannot disagree with them.
/// Every other field is stamped by `build.rs`.
pub fn version(verbose: bool, format: crate::OutputFormat) -> i32 {
    if !verbose {
        println!("{}", env!("CARGO_PKG_VERSION"));
        return 0;
    }
    let identity = build_identity();
    match format {
        crate::OutputFormat::Text => {
            for (key, value) in &identity {
                let rendered = match value {
                    serde_json::Value::String(text) => text.clone(),
                    serde_json::Value::Array(items) => items
                        .iter()
                        .filter_map(serde_json::Value::as_str)
                        .collect::<Vec<_>>()
                        .join(","),
                    other => other.to_string(),
                };
                println!("{key:<16} {rendered}");
            }
        }
        crate::OutputFormat::Json => {
            let document = serde_json::Value::Object(
                identity
                    .into_iter()
                    .map(|(key, value)| (key.to_owned(), value))
                    .collect(),
            );
            match serde_json::to_string_pretty(&document) {
                Ok(text) => println!("{text}"),
                Err(error) => {
                    eprintln!("gmeow version: cannot serialize the build identity: {error}");
                    return 5;
                }
            }
        }
    }
    0
}

/// The build identity as ordered fields; text prints them in this order.
fn build_identity() -> Vec<(&'static str, serde_json::Value)> {
    use serde_json::Value;
    let features = env!("GMEOW_BUILD_TARGET_FEATURES")
        .split(',')
        .filter(|feature| !feature.is_empty())
        .map(|feature| Value::String(feature.to_owned()))
        .collect();
    let text = |value: &str| Value::String(value.to_owned());
    vec![
        ("version", text(env!("CARGO_PKG_VERSION"))),
        ("source_revision", text(env!("GMEOW_BUILD_SOURCE_REVISION"))),
        ("source_tree", text(env!("GMEOW_BUILD_SOURCE_TREE"))),
        ("purrdf_version", text(env!("GMEOW_BUILD_PURRDF_VERSION"))),
        ("bundle_sha256", text(&sha256_hex(crate::BUNDLE_GTS))),
        ("bundle_bytes", Value::from(crate::BUNDLE_GTS.len())),
        ("codebook_sha256", text(&sha256_hex(crate::GMN_CODEBOOK))),
        ("codebook_bytes", Value::from(crate::GMN_CODEBOOK.len())),
        ("target", text(env!("GMEOW_BUILD_TARGET"))),
        ("cpu_baseline", text(env!("GMEOW_BUILD_CPU_BASELINE"))),
        ("target_features", Value::Array(features)),
        ("profile", text(env!("GMEOW_BUILD_PROFILE"))),
        ("rustc", text(env!("GMEOW_BUILD_RUSTC"))),
    ]
}

/// Lowercase hexadecimal SHA-256 of `bytes`.
fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::Digest as _;
    purrdf_hash::hex::encode(&sha2::Sha256::digest(bytes))
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

// ── medium ───────────────────────────────────────────────────────────────────

/// `gmeow medium list [FILE]` — the artifact's whole declared medium axis.
///
/// Reads the medium registry, the generated realizations and the sealed envelopes out of
/// the graphs the artifact itself carries, and the pinned dictionary bytes out of its own
/// segment header. Nothing here consults a repository, a network, or a second artifact:
/// a bundle that could not answer this from its own bytes would not be self-sufficient
/// (Principle 13).
pub fn medium_list(reporter: &dyn Reporter, file: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(reporter, file) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    let inventory = match gmeow_pipeline::medium::inspect::inventory(&bytes) {
        Ok(inventory) => inventory,
        Err(diag) => return medium_fail(reporter, &diag),
    };

    println!("{}", medium_title(file));
    println!("  payload frames  {}", inventory.payload_frame_count);
    println!("  envelopes       {}", inventory.envelope_count);

    println!("\nmedia");
    for medium in &inventory.media {
        println!(
            "  {}\n    codec {} level {} source-kind {:?} requires {:?}",
            medium.iri,
            medium.codec,
            medium.zstd_level,
            medium.source_kind,
            medium.reader_capabilities
        );
        println!("    dictionaries {:?}", medium.dictionaries);
    }

    println!("\ndictionaries");
    for row in &inventory.dictionaries {
        println!(
            "  {} v{}  {} bytes  Dictionary_ID {}  in-band {}",
            row.id,
            row.version,
            row.byte_length,
            row.zstd_dictionary_id,
            row.in_band_bytes
                .map_or_else(|| "-".to_string(), |bytes| format!("{bytes} bytes")),
        );
        println!(
            "    strategy {} (measured {}) target {} corpus samples {}",
            row.strategy, row.measured_strategy, row.target_length, row.corpus_sample_count
        );
        println!("    {}", row.content_digest);
        println!("    primes {:?}", row.primes);
    }

    println!("\nassignment");
    for row in &inventory.assignment {
        println!(
            "  {:<32} {}  {}",
            row.rep,
            row.medium,
            row.dictionary.as_deref().unwrap_or("(no dictionary)")
        );
    }
    0
}

/// `gmeow medium verify [FILE]` — decode every frame and open every envelope.
///
/// The verb exists because "the bundle folded" is not evidence the bundle is whole:
/// purrdf's reader is total by design and stores a blob frame LAZILY, so a payload that
/// will not decode — or one that decodes to bytes other than the ones it claims — passes
/// an ordinary fold in silence. Every breach exits non-zero under its NAMED medium
/// failure class, and none of them ever authorizes a dictionary-less retry.
pub fn medium_verify(reporter: &dyn Reporter, file: Option<&Path>, registry: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(reporter, file) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    // An artifact that carries no registry of its own (a runtime `~/.gmeow/*.gts` store)
    // is resolved against the bundle whose dictionaries primed it — by default the
    // embedded one, because that IS the bundle a consumer's store was primed from. The
    // bytes are handed over unfolded: a self-describing artifact never reads them.
    let registry_bytes = match gts_bytes(reporter, registry) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    let report = match gmeow_pipeline::medium::inspect::verify(
        &bytes,
        gmeow_pipeline::medium::inspect::PrimingBundle::Bytes(&registry_bytes),
    ) {
        Ok(report) => report,
        Err(diag) => return medium_fail(reporter, &diag),
    };

    println!("{}", medium_title(file));
    println!("  class           {:?}", report.class);
    println!("  medium          {}", report.medium);
    println!("  payload frames  {}", report.frames.len());
    println!("  envelopes       {}", report.envelopes_verified);
    println!("  requires        {:?}", report.actual_capabilities);
    println!("  dictionaries    {:?}", report.dictionaries);
    for frame in &report.frames {
        println!(
            "  frame @{:<10} {:<32} {}  {} bytes",
            frame.offset,
            frame.rep,
            frame.dictionary.as_deref().unwrap_or("(no dictionary)"),
            frame.decoded_bytes
        );
    }
    println!(
        "verified: every payload frame decoded and {} envelope(s) re-derived",
        report.envelopes_verified
    );
    0
}

/// `gmeow medium explain <dict-id> [FILE]` — what one dictionary is and what it bought.
pub fn medium_explain(reporter: &dyn Reporter, dictionary: &str, file: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(reporter, file) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    let explanation = match gmeow_pipeline::medium::inspect::explain(&bytes, dictionary) {
        Ok(explanation) => explanation,
        Err(diag) => return medium_fail(reporter, &diag),
    };
    let row = &explanation.row;

    println!("{} v{}", row.id, row.version);
    println!("  dictionary      {}", row.iri);
    println!(
        "  strategy        {} (measured {}) at target length {}",
        row.strategy, row.measured_strategy, row.target_length
    );
    println!("  corpus          {}", row.corpus);
    for selector in &explanation.selectors {
        println!("    {selector}");
    }
    println!("  corpus samples  {}", row.corpus_sample_count);
    println!("  content digest  {}", row.content_digest);
    println!(
        "  bytes           {} (Dictionary_ID {})",
        row.byte_length, row.zstd_dictionary_id
    );
    println!("  primes          {:?}", row.primes);

    println!("  measured MDL contribution");
    for effect in &explanation.effects {
        println!(
            "    population {}: two-part code {} B = {} B of frames + {} B of in-band \
             dictionary, against a baseline of {} B over the same {} frame(s)",
            effect.population.wire(),
            effect.two_part_code_bytes(),
            effect.bytes_on_disk,
            effect.dictionary_in_band_bytes,
            effect.bytes_on_disk_baseline,
            effect.evaluated_frame_count
        );
        println!(
            "      bounded gain fraction {} (pays for itself: {})",
            effect.gain_fraction_lexical(),
            effect.wins()
        );
    }
    if explanation.effects.is_empty() {
        // Not a soft outcome: the runtime-store dictionaries are priced over a population
        // the BUNDLE cannot measure, and saying so is the honest answer rather than
        // printing a zero.
        println!(
            "    none in this artifact — the dictionary primes no frame of this artifact's \
             declared measurement population"
        );
    }
    0
}

/// Render the artifact's name for the `gmeow medium` report headers.
fn medium_title(file: Option<&Path>) -> String {
    file.and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "gmeow.gts".to_owned())
}

/// Emit a medium diagnostic under its OWN registered failure-class code and exit
/// non-zero.
///
/// The code is the diagnostic's, never a generic CLI one: the medium failure classes are the
/// vocabulary a caller (or a gate) dispatches on, and flattening them to
/// `gmeow-cli.medium.failed` would erase the only thing that says WHICH invariant broke.
fn medium_fail(reporter: &dyn Reporter, diag: &Diag) -> i32 {
    fail(
        reporter,
        gmeow_errors::code::code_str(diag.code()),
        diag.to_string(),
    )
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
    // the derivation attaches to). Honors the required hard-fail: a verdict that
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
/// `finding_iri`/anchor identity and its term IRI as `documented_terms` (the declared
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
    require_dictionary_evidence: bool,
) -> i32 {
    let bundle_bytes = match read_bytes(reporter, bundle) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let receipt_path = gmeow_gts_profile::ingestion_receipt_path(bundle);
    let receipt = match read_bytes(reporter, &receipt_path) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    if let Err(error) = gmeow_gts_profile::read_ingestion_receipt(&bundle_bytes, &receipt) {
        return fail(
            reporter,
            "gmeow-cli.verify-release.failed",
            format!("invalid release ingestion companion: {error}"),
        );
    }
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
            // The signature verdict comes first, so a forged bundle is reported as one. Only
            // then are the bundle's own dictionary records read: a positively stale record
            // is always refused, and absent records only when the strict form is requested.
            let requirement = if require_dictionary_evidence {
                gmeow_pipeline::medium::rdf::EvidenceRequirement::RequireRecords
            } else {
                gmeow_pipeline::medium::rdf::EvidenceRequirement::RefuseStale
            };
            if let Err(error) = gmeow_pipeline::medium::inspect::require_current_dictionary_evidence(
                &bundle_bytes,
                requirement,
            ) {
                return fail(
                    reporter,
                    "gmeow-cli.verify-release.stale-evidence",
                    format!("✗ release bundle does not show current dictionary evidence: {error}"),
                );
            }
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

// ── substrate ────────────────────────────────────────────────────────────────

/// `gmeow substrate` — answer, from the embedded bundle with no repo, what engine
/// produced this bundle, what it embeds, and whether the substrate claim sites
/// agree. Reads the reconciliation A-Box (gmeow:SubstrateComponent / gmeow:PinClaim
/// / gmeow:ReconciledPin / gmeow:embeds) the carrier folds into graph/provenance.
/// Generic over the reconciliation kernel: `--component` / `--dimension` filter.
pub fn substrate(
    reporter: &dyn Reporter,
    gts: Option<&Path>,
    component_filter: Option<&str>,
    dimension_filter: Option<&str>,
    format: &str,
) -> i32 {
    use std::collections::{BTreeMap, BTreeSet};

    // Reject unsupported output formats up front — only `human` and `json` are
    // produced. Anything else (e.g. `jsonl`, `sarif`) is a HARD FAIL, never a
    // silent fall-back to human text: once a format is selected it is mandatory
    // (no silent capability degradation).
    if format != "human" && format != "json" {
        return fail(
            reporter,
            "gmeow-cli.substrate.format",
            format!("unsupported --format '{format}': expected 'human' or 'json'"),
        );
    }

    let bytes = match gts_bytes(reporter, gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // The GTS fold carries SUPPRESSION overlays (GTS-SPEC §11 value-union +
    // suppression semantics). `flattened_dataset_from_bytes` copies base quads
    // verbatim and preserves the suppression list as an UNAPPLIED overlay
    // (`gts_to_ser` never reads `Graph::suppressions`), so a suppressed
    // historical claim would still surface in `owned_quads()`. Fold the
    // segments, project to the EFFECTIVE (post-suppression) view via
    // `effective_projection` — which drops every `term`- and `quad`-kind
    // suppressed base quad — and only THEN flatten to a dataset, so suppressed
    // claims are never reported as active.
    let graph = match purrdf::gts::read_all_segments(&bytes) {
        Ok(g) => g,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.substrate.fold",
                format!("cannot fold snapshot: {e}"),
            );
        }
    };
    let effective = purrdf::gts_certify::effective_projection(&graph);
    let dataset = match purrdf::gts::flattened_dataset_from_gts_graph(&effective) {
        Ok(d) => d,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.substrate.fold",
                format!("cannot fold snapshot: {e}"),
            );
        }
    };

    let ns = crate::NAMESPACE;
    let c_substrate = format!("{ns}SubstrateComponent");
    let p_name = format!("{ns}componentName");
    let p_version = format!("{ns}componentVersion");
    let p_embeds = format!("{ns}embeds");
    let p_claimed_comp = format!("{ns}claimedComponent");
    let p_claim_dim = format!("{ns}claimDimension");
    let p_claimed_val = format!("{ns}claimedValue");

    let mut components: BTreeSet<String> = BTreeSet::new();
    let mut name_of: BTreeMap<String, String> = BTreeMap::new();
    let mut version_of: BTreeMap<String, String> = BTreeMap::new();
    let mut embeds: BTreeSet<(String, String)> = BTreeSet::new();
    let mut claim_comp: BTreeMap<String, String> = BTreeMap::new();
    let mut claim_dim: BTreeMap<String, String> = BTreeMap::new();
    let mut claim_val: BTreeMap<String, String> = BTreeMap::new();

    for quad in dataset.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        let subj = subject.as_str().to_string();
        match quad.predicate.as_str() {
            RDF_TYPE_IRI => {
                if let RdfTerm::Iri(o) = &quad.object
                    && o.as_str() == c_substrate
                {
                    components.insert(subj);
                }
            }
            p if p == p_name => {
                if let RdfTerm::Literal(l) = &quad.object {
                    name_of.insert(subj, l.lexical_form.clone());
                }
            }
            p if p == p_version => {
                if let RdfTerm::Literal(l) = &quad.object {
                    version_of.insert(subj, l.lexical_form.clone());
                }
            }
            p if p == p_embeds => {
                if let RdfTerm::Iri(o) = &quad.object {
                    embeds.insert((subj, o.as_str().to_string()));
                }
            }
            p if p == p_claimed_comp => {
                if let RdfTerm::Iri(o) = &quad.object {
                    claim_comp.insert(subj, o.as_str().to_string());
                }
            }
            p if p == p_claim_dim => {
                if let RdfTerm::Iri(o) = &quad.object {
                    claim_dim.insert(subj, o.as_str().to_string());
                }
            }
            p if p == p_claimed_val => {
                if let RdfTerm::Literal(l) = &quad.object {
                    claim_val.insert(subj, l.lexical_form.clone());
                }
            }
            _ => {}
        }
    }

    // Agreement is keyed per (component, dimension): the set of distinct asserted
    // values. One value ⇒ the sites agree; more ⇒ drift.
    let mut groups: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
    for (claim, comp) in &claim_comp {
        if let (Some(dim), Some(val)) = (claim_dim.get(claim), claim_val.get(claim)) {
            groups
                .entry((comp.clone(), dim.clone()))
                .or_default()
                .insert(val.clone());
        }
    }

    let local = |iri: &str| iri.rsplit(['/', '#']).next().unwrap_or(iri).to_string();
    let dim_label = |iri: &str| local(iri).trim_start_matches("dimension").to_string();
    let comp_name = |iri: &str| name_of.get(iri).cloned().unwrap_or_else(|| local(iri));
    let component_matches =
        |iri: &str| component_filter.is_none_or(|f| comp_name(iri) == f || local(iri) == f);
    let dimension_matches = |dim_iri: &str| {
        dimension_filter.is_none_or(|f| dim_label(dim_iri) == f || local(dim_iri) == f)
    };

    // Drift is a per-(component, dimension) finding: more than one distinct
    // value across the claim sites. Compute and REPORT it up front — before the
    // JSON branch returns — so the finding reaches BOTH output paths through the
    // reporter, i.e. a structured/NDJSON consumer sees the drift as a diagnostic
    // even on `--format json`. Drift is a reported finding, not a process
    // failure: the exit code stays 0.
    let mut any_drift = false;
    for ((comp, dim), values) in &groups {
        if !component_matches(comp) || !dimension_matches(dim) {
            continue;
        }
        if values.len() > 1 {
            any_drift = true;
            let joined = values.iter().cloned().collect::<Vec<_>>().join(", ");
            emit_error(
                reporter,
                "gmeow-cli.substrate.drift",
                format!(
                    "substrate drift: {} / {}: {joined}",
                    comp_name(comp),
                    dim_label(dim)
                ),
            );
        }
    }

    if format == "json" {
        let mut component_docs: Vec<serde_json::Value> = Vec::new();
        for c in &components {
            if !component_matches(c) {
                continue;
            }
            let embedded: Vec<serde_json::Value> = embeds
                .iter()
                .filter(|(e, _)| e == c)
                .map(|(_, x)| serde_json::Value::String(comp_name(x)))
                .collect();
            component_docs.push(serde_json::json!({
                "name": comp_name(c),
                "version": version_of.get(c),
                "embeds": embedded,
            }));
        }
        let mut reconciliation_docs: Vec<serde_json::Value> = Vec::new();
        for ((comp, dim), values) in &groups {
            if !component_matches(comp) || !dimension_matches(dim) {
                continue;
            }
            reconciliation_docs.push(serde_json::json!({
                "component": comp_name(comp),
                "dimension": dim_label(dim),
                "agree": values.len() == 1,
                "values": values.iter().collect::<Vec<_>>(),
            }));
        }
        let doc = serde_json::json!({
            "components": component_docs,
            "reconciliation": reconciliation_docs,
        });
        println!(
            "{}",
            serde_json::to_string(&doc).unwrap_or_else(|_| doc.to_string())
        );
        return 0;
    }

    println!("substrate components (from the embedded bundle):");
    for c in &components {
        if !component_matches(c) {
            continue;
        }
        let version = version_of
            .get(c)
            .map(String::as_str)
            .unwrap_or("(unreconciled)");
        println!("  {} {version}", comp_name(c));
        for (_, embedded) in embeds.iter().filter(|(e, _)| e == c) {
            let ev = version_of.get(embedded).map(String::as_str).unwrap_or("");
            println!("    embeds {} {ev}", comp_name(embedded));
        }
    }

    println!("\nagreement across claim sites:");
    for ((comp, dim), values) in &groups {
        if !component_matches(comp) || !dimension_matches(dim) {
            continue;
        }
        if values.len() == 1 {
            println!(
                "  {} / {}: AGREE ({})",
                comp_name(comp),
                dim_label(dim),
                values.iter().next().unwrap()
            );
        } else {
            let joined = values.iter().cloned().collect::<Vec<_>>().join(", ");
            println!(
                "  {} / {}: DRIFT — {joined}",
                comp_name(comp),
                dim_label(dim)
            );
        }
    }
    if any_drift {
        println!("\nsubstrate drift detected: the claim sites above disagree.");
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
        // The bundled YAML-LD deliverable is the CLAIM CORPUS's YAML-LD-star projection
        // (the RDF 1.2 statement layer), which rides the `yaml-ld-archive` frame
        // — not a serialization of the whole carrier, whose JSON-LD-star form is
        // the ~666 MB `make build` artifact `dist/gmeow.jsonld` and rides no frame.
        if source.is_some() {
            return fail(
                reporter,
                "gmeow-cli.project.yaml-ld-source",
                "--format yaml-ld reads the bundled claim corpus only; do not pass a source file",
            );
        }
        let member = gmeow_pipeline::bundle_blobs::YAMLLD_YAMLLD_MEMBER;
        let yamlld = match gmeow_pipeline::bundle_blobs::bundled_yaml_ld(BUNDLE_GTS) {
            Ok(map) => map.get(member).cloned(),
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
                format!("bundled YAML-LD claim corpus ({member}) not found"),
            );
        };
        if let Err(e) = std::fs::create_dir_all(out) {
            return fail(
                reporter,
                "gmeow-cli.io.create-dir",
                format!("cannot create {}: {e}", out.display()),
            );
        }
        let target = out.join(member);
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
    let program = match gmeow_pipeline::projection_execution::PreparedProjectionProfile::from_bundle(
        BUNDLE_GTS, profile,
    ) {
        Ok(program) => program,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.project.profile-program",
                format!("cannot admit required projection profile {profile}: {e}"),
            );
        }
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

    match gmeow_pipeline::projections::project_graph(&source_nt, &program, tag_map) {
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

    let projection_programs = gmeow_pipeline::projection_execution::prepared_from_bundle(
        BUNDLE_GTS,
        gmeow_logic_compile::projections::get_leg::PROFILES,
    )?;
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
        projection_programs,
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
    if let Err(e) = gmeow_gts_profile::write_ingestion_receipt(
        &gts_path,
        &transform.gts_bytes,
        &transform.gts_ingestion,
    ) {
        return fail(
            reporter,
            "gmeow-cli.io.write",
            format!("cannot publish GTS ingestion receipt: {e}"),
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

    if let Some(ingestion) = &output.gts_ingestion {
        match out {
            Some(path) => {
                if let Err(e) =
                    gmeow_gts_profile::write_ingestion_receipt(path, &output.bytes, ingestion)
                {
                    return fail(
                        reporter,
                        "gmeow-cli.io.write",
                        format!("cannot publish GTS ingestion receipt: {e}"),
                    );
                }
            }
            None => {
                let receipt = match gmeow_gts_profile::ingestion_receipt(&output.bytes, ingestion) {
                    Ok(bytes) => bytes,
                    Err(e) => return fail(reporter, "gmeow-cli.convert.loss", e.to_string()),
                };
                let encoded = purrdf_hash::hex::encode(&receipt);
                gmeow_cli_core::note(
                    reporter,
                    "gmeow",
                    "gmeow-cli.convert.loss",
                    format!("GTS ingestion receipt (hex CBOR): {encoded}"),
                );
            }
        }
    }

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
/// The embedded [`BUNDLE_GTS`] snapshot is the sole ontology source (repo-free).
/// This binary links `gmeow-mcp` alone, never `gmeow-mcp-dev`, so the repo-reading
/// dev tools are not merely hidden here — they are not compiled in. Blocks on the
/// stdio JSON-RPC loop until EOF, then exits `0`; a construction or I/O error maps
/// to a nonzero exit.
pub fn mcp(reporter: &dyn Reporter) -> i32 {
    use gmeow_mcp::McpServer;
    // The medium registry rides an extension because the MCP engine is a leaf that does not
    // link the build executor; this binary DOES, so the shipped consumer surface is unchanged.
    let medium = gmeow_mcp::extension::Extension::new()
        .with_resource(gmeow_mcp::medium_resource_descriptor(), |server, _args| {
            gmeow_pipeline::medium::inspect::inventory_json(server.view().gts_bytes())
        });
    let server = match McpServer::from_snapshot_with(BUNDLE_GTS, medium) {
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
pub fn slice_quality(reporter: &dyn Reporter, dir: &Path, format: &str, profile: &str) -> i32 {
    let report = match gmeow_slice_quality::score_external_slice_bytes_with_profile(
        BUNDLE_GTS, dir, profile,
    ) {
        Ok(r) => r,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.slice.quality.score",
                format!("cannot score {}: {e}", dir.display()),
            );
        }
    };
    let incomplete = report
        .profile
        .as_ref()
        .is_some_and(|profile| !profile.complete);
    let rendered = match format {
        "human" => Ok(report.render_text()),
        "json" => report.to_report().and_then(|diagnostics| {
            gmeow_errors::render::to_json(&diagnostics).map_err(Diag::from)
        }),
        "sarif" => report.to_report().and_then(|diagnostics| {
            gmeow_errors::render::to_sarif(&diagnostics).map_err(Diag::from)
        }),
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
            if incomplete { 3 } else { 0 }
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
    let outcome =
        match gmeow_slice_quality::lint_report(&report, declared.as_ref(), required.as_ref()) {
            Ok(outcome) => outcome,
            Err(error) => {
                return fail(reporter, "gmeow-cli.slice.lint.metadata", error.to_string());
            }
        };
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
/// bundle path runs the SAME [`gmeow_mcp::extract_authoring_packets`] core the
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
/// [`gmeow_mcp::extract_authoring_packets`] core the MCP `slice_brief` tool
/// runs (one implementation, not two). Checkout-free: no repo root, no SHACL shape union.
fn slice_brief_from_bundle(
    reporter: &dyn Reporter,
    slice: &str,
    axis: Option<&str>,
    batch: Option<u32>,
    format: &str,
) -> i32 {
    let out =
        match gmeow_mcp::slice_brief_from_bundle(BUNDLE_GTS, slice, axis, batch.map(u64::from)) {
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
/// measured residue (which needs authored input; see [`slice_projection_debt`]).
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
                    v.owners.iter().collect::<Vec<_>>().join(","),
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
                    v.owners.iter().collect::<Vec<_>>().join(",")
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

/// Measure explicitly supplied authored sources against the embedded governance.
pub fn slice_projection_debt(reporter: &dyn Reporter, dir: &Path, format: &str) -> i32 {
    let result = gmeow_slice_quality::ceilings_from_gts(BUNDLE_GTS)
        .and_then(|governance| {
            let grounding =
                gmeow_logic_compile::projections::grounding_catalog::GroundingCatalog::from_bundle(
                    BUNDLE_GTS,
                )?;
            gmeow_slice_quality::projection_debt::measure_slice_debt(dir, &governance, &grounding)
        })
        .and_then(|rows| {
            gmeow_slice_quality::projection_debt::render_projection_debt(&rows, format)
        });
    match result {
        Ok(output) => {
            print!("{output}");
            0
        }
        Err(error) => fail(
            reporter,
            "gmeow-cli.slice.projection-debt",
            error.to_string(),
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
    // The family column is 20 wide because `interactive-runtime` is 19 characters: the
    // previous 14 made the console's row overflow its column and knocked every field after
    // it out of alignment.
    println!(
        "{:<10} {:<20} {:<40} {:<24} dropped-capabilities",
        "slug", "family", "consumers", "media-type"
    );
    for row in &rows {
        println!(
            "{:<10} {:<20} {:<40} {:<24} {}",
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
mod subsort_tests {
    use super::*;

    /// `gmeow logic backward --subsort` collects covering edges under BOTH
    /// spellings of the subsumption predicate.
    ///
    /// The blinding regression this pins: an `rdfs:`-only read handed the engine
    /// an EMPTY sort order for a subsort tower authored in the canonical
    /// `logic:subClassOf`, so every subsort-dependent goal silently failed to
    /// prove with no diagnostic on the shipped CLI.
    #[test]
    fn subsort_edges_are_collected_under_both_spellings() {
        let ttl = "@prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             @prefix math: <https://blackcatinformatics.ca/math/> .\n\
             math:Integer logic:subClassOf math:RationalNumber .\n\
             math:RationalNumber rdfs:subClassOf math:RealNumber .\n\
             math:Opaque logic:subClassOf [ rdfs:label \"a class expression\" ] .\n";
        let dataset = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("parse");
        let edges = collect_subclass_edges(&dataset);

        assert_eq!(
            edges,
            vec![
                (
                    "https://blackcatinformatics.ca/math/Integer".to_owned(),
                    "https://blackcatinformatics.ca/math/RationalNumber".to_owned(),
                ),
                (
                    "https://blackcatinformatics.ca/math/RationalNumber".to_owned(),
                    "https://blackcatinformatics.ca/math/RealNumber".to_owned(),
                ),
            ],
            "the canonical edge is collected alongside the projected one, and a \
             blank-node object is never a covering edge"
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
        assert_eq!(
            entails(r, &premise, &concl_pos, OutputFormat::Text, false),
            0,
            "entailed exits 0"
        );
        assert_eq!(
            entails(r, &premise, &concl_neg, OutputFormat::Text, false),
            0,
            "not-entailed exits 0"
        );
        assert_eq!(
            entails(r, &premise, &concl_gap, OutputFormat::Text, false),
            0,
            "an honest gap exits 0"
        );

        // The underlying verdicts are correct on the real datasets (parsed exactly as
        // the CLI does).
        let prem = parse_rdf_file(r, "gmeow-cli.entails", &premise).expect("premise parses");
        let pos = parse_rdf_file(r, "gmeow-cli.entails", &concl_pos).expect("pos parses");
        let neg = parse_rdf_file(r, "gmeow-cli.entails", &concl_neg).expect("neg parses");
        let gap = parse_rdf_file(r, "gmeow-cli.entails", &concl_gap).expect("gap parses");
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

        // A missing conclusion file is a typed malformed-input failure, never a
        // degraded verdict.
        let missing = dir.path().join("nope.ttl");
        assert_eq!(
            entails(r, &premise, &missing, OutputFormat::Text, false),
            reasoning_report::EXIT_MALFORMED,
            "missing input hard-fails"
        );
    }
}

// ---------------------------------------------------------------------------
// `gmeow logic frontier` / `gmeow logic saga` — the operator surface over a
// durable enactment.
//
// Both commands REASON before they print, and every row they print carries the
// provenance of its own conclusion. A frontier label the shipped `logic:Rule` set
// derived from an entry's lifecycle-axis tuple is authoritative and marked
// `derived`; a hand-written label the rules contradict is printed as an explicit
// DISAGREEMENT beside the derived value; a hand-written label no rule reproduces —
// eleven of the sixteen shipped labels still have no derivation rule — is printed
// as ASSERTED-UNCHECKED rather than dressed up as a conclusion.
//
// The distinction is the reasoner's own: `crates/logic` stamps every echoed EDB row
// with `provenance::ASSERT_RULE_IRI` and every rule conclusion with the IRI of the
// rule that fired, which is the split `derivation_graph.rs` and the chase both use.
// ---------------------------------------------------------------------------

/// The `logic:` namespace these commands read.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

use gmeow_logic::operator::FactRow;
use gmeow_logic::operator_rules::PreparedOperatorRules;

/// Hydrate the native rules from this executable's captured immutable bundle once.
/// Only the selected member and native preparation survive this initialization.
fn operator_rules(
    reporter: &dyn Reporter,
    code: &str,
) -> Result<&'static PreparedOperatorRules, i32> {
    static RULES: std::sync::OnceLock<gmeow_errors::Result<PreparedOperatorRules>> =
        std::sync::OnceLock::new();
    RULES
        .get_or_init(|| {
            let bundle = gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(BUNDLE_GTS)?;
            let bytes = bundle.required_archive_member(
                gmeow_gts_profile::archive::PREPARED_REASONING_REP,
                gmeow_logic::operator_rules::PREPARED_OPERATOR_MEMBER,
                gmeow_gts_profile::archive::MAX_NATIVE_MEMBER_BYTES,
            )?;
            PreparedOperatorRules::from_bytes(&bytes)
        })
        .as_ref()
        .map_err(|error| {
            fail(
                reporter,
                code,
                format!("cannot load the captured native operator rules: {error}"),
            )
        })
}

/// Parse only caller-owned input and execute the captured native preparation.
fn logic_derive(reporter: &dyn Reporter, path: &Path, code: &str) -> Result<Vec<FactRow>, i32> {
    let bytes = std::fs::read(path).map_err(|error| {
        fail(
            reporter,
            code,
            format!("cannot read {}: {error}", path.display()),
        )
    })?;
    let media = match path.extension().and_then(|extension| extension.to_str()) {
        Some("nq") => "application/n-quads",
        Some("nt") => "application/n-triples",
        Some("trig") => "application/trig",
        _ => "text/turtle",
    };
    let parsed = purrdf::parse_dataset(&bytes, media, None).map_err(|error| {
        fail(
            reporter,
            code,
            format!("cannot parse {}: {error}", path.display()),
        )
    })?;
    let derived = gmeow_logic::operator::derive(&parsed, operator_rules(reporter, code)?)
        .map_err(|error| fail(reporter, code, format!("{}: {error}", path.display())))?;
    report_operator_boundaries(reporter, path, code, &derived);
    Ok(derived.rows)
}

fn report_operator_source_diagnostics(
    reporter: &dyn Reporter,
    code: &str,
    diagnostics: &[gmeow_logic_compile::frontend::Diagnostic],
) {
    for diagnostic in diagnostics {
        emit_warning(
            reporter,
            &format!("{code}.source-{}", diagnostic.code),
            format!(
                "{}: {}",
                gmeow_logic::operator_rules::OPERATOR_SOURCE_IRI,
                diagnostic.message
            ),
        );
    }
}

fn report_operator_boundaries(
    reporter: &dyn Reporter,
    path: &Path,
    code: &str,
    derived: &gmeow_logic::operator::Derivation,
) {
    report_operator_source_diagnostics(reporter, code, &derived.diagnostics);
    let unsupported: BTreeSet<_> = derived
        .preservation
        .unsupported_constructs
        .iter()
        .chain(&derived.audit_preservation.unsupported_constructs)
        .collect();
    if !unsupported.is_empty() {
        emit_warning(
            reporter,
            &format!("{code}.positive-horn-lowering-boundary"),
            format!(
                "the selected PositiveHorn operator projection retains unsupported source constructs: {unsupported:?}; its frontier is not a whole-theory consistency certificate"
            ),
        );
    }
    if !derived.nested_statements.is_empty() {
        emit_warning(
            reporter,
            &format!("{code}.nested-triple-term-not-lowered"),
            format!(
                "{} carries {} RDF 1.2 attribution(s) reifying a statement that NESTS a triple \
             term. Flat statement metadata IS reasoned over — the rule set joins on the \
             lowered logic:reifiedStatementSubject / logic:reifiedStatementPredicate / \
             logic:reifiedStatementObject edges — but a nested statement has no non-term \
             component to decompose into, so it is not lowered and nothing derived here \
             is a conclusion about it. This is the residue recorded at \
             logic:rdf12-nested-triple-term, and nothing wider.",
                path.display(),
                derived.nested_statements.len()
            ),
        );
    }
}

/// Every `(subject IRI, object IRI)` pair carrying `predicate`, regardless of provenance.
///
/// The right reading for the structural predicates — axis witnesses, actions, gaps,
/// proposals — whose two ends are IRIs by construction and where asserted and derived rows
/// are equally usable input. It is the WRONG reading for `logic:entryLabel`, which is the
/// conclusion under scrutiny; that one goes through [`label_verdicts`].
///
/// This is a PROJECTION for one structural question, not a filter that loses data: every
/// row, whatever its term kinds, stays in the row set the commands print and serialize.
fn iri_pairs(rows: &[FactRow], predicate: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = rows
        .iter()
        .filter(|r| r.predicate == predicate)
        .filter_map(|r| Some((r.subject.iri()?.to_owned(), r.object.iri()?.to_owned())))
        .collect();
    out.sort();
    out.dedup();
    out
}

/// What the command knows about one frontier entry's `logic:entryLabel`.
///
/// The three cases are three different things to tell an operator, and flattening any pair
/// of them is the defect this type exists to make unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq)]
enum LabelVerdict {
    /// A rule concluded this label. Authoritative. `also_asserted` records that the input
    /// happens to agree — worth nothing operationally, but it is the difference between an
    /// author who is right and an author who never spoke.
    Derived { label: String, also_asserted: bool },
    /// A rule concluded `derived`, and the input asserts `asserted` instead. The derived
    /// value governs; the assertion is stale or wrong and must be shown as such.
    Disagreement { derived: String, asserted: String },
    /// The input asserts a label and NO shipped rule derives one for this entry. Nothing
    /// has checked it. Not an error — several frontier labels have no derivation rule yet —
    /// but an operator must never mistake it for a conclusion.
    AssertedUnchecked { asserted: String },
}

impl LabelVerdict {
    /// The `(label, source)` columns of this verdict's table row, or `None` when the
    /// verdict is a marker attached to the rows above it rather than a row of its own.
    ///
    /// A disagreement gets no row because the derived label already has one: printing the
    /// author's value in the label column, under any heading, is precisely how a
    /// hand-typed string acquires the authority of a conclusion.
    fn row(&self) -> Option<(&str, &'static str)> {
        match self {
            Self::Derived {
                label,
                also_asserted,
            } => Some((
                short(label),
                if *also_asserted {
                    "derived (input agrees)"
                } else {
                    "derived"
                },
            )),
            Self::Disagreement { .. } => None,
            Self::AssertedUnchecked { asserted } => Some((short(asserted), "ASSERTED-UNCHECKED")),
        }
    }

    /// The marker line printed under the row, when the entry owes the operator a warning.
    fn caveat(&self) -> Option<String> {
        match self {
            Self::Derived { .. } => None,
            Self::Disagreement { derived, asserted } => Some(format!(
                "  DISAGREEMENT  the input asserts logic:entryLabel {}, which the shipped rule \
                 set does NOT derive from this entry's axis witnesses; the derived label {} is \
                 authoritative",
                short(asserted),
                short(derived)
            )),
            Self::AssertedUnchecked { .. } => Some(
                "  UNCHECKED     no shipped logic:Rule derives a label for this entry, so the \
                 value above is an author's assertion that nothing has verified"
                    .to_owned(),
            ),
        }
    }

    /// The verdict as JSON: the SAME three-way provenance split the table prints, with the
    /// derived and asserted labels in separate keys so a consumer can never read one as
    /// the other. The full label IRIs are emitted, not the `logic:`-stripped display form —
    /// a machine consumer needs the term, not the column.
    fn to_json(&self, entry: &str) -> serde_json::Value {
        let mut map = serde_json::Map::new();
        map.insert("entry".to_owned(), entry.into());
        match self {
            Self::Derived {
                label,
                also_asserted,
            } => {
                map.insert(
                    "provenance".to_owned(),
                    if *also_asserted {
                        "derived (input agrees)"
                    } else {
                        "derived"
                    }
                    .into(),
                );
                map.insert("derived_label".to_owned(), label.clone().into());
                map.insert("input_agrees".to_owned(), (*also_asserted).into());
            }
            Self::Disagreement { derived, asserted } => {
                map.insert("provenance".to_owned(), "DISAGREEMENT".into());
                map.insert("derived_label".to_owned(), derived.clone().into());
                map.insert("asserted_label".to_owned(), asserted.clone().into());
            }
            Self::AssertedUnchecked { asserted } => {
                map.insert("provenance".to_owned(), "ASSERTED-UNCHECKED".into());
                map.insert("asserted_label".to_owned(), asserted.clone().into());
            }
        }
        if let Some(caveat) = self.caveat() {
            map.insert("caveat".to_owned(), caveat.trim().into());
        }
        serde_json::Value::Object(map)
    }
}

/// Render one structured command answer to the same explicit string sink as text.
fn render_json(
    output: &mut String,
    reporter: &dyn Reporter,
    code: &str,
    doc: &serde_json::Value,
) -> i32 {
    match serde_json::to_string_pretty(doc) {
        Ok(text) => {
            output.push_str(&text);
            output.push('\n');
            0
        }
        Err(error) => fail(
            reporter,
            code,
            format!("cannot render the result as JSON: {error}"),
        ),
    }
}

/// Every derived row as a JSON array — the whole reasoned closure with its per-row
/// provenance split, which is what lets a consumer fold the result back into a graph
/// instead of re-deriving it from a rendered table.
fn rows_json(rows: &[FactRow]) -> serde_json::Value {
    serde_json::Value::Array(rows.iter().map(FactRow::to_json).collect())
}

/// Split every entry's `logic:entryLabel` rows by provenance into per-entry verdicts.
///
/// One entry may yield several verdicts: two rules may conclude two labels, and each
/// asserted label a rule fails to reproduce is its own disagreement. Returning a flat,
/// entry-keyed list rather than one verdict per entry keeps every one of those visible
/// instead of letting a first-match win.
fn label_verdicts(rows: &[FactRow]) -> Vec<(String, LabelVerdict)> {
    let predicate = format!("{LOGIC_NS}entryLabel");
    let mut derived: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut asserted: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for row in rows.iter().filter(|r| r.predicate == predicate) {
        // A frontier label is one of the shipped `logic:Frontier…` individuals, so both
        // ends are IRIs. A row that is not is a different fact and keeps its place in the
        // full row set rather than being coerced into a verdict about a label.
        let (Some(subject), Some(object)) = (row.subject.iri(), row.object.iri()) else {
            continue;
        };
        if row.derived {
            derived.entry(subject).or_default().insert(object);
        }
        if row.asserted {
            asserted.entry(subject).or_default().insert(object);
        }
    }

    let entries: BTreeSet<&str> = derived.keys().chain(asserted.keys()).copied().collect();
    let mut out = Vec::new();
    for entry in entries {
        let concluded = derived.get(entry).cloned().unwrap_or_default();
        let typed = asserted.get(entry).cloned().unwrap_or_default();
        if concluded.is_empty() {
            out.extend(typed.into_iter().map(|asserted| {
                (
                    entry.to_owned(),
                    LabelVerdict::AssertedUnchecked {
                        asserted: asserted.to_owned(),
                    },
                )
            }));
            continue;
        }
        for label in &concluded {
            out.push((
                entry.to_owned(),
                LabelVerdict::Derived {
                    label: (*label).to_owned(),
                    also_asserted: typed.contains(label),
                },
            ));
        }
        // Whatever the author typed that no rule reproduced. Pairing it with the first
        // derived label is deliberate: the disagreement is with the derivation as a whole,
        // and naming one concrete conclusion is what makes it actionable.
        let governing = concluded
            .iter()
            .next()
            .expect("non-empty by the check above");
        for stale in typed.difference(&concluded) {
            out.push((
                entry.to_owned(),
                LabelVerdict::Disagreement {
                    derived: (*governing).to_owned(),
                    asserted: (*stale).to_owned(),
                },
            ));
        }
    }
    out
}

/// Strip the `logic:` namespace for display; leave anything else whole.
fn short(iri: &str) -> &str {
    iri.strip_prefix(LOGIC_NS).unwrap_or(iri)
}

/// The proposal fields a blocked step's remedy carries, in the order an operator reads
/// them. Named once so the text and JSON renderings can never drift apart.
const PROPOSAL_FIELDS: [&str; 7] = [
    "proposalMissingCapability",
    "proposalRequiredContract",
    "proposalExpectedInputs",
    "proposalExpectedOutputs",
    "proposalExpectedEffects",
    "proposalVerificationMethod",
    "proposalSecurityLifecycle",
];

/// `gmeow logic frontier` — print the derived frontier, or explain one action.
pub fn logic_frontier(
    reporter: &dyn Reporter,
    input: &Path,
    why_not: Option<&str>,
    format: OutputFormat,
) -> i32 {
    const CODE: &str = "gmeow-cli.logic-frontier";
    let rows = match logic_derive(reporter, input, CODE) {
        Ok(r) => r,
        Err(rc) => return rc,
    };

    let mut output = String::new();
    let code = render_logic_frontier(reporter, input, &rows, why_not, format, &mut output);
    print!("{output}");
    code
}

fn render_logic_frontier(
    reporter: &dyn Reporter,
    input: &Path,
    rows: &[FactRow],
    why_not: Option<&str>,
    format: OutputFormat,
    output: &mut String,
) -> i32 {
    const CODE: &str = "gmeow-cli.logic-frontier";
    let verdicts = label_verdicts(rows);
    let witnesses = iri_pairs(rows, &format!("{LOGIC_NS}entryAxisWitness"));
    let actions = iri_pairs(rows, &format!("{LOGIC_NS}entryAction"));

    if let Some(target) = why_not {
        // One action, in depth. The entry is named directly or reached through the
        // step it positions, because an operator asking "why not this step" should
        // not have to know the entry's IRI.
        let entry = verdicts
            .iter()
            .map(|(e, _)| e.clone())
            .find(|e| e == target)
            .or_else(|| {
                actions
                    .iter()
                    .find(|(_, step)| step == target)
                    .map(|(e, _)| e.clone())
            });
        let Some(entry) = entry else {
            return fail(
                reporter,
                CODE,
                format!(
                    "no frontier entry names <{target}> — neither as an entry IRI nor as the \
                     action of one"
                ),
            );
        };
        if matches!(format, OutputFormat::Json) {
            return render_json(
                output,
                reporter,
                CODE,
                &why_not_json(input, target, &entry, &verdicts, &witnesses, &actions, rows),
            );
        }
        writeln!(output, "action:  {target}").expect("writing to String is infallible");
        writeln!(output, "entry:   {entry}").expect("writing to String is infallible");
        let mut spoke = false;
        for (_, verdict) in verdicts.iter().filter(|(e, _)| *e == entry) {
            spoke = true;
            match verdict {
                // `(derived)` is reserved for a value a rule concluded. An asserted label
                // gets the opposite stamp, naming what has NOT happened to it.
                LabelVerdict::Derived { label, .. } => {
                    writeln!(output, "label:   {}   (derived)", short(label))
                        .expect("writing to String is infallible");
                }
                LabelVerdict::AssertedUnchecked { asserted } => {
                    writeln!(
                        output,
                        "label:   {}   (ASSERTED — no shipped logic:Rule derives a label for \
                         this entry, so nothing has verified this value)",
                        short(asserted)
                    )
                    .expect("writing to String is infallible");
                }
                LabelVerdict::Disagreement { derived, asserted } => {
                    writeln!(
                        output,
                        "DISAGREEMENT: the input asserts logic:entryLabel {}, which the shipped \
                         rule set does NOT derive from this entry's axis witnesses; the derived \
                         label {} is authoritative",
                        short(asserted),
                        short(derived)
                    )
                    .expect("writing to String is infallible");
                }
            }
        }
        if !spoke {
            writeln!(output, "label:   <no label derived and none asserted>")
                .expect("writing to String is infallible");
        }
        for (e, w) in &witnesses {
            if *e == entry {
                writeln!(output, "axis:    {}", short(w)).expect("writing to String is infallible");
            }
        }
        // The blockage, if the graph carries one.
        let step = actions
            .iter()
            .find(|(e, _)| *e == entry)
            .map(|(_, s)| s.clone());
        if let Some(step) = step {
            for (gap, blocked) in iri_pairs(rows, &format!("{LOGIC_NS}gapBlockedStep")) {
                if blocked == step {
                    writeln!(output, "gap:     {gap}").expect("writing to String is infallible");
                }
            }
            for (proposal, blocked) in iri_pairs(rows, &format!("{LOGIC_NS}proposalBlockedStep")) {
                if blocked == step {
                    writeln!(output, "proposal: {proposal}")
                        .expect("writing to String is infallible");
                    for field in PROPOSAL_FIELDS {
                        for row in rows {
                            if row.predicate == format!("{LOGIC_NS}{field}")
                                && row.subject.iri() == Some(proposal.as_str())
                            {
                                writeln!(output, "  {field}: {}", row.object.display())
                                    .expect("writing to String is infallible");
                            }
                        }
                    }
                }
            }
        }
        return 0;
    }

    if verdicts.is_empty() {
        return fail(
            reporter,
            CODE,
            format!(
                "no frontier label was derived from {} and none is asserted — the input \
                 carries no logic:FrontierEntry with axis witnesses the rule set can label",
                input.display()
            ),
        );
    }

    if matches!(format, OutputFormat::Json) {
        return render_json(
            output,
            reporter,
            CODE,
            &frontier_json(input, &verdicts, &witnesses, &actions, rows),
        );
    }

    // SOURCE is a column rather than a heading, because the heading cannot be true of every
    // row: eleven of the sixteen shipped frontier labels have no derivation rule yet, so an
    // input may legitimately carry an authored label the reasoner never touched. Saying so
    // per row is the only honest layout.
    writeln!(
        output,
        "{:<44}  {:<36}  {:<22}  AXIS TUPLE",
        "ENTRY", "LABEL", "SOURCE"
    )
    .expect("writing to String is infallible");
    for (entry, verdict) in &verdicts {
        if let Some((label, source)) = verdict.row() {
            let axes: Vec<&str> = witnesses
                .iter()
                .filter(|(e, _)| e == entry)
                .map(|(_, w)| short(w))
                .collect();
            writeln!(
                output,
                "{:<44}  {:<36}  {:<22}  {}",
                short(entry),
                label,
                source,
                if axes.is_empty() {
                    "-".to_owned()
                } else {
                    axes.join(" + ")
                }
            )
            .expect("writing to String is infallible");
        }
        if let Some(caveat) = verdict.caveat() {
            writeln!(output, "{caveat}").expect("writing to String is infallible");
        }
    }
    0
}

/// The whole frontier as JSON: every entry with its provenance-split verdict and axis
/// tuple, the entry→action positioning, AND the complete derived row set.
///
/// The row set is the part that makes the frontier a graph again rather than a report: an
/// agent runtime folds `facts` straight back in, keeping each row's `asserted` / `derived`
/// flags, instead of re-deriving a conclusion it has just been handed.
fn frontier_json(
    input: &Path,
    verdicts: &[(String, LabelVerdict)],
    witnesses: &[(String, String)],
    actions: &[(String, String)],
    rows: &[FactRow],
) -> serde_json::Value {
    let entries: Vec<serde_json::Value> = verdicts
        .iter()
        .map(|(entry, verdict)| {
            let mut obj = verdict.to_json(entry);
            let axes: Vec<&str> = witnesses
                .iter()
                .filter(|(e, _)| e == entry)
                .map(|(_, w)| w.as_str())
                .collect();
            let acts: Vec<&str> = actions
                .iter()
                .filter(|(e, _)| e == entry)
                .map(|(_, a)| a.as_str())
                .collect();
            if let serde_json::Value::Object(map) = &mut obj {
                map.insert("axis_witnesses".to_owned(), axes.into());
                map.insert("actions".to_owned(), acts.into());
            }
            obj
        })
        .collect();
    serde_json::json!({
        "command": "logic frontier",
        "input": input.display().to_string(),
        "entries": entries,
        "facts": rows_json(rows),
    })
}

/// One action's `--why-not` answer as JSON: the entry it belongs to, its verdicts, its
/// axis tuple, and every operational capability gap and proposal bound to its step, with
/// each proposal field carried as a full term rather than a flattened string.
fn why_not_json(
    input: &Path,
    target: &str,
    entry: &str,
    verdicts: &[(String, LabelVerdict)],
    witnesses: &[(String, String)],
    actions: &[(String, String)],
    rows: &[FactRow],
) -> serde_json::Value {
    let entry_verdicts: Vec<serde_json::Value> = verdicts
        .iter()
        .filter(|(e, _)| e == entry)
        .map(|(e, v)| v.to_json(e))
        .collect();
    let axes: Vec<&str> = witnesses
        .iter()
        .filter(|(e, _)| e == entry)
        .map(|(_, w)| w.as_str())
        .collect();
    let step = actions
        .iter()
        .find(|(e, _)| e == entry)
        .map(|(_, s)| s.clone());
    let mut gaps: Vec<String> = Vec::new();
    let mut proposals: Vec<serde_json::Value> = Vec::new();
    if let Some(step) = &step {
        for (gap, blocked) in iri_pairs(rows, &format!("{LOGIC_NS}gapBlockedStep")) {
            if &blocked == step {
                gaps.push(gap);
            }
        }
        for (proposal, blocked) in iri_pairs(rows, &format!("{LOGIC_NS}proposalBlockedStep")) {
            if &blocked != step {
                continue;
            }
            let mut fields = serde_json::Map::new();
            for field in PROPOSAL_FIELDS {
                let predicate = format!("{LOGIC_NS}{field}");
                let values: Vec<serde_json::Value> = rows
                    .iter()
                    .filter(|r| {
                        r.predicate == predicate && r.subject.iri() == Some(proposal.as_str())
                    })
                    .map(|r| r.object.to_json())
                    .collect();
                if !values.is_empty() {
                    fields.insert(field.to_owned(), values.into());
                }
            }
            proposals.push(serde_json::json!({
                "proposal": proposal,
                "fields": serde_json::Value::Object(fields),
            }));
        }
    }
    serde_json::json!({
        "command": "logic frontier --why-not",
        "input": input.display().to_string(),
        "action": target,
        "entry": entry,
        "verdicts": entry_verdicts,
        "axis_witnesses": axes,
        "step": step,
        "capability_gaps": gaps,
        "proposals": proposals,
        "facts": rows_json(rows),
    })
}

/// One attempt's settled position at the external-effect boundary, computed ONCE and then
/// rendered as text or as JSON.
///
/// The four outcomes are kept apart deliberately: a receipted attempt owes nothing, a
/// foreclosed one owes an escalation no probe can substitute for, an undetermined one owes
/// a reconciliation, and an attempt with neither record owes an authoring fix. Collapsing
/// any pair sends an operator to the wrong remedy.
struct SagaAttempt {
    attempt: String,
    /// The dispatch intent this attempt executes, when the graph names one.
    ///
    /// `None` is a first-class case, not a hole: an attempt reached only through the
    /// outcome, retry, or licence edges that point AT it carries no `logic:attemptOfIntent`
    /// of its own. Reading the roster off `attemptOfIntent` alone made exactly those
    /// attempts invisible — the graph rendered as zero bytes even though every record in it
    /// was about an unsettled effect.
    intent: Option<String>,
    /// The stable outcome tag: `receipted`, `FORECLOSED`, `UNDETERMINED`, or
    /// `no-effect-record`.
    outcome: &'static str,
    /// What must happen before anything else may, or `None` when nothing is owed.
    owed: Option<String>,
    /// A reconciliation probe has been issued for this attempt.
    probed: bool,
    /// Every retry dispatched against this attempt, with the licence it rides.
    retries: Vec<SagaRetry>,
}

/// One retry dispatched against an attempt, and the idempotency licence it rides.
///
/// A retry is not merely another record about the attempt: it is the one move that can turn
/// an undetermined outcome into a duplicated external effect, and whether it is safe is
/// decided by the licence — not by the licence's PRESENCE (a borrowed one is present too)
/// but by whether the licence covers THIS attempt. The reader that says what is owed must
/// carry that relation or an operator reads "retry, licensed" off a double charge.
struct SagaRetry {
    retry: String,
    /// The `logic:retryLicence` this retry names, or `None` when it names none.
    licence: Option<String>,
    /// The attempts that licence declares it covers (`logic:licenceCoversAttempt`).
    covers: Vec<String>,
    /// The licence names at least one attempt and NONE of them is the retried attempt — a
    /// licence borrowed from a neighbouring dispatch. Its idempotency key is a different
    /// key, so the provider sees a fresh request and the effect happens twice.
    borrowed: bool,
}

/// The `logic:` classes that make a graph an external-effect-boundary graph.
///
/// Used ONLY to tell "this input has no effect boundary at all" (a true answer, printed and
/// exited 0) apart from "this input is full of effect records and the roster is still empty"
/// (a defect in this reader, which must fail closed rather than print nothing).
const SAGA_RECORD_TYPES: [&str; 9] = [
    "EffectAttempt",
    "DispatchIntent",
    "EffectReceipt",
    "ExternalOutcomeUnknown",
    "ExternalOutcomeImpossible",
    "RetryDispatch",
    "IdempotencyContract",
    "ReconciliationProbe",
    "ReconciliationResult",
];

/// The `logic:` edges that NAME an attempt as their object.
///
/// `logic:attemptOfIntent` is handled apart because the attempt is its SUBJECT.
const SAGA_ATTEMPT_EDGES: [&str; 6] = [
    "receiptOfAttempt",
    "unknownOfAttempt",
    "impossibleOfAttempt",
    "probesAttempt",
    "retryOfAttempt",
    "licenceCoversAttempt",
];

/// `gmeow logic saga` — the external-effect boundary status.
pub fn logic_saga(reporter: &dyn Reporter, input: &Path, format: OutputFormat) -> i32 {
    const CODE: &str = "gmeow-cli.logic-saga";
    let rows = match logic_derive(reporter, input, CODE) {
        Ok(r) => r,
        Err(rc) => return rc,
    };

    let mut output = String::new();
    let code = render_logic_saga(reporter, input, &rows, format, &mut output);
    print!("{output}");
    code
}

fn render_logic_saga(
    reporter: &dyn Reporter,
    input: &Path,
    rows: &[FactRow],
    format: OutputFormat,
    output: &mut String,
) -> i32 {
    const CODE: &str = "gmeow-cli.logic-saga";
    let attempts = iri_pairs(rows, &format!("{LOGIC_NS}attemptOfIntent"));
    let receipts = iri_pairs(rows, &format!("{LOGIC_NS}receiptOfAttempt"));
    let unknowns = iri_pairs(rows, &format!("{LOGIC_NS}unknownOfAttempt"));
    // The foreclosed position. It is NOT an unknown: an unknown is not-yet-probed and
    // still resolvable, this one has no reconciliation semantics left to reach for. The
    // two owe different next actions, so the reader that says what is owed must tell
    // them apart or it silently sends an operator after evidence that cannot be got.
    let foreclosed = iri_pairs(rows, &format!("{LOGIC_NS}impossibleOfAttempt"));
    let probes = iri_pairs(rows, &format!("{LOGIC_NS}probesAttempt"));
    let all_verdicts = iri_pairs(rows, &format!("{LOGIC_NS}reconciliationVerdict"));
    let retry_of = iri_pairs(rows, &format!("{LOGIC_NS}retryOfAttempt"));
    let retry_licence = iri_pairs(rows, &format!("{LOGIC_NS}retryLicence"));
    let licence_covers = iri_pairs(rows, &format!("{LOGIC_NS}licenceCoversAttempt"));
    let types = iri_pairs(rows, RDF_TYPE_IRI);

    // THE ROSTER. An attempt is owed an answer no matter which edge reaches it: its own
    // `logic:attemptOfIntent`, an outcome record naming it, a retry re-sending it, a probe
    // asking after it, a licence claiming to cover it, or nothing but its `rdf:type`.
    // Enumerating only `attemptOfIntent` is what made a graph carrying two attempts, an
    // undetermined outcome, a retry, and a borrowed licence print zero bytes and exit 0.
    let mut roster: BTreeSet<String> = attempts.iter().map(|(a, _)| a.clone()).collect();
    for edge in SAGA_ATTEMPT_EDGES {
        for (_, attempt) in iri_pairs(rows, &format!("{LOGIC_NS}{edge}")) {
            roster.insert(attempt);
        }
    }
    let attempt_type = format!("{LOGIC_NS}EffectAttempt");
    for (subject, ty) in &types {
        if *ty == attempt_type {
            roster.insert(subject.clone());
        }
    }

    // Whether the input is an effect-boundary graph AT ALL — read from the record types it
    // carries, independent of the roster. The two conditions answer different questions and
    // are never collapsed: an input with no boundary records has a true answer ("there is no
    // boundary here"), while an input full of boundary records whose roster came out empty
    // is this reader failing, and must say so with a non-zero exit rather than print nothing.
    let record_types: Vec<&str> = SAGA_RECORD_TYPES
        .iter()
        .copied()
        .filter(|local| {
            let iri = format!("{LOGIC_NS}{local}");
            types.iter().any(|(_, ty)| *ty == iri)
        })
        .collect();

    if roster.is_empty() {
        if record_types.is_empty() {
            if matches!(format, OutputFormat::Json) {
                return render_json(
                    output,
                    reporter,
                    CODE,
                    &serde_json::json!({
                        "command": "logic saga",
                        "input": input.display().to_string(),
                        "has_effect_records": false,
                        "attempts": [],
                        "reconciliation_verdicts": [],
                        "facts": rows_json(rows),
                    }),
                );
            }
            writeln!(output, "no external-effect records in {}", input.display())
                .expect("writing to String is infallible");
            return 0;
        }
        return fail(
            reporter,
            CODE,
            format!(
                "{} carries external-effect records ({}) but names no logic:EffectAttempt for \
                 any of them — nothing can be said about what is owed, and saying nothing is \
                 not an answer. Give every record an attempt to hang on.",
                input.display(),
                record_types.join(", ")
            ),
        );
    }

    let mut settled_attempts: Vec<SagaAttempt> = Vec::new();
    for attempt in &roster {
        let intent = attempts
            .iter()
            .find(|(a, _)| a == attempt)
            .map(|(_, i)| i.clone());
        // The retries dispatched against THIS attempt, each carrying the licence it rides and
        // the attempts that licence actually covers.
        let retries: Vec<SagaRetry> = retry_of
            .iter()
            .filter(|(_, a)| a == attempt)
            .map(|(retry, _)| {
                let licence = retry_licence
                    .iter()
                    .find(|(r, _)| r == retry)
                    .map(|(_, l)| l.clone());
                let covers: Vec<String> = licence
                    .iter()
                    .flat_map(|l| {
                        licence_covers
                            .iter()
                            .filter(move |(c, _)| c == l)
                            .map(|(_, a)| a.clone())
                    })
                    .collect();
                let borrowed = !covers.is_empty() && !covers.iter().any(|c| c == attempt);
                SagaRetry {
                    retry: retry.clone(),
                    licence,
                    covers,
                    borrowed,
                }
            })
            .collect();
        let settled = receipts.iter().any(|(_, a)| a == attempt);
        let undetermined = unknowns.iter().any(|(_, a)| a == attempt);
        let unresolvable = foreclosed.iter().any(|(_, a)| a == attempt);
        let probed = probes.iter().any(|(_, a)| a == attempt);
        let (outcome, owed) = if settled {
            ("receipted", None)
        } else if unresolvable {
            (
                "FORECLOSED",
                Some(
                    "ESCALATION — the means of determining what happened no longer exist, so \
                     no probe can settle this and no retry is licensed. This is not an \
                     undetermined outcome awaiting reconciliation; it is one that can never be \
                     reconciled, and it needs a human decision on record."
                        .to_owned(),
                ),
            )
        } else if undetermined && probed {
            // The verdicts are reported at the DOCUMENT level, not per attempt: the shipped
            // vocabulary hangs `logic:reconciliationVerdict` on a `logic:ReconciliationResult`
            // that carries no edge back to the probe or the attempt, so there is no join to
            // make and claiming one would be inventing it.
            let joined = all_verdicts
                .iter()
                .map(|(_, v)| short(v))
                .collect::<Vec<_>>()
                .join(", ");
            (
                "UNDETERMINED",
                Some(format!(
                    "reconciliation probe issued; verdict: {}",
                    if joined.is_empty() {
                        "pending"
                    } else {
                        &joined
                    }
                )),
            )
        } else if undetermined {
            (
                "UNDETERMINED",
                Some(
                    "RECONCILIATION — the provider must be asked what happened. Retrying \
                     blindly risks a duplicate effect; compensating blindly reverses an effect \
                     that may never have occurred."
                        .to_owned(),
                ),
            )
        } else {
            ("no-effect-record", None)
        };
        // A borrowed licence outranks every other thing this attempt owes. The retry is
        // already dispatched, so the reconciliation the outcome asks for is no longer the
        // next move: stopping the re-send is, before the provider sees a second key and
        // performs the effect twice. Folding it into `owed` (rather than printing it as a
        // footnote) is what keeps the single line an operator reads true.
        let owed = if retries.iter().any(|r| r.borrowed) {
            let borrowed: Vec<String> = retries
                .iter()
                .filter(|r| r.borrowed)
                .map(|r| {
                    format!(
                        "{} rides {}, which covers {}",
                        short(&r.retry),
                        r.licence.as_deref().map_or("no licence", |l| short(l)),
                        r.covers
                            .iter()
                            .map(|c| short(c))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                })
                .collect();
            Some(format!(
                "STOP THE RETRY — a retry of this attempt rides a BORROWED idempotency \
                 licence ({}), not one covering this attempt. A borrowed licence reads \
                 identically to a real one and protects nothing: its key is a different key, \
                 so the provider sees a fresh request and the effect happens twice.{}",
                borrowed.join("; "),
                owed.map_or_else(String::new, |o| format!(" Then: {o}"))
            ))
        } else {
            owed
        };
        settled_attempts.push(SagaAttempt {
            attempt: attempt.clone(),
            intent,
            outcome,
            owed,
            probed,
            retries,
        });
    }

    if matches!(format, OutputFormat::Json) {
        let attempts_json: Vec<serde_json::Value> = settled_attempts
            .iter()
            .map(|a| {
                serde_json::json!({
                    "attempt": a.attempt,
                    "intent": a.intent,
                    "outcome": a.outcome,
                    "owed": a.owed,
                    "probed": a.probed,
                    "retries": a.retries
                        .iter()
                        .map(|r| serde_json::json!({
                            "retry": r.retry,
                            "licence": r.licence,
                            "licence_covers": r.covers,
                            "borrowed_licence": r.borrowed,
                        }))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        return render_json(
            output,
            reporter,
            CODE,
            &serde_json::json!({
                "command": "logic saga",
                "input": input.display().to_string(),
                "has_effect_records": true,
                "attempts": attempts_json,
                "reconciliation_verdicts": all_verdicts
                    .iter()
                    .map(|(result, verdict)| serde_json::json!({
                        "result": result,
                        "verdict": verdict,
                    }))
                    .collect::<Vec<_>>(),
                "facts": rows_json(rows),
            }),
        );
    }

    for a in &settled_attempts {
        writeln!(output, "attempt: {}", short(&a.attempt))
            .expect("writing to String is infallible");
        // An attempt the graph never joins to a dispatch intent says so, rather than being
        // dropped from the roster — the join is what is missing, not the attempt.
        writeln!(
            output,
            "  intent:      {}",
            a.intent
                .as_deref()
                .map_or("none — no logic:attemptOfIntent names this attempt", short)
        )
        .expect("writing to String is infallible");
        match a.outcome {
            "no-effect-record" => writeln!(
                output,
                "  outcome:     no receipt and no unknown-outcome record"
            )
            .expect("writing to String is infallible"),
            other => {
                writeln!(output, "  outcome:     {other}").expect("writing to String is infallible")
            }
        }
        for r in &a.retries {
            writeln!(
                output,
                "  retry:       {} (licence: {}{})",
                short(&r.retry),
                r.licence.as_deref().map_or("none", |l| short(l)),
                if r.covers.is_empty() {
                    String::new()
                } else {
                    format!(
                        ", covers {}{}",
                        r.covers
                            .iter()
                            .map(|c| short(c))
                            .collect::<Vec<_>>()
                            .join(", "),
                        if r.borrowed { " — BORROWED" } else { "" }
                    )
                }
            )
            .expect("writing to String is infallible");
        }
        if let Some(owed) = &a.owed {
            writeln!(output, "  owed:        {owed}").expect("writing to String is infallible");
        }
    }
    0
}

/// The label an operator reads for one derived rejection kind.
fn rejection_label(kind: gmeow_logic::RejectionKind) -> &'static str {
    match kind {
        gmeow_logic::RejectionKind::Precondition => "precondition",
        gmeow_logic::RejectionKind::Capability => "capability",
        gmeow_logic::RejectionKind::Resource => "resource",
        gmeow_logic::RejectionKind::Approval => "approval",
    }
}

/// Print one chase witness: the authored rule that concluded a row, and the premises it
/// concluded it from.
///
/// A roster row without this is an assertion; with it, the operator can follow the same
/// derivation the engine made. The premises are the chase's own, never reconstructed
/// here.
fn render_refine_witness(output: &mut String, indent: &str, witness: &gmeow_logic::ProofWitness) {
    writeln!(
        output,
        "{indent}derived by <{}> (proof height {})",
        witness.rule_iri, witness.proof_height
    )
    .expect("writing to String is infallible");
    for (subject, predicate, object) in &witness.premises {
        writeln!(output, "{indent}  from {subject} <{predicate}> {object}")
            .expect("writing to String is infallible");
    }
}

/// One chase witness as JSON: the rule that concluded the row, its proof height, and the
/// premises it fired on — the same three things the text rendering prints, so a consumer
/// can re-run the derivation rather than take the roster on trust.
fn refine_witness_json(witness: &gmeow_logic::ProofWitness) -> serde_json::Value {
    serde_json::json!({
        "rule_iri": witness.rule_iri,
        "proof_height": witness.proof_height,
        "premises": witness
            .premises
            .iter()
            .map(|(subject, predicate, object)| serde_json::json!({
                "subject": subject,
                "predicate": predicate,
                "object": object,
            }))
            .collect::<Vec<_>>(),
    })
}

/// `gmeow logic refine` — bounded, chase-derived means–end refinement.
///
/// Every line printed below is a row of the reasoner's closure: the roster, the
/// transitive expansion and each typed rejection are conclusions the authored `logic:`
/// rules drew, each carrying the rule that drew it. Nothing here searches.
pub fn logic_refine(
    reporter: &dyn Reporter,
    input: &Path,
    task: &str,
    fragment: &str,
    budget: u32,
    format: OutputFormat,
) -> i32 {
    const CODE: &str = "gmeow-cli.logic-refine";
    let bytes = match std::fs::read(input) {
        Ok(bytes) => bytes,
        Err(e) => {
            return fail(
                reporter,
                CODE,
                format!("cannot read {}: {e}", input.display()),
            );
        }
    };
    let media = match input.extension().and_then(|e| e.to_str()) {
        Some("nq") => "application/n-quads",
        Some("nt") => "application/n-triples",
        Some("trig") => "application/trig",
        _ => "text/turtle",
    };
    let parsed = match purrdf::parse_dataset(&bytes, media, None) {
        Ok(parsed) => parsed,
        Err(e) => {
            return fail(
                reporter,
                CODE,
                format!("cannot parse {}: {e}", input.display()),
            );
        }
    };

    let rules = match operator_rules(reporter, CODE) {
        Ok(rules) => rules,
        Err(code) => return code,
    };
    report_operator_source_diagnostics(reporter, CODE, rules.diagnostics());
    let report = gmeow_logic::refine(parsed.as_ref(), task, fragment, budget, rules);

    let mut output = String::new();
    let code = render_logic_refine(
        reporter,
        input,
        &gmeow_logic::operator::refinement::RefinementRecord::from(&report),
        budget,
        format,
        &mut output,
    );
    print!("{output}");
    code
}

fn render_logic_refine(
    reporter: &dyn Reporter,
    input: &Path,
    report: &gmeow_logic::operator::refinement::RefinementRecord,
    budget: u32,
    format: OutputFormat,
    output: &mut String,
) -> i32 {
    use gmeow_logic::operator::refinement::RefinementOutcome;
    let task = report.task.as_str();
    let fragment = report.fragment.as_str();
    const CODE: &str = "gmeow-cli.logic-refine";
    // The two outcomes that are ANSWERS about the method set — a closed roster and a
    // budget-cut one — are the two the structured mode serializes. Every other outcome is
    // a refusal, and a refusal travels the console's error rail (stderr, exit 1, an NDJSON
    // `finding` line under `--console jsonl`) rather than being dressed as a result
    // document a consumer could mistake for a roster.
    if matches!(format, OutputFormat::Json) {
        let (status, detail) = match &report.outcome {
            RefinementOutcome::Applied { consumed_steps, .. } => (
                "CLOSED",
                serde_json::json!({
                    "complete_for_fragment": fragment,
                    "derivations": consumed_steps,
                }),
            ),
            RefinementOutcome::Incomplete { status, cause } => (
                "INCOMPLETE",
                serde_json::json!({
                    "cut_status": format!("{status:?}"),
                    "cause": cause.to_string(),
                    "budget": budget,
                    "note": "the derivation was cut, so NO roster is reported: a partial roster \
                             presented here would be read as the roster",
                }),
            ),
            _ => ("", serde_json::Value::Null),
        };
        if !status.is_empty() {
            let closed = status == "CLOSED";
            return render_json(
                output,
                reporter,
                CODE,
                &serde_json::json!({
                    "command": "logic refine",
                    "input": input.display().to_string(),
                    "task": task,
                    "fragment": fragment,
                    "budget": budget,
                    "status": status,
                    "outcome": detail,
                    "candidates": if closed {
                        report.candidates.iter().map(|c| serde_json::json!({
                            "task": c.task,
                            "method": c.method,
                            "steps": c.steps,
                            "open_steps": c.open_steps,
                            "witness": refine_witness_json(&c.witness),
                        })).collect::<Vec<_>>()
                    } else { Vec::new() },
                    "rejections": if closed {
                        report.rejections.iter().map(|r| serde_json::json!({
                            "step": r.step,
                            "kind": rejection_label(r.kind),
                            "witness_iri": r.witness_iri,
                            "witness": refine_witness_json(&r.witness),
                        })).collect::<Vec<_>>()
                    } else { Vec::new() },
                    "reached": if closed { report.reached.clone() } else { Vec::new() },
                    // The PIN half of "validate the selected refinement and pin its
                    // executable subgraph". Empty when nothing in scope was authorized —
                    // which is the informative absence logic:selectedPin's own definition
                    // names, not a missing field.
                    "pins": if closed {
                        report.pins.iter().map(|p| serde_json::json!({
                            "episode": p.episode,
                            "pin": p.pin,
                            "method": p.method,
                            "steps": p.steps,
                            "digest": p.digest,
                            "authority": p.authority,
                            "witness": refine_witness_json(&p.witness),
                        })).collect::<Vec<_>>()
                    } else { Vec::new() },
                }),
            );
        }
    }

    match &report.outcome {
        RefinementOutcome::UnsupportedFragment { kind } => {
            // Out of fragment is a REFUSAL, not a thin result: exiting 0 with an empty
            // candidate list would read as "no decomposition exists", which is a
            // different and much more comforting claim than "your method set does not
            // terminate".
            let cycles = report
                .cycles
                .iter()
                .map(|task| format!("<{task}>"))
                .collect::<Vec<_>>()
                .join(", ");
            return fail(
                reporter,
                CODE,
                format!(
                    "method set is outside the declared search fragment <{fragment}> \
                     ({kind}): decomposition cycle — {cycles} {} reachable from \
                     {} through the authored method set, so no expansion terminates. No \
                     budget increase would help; the method set needs fixing.",
                    if report.cycles.len() == 1 {
                        "is"
                    } else {
                        "are"
                    },
                    if report.cycles.len() == 1 {
                        "itself"
                    } else {
                        "themselves"
                    },
                ),
            );
        }
        RefinementOutcome::Invalid { fault } => {
            // A malformed request must not be answered with a clean empty roster: that
            // reads as "nothing decomposes this", which is both wrong and reassuring.
            return fail(
                reporter,
                CODE,
                format!("the refinement request is invalid: {fault}"),
            );
        }
        RefinementOutcome::EngineFailure { diagnostic } => {
            return fail(reporter, CODE, format!("engine failure: {diagnostic}"));
        }
        RefinementOutcome::Applied { consumed_steps, .. } => {
            writeln!(output, "task:        <{task}>").expect("writing to String is infallible");
            writeln!(
                output,
                "status:      CLOSED (complete for fragment <{fragment}>)"
            )
            .expect("writing to String is infallible");
            writeln!(output, "derivations: {}", consumed_steps)
                .expect("writing to String is infallible");
        }
        RefinementOutcome::Incomplete { status, cause } => {
            writeln!(output, "task:        <{task}>").expect("writing to String is infallible");
            writeln!(
                output,
                "status:      INCOMPLETE — the derivation was cut ({status:?}, {cause}) under \
                 a budget of {budget}"
            )
            .expect("writing to String is infallible");
            writeln!(
                output,
                "             No roster is shown: this run is NOT closed, and a partial"
            )
            .expect("writing to String is infallible");
            writeln!(
                output,
                "             roster presented here would be read as the roster."
            )
            .expect("writing to String is infallible");
            return 0;
        }
        RefinementOutcome::Unsettled { detail } => {
            // Every remaining outcome is a routing verdict about the program, not an
            // answer about the method set, and printing a roster under one would attribute
            // a result to a run that never committed.
            return fail(
                reporter,
                CODE,
                format!("the refinement did not settle: {detail}"),
            );
        }
    }

    writeln!(output, "candidates:  {}", report.candidates.len())
        .expect("writing to String is infallible");
    for (i, candidate) in report.candidates.iter().enumerate() {
        writeln!(
            output,
            "  [{i}] <{}> via <{}>",
            candidate.task, candidate.method
        )
        .expect("writing to String is infallible");
        writeln!(output, "       steps: {}", candidate.steps.join(" -> "))
            .expect("writing to String is infallible");
        if !candidate.open_steps.is_empty() {
            writeln!(
                output,
                "       open:  {} (decomposed further below)",
                candidate.open_steps.join(", ")
            )
            .expect("writing to String is infallible");
        }
        render_refine_witness(output, "       ", &candidate.witness);
    }

    writeln!(output, "rejections:  {}", report.rejections.len())
        .expect("writing to String is infallible");
    for (i, rejection) in report.rejections.iter().enumerate() {
        writeln!(
            output,
            "  [{i}] <{}> rejected on {}: <{}>",
            rejection.step,
            rejection_label(rejection.kind),
            rejection.witness_iri
        )
        .expect("writing to String is infallible");
        render_refine_witness(output, "       ", &rejection.witness);
    }

    writeln!(output, "reached:     {}", report.reached.len())
        .expect("writing to String is infallible");
    for step in &report.reached {
        writeln!(output, "  <{step}>").expect("writing to String is infallible");
    }

    // The PIN half. A refinement that validated a selection and froze nothing has done
    // half of what it claims to do, so the count is printed even at zero: an operator must
    // be able to tell "no candidate was authorized" from "pins are not reported here".
    writeln!(output, "pins:        {}", report.pins.len())
        .expect("writing to String is infallible");
    for (i, pin) in report.pins.iter().enumerate() {
        writeln!(
            output,
            "  [{i}] <{}> selected by <{}>",
            pin.pin, pin.episode
        )
        .expect("writing to String is infallible");
        writeln!(output, "       instantiates: <{}>", pin.method)
            .expect("writing to String is infallible");
        writeln!(output, "       frozen steps: {}", pin.steps.join(" -> "))
            .expect("writing to String is infallible");
        writeln!(output, "       digest:       {}", pin.digest)
            .expect("writing to String is infallible");
        writeln!(output, "       authority:    <{}>", pin.authority)
            .expect("writing to String is infallible");
        render_refine_witness(output, "       ", &pin.witness);
    }
    0
}

/// R3.5's five explanation elements: the graph predicate and the heading it is read under.
///
/// Named once so the text and JSON renderings can never disagree about which five they
/// are — a report silently missing one still looks complete.
const EXPLANATION_ELEMENTS: [(&str, &str); 5] = [
    ("explanationProof", "proof"),
    ("explanationEvidence", "evidence"),
    ("explanationPolicy", "policy"),
    ("explanationCriterion", "criterion"),
    ("explanationDissent", "dissent"),
];

/// The DERIVED contestations bearing on one action: pairs of statement attributions that
/// take opposite stances on a statement the action is part of.
///
/// This is the surfaced end of the RDF 1.2 statement-metadata lowering. Each row exists
/// only because `logic:ruleAttributionContestedByOpposingVantage` could join two reifiers
/// through `logic:reifiedStatementSubject` / `Predicate` / `Object` — edges that did not
/// exist before the lowering, and without which the rule's two halves share no variable at
/// all. It is the derived counterpart of the AUTHORED `gmeow:explanationDissent`: the
/// authored one is somebody's note that there was an objection, this one is the objection
/// standing in the graph against the very claim the recommendation rests on.
fn derived_contestations(rows: &[FactRow], action: &str) -> Vec<(String, String, String, String)> {
    let subjects = iri_pairs(rows, &format!("{LOGIC_NS}reifiedStatementSubject"));
    let predicates = iri_pairs(rows, &format!("{LOGIC_NS}reifiedStatementPredicate"));
    let objects = iri_pairs(rows, &format!("{LOGIC_NS}reifiedStatementObject"));
    let component = |pairs: &[(String, String)], reifier: &str| -> Option<String> {
        pairs
            .iter()
            .find(|(r, _)| r == reifier)
            .map(|(_, value)| value.clone())
    };
    let mut out: Vec<(String, String, String, String)> = Vec::new();
    for (affirming, opposing) in iri_pairs(rows, &format!("{LOGIC_NS}contestedByAttribution")) {
        let (Some(subject), Some(predicate), Some(object)) = (
            component(&subjects, &affirming),
            component(&predicates, &affirming),
            component(&objects, &affirming),
        ) else {
            continue;
        };
        // Scoped to the action: a disagreement elsewhere in the graph is not this
        // recommendation's business, and attributing one to it would be the mirror image
        // of hiding the one that IS.
        if subject != action && object != action {
            continue;
        }
        out.push((
            affirming,
            opposing,
            subject,
            format!("{predicate} {object}"),
        ));
    }
    out.sort();
    out.dedup();
    out
}

/// `gmeow logic explain` — R3.5's five elements for one action.
pub fn logic_explain(
    reporter: &dyn Reporter,
    input: &Path,
    action: &str,
    format: OutputFormat,
) -> i32 {
    const CODE: &str = "gmeow-cli.logic-explain";
    let rows = match logic_derive(reporter, input, CODE) {
        Ok(r) => r,
        Err(rc) => return rc,
    };

    let mut output = String::new();
    let code = render_logic_explain(reporter, input, &rows, action, format, &mut output);
    print!("{output}");
    code
}

fn render_logic_explain(
    reporter: &dyn Reporter,
    input: &Path,
    rows: &[FactRow],
    action: &str,
    format: OutputFormat,
    output: &mut String,
) -> i32 {
    const CODE: &str = "gmeow-cli.logic-explain";
    const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
    // The action may be named directly or through the entry that positions it.
    let entries: Vec<String> = iri_pairs(rows, &format!("{LOGIC_NS}entryAction"))
        .into_iter()
        .filter(|(_, step)| step == action)
        .map(|(e, _)| e)
        .chain(std::iter::once(action.to_owned()))
        .collect();

    let explains = iri_pairs(rows, &format!("{GMEOW_NS}explainsEntry"));

    if matches!(format, OutputFormat::Json) {
        let matched: Vec<&(String, String)> = explains
            .iter()
            .filter(|(_, entry)| entries.contains(entry))
            .collect();
        if matched.is_empty() {
            return fail(
                reporter,
                CODE,
                format!("no gmeow:FrontierExplanation explains <{action}>"),
            );
        }
        let all_verdicts = label_verdicts(rows);
        let explanations: Vec<serde_json::Value> = matched
            .iter()
            .map(|(explanation, entry)| {
                let mut elements = serde_json::Map::new();
                for (field, heading) in EXPLANATION_ELEMENTS {
                    let predicate = format!("{GMEOW_NS}{field}");
                    // An element with no value is emitted as an EMPTY array rather than
                    // omitted: "recorded nothing" and "the key is not in this document"
                    // are different claims, and dissent is the one most easily lost.
                    let values: Vec<serde_json::Value> = rows
                        .iter()
                        .filter(|r| {
                            r.predicate == predicate
                                && r.subject.iri() == Some(explanation.as_str())
                        })
                        .map(|r| r.object.to_json())
                        .collect();
                    elements.insert(heading.to_owned(), values.into());
                }
                serde_json::json!({
                    "explanation": explanation,
                    "entry": entry,
                    "label_verdicts": all_verdicts
                        .iter()
                        .filter(|(e, _)| e == entry)
                        .map(|(e, v)| v.to_json(e))
                        .collect::<Vec<_>>(),
                    "elements": serde_json::Value::Object(elements),
                })
            })
            .collect();
        return render_json(
            output,
            reporter,
            CODE,
            &serde_json::json!({
                "command": "logic explain",
                "input": input.display().to_string(),
                "action": action,
                "explanations": explanations,
                // The DERIVED counterpart of the authored dissent element, and the
                // surfaced end of the RDF 1.2 statement-metadata lowering.
                "contested": derived_contestations(rows, action)
                    .into_iter()
                    .map(|(affirming, opposing, subject, rest)| serde_json::json!({
                        "affirming_attribution": affirming,
                        "opposing_attribution": opposing,
                        "statement": format!("{subject} {rest}"),
                        "derived_by": format!(
                            "{LOGIC_NS}ruleAttributionContestedByOpposingVantage"
                        ),
                    }))
                    .collect::<Vec<_>>(),
                "facts": rows_json(rows),
            }),
        );
    }

    let mut found = false;
    for (explanation, entry) in &explains {
        if !entries.contains(entry) {
            continue;
        }
        found = true;
        writeln!(output, "action:      {action}").expect("writing to String is infallible");
        writeln!(output, "entry:       {entry}").expect("writing to String is infallible");
        // The label is re-derived here so an explanation cannot disagree with the frontier
        // it explains — and when the input DOES disagree, the explanation says so rather
        // than quietly adopting the author's word.
        for (_, verdict) in label_verdicts(rows).iter().filter(|(e, _)| e == entry) {
            match verdict {
                LabelVerdict::Derived { label, .. } => {
                    writeln!(output, "label:       {}   (derived)", short(label))
                        .expect("writing to String is infallible");
                }
                LabelVerdict::AssertedUnchecked { asserted } => {
                    writeln!(
                        output,
                        "label:       {}   (ASSERTED — no shipped logic:Rule derives a label \
                         for this entry, so nothing has verified this value)",
                        short(asserted)
                    )
                    .expect("writing to String is infallible");
                }
                LabelVerdict::Disagreement { derived, asserted } => {
                    writeln!(
                        output,
                        "DISAGREEMENT: the input asserts logic:entryLabel {}, which the shipped \
                         rule set does NOT derive from this entry's axis witnesses; the derived \
                         label {} is authoritative",
                        short(asserted),
                        short(derived)
                    )
                    .expect("writing to String is infallible");
                }
            }
        }
        // R3.5's five elements, each named so a missing one is visible as missing
        // rather than silently absent from an otherwise plausible-looking report.
        for (field, heading) in EXPLANATION_ELEMENTS {
            let vals: Vec<String> = rows
                .iter()
                .filter(|r| {
                    r.subject.iri() == Some(explanation.as_str())
                        && r.predicate == format!("{GMEOW_NS}{field}")
                })
                .map(|r| r.object.display())
                .collect();
            if vals.is_empty() {
                writeln!(output, "{heading:<12} <none recorded>")
                    .expect("writing to String is infallible");
            } else {
                for v in vals {
                    writeln!(output, "{heading:<12} {v}").expect("writing to String is infallible");
                }
            }
        }
        // The DERIVED sixth line. Printed even at zero, because "no vantage contests the
        // claim this rests on" and "contestation is not reported here" are different
        // things, and the second is what an operator assumes when a section is missing.
        let contested = derived_contestations(rows, action);
        if contested.is_empty() {
            writeln!(output, "contested    <none derived>")
                .expect("writing to String is infallible");
        } else {
            for (affirming, opposing, subject, rest) in &contested {
                writeln!(output, "contested    {subject} {rest}")
                    .expect("writing to String is infallible");
                writeln!(output, "             affirmed by {affirming}")
                    .expect("writing to String is infallible");
                writeln!(output, "             opposed  by {opposing}")
                    .expect("writing to String is infallible");
                writeln!(
                    output,
                    "             (derived by <{LOGIC_NS}ruleAttributionContestedByOpposing\
                     Vantage> over the lowered RDF 1.2 statement components)"
                )
                .expect("writing to String is infallible");
            }
        }
    }

    if !found {
        return fail(
            reporter,
            CODE,
            format!("no gmeow:FrontierExplanation explains <{action}>"),
        );
    }
    0
}

#[cfg(test)]
mod operator_corpus_tests;
