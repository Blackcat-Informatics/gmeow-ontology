// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The wired command bodies. Each function marshals its inputs and delegates to
//! an already-native backend, following the console convention: product results
//! → stdout, errors/diagnostics → stderr, and a `0`/`1` exit code.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;

use gmeow_cli_core::ConsoleMode;

use crate::{BUNDLE_GTS, ExportFormat, NAMESPACE};

/// Print an error message to stderr and yield the failure exit code `1`.
fn fail(message: impl AsRef<str>) -> i32 {
    eprintln!("{}", message.as_ref());
    1
}

/// The bytes of `file` (read from disk), or the embedded [`BUNDLE_GTS`] when
/// `file` is `None` — the repo-free default every command shares.
fn gts_bytes(file: Option<&Path>) -> Result<Cow<'static, [u8]>, i32> {
    match file {
        None => Ok(Cow::Borrowed(BUNDLE_GTS)),
        Some(path) => match std::fs::read(path) {
            Ok(bytes) => Ok(Cow::Owned(bytes)),
            Err(e) => Err(fail(format!("cannot read {}: {e}", path.display()))),
        },
    }
}

/// Read a file's bytes or fail with a clean CLI error.
fn read_bytes(path: &Path) -> Result<Vec<u8>, i32> {
    std::fs::read(path).map_err(|e| fail(format!("cannot read {}: {e}", path.display())))
}

/// Build the internal→BCP-47 language tag map from a snapshot (its default-graph
/// N-Triples projection), for the language selector.
fn bundle_tag_map(bytes: &[u8]) -> Result<HashMap<String, String>, String> {
    let dataset = purrdf::gts::flattened_dataset_from_bytes(bytes)
        .map_err(|e| format!("cannot fold snapshot: {e}"))?;
    let nt = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("cannot project snapshot to N-Triples: {e}"))?;
    gmeow_validate::language_tags::load_tag_map(&nt, "n-triples")
}

/// Resolve the `--lang` / `GMEOW_LANG` request against a snapshot's tag map into a
/// [`LangSelector`](gmeow_validate::language_tags::LangSelector). The env read
/// happens here (the bin's concern); an explicit `--lang` (incl. `''`) wins.
fn resolve_selector(
    lang: Option<&str>,
    bytes: &[u8],
) -> Result<gmeow_validate::language_tags::LangSelector, i32> {
    let tag_map = bundle_tag_map(bytes).map_err(fail)?;
    let raw: Option<String> = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok());
    gmeow_validate::language_tags::resolve_lang_input(raw.as_deref(), &tag_map, None).map_err(|u| {
        fail(format!(
            "unknown language tag '{}'. Available languages: {}",
            u.tag,
            u.available.join(", ")
        ))
    })
}

// ── version / info ───────────────────────────────────────────────────────────

/// `gmeow version` — print the package version to stdout.
pub fn version() -> i32 {
    println!("{}", env!("CARGO_PKG_VERSION"));
    0
}

/// `gmeow info` — print a count summary of a GTS snapshot.
pub fn info(file: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(file) {
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
        eprintln!("{}: {}", diag.code, diag.detail);
    }
    0
}

// ── verify / verify-release-bundle ───────────────────────────────────────────

/// `gmeow verify` — shell the external `gts verify` binary for the OpenPGP
/// signature check, then print the source-free "Bundled Ontology Checks" table.
pub fn verify(file: Option<&Path>, trusted_key: Option<&Path>, allow_unsigned: bool) -> i32 {
    let bytes = match gts_bytes(file) {
        Ok(b) => b,
        Err(code) => return code,
    };

    // 1. Signature check via the external `gts` binary (never gpg directly).
    let exe = match crate::passthrough::resolve_gts_binary() {
        Some(exe) => exe,
        None => return fail(crate::passthrough::GTS_INSTALL_HINT),
    };
    // The bundle is embedded; write it to a temp file when verifying the default.
    let mut tmp: Option<tempfile_path::Temp> = None;
    let target: std::path::PathBuf = match file {
        Some(path) => path.to_path_buf(),
        None => match tempfile_path::Temp::write(&bytes) {
            Ok(t) => {
                let p = t.path().to_path_buf();
                tmp = Some(t);
                p
            }
            Err(e) => return fail(format!("cannot stage bundled snapshot: {e}")),
        },
    };
    let mut cmd = std::process::Command::new(&exe);
    cmd.arg("verify").arg(&target);
    if let Some(key) = trusted_key {
        cmd.arg("--trusted-key").arg(key);
    }
    if allow_unsigned {
        cmd.arg("--allow-unsigned");
    }
    let status = match cmd.status() {
        Ok(s) => s,
        Err(e) => return fail(format!("failed to run gts: {e}")),
    };
    drop(tmp);
    let sig_ok = status.success();

    // 2. Source-free ontology checks over the folded snapshot.
    let checks = match gmeow_pipeline::cli_ops::confirmations::bundle_term_summaries(&bytes) {
        Ok(terms) => terms,
        Err(e) => return fail(format!("bundled ontology checks failed: {e}")),
    };
    let missing_label = checks.iter().filter(|(_, l, _)| l.is_empty()).count();
    let missing_def = checks.iter().filter(|(_, _, d)| d.is_empty()).count();

    println!("Bundled Ontology Checks");
    let mut ok = sig_ok;
    let mut row = |name: &str, passed: bool, detail: String| {
        ok = ok && passed;
        println!(
            "  {name}: {} ({detail})",
            if passed { "pass" } else { "fail" }
        );
    };
    row(
        "term catalog",
        !checks.is_empty(),
        format!("{} terms", checks.len()),
    );
    row(
        "labels",
        missing_label == 0,
        format!("{missing_label} missing"),
    );
    row(
        "definitions",
        missing_def == 0,
        format!("{missing_def} missing"),
    );
    row("signatures", sig_ok, "gts verify".to_owned());

    if !ok {
        return fail("verification failed");
    }
    println!("verification passed");
    0
}

