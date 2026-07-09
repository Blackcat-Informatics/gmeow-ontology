// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repo-anchored projection / description commands: `describe`, `export-docs`,
//! `docs-on`, `temporal`, `import-foundation`, `crossref`, and `compliance-report`.

use std::collections::BTreeMap;
use std::path::Path;

use crate::dev_common::{fail, fail_code, project_root, snapshot_bytes};

/// `gmeow-dev describe TERM [--gts --lang]` — render one term card.
pub fn describe(term: &str, gts: Option<&Path>, lang: Option<&str>) -> i32 {
    let root = project_root();
    let bytes = match gts {
        Some(path) => match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        },
        None => match snapshot_bytes(&root) {
            Ok(b) => b,
            Err(code) => return code,
        },
    };
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

/// `gmeow-dev export-docs [GTS] --format F -d DIR [--force --lang]` — write one (or
/// every) documentation projection from a GTS snapshot.
pub fn export_docs(
    gts_file: Option<&Path>,
    format: &crate::ExportFormat,
    directory: &Path,
    force: bool,
    lang: Option<&str>,
) -> i32 {
    let root = project_root();
    let bytes = match gts_file {
        Some(path) => match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        },
        None => match snapshot_bytes(&root) {
            Ok(b) => b,
            Err(code) => return code,
        },
    };
    let available = match gmeow_pipeline::cli_ops::confirmations::available_doc_languages(&bytes) {
        Ok(langs) => langs,
        Err(e) => return fail(format!("cannot read docs languages: {e}")),
    };
    let internal = pick_internal_lang(lang, &available);

    if !force
        && let Ok(mut entries) = std::fs::read_dir(directory)
        && entries.next().is_some()
    {
        return fail(format!(
            "{} is not empty; pass --force to write into it",
            directory.display()
        ));
    }

    use crate::ExportFormat;
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
    tree: Result<BTreeMap<String, Vec<u8>>, gmeow_errors::Diag>,
) -> i32 {
    let tree = match tree {
        Ok(t) => t,
        Err(e) => return fail(format!("cannot create docs tree: {e}")),
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
    println!("docs -> {}", dir.display());
    0
}

/// `gmeow-dev docs-on TERM [--card --gts --lang]` — print one term's documentation
/// page (or its prompt-ready card) from a GTS snapshot's ontology-docs blob.
pub fn docs_on(term: &str, card: bool, gts: Option<&Path>, lang: Option<&str>) -> i32 {
    use gmeow_docs::describe::{DescribeGraph, resolve_term};

    let root = project_root();
    let bytes = match gts {
        Some(path) => match std::fs::read(path) {
            Ok(b) => b,
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        },
        None => match snapshot_bytes(&root) {
            Ok(b) => b,
            Err(code) => return code,
        },
    };
    let graph = match DescribeGraph::from_gts_bytes(&bytes) {
        Ok(g) => g,
        Err(e) => return fail(e),
    };
    let (resolved, candidates) = resolve_term(&graph, term);
    let Some(iri) = resolved else {
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

    let available = match gmeow_pipeline::cli_ops::confirmations::available_doc_languages(&bytes) {
        Ok(langs) => langs,
        Err(e) => return fail(format!("cannot read docs languages: {e}")),
    };
    let internal = pick_internal_lang(lang, &available);

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
            "snapshot has no {leaf} page for '{term}' (gmeow:{slug}) in {internal}"
        )),
    }
}

/// Choose the internal `x-gmeow-*` docs language: an exact requested internal tag,
/// else English, else the first available.
fn pick_internal_lang(lang: Option<&str>, available: &[String]) -> String {
    let english = "x-gmeow-english".to_owned();
    let requested = lang
        .map(str::to_owned)
        .or_else(|| std::env::var("GMEOW_LANG").ok());
    if let Some(req) = requested {
        for tag in req.split(',').map(str::trim) {
            if let Some(hit) = available.iter().find(|a| a.as_str() == tag) {
                return hit.clone();
            }
        }
    }
    if available.iter().any(|a| a == &english) {
        english
    } else {
        available.first().cloned().unwrap_or(english)
    }
}

