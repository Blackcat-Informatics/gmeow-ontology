// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Installed knowledge, reasoning and validation command bodies.

use super::*;

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
    // must never be fabricated).
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

/// The hot store medium, resolved from the embedded bundle exactly as the MCP engine resolves
/// it — the CLI and the tool run one implementation, so they prime one way.
fn hot_store_medium() -> gmeow_errors::Result<gmeow_mcp::StoreMedium> {
    gmeow_mcp::store_medium(BUNDLE_GTS, gmeow_mcp::MEMORY_HOT_DICTIONARY)
}

/// `gmeow conjecture test` — test a candidate `logic:` formula against a KB in an
/// isolated, standpoint-scoped scenario world, print the engine verdict, and —
/// unless `--dry-run` — APPEND it to the append-only conjecture library. Delegates
/// to the SHARED [`gmeow_mcp::run_conjecture_test`] core (the same path
/// the MCP `conjecture_test` tool runs), so there is one implementation, not two.
#[allow(clippy::too_many_arguments)]
pub fn conjecture_test(
    reporter: &dyn Reporter,
    artifact_path: &Path,
    formula: &Path,
    kb: &Path,
    standpoint: &str,
    math_conjecture: Option<&str>,
    dry_run: bool,
    max_steps: Option<u64>,
    max_answers: Option<usize>,
) -> i32 {
    let artifact_path = match crate::output::resolve_path(artifact_path) {
        Ok(path) => path,
        Err(error) => {
            return fail(
                reporter,
                "gmeow-cli.conjecture.output",
                format!(
                    "cannot resolve artifact output {}: {error}",
                    artifact_path.display()
                ),
            );
        }
    };
    let library = match gmeow_mcp::storage::storage()
        .conjecture_library()
        .and_then(|library| {
            library.ensure_distinct_output(&artifact_path)?;
            Ok(library)
        }) {
        Ok(library) => library,
        Err(error) => {
            return fail(
                reporter,
                "gmeow-cli.conjecture.output",
                format!("cannot select a distinct artifact output: {error}"),
            );
        }
    };
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

    let medium = match hot_store_medium() {
        Ok(medium) => medium,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.conjecture.failed",
                format!("the bundle pins no runtime-store dictionary: {e}"),
            );
        }
    };
    let policy = match gmeow_mcp::prepared_action_policy(BUNDLE_GTS) {
        Ok(policy) => policy,
        Err(error) => {
            return fail(
                reporter,
                "gmeow-cli.action-policy",
                format!("cannot read the required native action policy: {error}"),
            );
        }
    };
    let out = match gmeow_mcp::run_conjecture_test_in(
        &gmeow_mcp::ConjectureRunInput {
            formula_ttl: &formula_ttl,
            kb_ttl: &kb_ttl,
            standpoint,
            math_conjecture,
            dry_run,
            max_steps,
            max_answers,
        },
        &medium,
        &policy,
        Arc::clone(&library),
    ) {
        Ok(out) => out,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.conjecture.failed",
                format!("conjecture test failed: {e}"),
            );
        }
    };

    if let Err(error) = library
        .ensure_distinct_output(&artifact_path)
        .and_then(|()| crate::output::write_bytes(&artifact_path, &out.artifact))
    {
        return fail(
            reporter,
            "gmeow-cli.conjecture.output",
            format!(
                "cannot publish complete GTS artifact {}: {error}; conjecture {}; committed={}; dry_run={}",
                artifact_path.display(),
                out.node_iri,
                out.committed,
                out.dry_run,
            ),
        );
    }

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
        println!("persisted dry-run (library unchanged)");
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
/// library. Delegates to the SHARED [`gmeow_mcp::run_submit_candidate`] core (the same
/// path the MCP `submit_candidate` tool runs), so there is one implementation, not two. A refuted
/// or open candidate is not admitted (a non-zero exit), and `--dry-run` leaves the library
/// unchanged. Every evaluated result writes its complete artifact to the selected output.
#[allow(clippy::too_many_arguments)]
pub fn candidate_submit(
    reporter: &dyn Reporter,
    artifact_path: &Path,
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
    let artifact_path = match crate::output::resolve_path(artifact_path) {
        Ok(path) => path,
        Err(error) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.output",
                format!(
                    "cannot resolve artifact output {}: {error}",
                    artifact_path.display()
                ),
            );
        }
    };
    let library = match gmeow_mcp::storage::storage()
        .candidate_library()
        .and_then(|library| {
            library.ensure_distinct_output(&artifact_path)?;
            Ok(library)
        }) {
        Ok(library) => library,
        Err(error) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.output",
                format!("cannot select a distinct artifact output: {error}"),
            );
        }
    };
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

    let medium = match hot_store_medium() {
        Ok(medium) => medium,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.failed",
                format!("the bundle pins no runtime-store dictionary: {e}"),
            );
        }
    };
    let policy = match gmeow_mcp::prepared_action_policy(BUNDLE_GTS) {
        Ok(policy) => policy,
        Err(error) => {
            return fail(
                reporter,
                "gmeow-cli.action-policy",
                format!("cannot read the required native action policy: {error}"),
            );
        }
    };
    let out = match gmeow_mcp::run_submit_candidate_in(
        &gmeow_mcp::CandidateSubmitInput {
            formula_ttl: &formula_ttl,
            kb_ttl: &kb_ttl,
            kb_format: "text/turtle",
            standpoint,
            math_conjecture,
            for_slice,
            for_packet,
            dry_run,
            max_steps,
            max_answers,
        },
        &medium,
        &policy,
        Arc::clone(&library),
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

    if let Err(error) = library
        .ensure_distinct_output(&artifact_path)
        .and_then(|()| crate::output::write_bytes(&artifact_path, &out.artifact))
    {
        return fail(
            reporter,
            "gmeow-cli.candidate.output",
            format!(
                "cannot publish complete GTS artifact {}: {error}; candidate {}; committed={}; dry_run={}",
                artifact_path.display(),
                out.node_iri,
                out.committed,
                out.dry_run,
            ),
        );
    }
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
        println!("persisted dry-run (library unchanged)");
    } else if out.committed {
        println!("persisted committed");
    } else {
        println!("persisted no");
    }
    0
}