/// `gmeow verify-release-bundle` — native COSE + attestation-walk verification.
pub fn verify_release_bundle(bundle: &Path, public_key: Option<&Path>) -> i32 {
    let bundle_bytes = match read_bytes(bundle) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let armor = match public_key {
        None => None,
        Some(path) => match std::fs::read_to_string(path) {
            Ok(s) => Some(s),
            Err(e) => {
                return fail(format!(
                    "✗ public key {} is unreadable: {e}",
                    path.display()
                ));
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
        Err(e) => fail(format!("✗ release verification failed: {e}")),
    }
}

// ── describe ─────────────────────────────────────────────────────────────────

/// `gmeow describe` — render one term card from a GTS snapshot.
pub fn describe(term: &str, gts: Option<&Path>, lang: Option<&str>) -> i32 {
    let bytes = match gts_bytes(gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // The env/`--lang` precedence is the bin's concern; the backend does the
    // snapshot-aware language resolution. An explicit `--lang` (incl. `''`) wins
    // over `GMEOW_LANG`.
    let resolved: Option<String> = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok());
    let (text, code) = gmeow_docs::describe(term, &bytes, resolved.as_deref());
    if code == 0 {
        println!("{text}");
    } else {
        eprintln!("{text}");
    }
    code
}

// ── conjecture ─────────────────────────────────────────────────────────────────

/// `gmeow conjecture test` — test a candidate `logic:` formula against a KB in an
/// isolated, standpoint-scoped scenario world, print the engine verdict, and —
/// unless `--dry-run` — APPEND it to the append-only conjecture library. Delegates
/// to the SHARED [`gmeow_pipeline::mcp::run_conjecture_test`] core (the same path
/// the MCP `conjecture_test` tool runs), so there is one implementation, not two.
pub fn conjecture_test(
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
        Err(e) => return fail(format!("cannot read {}: {e}", formula.display())),
    };
    let kb_ttl = match std::fs::read_to_string(kb) {
        Ok(text) => text,
        Err(e) => return fail(format!("cannot read {}: {e}", kb.display())),
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
            Err(e) => return fail(format!("conjecture test failed: {e}")),
        };

    // A precondition-unmet TR gate refused the write: report it and fail (exit 1),
    // mirroring the MCP `ok:false` path — the verdict was computed but not persisted.
    if let Some(reason) = &out.precondition_unmet {
        eprintln!("Error: persistConjecture precondition unmet: {reason}");
        eprintln!("lifecycle {}", out.lifecycle);
        eprintln!("information {}", out.information);
        return 1;
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
    instance: &Path,
    schema: Option<&Path>,
    format: &str,
    deep: bool,
    _console: ConsoleMode,
) -> i32 {
    let suffix = instance
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let rdf_format = rdf_format_for_suffix(&suffix);
    if deep && (schema.is_some() || rdf_format.is_none()) {
        return fail("--deep is only supported for RDF validation without --schema");
    }
    if schema.is_none()
        && let Some(fmt) = rdf_format
    {
        return validate_rdf(instance, fmt, format, deep);
    }
    validate_instance(instance, schema)
}

/// The repo-free RDF Tier-1 (and opt-in Tier-2) conformance path.
fn validate_rdf(instance: &Path, fmt: &str, output: &str, deep: bool) -> i32 {
    let output = output.to_lowercase();
    if !matches!(output.as_str(), "human" | "sarif" | "json") {
        return fail(format!(
            "unknown --format {output:?}: expected human, sarif, or json"
        ));
    }
    let data = match read_bytes(instance) {
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
        Err(e) => return fail(format!("validation error: {e}")),
    };

    match output.as_str() {
        "sarif" => match gmeow_errors::render::to_sarif(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => return fail(format!("cannot render SARIF: {e}")),
        },
        "json" => match gmeow_errors::render::to_json(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => return fail(format!("cannot render JSON: {e}")),
        },
        _ => {
            let text = gmeow_errors::render::to_text(&report);
            if !text.trim().is_empty() {
                eprintln!("{text}");
            }
            if report.error_count() == 0 && report.warning_count() == 0 {
                println!("validation passed");
            }
        }
    }
    if report.error_count() > 0 { 1 } else { 0 }
}

/// The JSON/YAML instance-against-schema path.
fn validate_instance(instance: &Path, schema: Option<&Path>) -> i32 {
    let suffix = instance
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy().to_lowercase()))
        .unwrap_or_default();
    let fmt = match suffix.as_str() {
        ".json" | ".jsonld" => gmeow_validate::instance::InstanceFormat::Json,
        ".yaml" | ".yml" => gmeow_validate::instance::InstanceFormat::Yaml,
        _ => {
            return fail(format!(
                "cannot infer format from {}: expected a .json, .jsonld, .yaml, or .yml \
                 instance for JSON-Schema validation",
                instance
                    .file_name()
                    .map(|n| n.to_string_lossy())
                    .unwrap_or_default()
            ));
        }
    };
    let instance_bytes = match read_bytes(instance) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let schema_bytes = match schema {
        Some(path) => match read_bytes(path) {
            Ok(b) => b,
            Err(code) => return code,
        },
        None => match gmeow_pipeline::bundle_blobs::bundled_schema(BUNDLE_GTS) {
            Ok(Some(b)) => b,
            Ok(None) => return fail("no bundled JSON Schema; pass one with --schema"),
            Err(e) => return fail(format!("cannot read bundled JSON Schema: {e}")),
        },
    };
    match gmeow_validate::instance::validate_instance(&instance_bytes, fmt, &schema_bytes) {
        Ok(violations) if violations.is_empty() => {
            println!("validation passed");
            0
        }
        Ok(violations) => {
            for v in &violations {
                eprintln!("{v}");
            }
            1
        }
        Err(e) => fail(format!("validation error: {e}")),
    }
}