/// `gmeow-dev temporal QUERY [--data --focus --window-* --valid-at --as-of]`.
#[allow(clippy::too_many_arguments)]
pub fn temporal(
    query: &str,
    data: Option<&Path>,
    focus: Option<&str>,
    window_start: Option<&str>,
    window_end: Option<&str>,
    valid_at: Option<&str>,
    as_of: Option<&str>,
) -> i32 {
    let root = project_root();
    let query_dir = root.join("slices/core/temporal/queries/tql");
    let queries = gmeow_pipeline::cli_ops::temporal::temporal_queries();
    if !queries.contains_key(query) {
        eprintln!("unknown TQL query {query:?}. Available:");
        for (name, q) in &queries {
            eprintln!("  {name:<20} {}", q.summary);
        }
        return fail(format!("unknown TQL query {query:?}"));
    }

    // The events graph = the authored temporal sources merged with any --data file.
    let mut source_ttl = String::new();
    if let Some(path) = data {
        match std::fs::read_to_string(path) {
            Ok(s) => source_ttl.push_str(&s),
            Err(e) => return fail(format!("cannot read {}: {e}", path.display())),
        }
    }
    // Merge the committed temporal module so the query has an events model even
    // without a --data file.
    let module = root.join("slices/core/temporal/module.ttl");
    if let Ok(s) = std::fs::read_to_string(&module) {
        source_ttl.push('\n');
        source_ttl.push_str(&s);
    }

    const XSD_DT: &str = "http://www.w3.org/2001/XMLSchema#dateTime";
    let mut bindings: Vec<(String, purrdf::TermValue)> = Vec::new();
    if let Some(f) = focus {
        bindings.push(("focus".to_owned(), purrdf::TermValue::iri(f)));
    }
    if let Some(v) = window_start {
        bindings.push((
            "windowStart".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }
    if let Some(v) = window_end {
        bindings.push((
            "windowEnd".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }
    if let Some(v) = valid_at {
        bindings.push((
            "validAt".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }
    if let Some(v) = as_of {
        bindings.push((
            "asOf".to_owned(),
            purrdf::TermValue::typed_literal(v, XSD_DT),
        ));
    }

    match gmeow_pipeline::cli_ops::temporal::run_temporal_query(
        &query_dir,
        query,
        &source_ttl,
        &bindings,
    ) {
        Ok(solutions) => {
            for row in &solutions.rows {
                let rendered: Vec<String> = row
                    .iter()
                    .map(|v| v.as_ref().map(|t| format!("{t:?}")).unwrap_or_default())
                    .collect();
                println!("{}", rendered.join(" "));
            }
            println!("{query}: {} row(s)", solutions.rows.len());
            0
        }
        Err(e) => fail(format!("temporal query failed: {e}")),
    }
}

/// `gmeow-dev import-foundation JSONL --out --nq`.
pub fn import_foundation(jsonl: &Path, out_dir: &Path, nq: Option<&Path>) -> i32 {
    match gmeow_foundation_corpus::run_import(jsonl, out_dir, nq) {
        Ok((_dataset, budget)) => {
            println!("{}", budget.as_text());
            println!("artifacts -> {}", out_dir.display());
            0
        }
        Err(e) => fail(format!("import-foundation failed: {e}")),
    }
}

/// `gmeow-dev crossref` — generate CrossRef DOI deposit XML from self-description.
pub fn crossref() -> i32 {
    let root = project_root();
    let bytes = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    let dataset = match purrdf::gts::flattened_dataset_from_bytes(&bytes) {
        Ok(ds) => ds,
        Err(e) => return fail(format!("cannot fold snapshot: {e}")),
    };
    let meta = match gmeow_validate::self_desc::load_self_description_from_dataset(&dataset) {
        Ok(m) => m,
        Err(e) => return fail(format!("self-description unavailable: {e}")),
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
            return fail(format!("{} doi-lint problem(s)", problems.len()));
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
    let out = root.join("dist").join("crossref-deposit.xml");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out, format!("{xml}\n")) {
        return fail(format!("cannot write {}: {e}", out.display()));
    }
    println!("wrote {}", out.display());
    0
}

/// `gmeow-dev compliance-report [--from-passing-check]`.
pub fn compliance_report(from_passing_check: bool) -> i32 {
    let root = project_root();
    let manifest = root.join("governance").join("constitution.ttl");
    let constitution = root.join("CONSTITUTION.md");
    let gate_runs: BTreeMap<String, gmeow_validate::compliance::GateRun> = BTreeMap::new();
    let evidence_mode = if from_passing_check {
        "from-passing-check"
    } else {
        "in-process"
    };
    let report = match gmeow_validate::compliance::compliance_report(
        &manifest,
        &constitution,
        &root,
        &gate_runs,
        env!("CARGO_PKG_VERSION"),
        evidence_mode,
    ) {
        Ok(r) => r,
        Err(e) => return fail(format!("compliance-report failed: {e}")),
    };
    let out = root.join("dist").join("compliance-report.ttl");
    if let Some(parent) = out.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Err(e) = std::fs::write(&out, report) {
        return fail(format!("cannot write {}: {e}", out.display()));
    }
    println!("compliance report written to {}", out.display());
    0
}

// ── up-projection-audit ─────────────────────────────────────────────────────

/// `gmeow-dev up-projection-audit [--report --gaps]` — the correspondence-gate
/// verdict ledger over the vendored real-world corpus.
pub fn up_projection_audit(report_path: Option<&Path>, show_gaps: bool) -> i32 {
    let root = project_root();
    let snapshot = match snapshot_bytes(&root) {
        Ok(b) => b,
        Err(code) => return code,
    };
    // The SSSOM lift maps + projection/EDOAL TTLs folded into the bundle.
    let sssom_texts: Vec<String> = match gmeow_pipeline::bundle_blobs::bundled_sssom(&snapshot) {
        Ok(m) => m
            .into_values()
            .map(|v| String::from_utf8_lossy(&v).into_owned())
            .collect(),
        Err(e) => return fail(format!("cannot read bundled SSSOM: {e}")),
    };
    let projection_ttls: Vec<String> =
        match gmeow_pipeline::bundle_blobs::Bundle::from_snapshot(&snapshot) {
            Ok(b) => match b.archive(gmeow_pipeline::bundle_blobs::REP_MAPPINGS) {
                Ok(a) => a
                    .into_iter()
                    .filter(|(k, _)| k.ends_with(".ttl"))
                    .map(|(_, v)| String::from_utf8_lossy(&v).into_owned())
                    .collect(),
                Err(e) => return fail(format!("cannot read bundled mappings: {e}")),
            },
            Err(e) => return fail(format!("cannot fold bundle: {e}")),
        };
    // The fixed real-world corpus snapshots (never authored fixtures).
    let mut corpus: Vec<(String, String)> = Vec::new();
    for name in ["bii", "paudley"] {
        let path = root
            .join("tests/fixtures/coverage/external")
            .join(format!("{name}.ttl"));
        match std::fs::read_to_string(&path) {
            Ok(text) => corpus.push((name.to_owned(), text)),
            Err(e) => return fail(format!("cannot read corpus {}: {e}", path.display())),
        }
    }

    let (ledger, markdown) = match gmeow_pipeline::cli_ops::confirmations::up_projection_gate_audit(
        &sssom_texts,
        &projection_ttls,
        &corpus,
    ) {
        Ok(pair) => pair,
        Err(e) => return fail(format!("up-projection-audit failed: {e}")),
    };
    if let Some(path) = report_path {
        if let Err(e) = std::fs::write(path, &markdown) {
            return fail(format!("cannot write {}: {e}", path.display()));
        }
        println!("wrote {}", path.display());
    }
    let liftable = ledger.totals.liftable();
    let total = ledger.totals.total();
    let pct = liftable
        .checked_mul(100)
        .and_then(|n| n.checked_div(total))
        .unwrap_or(0);
    println!(
        "liftable {liftable}/{total} ({pct}%) · proved {} · claimed {} · excluded {} · unsupported {}",
        ledger.totals.proved,
        ledger.totals.claimed,
        ledger.totals.red_excluded,
        ledger.totals.unsupported
    );
    println!("gaps {} distinct terms", ledger.gaps.len());
    if show_gaps {
        for term in &ledger.gaps {
            eprintln!("gap {term}");
        }
    }
    0
}

// ── refresh-target-axioms ────────────────────────────────────────────────────

/// One IMPORT_OK target's canonical source document (mirrors the pipeline's
/// `TARGET_SOURCES` — reference-only targets are fetched live at lint time, never
/// vendored, so they are absent here).
struct TargetSource {
    prefix: &'static str,
    url: &'static str,
    media_type: &'static str,
}

/// The vendorable target-axiom sources (IMPORT_OK license family only).
const TARGET_SOURCES: &[TargetSource] = &[
    TargetSource {
        prefix: "org",
        url: "https://www.w3.org/ns/org.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "foaf",
        url: "http://xmlns.com/foaf/spec/index.rdf",
        media_type: "application/rdf+xml",
    },
    TargetSource {
        prefix: "vcard",
        url: "https://www.w3.org/2006/vcard/ns.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "prov",
        url: "https://www.w3.org/ns/prov-o.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "time",
        url: "https://www.w3.org/2006/time.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "geo",
        url: "https://opengeospatial.github.io/ogc-geosparql/geosparql11/geo.ttl",
        media_type: "text/turtle",
    },
    TargetSource {
        prefix: "bfo",
        url: "http://purl.obolibrary.org/obo/bfo.owl",
        media_type: "application/rdf+xml",
    },
];

/// The structural predicates a vendored target snapshot keeps: domain/range,
/// inverse, and the property-type declarations.
const STRUCTURAL_PREDICATES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#domain",
    "http://www.w3.org/2000/01/rdf-schema#range",
    "http://www.w3.org/2000/01/rdf-schema#subPropertyOf",
    "http://www.w3.org/2002/07/owl#inverseOf",
];

/// `gmeow-dev refresh-target-axioms [--target]` — re-vendor minimal target-axiom
/// snapshots into `imports/targets/`. Network; IMPORT_OK targets only.
pub fn refresh_target_axioms(target: &str) -> i32 {
    let root = project_root();
    let out_dir = root.join("imports").join("targets");
    if let Err(e) = std::fs::create_dir_all(&out_dir) {
        return fail(format!("cannot create {}: {e}", out_dir.display()));
    }
    let selected: Vec<&TargetSource> = if target == "all" {
        TARGET_SOURCES.iter().collect()
    } else {
        TARGET_SOURCES
            .iter()
            .filter(|s| s.prefix == target)
            .collect()
    };
    if selected.is_empty() {
        // A named target that is not IMPORT_OK is skipped with a clear note, never
        // vendored (reference-only targets are fetched live at lint time).
        eprintln!("skip {target}: not an IMPORT_OK vendorable target (reference-only or unknown)");
        return 0;
    }
    let mut written = 0usize;
    for source in selected {
        match refresh_one(source, &out_dir) {
            Ok(path) => {
                println!("{}", path.display());
                written += 1;
            }
            Err(e) => return fail_code(format!("fetch failed for {}: {e}", source.prefix), 2),
        }
    }
    println!("refreshed {written} target snapshot(s)");
    0
}

/// Fetch, structurally filter, and write one target's axiom snapshot.
fn refresh_one(source: &TargetSource, out_dir: &Path) -> Result<std::path::PathBuf, String> {
    // A network vendor step must fail fast rather than hang: cap the whole
    // request/response with a global timeout so an unreachable or stalled remote
    // surfaces as an error instead of blocking the CLI indefinitely.
    let body = ureq::get(source.url)
        .config()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .build()
        .call()
        .map_err(|e| format!("HTTP {e}"))?
        .into_body()
        .read_to_string()
        .map_err(|e| format!("read body: {e}"))?;
    let media = if source.media_type.contains("rdf+xml") {
        "application/rdf+xml"
    } else {
        "text/turtle"
    };
    let dataset =
        purrdf::parse_dataset(body.as_bytes(), media, None).map_err(|e| format!("parse: {e}"))?;

    // Keep only the structural-axiom quads (domain / range / subPropertyOf /
    // inverseOf, plus property-type declarations) — a minimal, deterministic
    // vendored snapshot. Filtering the parsed quads in memory (rather than a
    // serialize → line-match → re-parse round trip) is exact: it matches on the
    // predicate term itself, so a literal that happens to embed a predicate URI
    // can never masquerade as a structural axiom.
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let mut filtered_quads = Vec::new();
    for quad in dataset.owned_quads() {
        let pred = quad.predicate.as_str();
        let keep = STRUCTURAL_PREDICATES.contains(&pred)
            || (pred == rdf_type
                && matches!(&quad.object, purrdf::RdfTerm::Iri(iri) if iri.ends_with("Property")));
        if keep {
            filtered_quads.push(quad);
        }
    }
    let filtered = purrdf::flat_dataset_from_quads(&filtered_quads)
        .map_err(|e| format!("flatten filtered: {e}"))?;
    let prefixes = vec![(source.prefix.to_owned(), namespace_for(source.prefix))];
    let ttl = purrdf::turtle_normalize::render(&filtered, &prefixes);
    let path = out_dir.join(format!("{}.ttl", source.prefix));
    std::fs::write(&path, ttl).map_err(|e| format!("write {}: {e}", path.display()))?;
    Ok(path)
}

/// A best-effort namespace binding for a target prefix (cosmetic in the snapshot).
fn namespace_for(prefix: &str) -> String {
    match prefix {
        "org" => "http://www.w3.org/ns/org#",
        "foaf" => "http://xmlns.com/foaf/0.1/",
        "vcard" => "http://www.w3.org/2006/vcard/ns#",
        "prov" => "http://www.w3.org/ns/prov#",
        "time" => "http://www.w3.org/2006/time#",
        "geo" => "http://www.opengis.net/ont/geosparql#",
        "bfo" => "http://purl.obolibrary.org/obo/",
        _ => "http://example.org/",
    }
    .to_owned()
}