/// `gmeow candidate withdraw` — withdraw a persisted candidate (P10 supersession). Delegates to
/// the SHARED [`gmeow_mcp::run_withdraw_candidate`] core the MCP tool runs. An unknown
/// or already-withdrawn id is a hard error (a non-zero exit).
pub fn candidate_withdraw(
    reporter: &dyn Reporter,
    candidate_id: &str,
    reason: Option<&str>,
    dry_run: bool,
) -> i32 {
    let medium = match hot_store_medium() {
        Ok(medium) => medium,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.candidate.failed",
                format!("the bundle pins no runtime-store dictionary: {e}"),
            );
        }
    };
    let policy = match gmeow_mcp::prepared_action_policy(BUNDLE_GTS) {
        Ok(policy) => policy,
        Err(error) => {
            return fail(
                reporter,
                "gmeow-cli.action-policy",
                format!("cannot read the required native action policy: {error}"),
            );
        }
    };
    let body = match gmeow_mcp::run_withdraw_candidate(
        candidate_id,
        reason.unwrap_or(""),
        dry_run,
        &medium,
        &policy,
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
/// Delegates to the SHARED [`gmeow_mcp::run_list_candidates`] core.
pub fn candidate_list(
    reporter: &dyn Reporter,
    slice: Option<&str>,
    disposition: Option<&str>,
) -> i32 {
    let body = match gmeow_mcp::run_list_candidates(slice, disposition) {
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
/// re-home every RDF record into the isolated [`HYBRID_QUERY_WORLD`] — the
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
    let parsed = purrdf::parse_dataset(&bytes, media, None).map_err(|error| {
        reporter.report(&report_diag(
            Diag::from(error)
                .with_context("parse hybrid-query facts")
                .with_focus(facts.display().to_string()),
            "gmeow",
        ));
        1
    })?;
    let dataset = place_world_dataset(parsed, HYBRID_QUERY_WORLD).map_err(|error| {
        reporter.report(&report_diag(
            error
                .with_context("place hybrid-query facts in the selected world")
                .with_focus(facts.display().to_string()),
            "gmeow",
        ));
        1
    })?;
    let store = WorldStore::new().map_err(|error| {
        reporter.report(&report_diag(
            error.with_context("create hybrid-query world store"),
            "gmeow",
        ));
        1
    })?;
    store.load_dataset(&dataset).map_err(|error| {
        reporter.report(&report_diag(
            error
                .with_context("load hybrid-query facts")
                .with_focus(facts.display().to_string()),
            "gmeow",
        ));
        1
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
    /// RDF 1.2 term kind of the corpus rows (both query and candidate columns):
    /// `iri`, `triple-term`, or `literal`.
    pub term_kind: &'a str,
    /// Ordered-prefix row limit pushed into the provider on each call.
    pub per_call_limit: usize,
    /// Deterministic operation-wide provider call budget.
    pub max_calls: u64,
    /// Deterministic operation-wide provider row budget.
    pub max_rows: u64,
}

/// Mint a typed PURREMB-selection diagnostic (the sole first-party error substrate; a
/// bare `String` error is banned by the Diag-substrate invariant).
fn purremb_selection_err(detail: impl Into<String>) -> Diag {
    Diag::of_kind(crate::error::PurrembSelectionInvalid {
        detail: detail.into(),
    })
}

/// Decode exactly 32 hex bytes into a raw identity array, failing closed on a
/// wrong length or a non-hex character (a bad identity is a hard fail, never a
/// silent reinterpretation).
fn parse_identity_hex(label: &str, hex: &str) -> gmeow_errors::Result<[u8; 32]> {
    let hex = hex.trim();
    if hex.len() != 64 {
        return Err(purremb_selection_err(format!(
            "{label} identity must be 64 hex characters (32 bytes), got {}",
            hex.len()
        )));
    }
    purrdf_hash::hex::decode_32(hex)
        .ok_or_else(|| purremb_selection_err(format!("{label} identity has a non-hex character")))
}

/// Parse the declared distance metric, rejecting anything the retrieval scan has
/// no canonical scoring rule for.
fn parse_metric(value: &str) -> gmeow_errors::Result<DistanceMetric> {
    match value {
        "cosine" => Ok(DistanceMetric::Cosine),
        "negative-dot" => Ok(DistanceMetric::NegativeDot),
        "squared-euclidean" => Ok(DistanceMetric::SquaredEuclidean),
        other => Err(purremb_selection_err(format!(
            "unknown metric '{other}' (expected cosine, negative-dot, or squared-euclidean)"
        ))),
    }
}

/// Parse the declared stored scalar type.
fn parse_dtype(value: &str) -> gmeow_errors::Result<VectorDtype> {
    match value {
        "f32" => Ok(VectorDtype::F32),
        "f64" => Ok(VectorDtype::F64),
        other => Err(purremb_selection_err(format!(
            "unknown dtype '{other}' (expected f32 or f64)"
        ))),
    }
}

/// Parse the effective-space prefix postprocessing policy.
fn parse_postprocessing(value: &str) -> gmeow_errors::Result<PrefixPostprocessing> {
    match value {
        "none" => Ok(PrefixPostprocessing::None),
        "deterministic-l2" => Ok(PrefixPostprocessing::DeterministicL2),
        other => Err(purremb_selection_err(format!(
            "unknown postprocessing '{other}' (expected none or deterministic-l2)"
        ))),
    }
}

/// Parse the selected retrieval branch.
fn parse_retrieval_policy(value: &str) -> gmeow_errors::Result<RetrievalPolicy> {
    match value {
        "exact-full-space" => Ok(RetrievalPolicy::ExactFullSpace),
        "matryoshka-prefix-then-rerank" => Ok(RetrievalPolicy::MatryoshkaPrefixThenRerank),
        other => Err(purremb_selection_err(format!(
            "unknown retrieval policy '{other}' (expected exact-full-space or \
             matryoshka-prefix-then-rerank)"
        ))),
    }
}

/// Resolve the `--term-kind` selector to the corpus's RDF 1.2 column kind, applied to
/// both the query and candidate columns. A `triple-term` corpus is queried with a
/// `<<( s p o )>>` goal term; `literal` selects any-datatype literal targets.
fn parse_term_kind(value: &str) -> gmeow_errors::Result<ColumnKind> {
    match value {
        "iri" => Ok(ColumnKind::Iri),
        "triple-term" => Ok(ColumnKind::TripleTerm),
        "literal" => Ok(ColumnKind::Literal { datatype: None }),
        other => Err(purremb_selection_err(format!(
            "unknown term kind '{other}' (expected iri, triple-term, or literal)"
        ))),
    }
}

/// Build the fully validated [`PurrembSelection`] from the declared hex/enum
/// inputs, failing closed on any malformed identity or unknown code.
fn purremb_selection(args: &PurrembHybridQuery<'_>) -> gmeow_errors::Result<PurrembSelection> {
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
    let term_kind = match parse_term_kind(args.term_kind) {
        Ok(kind) => kind,
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

    let binding = match PurrembBinding::open(&artifact_bytes, &source_bytes, selection) {
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
    // selection so a differing retrieval policy is never cache-aliased.
    let generation_iri =
        purremb_generation_iri(args.generation_base, binding.artifact_root_hex(), policy);

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
        // Both slots carry the corpus term kind: `resolve_mode` chooses the candidate
        // slot per call by bound-ness, so whichever slot is the candidate must gate the
        // emitted target kind correctly.
        vec![term_kind.clone(), term_kind],
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
                let spaces: Vec<String> = annotation
                    .spaces
                    .iter()
                    .map(|space| purrdf_hash::hex::encode(space))
                    .collect();
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
                 source-profile={} target-set={} matrix={} projection={} vector-space={} \
                 family={} metric={} metric-code={} dimension={} postprocessing={} \
                 policy={} recall={} loss={} index-guard={}",
                receipt.artifact_root,
                receipt.source_exact_digest,
                receipt.certified_rdf_digest,
                receipt.source_verification_profile,
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
pub(super) fn parse_rdf_file(
    reporter: &dyn Reporter,
    prefix: &'static str,
    path: &Path,
) -> Result<std::sync::Arc<purrdf::RdfDataset>, i32> {
    match load_reasoning_input(path, "rdf-input", prefix) {
        Ok(loaded) => Ok(loaded.dataset),
        Err(error) => {
            reporter.report(&report_diag(error, "gmeow"));
            Err(1)
        }
    }
}

fn native_program(operation: ReasoningOperation, fragment: &str) -> ProgramIdentity {
    match operation {
        ReasoningOperation::Consistency
        | ReasoningOperation::Classification
        | ReasoningOperation::Realization => ProgramIdentity::selected(
            fragment,
            &[
                operation.as_str(),
                purrdf::entail::DL_CALCULUS_VERSION,
                purrdf::entail::DL_TRUST_BASE_VERSION,
            ],
        ),
        ReasoningOperation::Entailment | ReasoningOperation::AxiomConsistency => {
            let contract = gmeow_logic::reason::native_contract_hash();
            ProgramIdentity::selected(
                fragment,
                &[
                    operation.as_str(),
                    gmeow_logic::counterfactual::SOLVER_VERSION,
                    &contract,
                ],
            )
        }
    }
}

fn reasoning_engine(operation: ReasoningOperation) -> EngineIdentity {
    match operation {
        ReasoningOperation::Consistency
        | ReasoningOperation::Classification
        | ReasoningOperation::Realization => EngineIdentity::purrdf_dl(),
        ReasoningOperation::Entailment | ReasoningOperation::AxiomConsistency => {
            EngineIdentity::gmeow_native()
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn publish_reasoning_failure(
    reporter: &dyn Reporter,
    diagnostic_prefix: &str,
    operation: ReasoningOperation,
    fragment: &str,
    decision: DecisionClass,
    verdict: &str,
    input_status: &str,
    inputs: Vec<InputIdentity>,
    diagnostic: ReportDiagnostic,
    format: OutputFormat,
    gate: bool,
) -> i32 {
    emit_error(
        reporter,
        &format!(
            "{diagnostic_prefix}.{}",
            diagnostic.code.to_ascii_lowercase()
        ),
        diagnostic.detail.clone(),
    );
    NativeReasoningReport {
        common: ReasoningReportCore::refused(
            operation,
            decision,
            verdict,
            input_status,
            inputs,
            native_program(operation, fragment),
            reasoning_engine(operation),
            diagnostic,
        ),
        result: ReasoningPayload::Empty,
    }
    .publish(format, gate)
}

#[allow(clippy::too_many_arguments)]
fn dl_service_failure_report(
    reporter: &dyn Reporter,
    diagnostic_prefix: &str,
    operation: ReasoningOperation,
    fragment: &str,
    input: InputIdentity,
    error: Diag,
    format: OutputFormat,
    gate: bool,
) -> i32 {
    use gmeow_logic::reasoner_services::DlServiceFailureKind;

    let kind = match error.downcast_ref::<gmeow_logic::error::DlService>() {
        Some(failure) => failure.kind,
        None => {
            return publish_reasoning_failure(
                reporter,
                diagnostic_prefix,
                operation,
                fragment,
                DecisionClass::ExecutionFailure,
                "execution-failure",
                "valid",
                vec![input],
                ReportDiagnostic::from_diag(
                    &error.with_context("native DL failure has no service disposition"),
                )
                .with_operation_code("REASONER_FAILURE_CONTRACT_INVALID"),
                format,
                gate,
            );
        }
    };
    let detail = error.message().to_owned();
    match kind {
        DlServiceFailureKind::NoModel => {
            let verdict = if operation == ReasoningOperation::Consistency {
                "false"
            } else {
                "no-model"
            };
            emit_error(
                reporter,
                &format!("{diagnostic_prefix}.no-model"),
                detail.clone(),
            );
            NativeReasoningReport {
                common: ReasoningReportCore {
                    schema_version: 1,
                    operation,
                    verdict: verdict.to_owned(),
                    decision: DecisionClass::Negative,
                    input_status: "valid".to_owned(),
                    evaluation_status: "completed".to_owned(),
                    completeness: "complete-for-fragment".to_owned(),
                    information_state: "opposed".to_owned(),
                    preservation: PreservationReport::exact(),
                    evidence_grade: EvidenceGrade::Certificate,
                    gate_admission: GateAdmission::Refused,
                    declared_fragment: Some(fragment.to_owned()),
                    inputs: vec![input],
                    program: native_program(operation, fragment),
                    engine: reasoning_engine(operation),
                    metrics: ReasoningMetrics::default(),
                    boundaries: Vec::new(),
                    diagnostics: vec![
                        ReportDiagnostic::from_diag(&error)
                            .with_operation_code("ONTOLOGY_HAS_NO_MODEL"),
                    ],
                },
                result: if operation == ReasoningOperation::Consistency {
                    ReasoningPayload::Consistency {
                        has_model: Some(false),
                    }
                } else {
                    ReasoningPayload::Empty
                },
            }
            .publish(format, gate)
        }
        DlServiceFailureKind::Malformed => publish_reasoning_failure(
            reporter,
            diagnostic_prefix,
            operation,
            fragment,
            DecisionClass::Malformed,
            "malformed",
            "invalid",
            vec![input],
            ReportDiagnostic::from_diag(&error).with_operation_code("REASONER_INPUT_MALFORMED"),
            format,
            gate,
        ),
        DlServiceFailureKind::Execution => publish_reasoning_failure(
            reporter,
            diagnostic_prefix,
            operation,
            fragment,
            DecisionClass::ExecutionFailure,
            "execution-failure",
            "valid",
            vec![input],
            ReportDiagnostic::from_diag(&error).with_operation_code("REASONER_EXECUTION_FAILED"),
            format,
            gate,
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn input_failure_report(
    reporter: &dyn Reporter,
    diagnostic_prefix: &str,
    operation: ReasoningOperation,
    fragment: &str,
    error: Diag,
    mut prior: Vec<InputIdentity>,
    format: OutputFormat,
    gate: bool,
) -> i32 {
    let Some(failure) = error.downcast_ref::<ReasoningInputFailure>() else {
        return publish_reasoning_failure(
            reporter,
            diagnostic_prefix,
            operation,
            fragment,
            DecisionClass::ExecutionFailure,
            "execution-failure",
            "unknown",
            prior,
            ReportDiagnostic::from_diag(
                &error.with_context("reasoning input failure has no input evidence"),
            )
            .with_operation_code("REASONING_INPUT_CONTRACT_INVALID"),
            format,
            gate,
        );
    };
    prior.extend(failure.identities.iter().cloned());
    publish_reasoning_failure(
        reporter,
        diagnostic_prefix,
        operation,
        fragment,
        DecisionClass::Malformed,
        "malformed",
        "invalid",
        prior,
        ReportDiagnostic::from_diag(&error).with_operation_code(failure.code),
        format,
        gate,
    )
}

#[allow(clippy::too_many_arguments)]
fn dl_report_core<T>(
    operation: ReasoningOperation,
    fragment: &str,
    verdict: &str,
    decision: DecisionClass,
    information_state: &str,
    evidence_grade: EvidenceGrade,
    gate_admission: GateAdmission,
    input: InputIdentity,
    certified: &purrdf::entail::Certified<T>,
) -> ReasoningReportCore {
    let certificate = certified.certificate();
    let completeness = certificate.completeness();
    let boundaries: Vec<_> = certificate
        .boundaries()
        .iter()
        .map(|boundary| ReasoningBoundary {
            code: boundary.construct().to_string(),
            detail: boundary.reason().to_owned(),
        })
        .collect();
    let preservation = if completeness == purrdf::entail::DlCompleteness::Decided {
        PreservationReport::exact()
    } else {
        PreservationReport::sound_under(boundaries.iter().map(|row| row.code.clone()).collect())
    };
    ReasoningReportCore {
        schema_version: 1,
        operation,
        verdict: verdict.to_owned(),
        decision,
        input_status: "valid".to_owned(),
        evaluation_status: if completeness.is_decided() {
            "completed"
        } else {
            "budget-exhausted"
        }
        .to_owned(),
        completeness: if completeness.is_decided() {
            "complete-for-fragment"
        } else {
            "incomplete"
        }
        .to_owned(),
        information_state: information_state.to_owned(),
        preservation,
        evidence_grade,
        gate_admission,
        declared_fragment: Some(fragment.to_owned()),
        inputs: vec![input],
        program: native_program(operation, fragment),
        engine: reasoning_engine(operation),
        metrics: ReasoningMetrics {
            decisions: Some(certificate.decisions()),
            steps: Some(certificate.steps()),
            budget: Some(certificate.budget()),
        },
        boundaries,
        diagnostics: Vec::new(),
    }
}

/// `gmeow entails` — decide whether the premise graph entails the conclusion
/// (`A ⊨ C`) natively, by refutation over the DL consistency calculus
/// ([`gmeow_logic::entail::dl_entailment_assessment`]). Prints a stable, greppable verdict:
/// `verdict entailed` / `verdict not-entailed` / `verdict gap` (plus `gap-shape` /
/// `gap-detail` for a gap). Malformed input exits 4, execution failure exits 5,
/// and reporter mode retains exit 0 for every well-formed operation result.
pub fn entails(
    reporter: &dyn Reporter,
    premise: &Path,
    conclusion: &Path,
    format: OutputFormat,
    gate: bool,
) -> i32 {
    const FRAGMENT: &str = "gmeow-native-default-context-dl-refutation-v1";
    let premise_input = match load_reasoning_input(premise, "premise", "gmeow-cli.entails") {
        Ok(input) => input,
        Err(error) => {
            return input_failure_report(
                reporter,
                "gmeow-cli.entails",
                ReasoningOperation::Entailment,
                FRAGMENT,
                error,
                Vec::new(),
                format,
                gate,
            );
        }
    };
    let conclusion_input = match load_reasoning_input(conclusion, "conclusion", "gmeow-cli.entails")
    {
        Ok(input) => input,
        Err(error) => {
            let prior = vec![premise_input.identity];
            return input_failure_report(
                reporter,
                "gmeow-cli.entails",
                ReasoningOperation::Entailment,
                FRAGMENT,
                error,
                prior,
                format,
                gate,
            );
        }
    };
    let inputs = vec![premise_input.identity, conclusion_input.identity];

    let assessment = match gmeow_logic::entail::dl_entailment_assessment(
        premise_input.dataset.as_ref(),
        conclusion_input.dataset.as_ref(),
    ) {
        Ok(v) => v,
        Err(e) => {
            return publish_reasoning_failure(
                reporter,
                "gmeow-cli.entails.failed",
                ReasoningOperation::Entailment,
                FRAGMENT,
                DecisionClass::ExecutionFailure,
                "execution-failure",
                "valid",
                inputs,
                ReportDiagnostic::from_diag(&e.with_context("entailment check failed"))
                    .with_operation_code("ENTAILMENT_EXECUTION_FAILED"),
                format,
                gate,
            );
        }
    };

    let (decision, information, evidence, admission, preservation, boundaries, payload) =
        match &assessment.verdict {
            gmeow_logic::entail::EntailmentVerdict::Entailed => (
                DecisionClass::Positive,
                "supported",
                EvidenceGrade::Certificate,
                GateAdmission::Certificate,
                PreservationReport::exact(),
                Vec::new(),
                ReasoningPayload::Entailment {
                    entailed: Some(true),
                    gap_shape: None,
                    gap_detail: None,
                },
            ),
            gmeow_logic::entail::EntailmentVerdict::NotEntailed => (
                DecisionClass::Negative,
                "opposed",
                EvidenceGrade::Certificate,
                GateAdmission::Refused,
                PreservationReport::exact(),
                Vec::new(),
                ReasoningPayload::Entailment {
                    entailed: Some(false),
                    gap_shape: None,
                    gap_detail: None,
                },
            ),
            gmeow_logic::entail::EntailmentVerdict::Gap(gap) => {
                let shape = gap.shape.as_token().to_owned();
                (
                    DecisionClass::Unsupported,
                    "not-evaluated",
                    EvidenceGrade::Refused,
                    GateAdmission::Refused,
                    PreservationReport::unsupported(vec![shape.clone()]),
                    vec![ReasoningBoundary {
                        code: shape.clone(),
                        detail: gap.detail.clone(),
                    }],
                    ReasoningPayload::Entailment {
                        entailed: None,
                        gap_shape: Some(shape),
                        gap_detail: Some(gap.detail.clone()),
                    },
                )
            }
        };
    NativeReasoningReport {
        common: ReasoningReportCore {
            schema_version: 1,
            operation: ReasoningOperation::Entailment,
            verdict: assessment.verdict.as_token().to_owned(),
            decision,
            input_status: "valid".to_owned(),
            evaluation_status: if decision == DecisionClass::Unsupported {
                "unsupported"
            } else {
                "completed"
            }
            .to_owned(),
            completeness: if decision == DecisionClass::Unsupported {
                "unknown"
            } else {
                "complete-for-fragment"
            }
            .to_owned(),
            information_state: information.to_owned(),
            preservation,
            evidence_grade: evidence,
            gate_admission: admission,
            declared_fragment: Some(FRAGMENT.to_owned()),
            inputs,
            program: native_program(ReasoningOperation::Entailment, FRAGMENT),
            engine: reasoning_engine(ReasoningOperation::Entailment),
            metrics: ReasoningMetrics {
                decisions: Some(assessment.decisions),
                steps: Some(assessment.steps),
                budget: assessment.budget,
            },
            boundaries,
            diagnostics: Vec::new(),
        },
        result: payload,
    }
    .publish(format, gate)
}

/// `gmeow consistency` — decide whether an ontology has a model (OWL 2
/// Direct-Semantics consistency), natively over the purrdf DL reasoner
/// ([`gmeow_logic::reasoner_services::DlReasoner`]). The reasoner reads OWL/RDFS
/// spelling only; canonical `logic:` axioms, formulas and rules are not read here
/// (`gmeow prove` decides a `logic:` selection). Prints a stable, greppable
/// three-valued verdict (`true` / `false` / `unknown`) together with the run's
/// completeness, measured cost, and every construct boundary — none of it flattened
/// away. Malformed input exits 4 and execution failure exits 5; an `unknown`
/// verdict remains an honestly reported result in reporter mode.
pub fn consistency(
    reporter: &dyn Reporter,
    ontology: &Path,
    step_cap: Option<u64>,
    format: OutputFormat,
    gate: bool,
) -> i32 {
    const FRAGMENT: &str = "owl2-direct-semantics-dl-v1";
    let input = match load_reasoning_input(ontology, "ontology", "gmeow-cli.consistency") {
        Ok(input) => input,
        Err(error) => {
            return input_failure_report(
                reporter,
                "gmeow-cli.consistency",
                ReasoningOperation::Consistency,
                FRAGMENT,
                error,
                Vec::new(),
                format,
                gate,
            );
        }
    };
    // A `--step-cap` narrows the per-decision tableau budget (clamped down only), so the honest
    // budget-exhausted `unknown` verdict is deterministically reachable; without it the reasoner
    // runs under its full size-derived ceiling.
    let opened = match step_cap {
        Some(cap) => {
            gmeow_logic::reasoner_services::DlReasoner::with_step_cap(input.dataset.as_ref(), cap)
        }
        None => gmeow_logic::reasoner_services::DlReasoner::new(input.dataset.as_ref()),
    };
    let reasoner = match opened {
        Ok(r) => r,
        Err(e) => {
            return dl_service_failure_report(
                reporter,
                "gmeow-cli.consistency",
                ReasoningOperation::Consistency,
                FRAGMENT,
                input.identity,
                e,
                format,
                gate,
            );
        }
    };
    let certified = reasoner.consistency();
    let completeness = certified.certificate().completeness();
    let (decision, information, evidence, admission, has_model) = match *certified.answer() {
        purrdf::entail::Verdict::True
            if completeness == purrdf::entail::DlCompleteness::Decided =>
        {
            (
                DecisionClass::Positive,
                "supported",
                EvidenceGrade::Certificate,
                GateAdmission::Certificate,
                Some(true),
            )
        }
        purrdf::entail::Verdict::True => (
            DecisionClass::Positive,
            "undetermined",
            EvidenceGrade::Attestation,
            GateAdmission::Attestation,
            Some(true),
        ),
        purrdf::entail::Verdict::False if completeness.is_decided() => (
            DecisionClass::Negative,
            "opposed",
            EvidenceGrade::Certificate,
            GateAdmission::Refused,
            Some(false),
        ),
        purrdf::entail::Verdict::False => (
            DecisionClass::Undecided,
            "undetermined",
            EvidenceGrade::Attestation,
            GateAdmission::Attestation,
            None,
        ),
        purrdf::entail::Verdict::Unknown => (
            DecisionClass::Undecided,
            "undetermined",
            EvidenceGrade::Attestation,
            GateAdmission::Attestation,
            None,
        ),
    };
    NativeReasoningReport {
        common: dl_report_core(
            ReasoningOperation::Consistency,
            FRAGMENT,
            &certified.answer().to_string(),
            decision,
            information,
            evidence,
            admission,
            input.identity,
            &certified,
        ),
        result: ReasoningPayload::Consistency { has_model },
    }
    .publish(format, gate)
}

/// `gmeow profile` — certify an ontology against the OWL 2 profiles (EL, QL, RL, DL,
/// Full), natively over the purrdf profile checker
/// ([`gmeow_logic::reasoner_services::certify_profiles`]). Prints every profile the
/// ontology is provably in (`certified <PROFILE>`) and every structural violation
/// blocking the others (`violation <PROFILE>: <reason>`). A clean certification
/// proves membership; a violation proves only that the cheap syntactic check failed.
/// A malformed / unparsable input is a hard fail (exit 1).
pub fn profile(reporter: &dyn Reporter, ontology: &Path) -> i32 {
    let dataset = match parse_rdf_file(reporter, "gmeow-cli.profile", ontology) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let certificate = gmeow_logic::reasoner_services::certify_profiles(dataset.as_ref());
    for certified_profile in certificate.certified() {
        println!("certified {}", certified_profile.as_str());
    }
    for violation in certificate.violations() {
        println!("violation {violation}");
    }
    0
}

/// Render a reasoner-side [`TermValue`] to its IRI text for the greppable
/// classify / realize / module surfaces. The DL classification, realization and
/// module signatures range over NAMED classes and individuals, so every term IS an
/// IRI; a non-IRI term — which the reasoner never places here — falls back to its
/// debug form rather than being silently dropped.
fn render_term(term: &TermValue) -> String {
    term.as_iri()
        .map_or_else(|| format!("{term:?}"), str::to_owned)
}

/// Render certified direct rows, or withhold them. purrdf proves directness only
/// for a fully decided ontology: an undecided run could be missing an intermediate
/// class or a more specific type. A withheld answer is `None`, never an empty list
/// (which would claim there are no direct rows), and its refusal is recorded as an
/// `owl-directness-unavailable` report diagnostic beside the retained certificate.
fn certified_direct_rows(
    rows: Result<&[(TermValue, TermValue)], purrdf::entail::DirectnessRefusal<'_>>,
    diagnostics: &mut Vec<ReportDiagnostic>,
) -> Option<Vec<TermPair>> {
    match rows {
        Ok(rows) => Some(
            rows.iter()
                .map(|(left, right)| TermPair {
                    left: render_term(left),
                    right: render_term(right),
                })
                .collect(),
        ),
        Err(refusal) => {
            diagnostics.push(ReportDiagnostic::message(
                "owl-directness-unavailable",
                refusal.to_string(),
            ));
            None
        }
    }
}

/// `gmeow classify` — print the subsumption hierarchy over an ontology's named
/// classes, natively over the purrdf DL reasoner
/// ([`gmeow_logic::reasoner_services::DlReasoner::classify`]). Emits the transitively
/// closed subsumptions (`subsumption C D`), the transitive reduction (`direct C D`),
/// the equivalences (`equivalence C D`), the classes forced empty (`unsatisfiable C`),
/// and the run's completeness + measured cost. The transitive reduction is emitted
/// only for a fully decided run; otherwise it is withheld with an
/// `owl-directness-unavailable` diagnostic. An ontology with no model has no
/// meaningful hierarchy, so classification returns a typed negative result rather than
/// emitting an empty answer.
pub fn classify(reporter: &dyn Reporter, ontology: &Path, format: OutputFormat, gate: bool) -> i32 {
    const FRAGMENT: &str = "owl2-direct-semantics-dl-v1";
    let input = match load_reasoning_input(ontology, "ontology", "gmeow-cli.classify") {
        Ok(input) => input,
        Err(error) => {
            return input_failure_report(
                reporter,
                "gmeow-cli.classify",
                ReasoningOperation::Classification,
                FRAGMENT,
                error,
                Vec::new(),
                format,
                gate,
            );
        }
    };
    let reasoner = match gmeow_logic::reasoner_services::DlReasoner::new(input.dataset.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            return dl_service_failure_report(
                reporter,
                "gmeow-cli.classify",
                ReasoningOperation::Classification,
                FRAGMENT,
                input.identity,
                e,
                format,
                gate,
            );
        }
    };
    let certified = match reasoner.classify() {
        Ok(c) => c,
        Err(e) => {
            return dl_service_failure_report(
                reporter,
                "gmeow-cli.classify",
                ReasoningOperation::Classification,
                FRAGMENT,
                input.identity,
                e,
                format,
                gate,
            );
        }
    };
    let hierarchy = certified.answer();
    let exact = certified.certificate().completeness() == purrdf::entail::DlCompleteness::Decided;
    let mut common = dl_report_core(
        ReasoningOperation::Classification,
        FRAGMENT,
        if exact { "classified" } else { "partial" },
        if exact {
            DecisionClass::Positive
        } else {
            DecisionClass::Undecided
        },
        if exact { "supported" } else { "undetermined" },
        if exact {
            EvidenceGrade::Certificate
        } else {
            EvidenceGrade::Attestation
        },
        if exact {
            GateAdmission::Certificate
        } else {
            GateAdmission::Attestation
        },
        input.identity,
        &certified,
    );
    let direct_subsumptions =
        certified_direct_rows(certified.direct_subsumptions(), &mut common.diagnostics);
    NativeReasoningReport {
        common,
        result: ReasoningPayload::Classification {
            subsumptions: hierarchy
                .subsumptions()
                .iter()
                .map(|(left, right)| TermPair {
                    left: render_term(left),
                    right: render_term(right),
                })
                .collect(),
            direct_subsumptions,
            equivalences: hierarchy
                .equivalences()
                .iter()
                .map(|(left, right)| TermPair {
                    left: render_term(left),
                    right: render_term(right),
                })
                .collect(),
            unsatisfiable: hierarchy.unsatisfiable().iter().map(render_term).collect(),
        },
    }
    .publish(format, gate)
}

/// `gmeow realize` — print the entailed types of an ontology's named individuals,
/// natively over the purrdf DL reasoner
/// ([`gmeow_logic::reasoner_services::DlReasoner::realize`]). Emits every established
/// type (`type a C`), the most-specific ones (`direct-type a C`), and the run's
/// completeness + measured cost. Most-specific types are emitted only for a fully
/// decided run; otherwise they are withheld with an `owl-directness-unavailable`
/// diagnostic. An ontology with no model has no meaningful
/// realization, so it returns a typed negative result rather than emitting an empty
/// answer.
pub fn realize(reporter: &dyn Reporter, ontology: &Path, format: OutputFormat, gate: bool) -> i32 {
    const FRAGMENT: &str = "owl2-direct-semantics-dl-v1";
    let input = match load_reasoning_input(ontology, "ontology", "gmeow-cli.realize") {
        Ok(input) => input,
        Err(error) => {
            return input_failure_report(
                reporter,
                "gmeow-cli.realize",
                ReasoningOperation::Realization,
                FRAGMENT,
                error,
                Vec::new(),
                format,
                gate,
            );
        }
    };
    let reasoner = match gmeow_logic::reasoner_services::DlReasoner::new(input.dataset.as_ref()) {
        Ok(r) => r,
        Err(e) => {
            return dl_service_failure_report(
                reporter,
                "gmeow-cli.realize",
                ReasoningOperation::Realization,
                FRAGMENT,
                input.identity,
                e,
                format,
                gate,
            );
        }
    };
    let certified = match reasoner.realize() {
        Ok(c) => c,
        Err(e) => {
            return dl_service_failure_report(
                reporter,
                "gmeow-cli.realize",
                ReasoningOperation::Realization,
                FRAGMENT,
                input.identity,
                e,
                format,
                gate,
            );
        }
    };
    let realization = certified.answer();
    let exact = certified.certificate().completeness() == purrdf::entail::DlCompleteness::Decided;
    let mut common = dl_report_core(
        ReasoningOperation::Realization,
        FRAGMENT,
        if exact { "realized" } else { "partial" },
        if exact {
            DecisionClass::Positive
        } else {
            DecisionClass::Undecided
        },
        if exact { "supported" } else { "undetermined" },
        if exact {
            EvidenceGrade::Certificate
        } else {
            EvidenceGrade::Attestation
        },
        if exact {
            GateAdmission::Certificate
        } else {
            GateAdmission::Attestation
        },
        input.identity,
        &certified,
    );
    let direct_types = certified_direct_rows(certified.direct_types(), &mut common.diagnostics);
    NativeReasoningReport {
        common,
        result: ReasoningPayload::Realization {
            types: realization
                .types()
                .iter()
                .map(|(left, right)| TermPair {
                    left: render_term(left),
                    right: render_term(right),
                })
                .collect(),
            direct_types,
        },
    }
    .publish(format, gate)
}

/// `gmeow module` — extract the syntactic-locality module of an ontology for a seed
/// signature, natively over the purrdf module extractor
/// ([`gmeow_logic::reasoner_services::extract_module`]). Emits the notion used
/// (`method BOT|TOP|STAR`), the kept-axiom count (`axioms N`), the module's triple
/// count (`triples N`), the signature the fixpoint closed to (`signature IRI`), and
/// every triple kept conservatively rather than by exact locality
/// (`conservative-keep S P`). An unknown `--method` or a malformed / unparsable input
/// is a hard fail (never a silent fallback to a default notion).
pub fn module(reporter: &dyn Reporter, ontology: &Path, seed: &[String], method: &str) -> i32 {
    let dataset = match parse_rdf_file(reporter, "gmeow-cli.module", ontology) {
        Ok(ds) => ds,
        Err(code) => return code,
    };
    let method = match method {
        "bot" | "BOT" => purrdf::entail::ModuleMethod::Bot,
        "top" | "TOP" => purrdf::entail::ModuleMethod::Top,
        "star" | "STAR" => purrdf::entail::ModuleMethod::Star,
        other => {
            return fail(
                reporter,
                "gmeow-cli.module.method",
                format!("unknown module method `{other}` (expected `bot`, `top`, or `star`)"),
            );
        }
    };
    let signature: Vec<TermValue> = seed.iter().map(TermValue::iri).collect();
    let extraction = match gmeow_logic::reasoner_services::extract_module(
        dataset.as_ref(),
        &signature,
        method,
    ) {
        Ok(m) => m,
        Err(e) => {
            return fail(
                reporter,
                "gmeow-cli.module.failed",
                format!("module extraction failed for {}: {e}", ontology.display()),
            );
        }
    };
    println!("method {}", extraction.method());
    println!("axioms {}", extraction.axioms());
    println!("triples {}", extraction.module().quad_count());
    for term in extraction.signature() {
        println!("signature {}", render_term(term));
    }
    for keep in extraction.conservative_keeps() {
        println!(
            "conservative-keep {} {}",
            render_term(keep.subject()),
            render_term(keep.predicate())
        );
    }
    0
}

// ── logic fragments (the shipped decidability-surface query) ──────────────────

/// The `logic:` grounding namespace the decidability manifest is authored in.
const LOGIC_FRAGMENTS_NS: &str = "https://blackcatinformatics.ca/logic/";

/// `rdf:type` and `rdfs:label` — the two external predicates the manifest query keys on.
pub(super) const RDF_TYPE_IRI: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_LABEL_IRI: &str = "http://www.w3.org/2000/01/rdf-schema#label";

/// One decided construct family read off the shipped `logic:DecidedFragment`
/// manifest: its stable id (the individual's local name), the
/// `logic:RefutationPattern` local name it closes under, its human label, and its
/// technical `logic:fragmentCompletenessBound`.
struct DecidedFragmentRow {
    id: String,
    pattern: String,
    label: String,
    bound: Vec<gmeow_errors::SourceAnnotation>,
}

/// One retained withhold read off the shipped `logic:expressivenessBoundary`
/// records: its stable id (local name), its human label, and its technical
/// `logic:fragmentBoundaryReason`.
struct RetainedBoundaryRow {
    id: String,
    label: String,
    reason: Vec<gmeow_errors::SourceAnnotation>,
}

/// A source grammar contract read from the same shipped native manifest.
struct SourceAdmissionRow {
    id: String,
    label: String,
    requirement: Vec<gmeow_errors::SourceAnnotation>,
}

/// The decidability surface extracted from a graph source.
struct DecidabilitySurface {
    decided: Vec<DecidedFragmentRow>,
    boundaries: Vec<RetainedBoundaryRow>,
    source_admissions: Vec<SourceAdmissionRow>,
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
                parse_rdf_file(reporter, "gmeow-cli.logic-fragments", path)
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
fn extract_decidability_surface(
    dataset: &purrdf::RdfDataset,
) -> gmeow_errors::Result<DecidabilitySurface> {
    let registry = gmeow_logic::reason::NativeFragmentRegistry::observe(dataset)?;
    let mut labels = BTreeMap::new();
    for quad in dataset.owned_quads() {
        if quad.predicate != RDFS_LABEL_IRI {
            continue;
        }
        if let (RdfTerm::Iri(subject), RdfTerm::Literal(label)) = (&quad.subject, &quad.object)
            && let Some(local) = subject.strip_prefix(LOGIC_FRAGMENTS_NS)
        {
            labels.insert(local.to_owned(), label.lexical_form.clone());
        }
    }
    let source_admissions = registry
        .source_admission_ids
        .iter()
        .map(|id| {
            let label = labels
                .get(id)
                .filter(|label| !label.trim().is_empty())
                .ok_or_else(|| {
                    Diag::of_kind(crate::error::RdfPipelineFailed {
                        detail: format!("source admission {id} has no label"),
                    })
                })?
                .clone();
            Ok(SourceAdmissionRow {
                id: id.clone(),
                label,
                requirement: registry.source_admission_requirements[id].clone(),
            })
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let decided = registry
        .decided_ids
        .iter()
        .map(|id| {
            let missing = |property: &str| {
                Diag::of_kind(crate::error::RdfPipelineFailed {
                    detail: format!("decided fragment {id} has no {property}"),
                })
            };
            Ok(DecidedFragmentRow {
                id: id.clone(),
                label: labels.get(id).cloned().unwrap_or_default(),
                pattern: registry
                    .deciding_patterns
                    .get(id)
                    .cloned()
                    .ok_or_else(|| missing("deciding pattern"))?,
                bound: registry
                    .completeness_bounds
                    .get(id)
                    .cloned()
                    .ok_or_else(|| missing("completeness bound"))?,
            })
        })
        .collect::<gmeow_errors::Result<Vec<_>>>()?;
    let boundaries = registry
        .boundary_reasons
        .into_iter()
        .map(|(id, reason)| RetainedBoundaryRow {
            label: labels.get(&id).cloned().unwrap_or_default(),
            id,
            reason,
        })
        .collect();
    Ok(DecidabilitySurface {
        decided,
        boundaries,
        source_admissions,
    })
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
        render_fragment_literals(&mut out, "bound", &f.bound);
    }
    let _ = writeln!(out, "retained-boundaries {}", surface.boundaries.len());
    for b in &surface.boundaries {
        let _ = writeln!(out, "boundary {}", b.id);
        let _ = writeln!(out, "  label {}", b.label);
        render_fragment_literals(&mut out, "reason", &b.reason);
    }
    let _ = writeln!(
        out,
        "source-admission-contracts {}",
        surface.source_admissions.len()
    );
    for admission in &surface.source_admissions {
        let _ = writeln!(out, "source-admission {}", admission.id);
        let _ = writeln!(out, "  label {}", admission.label);
        render_fragment_literals(&mut out, "requirement", &admission.requirement);
    }
    out
}

/// Display every source literal and its RDF metadata without choosing a language.
fn render_fragment_literals(
    out: &mut String,
    role: &str,
    values: &[gmeow_errors::SourceAnnotation],
) {
    use std::fmt::Write as _;
    for value in values {
        let literal = value.object();
        let _ = writeln!(out, "  {role} {}", literal.lexical_form);
        let _ = writeln!(out, "    datatype {}", literal.datatype_iri());
        if let Some(language) = &literal.language {
            let _ = writeln!(out, "    language {language}");
        }
        if let Some(direction) = literal.direction {
            let _ = writeln!(out, "    direction {}", direction.as_str());
        }
    }
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
        "source_admission_contracts": surface.source_admissions.iter().map(|admission| serde_json::json!({
            "id": admission.id, "label": admission.label, "requirement": admission.requirement,
        })).collect::<Vec<_>>(),
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
/// bundle by `make check`, so an empty read points the user at that.
pub fn logic_fragments(
    reporter: &dyn Reporter,
    bundle: Option<&Path>,
    format: OutputFormat,
) -> i32 {
    let dataset = match fragments_graph_source(reporter, bundle) {
        Ok(d) => d,
        Err(code) => return code,
    };
    let surface = match extract_decidability_surface(dataset.as_ref()) {
        Ok(surface) => surface,
        Err(diagnostic) => {
            return fail(
                reporter,
                "gmeow-cli.logic-fragments.invalid-surface",
                diagnostic.to_string(),
            );
        }
    };
    if surface.decided.is_empty()
        && surface.boundaries.is_empty()
        && surface.source_admissions.is_empty()
    {
        return fail(
            reporter,
            "gmeow-cli.logic-fragments.empty-surface",
            "the graph source carries no logic:DecidedFragment / logic:expressivenessBoundary \
             decidability manifest; the embedded bundle is materialized by `make check`, or pass \
             --bundle pointing at a graph source that ships the manifest (e.g. the logic slice \
             module.ttl)",
        );
    }
    let rendered = match format {
        OutputFormat::Text => Ok(render_fragments_text(&surface)),
        OutputFormat::Json => render_fragments_json(&surface)
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

/// The TOLD class-subsumption covering edges `(sub IRI, super IRI)` in `dataset`
/// — an IRI-to-IRI triple on EITHER spelling of the subsumption predicate
/// ([`gmeow_ns::SUB_CLASS_OF`]: the canonical `logic:subClassOf` and its `rdfs:`
/// projection) — deduplicated and sorted. These are the covering edges
/// `stage-goal-directed` filters its reasoned closure on
/// (`crates/pipeline/src/stages/goal_directed.rs`), read here directly off a
/// parsed source graph instead of a full pipeline reasoning pass.
///
/// Reading only the `rdfs:` projection makes this command silently return NOTHING
/// for a subsort chain authored in the canonical spelling — `--subsort` would hand
/// the engine an empty sort order and every subsort-dependent goal would simply
/// fail to prove, with no diagnostic.
///
/// `crate::physical::unify::SortOrder::from_subclass_edges` (inside
/// `gmeow_logic`) computes its OWN reflexive-transitive closure over whatever
/// covering edges it is handed, so passing the told edges of a simple subsort
/// chain (e.g. `math:Integer ⊑ math:RationalNumber ⊑ math:RealNumber`) is
/// sufficient for the engine to accept `math:Integer ⊑ math:RealNumber` —
/// there is no need (and this command makes no attempt) to pre-compute a
/// reasoned closure.
pub(super) fn collect_subclass_edges(dataset: &RdfDataset) -> Vec<(String, String)> {
    let mut edges: Vec<(String, String)> = dataset
        .owned_quads()
        .filter(|q| gmeow_ns::SUB_CLASS_OF.contains(&q.predicate.as_str()))
        .filter_map(|q| match (&q.subject, &q.object) {
            (RdfTerm::Iri(s), RdfTerm::Iri(o)) => Some((s.clone(), o.clone())),
            _ => None,
        })
        .collect();
    edges.sort();
    edges.dedup();
    edges
}

/// Parse a Turtle file into a raw [`RdfDataset`] for class-subsumption edge
/// extraction (either spelling — see [`collect_subclass_edges`]) —
/// deliberately independent of the `logic:` compiler frontend (which never
/// surfaces a told `logic:subClassOf`/`rdfs:subClassOf` triple as
/// `LogicAxiom`/`Formula` data an engine caller can read back), so this reads
/// the file's own triples directly.
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

/// Assess a caller-supplied contextual query with the native finite modal kernel.
/// The complete shared result and proof are emitted even for fragment refusal or
/// budget exhaustion; either computation outcome returns a nonzero status.
pub fn logic_evaluate(
    reporter: &dyn Reporter,
    input: &Path,
    request: &str,
    max_steps: Option<u64>,
) -> i32 {
    const CODE: &str = "gmeow-cli.logic-evaluate";
    let bytes = match read_bytes(reporter, input) {
        Ok(bytes) => bytes,
        Err(code) => return code,
    };
    let suffix = input
        .extension()
        .map(|suffix| format!(".{}", suffix.to_string_lossy().to_ascii_lowercase()));
    let Some(format) = suffix.as_deref().and_then(rdf_format_for_suffix) else {
        return fail(
            reporter,
            CODE,
            format!("unknown RDF format: {}", input.display()),
        );
    };
    let dataset = match purrdf::parse_dataset(&bytes, format, None) {
        Ok(dataset) => dataset,
        Err(error) => {
            return fail(
                reporter,
                CODE,
                format!("cannot parse {}: {error}", input.display()),
            );
        }
    };
    let assessment =
        match gmeow_logic::contextual::evaluate_request(&dataset, request, max_steps, None) {
            Ok(assessment) => assessment,
            Err(error) => return fail(reporter, CODE, error.to_string()),
        };
    let artifact = match gmeow_logic::result_rdf::project_contextual_artifact(&assessment) {
        Ok(projection) => projection,
        Err(error) => {
            reporter.report(&report_diag(error, "gmeow"));
            return 1;
        }
    };
    let mut output = std::io::stdout().lock();
    if let Err(error) = std::io::Write::write_all(&mut output, &artifact.bytes)
        .and_then(|()| std::io::Write::flush(&mut output))
    {
        reporter.report(&report_diag(
            Diag::from(error).with_context("write complete contextual GTS artifact"),
            "gmeow",
        ));
        return 1;
    }
    i32::from(assessment.result.evaluation != gmeow_logic::result::EvaluationStatus::Completed)
}

/// `gmeow logic backward` — evaluate one or more authored `logic:ReasoningProgram`
/// cells through the native proof-carrying SLG-WFS backward engine
/// ([`gmeow_logic::goal_directed::evaluate_reasoning_programs`]) — the SAME
/// production path `stage-goal-directed` folds into `gmeow.gts`'s
/// `graph/goal-directed`. Never a reimplementation: this command lowers the
/// authored cell via `gmeow_logic_compile::frontend::parse_logic_path`,
/// collects class-subsumption covering edges (either spelling) straight off
/// the parsed source (see [`collect_subclass_edges`]), and hands both to the
/// SAME engine entry point the pipeline stage calls.
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

    // Collect class-subsumption covering edges (either spelling) directly off
    // the parsed source graph(s) — never a hardcoded subsort tower (see
    // `collect_subclass_edges`).
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
/// extension) and re-home every RDF record into the single named-graph `world`, so the
/// façade sees exactly one world regardless of the source serialization's graph
/// structure. A hard fail (never a degraded empty world) on read/parse/freeze error.
fn session_load_world_dataset(
    reporter: &dyn Reporter,
    path: &Path,
    world: &str,
) -> Result<Arc<RdfDataset>, i32> {
    let parsed = parse_rdf_file(reporter, "gmeow-cli.logic-session", path)?;
    place_world_dataset(parsed, world).map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-session.edb-rehome",
            format!(
                "cannot place {} into the session world: {e}",
                path.display()
            ),
        )
    })
}

/// The single CLI world-placement path for session and hybrid facts. All source
/// graph roles are deliberately collapsed into `world`, including native reifier
/// bindings, annotations and empty declarations. Source graph IRIs used as RDF
/// values survive; source blank identities remain shared within this one input.
/// Physical parser locations are not part of the resulting logical world.
///
/// # Errors
/// Refuses an invalid placement or native materialization; never returns a
/// partial world or a weaker ordinary-triple projection.
fn place_world_dataset(
    parsed: Arc<RdfDataset>,
    world: &str,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let source = purrdf::CompositeSource::new(parsed)
        .with_graph_placement(purrdf::GraphPlacement::Named(purrdf::TermValue::iri(world)));
    let view = purrdf::CompositeDatasetView::from_shared_sources(
        vec![source],
        purrdf::ViewLimits::default(),
    )?;
    Ok(purrdf::dataset_from_view(&view)?)
}

#[cfg(test)]
#[path = "world_routing_tests.rs"]
mod world_routing_tests;

/// A shared empty additions dataset for a suppression-only committed delta.
fn session_empty_world_dataset(reporter: &dyn Reporter) -> Result<Arc<RdfDataset>, i32> {
    RdfDatasetBuilder::new().freeze().map_err(|e| {
        fail(
            reporter,
            "gmeow-cli.logic-session.empty-additions",
            format!("cannot build an empty additions dataset: {e}"),
        )
    })
}

/// Open a session over `edb`/`program` under the default contract and the exact
/// annotation semiring, mapping a façade open error to a hard CLI fail.
fn session_open(
    reporter: &dyn Reporter,
    edb: &Arc<RdfDataset>,
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
            builder.push_owned_quad(quad).map_err(|e| {
                fail(
                    reporter,
                    "gmeow-cli.logic-session.paged-page-push",
                    format!("cannot stage a quad into a paged world page: {e}"),
                )
            })?;
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