// ── build ────────────────────────────────────────────────────────────────────

/// `gmeow build` — write derived serializations of a GTS snapshot.
pub fn build(out: &Path, gts: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let dataset = match purrdf::gts::flattened_dataset_from_bytes(&bytes) {
        Ok(ds) => ds,
        Err(e) => return fail(format!("cannot fold snapshot: {e}")),
    };
    if let Err(e) = std::fs::create_dir_all(out) {
        return fail(format!("cannot create {}: {e}", out.display()));
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
                    return fail(format!("cannot write {}: {e}", target.display()));
                }
                println!("wrote {}", target.display());
            }
            Err(e) => return fail(format!("cannot serialize {name}: {e}")),
        }
    }

    // RDF-1.2-star: JSON-LD-star + YAML-LD-star, via the native pipeline serializer.
    match gmeow_pipeline::stages::yaml_ld::serialize_graph(&dataset) {
        Ok(text) => {
            let target = out.join("gmeow.jsonld");
            if let Err(e) = std::fs::write(&target, text) {
                return fail(format!("cannot write {}: {e}", target.display()));
            }
            println!("wrote {}", target.display());
        }
        Err(e) => return fail(format!("cannot serialize gmeow.jsonld: {e}")),
    }
    match gmeow_pipeline::stages::yaml_ld::serialize_graph_yaml(&dataset, None) {
        Ok(text) => {
            let target = out.join("gmeow.yamlld");
            if let Err(e) = std::fs::write(&target, text) {
                return fail(format!("cannot write {}: {e}", target.display()));
            }
            println!("wrote {}", target.display());
        }
        Err(e) => return fail(format!("cannot serialize gmeow.yamlld: {e}")),
    }
    0
}

// ── project ──────────────────────────────────────────────────────────────────

/// Re-serialize an N-Triples document as Turtle for the projection output.
fn nt_to_turtle(nt: &str) -> Result<Vec<u8>, String> {
    let dataset = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .map_err(|e| format!("projected N-Triples parse failed: {e}"))?;
    purrdf::serialize_dataset(
        &dataset,
        "text/turtle",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("Turtle serialization failed: {e}"))
}

/// `gmeow project` — a per-profile CONSTRUCT over a data file, or a view filter
/// over a `.gts` / the bundle.
pub fn project(
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
                "--format yaml-ld reads the bundled snapshot only; do not pass a source file",
            );
        }
        let yamlld = match gmeow_pipeline::bundle_blobs::bundled_yaml_ld(BUNDLE_GTS) {
            Ok(map) => map.get("gmeow.yamlld").cloned(),
            Err(e) => return fail(format!("cannot read bundled YAML-LD: {e}")),
        };
        let Some(yamlld) = yamlld else {
            return fail("bundled YAML-LD snapshot not found");
        };
        if let Err(e) = std::fs::create_dir_all(out) {
            return fail(format!("cannot create {}: {e}", out.display()));
        }
        let target = out.join("gmeow.yamlld");
        if let Err(e) = std::fs::write(&target, yamlld) {
            return fail(format!("cannot write {}: {e}", target.display()));
        }
        println!("wrote {}", target.display());
        return 0;
    }
    if !matches!(fmt_lower.as_str(), "turtle" | "ttl") {
        return fail(format!("unknown --format: {format}"));
    }

    let is_gts = source.is_some_and(|s| {
        s.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("gts"))
    });
    let is_data_file = source.is_some() && !is_gts;

    // The language selector is resolved against the target graph (the bundle for
    // a data file, else the supplied snapshot).
    let selector_bytes = match gts_bytes(if is_data_file { None } else { source }) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // Resolving the selector validates `--lang` (an unknown tag hard-fails); the
    // retag itself uses the full internal→public tag map below.
    let _selector = match resolve_selector(lang, &selector_bytes) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let tag_map: TagMap = build_project_tag_map(&selector_bytes);

    if let Err(e) = std::fs::create_dir_all(out) {
        return fail(format!("cannot create {}: {e}", out.display()));
    }

    if is_data_file {
        let source = source.expect("checked");
        let known = projections::profiles();
        if !known.contains_key(profile) {
            return fail(format!(
                "unknown projection profile: {profile} (a vocabulary profile)"
            ));
        }
        return project_data_file(source, profile, out, &tag_map);
    }

    // View filter over a `.gts` / the bundle.
    let known = projections::profiles();
    let valid =
        known.contains_key(profile) || profile == GTS_VIEW_GMEOW || GTS_VIEW_ALL.contains(&profile);
    if !valid {
        return fail(format!(
            "unknown view: {profile} (vocab | gmeow | all | maximal)"
        ));
    }
    let bytes = match gts_bytes(source) {
        Ok(b) => b,
        Err(code) => return code,
    };
    match projections::project_gts_subset(&bytes, profile, &tag_map) {
        Ok(nt) => match nt_to_turtle(&nt) {
            Ok(ttl) => {
                let target = out.join(format!("{profile}.ttl"));
                if let Err(e) = std::fs::write(&target, ttl) {
                    return fail(format!("cannot write {}: {e}", target.display()));
                }
                println!("wrote {}", target.display());
                0
            }
            Err(e) => fail(e),
        },
        Err(e) => fail(e.to_string()),
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
    source: &Path,
    profile: &str,
    out: &Path,
    tag_map: &gmeow_pipeline::projections::TagMap,
) -> i32 {
    // The compiled CONSTRUCT for this profile, from the bundle's query archive.
    let queries = match gmeow_pipeline::bundle_blobs::bundled_queries(BUNDLE_GTS) {
        Ok(q) => q,
        Err(e) => return fail(format!("cannot read bundled queries: {e}")),
    };
    let want = format!("{profile}.rq");
    let query = queries
        .iter()
        .find(|(k, _)| k.ends_with(&want))
        .map(|(_, v)| String::from_utf8_lossy(v).into_owned());
    let Some(query) = query else {
        return fail(format!("no bundled CONSTRUCT query for profile {profile}"));
    };

    // source_nt = the bundle ontology base graph + the user's instance data.
    let base = match gmeow_pipeline::projections::gts_base_graph(BUNDLE_GTS) {
        Ok(b) => b,
        Err(e) => return fail(format!("cannot read bundle base graph: {e}")),
    };
    let ontology_nt = match quads_to_nt(&base) {
        Ok(nt) => nt,
        Err(e) => return fail(e),
    };
    let instance_bytes = match read_bytes(source) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let instance_ds = match purrdf::parse_dataset(&instance_bytes, "text/turtle", None) {
        Ok(ds) => ds,
        Err(e) => return fail(format!("cannot parse {}: {e}", source.display())),
    };
    let instance_nt = match purrdf::serialize_dataset(
        &instance_ds,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    ) {
        Ok(b) => String::from_utf8_lossy(&b).into_owned(),
        Err(e) => return fail(format!("cannot project instance to N-Triples: {e}")),
    };
    let source_nt = format!("{ontology_nt}\n{instance_nt}");

    match gmeow_pipeline::projections::project_graph(&source_nt, &query, tag_map) {
        Ok(nt) => match nt_to_turtle(&nt) {
            Ok(ttl) => {
                let target = out.join(format!("{profile}.ttl"));
                if let Err(e) = std::fs::write(&target, ttl) {
                    return fail(format!("cannot write {}: {e}", target.display()));
                }
                println!("wrote {}", target.display());
                0
            }
            Err(e) => fail(e),
        },
        Err(e) => fail(e.to_string()),
    }
}

/// Serialize a flat default-graph quad stream to canonical N-Triples.
fn quads_to_nt(quads: &[purrdf::RdfQuad]) -> Result<String, String> {
    let flat = purrdf::flat_dataset_from_quads(quads)
        .map_err(|e| format!("N-Triples flatten failed: {e}"))?;
    let bytes = purrdf::serialize_dataset(
        &flat,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("N-Triples serialization failed: {e}"))?;
    String::from_utf8(bytes).map_err(|e| format!("N-Triples output is not UTF-8: {e}"))
}

// ── transpile ────────────────────────────────────────────────────────────────

/// `gmeow transpile` — consumer RDF → pure GMEOW → MAXIMAL multi-vocab, or an OKF
/// bundle directory routed through the OKF lift lane.
pub fn transpile(source: &Path, out: Option<&Path>, profiles: &str, lang: Option<&str>) -> i32 {
    use gmeow_pipeline::projections::{self, TagMap};

    let selector_bytes: Cow<'static, [u8]> = Cow::Borrowed(BUNDLE_GTS);
    let _selector = match resolve_selector(lang, &selector_bytes) {
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
            return fail(format!(
                "unknown projection profile(s): {}",
                unknown.join(", ")
            ));
        }
    }

    // Assemble the lawful up-projection + maximal inputs from the embedded bundle.
    let (up_inputs, maximal_inputs) = match assemble_transpile_inputs() {
        Ok(pair) => pair,
        Err(e) => return fail(e),
    };

    // An OKF bundle directory routes through the OKF lift lane.
    if source.is_dir() {
        let report = match gmeow_pipeline::cli_ops::okf_import::transpile_okf(
            source,
            &maximal_inputs,
            None,
            None,
        ) {
            Ok(r) => r,
            Err(e) => return fail(e.to_string()),
        };
        eprintln!(
            "lifted {} okf facts · retained {} annotation(s) · subjects {}",
            report.lift.lifted, report.lift.retained, report.lift.subjects
        );
        return write_transpile_outputs(out, source, &report.draft_nt, &report.transform);
    }

    // A source RDF file (Turtle) or stdin (`-`).
    let (source_nt, stem) = match load_transpile_source(source) {
        Ok(pair) => pair,
        Err(e) => return fail(e),
    };
    match projections::transpile_graph(&source_nt, &stem, &up_inputs, &maximal_inputs, &tag_map) {
        Ok(report) => {
            eprintln!(
                "lifted {} facts · claimed {} inferred · gap {}",
                report.lifted, report.claimed, report.gap_terms
            );
            eprintln!(
                "maximal asserted {} · saturated {} · projected {}",
                report.transform.asserted, report.transform.saturated, report.transform.projected
            );
            write_transpile_outputs(out, source, &report.draft_nt, &report.transform)
        }
        Err(e) => fail(e.to_string()),
    }
}

/// Read a transpile source: Turtle from a file, or Turtle from stdin (`-`).
fn load_transpile_source(source: &Path) -> Result<(String, String), String> {
    let is_stdin = source.as_os_str() == "-";
    let bytes = if is_stdin {
        use std::io::Read;
        let mut buf = Vec::new();
        std::io::stdin()
            .read_to_end(&mut buf)
            .map_err(|e| format!("cannot read stdin: {e}"))?;
        buf
    } else {
        std::fs::read(source).map_err(|e| format!("cannot read {}: {e}", source.display()))?
    };
    let dataset = purrdf::parse_dataset(&bytes, "text/turtle", None)
        .map_err(|e| format!("cannot parse Turtle source: {e}"))?;
    let nt = purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("cannot project source to N-Triples: {e}"))?;
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
fn assemble_transpile_inputs() -> Result<
    (
        gmeow_pipeline::projections::UpProjectionInputs,
        gmeow_pipeline::projections::MaximalInputs,
    ),
    String,
> {
    use gmeow_pipeline::bundle_blobs;

    let sssom_texts: Vec<String> = bundle_blobs::bundled_sssom(BUNDLE_GTS)
        .map_err(|e| format!("cannot read bundled SSSOM: {e}"))?
        .into_values()
        .map(|v| String::from_utf8_lossy(&v).into_owned())
        .collect();
    // The authored `gmeow:ProjectionMapping` cells live in the CELLS archive (the mappings
    // archive holds only the SSSOM surface). Reading REP_CELLS is what puts the EDOAL `=` cells
    // in front of the lawful-lift program; the old REP_MAPPINGS read folded an EMPTY `.ttl` set.
    let projection_ttls: Vec<String> = bundle_blobs::Bundle::from_snapshot(BUNDLE_GTS)
        .map_err(|e| format!("cannot fold bundle: {e}"))?
        .archive(bundle_blobs::REP_CELLS)
        .map_err(|e| format!("cannot read bundled cells: {e}"))?
        .into_iter()
        .filter(|(k, _)| k.ends_with(".ttl"))
        .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
        .collect();
    // The A→B authorization channel: the discharged mnemomorphic `=` cells (Deliverable A),
    // read from the bundle's `graph/correspondence-laws`.
    let discharged_section_cells =
        gmeow_pipeline::projections::discharged_section_cells_from_bundle(BUNDLE_GTS)?;
    let base =
        gmeow_pipeline::projections::gts_base_graph(BUNDLE_GTS).map_err(|e| e.to_string())?;
    let ontology_nt = quads_to_nt(&base)?;

    let projection_queries: Vec<(String, String)> = bundle_blobs::bundled_queries(BUNDLE_GTS)
        .map_err(|e| format!("cannot read bundled queries: {e}"))?
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
        .map_err(|e| format!("cannot read denied cells: {e}"))?
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
        return fail(format!("cannot create {}: {e}", out_dir.display()));
    }
    let draft_path = out_dir.join(format!("{stem}.gmeow.nt"));
    if let Err(e) = std::fs::write(&draft_path, draft_nt) {
        return fail(format!("cannot write {}: {e}", draft_path.display()));
    }
    println!("wrote {}", draft_path.display());
    let gts_path = out_dir.join(format!("{stem}.gts"));
    if let Err(e) = std::fs::write(&gts_path, &transform.gts_bytes) {
        return fail(format!("cannot write {}: {e}", gts_path.display()));
    }
    println!("wrote {}", gts_path.display());
    0
}

// ── export ───────────────────────────────────────────────────────────────────

/// `gmeow export` — write every flat consumer view from a GTS snapshot.
pub fn export(out: &Path, gts: Option<&Path>, lang: Option<&str>) -> i32 {
    let bytes = match gts_bytes(gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let selector = match resolve_selector(lang, &bytes) {
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
        Err(e) => fail(e.to_string()),
    }
}

// ── convert ──────────────────────────────────────────────────────────────────

/// `gmeow convert` — transcode any RDF-1.2 syntax/projection to any other,
/// recording loss.
pub fn convert(
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
            return fail(format!("cannot read stdin: {e}"));
        }
        buf
    } else {
        match std::fs::read(source) {
            Ok(b) => b,
            Err(e) => return fail(format!("cannot read {source}: {e}")),
        }
    };

    let from_codec = match Codec::from_cli_str(from) {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };
    let to_codec = match Codec::from_cli_str(to) {
        Ok(c) => c,
        Err(e) => return fail(e.to_string()),
    };
    let output = match run_transcode(&data, from_codec, to_codec, base) {
        Ok(o) => o,
        Err(e) => return fail(e.to_string()),
    };

    match out {
        Some(path) => {
            if let Err(e) = std::fs::write(path, &output.bytes) {
                return fail(format!("cannot write {}: {e}", path.display()));
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
                return fail(format!("cannot write {}: {e}", path.display()));
            }
            eprintln!("loss {}", path.display());
        }
        None => {
            let trimmed = loss_json.trim();
            if !trimmed.is_empty() && trimmed != "[]" {
                eprintln!("loss {loss_json}");
            }
        }
    }
    0
}

// ── export-docs ──────────────────────────────────────────────────────────────

/// `gmeow export-docs` — write one or every documentation projection (site, mdbook,
/// PDF, snippets) of the bundled docs from a GTS snapshot.
///
/// A single format writes its tree directly into `directory`; `all` writes each
/// projection into its own subdirectory (`site/`, `mdbook/`, `pdf/`, `snippets/`).
/// The mdbook and PDF projections are English-only (they ignore `--lang`).
pub fn export_docs(
    format: &ExportFormat,
    directory: &Path,
    file: Option<&Path>,
    force: bool,
    lang: Option<&str>,
) -> i32 {
    let bytes = match gts_bytes(file) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // Resolve `--lang` to a public BCP-47 tag, then map back to the internal
    // `x-gmeow-*` doc-tree language the site/snippet blobs are keyed by.
    let selector = match resolve_selector(lang, &bytes) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let available = match gmeow_pipeline::cli_ops::confirmations::available_doc_languages(&bytes) {
        Ok(langs) => langs,
        Err(e) => return fail(format!("cannot read docs languages: {e}")),
    };
    let internal = pick_internal_lang(&selector, &available);

    // Guard against clobbering a non-empty directory unless --force is given.
    if !force
        && let Ok(mut entries) = std::fs::read_dir(directory)
        && entries.next().is_some()
    {
        return fail(format!(
            "{} is not empty; pass --force to write into it",
            directory.display()
        ));
    }

    use gmeow_pipeline::cli_ops::confirmations as conf;
    match format {
        ExportFormat::Site => {
            write_docs_projection(directory, conf::export_docs_site(&bytes, &internal))
        }
        ExportFormat::Mdbook => write_docs_projection(directory, conf::export_docs_book(&bytes)),
        ExportFormat::Pdf => write_docs_projection(directory, conf::export_docs_print(&bytes)),
        ExportFormat::Snippets => {
            write_docs_projection(directory, conf::export_docs_snippets(&bytes, &internal))
        }
        ExportFormat::All => {
            let plan = [
                ("site", conf::export_docs_site(&bytes, &internal)),
                ("mdbook", conf::export_docs_book(&bytes)),
                ("pdf", conf::export_docs_print(&bytes)),
                ("snippets", conf::export_docs_snippets(&bytes, &internal)),
            ];
            for (sub, tree) in plan {
                let code = write_docs_projection(&directory.join(sub), tree);
                if code != 0 {
                    return code;
                }
            }
            0
        }
    }
}

/// Write one docs projection tree into `dir`, reporting the confirmations error on a
/// fold/selection failure and any I/O error on write. Returns the process exit code.
fn write_docs_projection(
    dir: &Path,
    tree: Result<std::collections::BTreeMap<String, Vec<u8>>, gmeow_errors::Diag>,
) -> i32 {
    let tree = match tree {
        Ok(t) => t,
        Err(e) => return fail(e.to_string()),
    };
    for (rel, data) in &tree {
        let target = dir.join(rel);
        if let Some(parent) = target.parent()
            && let Err(e) = std::fs::create_dir_all(parent)
        {
            return fail(format!("cannot create {}: {e}", parent.display()));
        }
        if let Err(e) = std::fs::write(&target, data) {
            return fail(format!("cannot write {}: {e}", target.display()));
        }
    }
    println!("wrote docs -> {}", dir.display());
    0
}

// ── docs-on ──────────────────────────────────────────────────────────────────

/// `gmeow docs-on` — print the documentation page (or `--card`) for one GMEOW term
/// from a GTS snapshot's ontology-docs blob.
pub fn docs_on(term: &str, card: bool, gts: Option<&Path>, lang: Option<&str>) -> i32 {
    use gmeow_docs::describe::{DescribeGraph, resolve_term};

    let bytes = match gts_bytes(gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let graph = match DescribeGraph::from_gts_bytes(&bytes) {
        Ok(g) => g,
        Err(e) => return fail(e),
    };
    let (resolved, candidates) = resolve_term(&graph, term);
    let Some(iri) = resolved else {
        // Mirror `describe`'s ambiguity/no-match handling: list candidates (if any)
        // to stderr and exit non-zero.
        if candidates.is_empty() {
            return fail(format!("no GMEOW term matches '{term}'"));
        }
        let options = candidates
            .iter()
            .map(|c| format!("  gmeow:{c}"))
            .collect::<Vec<_>>()
            .join("\n");
        return fail(format!(
            "ambiguous or unknown term '{term}' — candidates:\n{options}"
        ));
    };

    // Map `--lang` to the internal `x-gmeow-*` doc-tree language the page is keyed by.
    let selector = match resolve_selector(lang, &bytes) {
        Ok(s) => s,
        Err(code) => return code,
    };
    let available = match gmeow_pipeline::cli_ops::confirmations::available_doc_languages(&bytes) {
        Ok(langs) => langs,
        Err(e) => return fail(format!("cannot read docs languages: {e}")),
    };
    let internal = pick_internal_lang(&selector, &available);

    let docs = match gmeow_pipeline::bundle_blobs::bundled_ontology_docs(&bytes) {
        Ok(d) => d,
        Err(e) => return fail(format!("snapshot carries no ontology-docs pages: {e}")),
    };
    let slug = gmeow_docs::render::slug_for_iri(&iri);
    let leaf = if card { "card.md" } else { "index.md" };
    let key = format!("{internal}/terms/{slug}/{leaf}");
    match docs.get(&key) {
        Some(data) => {
            print!("{}", String::from_utf8_lossy(data));
            0
        }
        None => fail(format!(
            "no documentation page for {} (expected {key} in the ontology-docs blob)",
            iri
        )),
    }
}

/// Choose the internal `x-gmeow-*` doc language for the requested selector: the
/// first requested tag with a matching `x-gmeow-<...>` subtree, else English.
fn pick_internal_lang(
    selector: &gmeow_validate::language_tags::LangSelector,
    available: &[String],
) -> String {
    let english = "x-gmeow-english".to_owned();
    for req in &selector.requested {
        // The internal tags carry the language name (`x-gmeow-french`); a BCP-47
        // request like `fr` won't substring-match, so English is the safe default
        // when no exact internal tag is requested. An internal tag passed through
        // `--lang` maps to itself.
        if let Some(hit) = available.iter().find(|a| a.as_str() == req) {
            return hit.clone();
        }
    }
    if available.iter().any(|a| a == &english) {
        english
    } else {
        available.first().cloned().unwrap_or(english)
    }
}

// ── crossref ─────────────────────────────────────────────────────────────────

/// `gmeow crossref` — generate CrossRef DOI deposit XML from self-description.
pub fn crossref(out: &Path, gts: Option<&Path>) -> i32 {
    let bytes = match gts_bytes(gts) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let dataset = match purrdf::gts::flattened_dataset_from_bytes(&bytes) {
        Ok(ds) => ds,
        Err(e) => return fail(format!("cannot fold snapshot: {e}")),
    };
    let meta = match gmeow_validate::self_desc::load_self_description_from_dataset(&dataset) {
        Ok(m) => m,
        Err(e) => return fail(format!("self-description unavailable in GTS snapshot: {e}")),
    };
    let lint_json = match gmeow_validate::self_desc::lint_input_json(&meta, None, None) {
        Ok(j) => j,
        Err(e) => return fail(format!("cannot assemble lint input: {e}")),
    };
    match gmeow_validate::crossref::lint_deposit(&lint_json) {
        Ok(problems) if problems.is_empty() => {}
        Ok(problems) => {
            for p in &problems {
                eprintln!("doi-lint {p}");
            }
            return fail(format!(
                "✗ {} doi-lint problem(s) — fix metadata/gmeow-self.ttl",
                problems.len()
            ));
        }
        Err(e) => return fail(format!("doi-lint failed: {e}")),
    }
    let (ts, batch) = gmeow_validate::self_desc::live_stamp(&meta);
    let deposit_json = match gmeow_validate::self_desc::deposit_input_json(&meta) {
        Ok(j) => j,
        Err(e) => return fail(format!("cannot assemble deposit input: {e}")),
    };
    let xml = match gmeow_validate::crossref::build_deposit_xml(&deposit_json, &ts, &batch) {
        Ok(x) => x,
        Err(e) => return fail(format!("cannot build deposit XML: {e}")),
    };
    if let Some(parent) = out.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return fail(format!("cannot create {}: {e}", parent.display()));
    }
    if let Err(e) = std::fs::write(out, format!("{xml}\n")) {
        return fail(format!("cannot write {}: {e}", out.display()));
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
pub fn mcp() -> i32 {
    use gmeow_pipeline::mcp::{McpMode, McpServer};
    let server = match McpServer::from_snapshot(BUNDLE_GTS, None, McpMode::Consumer) {
        Ok(server) => server,
        Err(e) => return fail(format!("mcp: {e}")),
    };
    match server.run_stdio() {
        Ok(()) => 0,
        Err(e) => fail(format!("mcp: {e}")),
    }
}

/// A tiny scoped temp file: writes bytes to a uniquely named file under the
/// system temp dir and removes it on drop. Used to stage the embedded bundle for
/// the external `gts` binary, which expects a filesystem path.
mod tempfile_path {
    use std::path::{Path, PathBuf};

    /// A temp file removed on drop.
    pub struct Temp {
        path: PathBuf,
    }

    impl Temp {
        /// Write `bytes` to a fresh temp file and return its handle.
        pub fn write(bytes: &[u8]) -> std::io::Result<Self> {
            let mut path = std::env::temp_dir();
            let unique = format!(
                "gmeow-{}-{}.gts",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            path.push(unique);
            std::fs::write(&path, bytes)?;
            Ok(Self { path })
        }

        /// The staged file's path.
        pub fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}
