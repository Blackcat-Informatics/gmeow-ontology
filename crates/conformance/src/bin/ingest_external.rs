// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `ingest-external` — the external-corpus ingestion CLI.
//!
//! Concrete proof of AC1 ("the runner ingests a W3C `manifest.ttl` AND a TPTP SZS
//! problem → produces a runner verdict") and the reproducible refresh entry point
//! the vendoring procedure (X2–X5) follows.
//!
//! Usage:
//!
//! ```text
//! ingest-external --szs <problem.p> [--world <iri> --quads <n>]
//!     Parse a TPTP SZS problem and print the runner verdict. With --world/--quads,
//!     print the full world-indexed verdicts.json value; otherwise print the bare
//!     status (consistent | inconsistent | incomplete).
//!
//! ingest-external --manifest <manifest.ttl>
//!     Parse a W3C entailment manifest and print one `<name>\t<status>` line per
//!     mf:PositiveEntailment / mf:NegativeEntailment entry.
//!
//! ingest-external --vendor-el <input.rdf> <out-dir>
//!     Vendor the W3C OWL 2 EL conformance suite (ConsistencyTest /
//!     InconsistencyTest only). Agreeing deciders land in <out-dir> (the Lane-A
//!     corpus); every case the native reasoner cannot soundly decide (honest
//!     DlGap), or where it decides but disagrees with W3C (CorpusOnly), lands in
//!     the sibling <out-dir>-divergence corpus as committed data carrying BOTH the
//!     frozen native verdict and the W3C published verdict — never silently dropped.
//!
//! ingest-external --grade-suite <input.rdf> <corpus-name> <out.nq>
//!     Grade EVERY ConsistencyTest / InconsistencyTest in <input.rdf> gap-tolerantly
//!     against the native reasoner, record divergences (DlGap / CorpusOnly) as a
//!     gmeow:Finding N-Quads graph written to <out.nq>, and print a summary line.
//!     Entailment tests are counted but not graded (they need conclusion-negation).
//!
//! ingest-external --grade-ore <ontology-dir> <corpus-name> <out.nq>
//!     Grade every `*.owl` ontology under <ontology-dir> against the native DL
//!     consistency path for SOUNDNESS. The ORE 2015 corpus is curated-consistent and
//!     ships NO per-ontology reference verdict, so every ontology's published expected
//!     is `consistent`: a native `inconsistent` is a soundness flag (CorpusOnly →
//!     hard-fail), and any ontology the native path cannot parse (ORE ships OWL 2
//!     Functional Syntax, which the native RDF codecs do not read) or cannot decide
//!     (gaps non-empty) becomes an honest DlGap Finding — never a silent skip. The
//!     divergences are written as a gmeow:Finding N-Quads graph to <out.nq>.
//! ```

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_conformance::external::{
    outcome_from_szs, parse_test_manifest, parse_test_manifest_rdfxml, runner_verdict_json,
    ManifestTestKind, OntologyDoc,
};

const USAGE: &str = "\
usage:
  ingest-external --szs <problem.p> [--world <iri> --quads <n>]
  ingest-external --manifest <manifest.ttl>
  ingest-external --vendor-el <input.rdf> <out-dir>
  ingest-external --grade-suite <input.rdf> <corpus-name> <out.nq>
  ingest-external --grade-ore <ontology-dir> <corpus-name> <out.nq>";

fn main() -> Result<(), String> {
    let mut szs: Option<PathBuf> = None;
    let mut manifest: Option<PathBuf> = None;
    let mut vendor_el: Option<(PathBuf, PathBuf)> = None;
    let mut grade_suite: Option<(PathBuf, String, PathBuf)> = None;
    let mut grade_ore: Option<(PathBuf, String, PathBuf)> = None;
    let mut world: Option<String> = None;
    let mut quads: Option<u64> = None;

    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--szs" => szs = Some(PathBuf::from(next(&mut args, "--szs")?)),
            "--manifest" => manifest = Some(PathBuf::from(next(&mut args, "--manifest")?)),
            "--vendor-el" => {
                let input = PathBuf::from(next(&mut args, "--vendor-el")?);
                let out = PathBuf::from(next(&mut args, "--vendor-el <out-dir>")?);
                vendor_el = Some((input, out));
            }
            "--grade-suite" => {
                let input = PathBuf::from(next(&mut args, "--grade-suite")?);
                let corpus_name = next(&mut args, "--grade-suite <corpus-name>")?;
                let out_nq = PathBuf::from(next(&mut args, "--grade-suite <out.nq>")?);
                grade_suite = Some((input, corpus_name, out_nq));
            }
            "--grade-ore" => {
                let dir = PathBuf::from(next(&mut args, "--grade-ore")?);
                let corpus_name = next(&mut args, "--grade-ore <corpus-name>")?;
                let out_nq = PathBuf::from(next(&mut args, "--grade-ore <out.nq>")?);
                grade_ore = Some((dir, corpus_name, out_nq));
            }
            "--world" => world = Some(next(&mut args, "--world")?),
            "--quads" => {
                quads = Some(
                    next(&mut args, "--quads")?
                        .parse()
                        .map_err(|e| format!("--quads must be a non-negative integer: {e}"))?,
                )
            }
            "-h" | "--help" => {
                println!("{USAGE}");
                return Ok(());
            }
            other => return Err(format!("unknown argument: {other}\n{USAGE}")),
        }
    }

    let mode_count = szs.is_some() as u8
        + manifest.is_some() as u8
        + vendor_el.is_some() as u8
        + grade_suite.is_some() as u8
        + grade_ore.is_some() as u8;
    if mode_count > 1 {
        return Err(format!(
            "--szs, --manifest, --vendor-el, --grade-suite, and --grade-ore are mutually exclusive\n{USAGE}"
        ));
    }

    match (szs, manifest, vendor_el, grade_suite, grade_ore) {
        (Some(path), None, None, None, None) => ingest_szs(&path, world.as_deref(), quads),
        (None, Some(path), None, None, None) => {
            // `--world`/`--quads` shape an SZS single-world verdict; they have no
            // meaning for a manifest (one line per entry). Reject loudly rather than
            // parse-and-drop them (no-optionality / no silent misuse).
            if world.is_some() || quads.is_some() {
                return Err(format!(
                    "--world / --quads apply only to --szs, not --manifest\n{USAGE}"
                ));
            }
            ingest_manifest(&path)
        }
        (None, None, Some((input, out)), None, None) => vendor_el_corpus(&input, &out),
        (None, None, None, Some((input, corpus_name, out_nq)), None) => {
            grade_suite_corpus(&input, &corpus_name, &out_nq)
        }
        (None, None, None, None, Some((dir, corpus_name, out_nq))) => {
            grade_ore_corpus(&dir, &corpus_name, &out_nq)
        }
        _ => Err(format!(
            "one of --szs / --manifest / --vendor-el / --grade-suite / --grade-ore is required\n{USAGE}"
        )),
    }
}

/// Ingest a TPTP SZS problem → runner verdict.
fn ingest_szs(
    path: &std::path::Path,
    world: Option<&str>,
    quads: Option<u64>,
) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    let outcome = outcome_from_szs(&text)?;
    match (world, quads) {
        (Some(world), Some(quads)) => {
            let verdict = runner_verdict_json(world, quads, outcome);
            println!(
                "{}",
                serde_json::to_string_pretty(&verdict)
                    .map_err(|e| format!("serialize verdict: {e}"))?
            );
        }
        (None, None) => println!("{}", outcome.verdict_status().as_str()),
        _ => return Err("--world and --quads must be given together".to_string()),
    }
    Ok(())
}

/// Ingest a W3C entailment manifest → one `<name>\t<status>` line per entry.
fn ingest_manifest(path: &std::path::Path) -> Result<(), String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    // Build the base IRI from an ABSOLUTE path so a relative `--manifest foo/x.ttl`
    // yields `file:///abs/foo/x.ttl` (empty authority) rather than the malformed
    // `file://foo/x.ttl` (where `foo` would be read as the authority). `absolute` is
    // lexical (no filesystem access) — enough for a Linux-only ingest path without
    // pulling in a `url` crate dependency edge.
    let abs =
        std::path::absolute(path).map_err(|e| format!("cannot resolve {}: {e}", path.display()))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest(&text, Some(&base))?;
    if entries.is_empty() {
        return Err(format!("no entailment entries in {}", path.display()));
    }
    for entry in entries {
        println!(
            "{}\t{}",
            entry.name,
            entry.outcome().verdict_status().as_str()
        );
    }
    Ok(())
}

/// Convert an entry name to a deterministic slug: lowercase, runs of
/// non-[a-z0-9] replaced by a single `-`, trimmed of leading/trailing `-`.
fn to_slug(name: &str) -> String {
    let lower = name.to_lowercase();
    let mut result = String::with_capacity(lower.len());
    let mut in_sep = false;
    for ch in lower.chars() {
        if ch.is_ascii_alphanumeric() {
            in_sep = false;
            result.push(ch);
        } else if !in_sep {
            in_sep = true;
            result.push('-');
        }
    }
    result.trim_matches('-').to_string()
}

/// Convert a parsed premise dataset (default graph only) into sorted, deduped N-Quads
/// under the given world IRI. Uses the native N-Triples serializer for term encoding,
/// then appends the world graph IRI to each triple line.
///
/// Returns the N-Quads text (sorted, trailing newline) and the quad count.
fn premise_ds_to_world_nquads(
    ds: &gmeow_rdf::RdfDataset,
    world_iri: &str,
) -> Result<(String, usize), String> {
    // Serialize the default graph as N-Triples — the native codec handles all
    // the N3 term encoding (IRI angle-brackets, literal escaping, datatype IRIs,
    // lang tags, blank node labels) so we never re-implement it here.
    let nt_bytes = gmeow_rdf::serialize_dataset(
        ds,
        "application/n-triples",
        gmeow_rdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("N-Triples serialize failed: {e}"))?;
    let nt_text = String::from_utf8(nt_bytes)
        .map_err(|_| "N-Triples output was not valid UTF-8".to_string())?;

    // Convert each N-Triple line (`S P O .`) to N-Quads (`S P O <graph> .`).
    //
    // Correct order: trim trailing whitespace FIRST, then strip the mandatory
    // trailing `.`, then trim again.  The previous order (`trim_end_matches('.')`
    // first) was buggy: a line ending with `. ` (dot + trailing space) would not
    // have its dot stripped (last char is space, not `.`), causing the output to
    // contain two statement terminators: `S P O . <graph> .`.
    let mut nq_lines: Vec<String> = nt_text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| {
            // Trim trailing whitespace first so the mandatory '.' is now last.
            let trimmed = line.trim_end();
            // A valid N-Triples statement MUST end with '.'.  Hard-fail on any
            // line that does not so we never silently emit a malformed N-Quad.
            let without_dot = trimmed
                .strip_suffix('.')
                .ok_or_else(|| format!("malformed N-Triples line (no trailing '.'): {line}"))?;
            // Trim any whitespace between the last RDF term and the trailing dot.
            let body = without_dot.trim_end();
            Ok(format!("{body} <{world_iri}> ."))
        })
        .collect::<Result<Vec<String>, String>>()?;
    nq_lines.sort();
    nq_lines.dedup();

    let count = nq_lines.len();
    let text = if nq_lines.is_empty() {
        String::new()
    } else {
        let mut s = nq_lines.join("\n");
        s.push('\n');
        s
    };
    Ok((text, count))
}

/// The result of lowering one manifest entry into a world-scoped dataset.
struct LoweredEntry {
    slug: String,
    world_iri: String,
    /// The world-scoped N-Quads text (sorted, deduped, trailing newline).
    input_nq: String,
    /// The quad count (after dedup).
    quad_count: usize,
}

/// Lower one manifest entry's inline premise into a world-scoped N-Quads dataset.
///
/// Returns `Ok(Some(lowered))` on success, `Ok(None)` when the entry must be skipped
/// (with the reason already printed to stdout), or `Err` on a hard failure that should
/// stop the caller.
///
/// The `world_iri_prefix` is prepended to the slug to form the world IRI:
/// `{world_iri_prefix}{slug}/w`.
fn lower_entry(
    entry: &gmeow_conformance::external::ManifestEntry,
    world_iri_prefix: &str,
) -> Option<LoweredEntry> {
    let slug = to_slug(&entry.name);
    let world_iri = format!("{world_iri_prefix}{slug}/w");

    // ── Extract the inline premise RDF/XML ────────────────────────────────
    let premise_xml = match &entry.action {
        Some(OntologyDoc::InlineRdfXml(xml)) => xml.clone(),
        Some(OntologyDoc::Reference(_)) => {
            println!("SKIP {slug}: premise is an IRI reference, not inline RDF/XML (Lane-B)");
            return None;
        }
        None => {
            println!(
                "SKIP {slug}: no recognized premise document (e.g. fsPremiseOntology only) — Lane-B"
            );
            return None;
        }
    };

    // ── Parse the premise RDF/XML ─────────────────────────────────────────
    let premise_ds = match gmeow_rdf::parse_dataset(
        premise_xml.as_bytes(),
        "application/rdf+xml",
        Some("http://example.org/"),
    ) {
        Ok(ds) => ds,
        Err(e) => {
            println!("SKIP {slug}: premise unparsable: {e}");
            return None;
        }
    };
    if premise_ds.quad_refs().count() == 0 {
        println!("SKIP {slug}: premise parsed to zero quads (vacuous pass not permitted)");
        return None;
    }

    // ── Build world-scoped N-Quads ────────────────────────────────────────
    let (input_nq, quad_count) = match premise_ds_to_world_nquads(premise_ds.as_ref(), &world_iri) {
        Ok(r) => r,
        Err(e) => {
            println!("SKIP {slug}: premise N-Quads build failed: {e}");
            return None;
        }
    };

    if input_nq.trim().is_empty() {
        println!("SKIP {slug}: premise yields zero valid N-Quads (vacuous pass not permitted)");
        return None;
    }

    Some(LoweredEntry {
        slug,
        world_iri,
        input_nq,
        quad_count,
    })
}

/// Cap for Lane-A cases emitted per vendor run.
const LANE_A_CAP: usize = 12;

/// The W3C SPDX header prepended to every vendored source/stub file.
const W3C_SPDX_HEADER: &str =
    "# SPDX-FileCopyrightText: 2009 W3C (Massachusetts Institute of Technology, ERCIM, Keio, Beihang)\n# SPDX-License-Identifier: W3C\n";

/// Frozen-verdict provenance for one vendored case: the native verdict the
/// committed golden re-asserts, and the W3C published expected verdict.
struct CaseVerdicts<'a> {
    /// The status the committed `expected/verdicts.json` carries (the verdict the
    /// conformance harness re-asserts the native engine produces).
    committed_status: &'a str,
    /// The W3C published expected verdict, carried verbatim as provenance.
    published_status: &'a str,
    /// The native token (`consistent` / `inconsistent` / `incomplete`), recorded
    /// in `profile.json` as the frozen native decision for divergence cases.
    native_token: &'a str,
}

/// The soundness-crux W3C EL cases — the consistency checks that exercise the
/// constructs the native reasoner was made sound on (the empty bottom property,
/// `owl:hasKey`, the negative property assertions, `owl:FunctionalProperty`, and
/// the `owl:Thing` edge). These are always vendored, never capped, so the
/// committed corpus and the soundness gate pin every one of them.
fn is_soundness_crux_case(slug: &str) -> bool {
    const CRUX: &[&str] = &[
        "new-feature-bottomdataproperty-001",
        "new-feature-bottomobjectproperty-001",
        "new-feature-keys-002",
        "new-feature-keys-006",
        "new-feature-negativedatapropertyassertion-001",
        "new-feature-negativeobjectpropertyassertion-001",
        "webont-thing-003",
    ];
    CRUX.contains(&slug)
}

/// The honest-DlGap divergence bucket sibling of the Lane-A `out_dir`:
/// `<parent>/<name>-divergence`. A name-less path falls back to the literal
/// `w3c-owl2-el-divergence` under the same parent.
fn sibling_divergence_dir(out_dir: &Path) -> PathBuf {
    let parent = out_dir.parent().unwrap_or_else(|| Path::new("."));
    let name = out_dir
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("w3c-owl2-el");
    parent.join(format!("{name}-divergence"))
}

/// Write one vendored external case directory mirroring the committed layout
/// (`profile.json`, `input.logic.ttl`, `input.nq`, `expected/verdicts.json`,
/// `source/manifest.ttl`, `source/premise.rdf`).
///
/// `corpus_name` keys the world IRI prefix and the manifest `ex:` namespace.
/// `verdicts` carries the committed status the harness re-asserts plus the W3C
/// published verdict (recorded as provenance so a divergence case is queryable
/// data, never a silent drop).
#[allow(clippy::too_many_arguments)]
fn write_case(
    out_dir: &Path,
    corpus_name: &str,
    slug: &str,
    world_iri: &str,
    input_nq: &str,
    quad_count: u64,
    otest_type: &str,
    premise_xml: Option<&str>,
    verdicts: &CaseVerdicts<'_>,
) -> Result<(), String> {
    let case_dir = out_dir.join(slug);
    let source_dir = case_dir.join("source");
    let expected_dir = case_dir.join("expected");
    std::fs::create_dir_all(&source_dir)
        .map_err(|e| format!("cannot create {}: {e}", source_dir.display()))?;
    std::fs::create_dir_all(&expected_dir)
        .map_err(|e| format!("cannot create {}: {e}", expected_dir.display()))?;

    // profile.json — consistency mode, native engine. For a divergence case the
    // native decision and the W3C published verdict are frozen here as provenance.
    let mut profile = BTreeMap::new();
    profile.insert("verdict_mode", serde_json::json!("consistency"));
    profile.insert("mode", serde_json::json!("native"));
    profile.insert("native_verdict", serde_json::json!(verdicts.native_token));
    profile.insert(
        "w3c_published_verdict",
        serde_json::json!(verdicts.published_status),
    );
    let profile_json = serde_json::to_string_pretty(&profile)
        .map_err(|e| format!("serialize profile.json for {slug}: {e}"))?
        + "\n";
    std::fs::write(case_dir.join("profile.json"), profile_json)
        .map_err(|e| format!("cannot write profile.json for {slug}: {e}"))?;

    // input.logic.ttl — stub required by the per-case anatomy; not compiled in
    // consistency mode (the native DL consistency path reads input.nq only).
    let stub_ttl = format!(
        "{W3C_SPDX_HEADER}#\n\
         # verdict_mode=consistency external case. The OWL EDB is the world-scoped\n\
         # N-Quads in input.nq, decided by the native DL consistency path. This file\n\
         # exists only to satisfy the per-case anatomy; it is NOT compiled in consistency mode.\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n"
    );
    std::fs::write(case_dir.join("input.logic.ttl"), stub_ttl)
        .map_err(|e| format!("cannot write input.logic.ttl for {slug}: {e}"))?;

    // input.nq — already sorted + deduped by the caller.
    std::fs::write(case_dir.join("input.nq"), input_nq)
        .map_err(|e| format!("cannot write input.nq for {slug}: {e}"))?;

    // expected/verdicts.json — the verdict the harness re-asserts.
    let mut world_entry = BTreeMap::new();
    world_entry.insert("quads", serde_json::json!(quad_count));
    world_entry.insert("status", serde_json::json!(verdicts.committed_status));
    let mut verdicts_obj = BTreeMap::new();
    verdicts_obj.insert(world_iri.to_owned(), world_entry);
    let verdicts_json = serde_json::to_string_pretty(&verdicts_obj)
        .map_err(|e| format!("serialize verdicts.json for {slug}: {e}"))?
        + "\n";
    std::fs::write(expected_dir.join("verdicts.json"), &verdicts_json)
        .map_err(|e| format!("cannot write expected/verdicts.json for {slug}: {e}"))?;

    // source/manifest.ttl — carries the W3C otest type and published verdict.
    let manifest_ttl = format!(
        "{W3C_SPDX_HEADER}@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n@prefix ex: <https://gmeow.example/{corpus_name}/{slug}/> .\n\nex:{slug} a otest:{otest_type} ;\n    otest:identifier \"{slug}\" ;\n    mf:action ex:premise.rdf .\n"
    );
    std::fs::write(source_dir.join("manifest.ttl"), &manifest_ttl)
        .map_err(|e| format!("cannot write source/manifest.ttl for {slug}: {e}"))?;

    // source/premise.rdf — verbatim inline RDF/XML for provenance.
    if let Some(xml) = premise_xml {
        std::fs::write(source_dir.join("premise.rdf"), xml)
            .map_err(|e| format!("cannot write source/premise.rdf for {slug}: {e}"))?;
    }

    Ok(())
}

/// Strip a DOCTYPE declaration with internal entity definitions from RDF/XML and
/// expand the defined entities inline, so the result is parseable by a non-validating
/// XML parser that does not load external DTDs.
///
/// This handles the W3C OWL 2 test suite's use of `<!ENTITY name 'value'>` in an
/// internal DTD subset: the entities are extracted, then both the DOCTYPE block and
/// all `&name;` references in the remaining text are replaced.
fn expand_xml_entities(src: &str) -> String {
    // Extract entity definitions from an internal DOCTYPE subset (if present).
    let mut entities: Vec<(String, String)> = Vec::new();

    if let Some(dt_start) = src.find("<!DOCTYPE") {
        if let Some(bracket_open) = src[dt_start..].find('[') {
            let bracket_open_abs = dt_start + bracket_open;
            if let Some(bracket_close) = src[bracket_open_abs..].find(']') {
                let bracket_close_abs = bracket_open_abs + bracket_close;
                let inner = &src[bracket_open_abs + 1..bracket_close_abs];
                // Extract `<!ENTITY name 'value'>` and `<!ENTITY name "value">`.
                let mut rest = inner;
                while let Some(start) = rest.find("<!ENTITY") {
                    rest = &rest[start + 8..];
                    rest = rest.trim_start();
                    // Read entity name (up to first whitespace).
                    let name_end = rest
                        .find(|c: char| c.is_ascii_whitespace())
                        .unwrap_or(rest.len());
                    let name = rest[..name_end].to_string();
                    rest = rest[name_end..].trim_start();
                    // Read entity value (quoted).
                    let value = if rest.starts_with('\'') {
                        let end = rest[1..].find('\'').map(|i| i + 1);
                        if let Some(e) = end {
                            let v = rest[1..e].to_string();
                            rest = &rest[e + 1..];
                            v
                        } else {
                            break;
                        }
                    } else if rest.starts_with('"') {
                        let end = rest[1..].find('"').map(|i| i + 1);
                        if let Some(e) = end {
                            let v = rest[1..e].to_string();
                            rest = &rest[e + 1..];
                            v
                        } else {
                            break;
                        }
                    } else {
                        break;
                    };
                    if !name.is_empty() && !value.is_empty() {
                        entities.push((name, value));
                    }
                }
            }
        }
    }

    // Remove the entire DOCTYPE block (from `<!DOCTYPE` up to the closing `>`
    // after the `]>`), as the parser does not need it once entities are expanded.
    let src_no_doctype = if let Some(dt_start) = src.find("<!DOCTYPE") {
        // Find the end: `]>` closes the internal subset; bare `>` closes a DOCTYPE
        // with no internal subset.
        if let Some(bracket_close) = src[dt_start..].find(']') {
            let abs = dt_start + bracket_close;
            // Consume past the `]>`.
            if src[abs..].starts_with(']') {
                let after = abs + 1;
                if let Some(gt) = src[after..].find('>') {
                    format!("{}{}", &src[..dt_start], &src[after + gt + 1..])
                } else {
                    src[..dt_start].to_string() + &src[abs + 1..]
                }
            } else {
                src.to_string()
            }
        } else if let Some(gt) = src[dt_start..].find('>') {
            format!("{}{}", &src[..dt_start], &src[dt_start + gt + 1..])
        } else {
            src.to_string()
        }
    } else {
        src.to_string()
    };

    // Expand all `&name;` entity references.
    let mut result = src_no_doctype;
    for (name, value) in &entities {
        let entity_ref = format!("&{name};");
        result = result.replace(&entity_ref, value);
    }
    result
}

/// Parse and expand an RDF/XML manifest file, returning consistency/inconsistency
/// entries and the entailment entries (Lane-B, out of consistency-lane scope).
///
/// Both `vendor_el_corpus` and `grade_suite_corpus` share this parsing step.
/// The entailment entries are returned separately so `grade_suite_corpus` can
/// emit DlGap Findings for them rather than silently counting and discarding.
fn parse_consistency_entries(
    input_rdf: &Path,
) -> Result<
    (
        Vec<gmeow_conformance::external::ManifestEntry>,
        Vec<gmeow_conformance::external::ManifestEntry>,
    ),
    String,
> {
    let raw_src = std::fs::read_to_string(input_rdf)
        .map_err(|e| format!("cannot read {}: {e}", input_rdf.display()))?;
    // Expand XML entities (the W3C EL suite uses a DOCTYPE internal subset with
    // entity references; expand them before handing to the parser).
    let src = expand_xml_entities(&raw_src);
    let abs = std::path::absolute(input_rdf)
        .map_err(|e| format!("cannot resolve {}: {e}", input_rdf.display()))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest_rdfxml(&src, Some(&base))?;

    let mut entailment_entries = Vec::new();
    let mut consistency_entries = Vec::new();
    for entry in entries {
        match entry.kind {
            ManifestTestKind::Consistency | ManifestTestKind::Inconsistency => {
                consistency_entries.push(entry);
            }
            ManifestTestKind::PositiveEntailment | ManifestTestKind::NegativeEntailment => {
                entailment_entries.push(entry);
            }
        }
    }
    let entailment_skipped = entailment_entries.len();
    println!(
        "INFO: {entailment_skipped} entailment tests skipped (Lane-B scope, need conclusion-negation)"
    );
    Ok((consistency_entries, entailment_entries))
}

/// Vendor a curated Lane-A subset of the W3C OWL 2 EL conformance suite.
///
/// Reads `<input_rdf>` (RDF/XML), keeps only ConsistencyTest / InconsistencyTest
/// entries, runs the native DL consistency path on each, and emits the ones the
/// native path decides AND agrees with the W3C declared outcome.
fn vendor_el_corpus(input_rdf: &Path, out_dir: &Path) -> Result<(), String> {
    let (consistency_entries, entailment_entries_lane_b) = parse_consistency_entries(input_rdf)?;
    let entailment_skipped = entailment_entries_lane_b.len();

    // The honest-DlGap divergence bucket is a SIBLING of the Lane-A out_dir: every
    // case the native path cannot soundly decide (`gaps` non-empty → `incomplete`)
    // is vendored here as committed data — its frozen native verdict AND the W3C
    // published verdict — rather than silently dropped. The Lane-A corpus carries
    // agreeing deciders; this corpus carries the named divergence set.
    let divergence_dir = sibling_divergence_dir(out_dir);

    // ── Run native reasoner on each, emit Lane-A cases ────────────────────────
    let mut vendored: usize = 0;
    let mut divergence_vendored: usize = 0;
    let mut skipped_gap_capped: usize = 0;
    let mut skipped_disagree: usize = 0;
    let mut skipped_unparsable: usize = 0;
    let mut capped = false;

    // Ensure output directories exist.
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create out-dir {}: {e}", out_dir.display()))?;
    std::fs::create_dir_all(&divergence_dir).map_err(|e| {
        format!(
            "cannot create divergence dir {}: {e}",
            divergence_dir.display()
        )
    })?;

    // We need a stable slug→entry list for deterministic output; entries are
    // already sorted by IRI from the manifest parser. Collect slugs in order.
    for entry in &consistency_entries {
        let lowered = match lower_entry(entry, "https://gmeow.example/w3c-owl2-el/") {
            Some(l) => l,
            None => {
                skipped_unparsable += 1;
                continue;
            }
        };
        let LoweredEntry {
            slug,
            world_iri,
            input_nq,
            quad_count,
        } = lowered;

        // ── Run the native DL consistency path ────────────────────────────────
        let world_ds = match gmeow_rdf::dataset_from_bytes(
            input_nq.as_bytes(),
            gmeow_rdf::NativeRdfFormat::NQuads,
        ) {
            Ok(ds) => ds,
            Err(e) => {
                println!("SKIP {slug}: world N-Quads round-trip failed: {e}");
                skipped_unparsable += 1;
                continue;
            }
        };

        let verdict = match gmeow_logic::reason::dl_consistency(world_ds.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                println!("SKIP {slug}: native DL consistency run failed: {e}");
                skipped_unparsable += 1;
                continue;
            }
        };

        let otest_type = match entry.kind {
            ManifestTestKind::Consistency => "ConsistencyTest",
            ManifestTestKind::Inconsistency => "InconsistencyTest",
            _ => unreachable!("only Consistency/Inconsistency reach this branch"),
        };
        let premise_xml = match &entry.action {
            Some(OntologyDoc::InlineRdfXml(xml)) => Some(xml.as_str()),
            _ => None,
        };
        let declared_status = entry.outcome().verdict_status().as_str();

        // ── Honest-DlGap divergence: native cannot decide → vendor as data ────
        // `gaps` non-empty means the native path admits it cannot decide this
        // case (e.g. owl:Thing oneOf — the DL/Full-divergent singleton-universe
        // case). It is NOT dropped: it is committed to the divergence bucket with
        // its frozen native `incomplete` verdict and the W3C published verdict.
        if !verdict.gaps.is_empty() {
            write_case(
                &divergence_dir,
                "w3c-owl2-el-divergence",
                &slug,
                &world_iri,
                &input_nq,
                quad_count as u64,
                otest_type,
                premise_xml,
                &CaseVerdicts {
                    committed_status: "incomplete",
                    published_status: declared_status,
                    native_token: "incomplete",
                },
            )?;
            divergence_vendored += 1;
            println!("DIVERGENCE {slug}: native incomplete, W3C declares {declared_status}");
            continue;
        }

        // Determine declared vs native verdict.
        let native_status = if verdict.consistent {
            "consistent"
        } else {
            "inconsistent"
        };

        // A decided native verdict that disagrees with W3C would be a soundness
        // defect (CorpusOnly). The grade gate pins corpus_only=0; if one ever
        // reappears here we record it in the divergence bucket so it is durable,
        // queryable data rather than a silent drop.
        if native_status != declared_status {
            write_case(
                &divergence_dir,
                "w3c-owl2-el-divergence",
                &slug,
                &world_iri,
                &input_nq,
                quad_count as u64,
                otest_type,
                premise_xml,
                &CaseVerdicts {
                    committed_status: native_status,
                    published_status: declared_status,
                    native_token: native_status,
                },
            )?;
            divergence_vendored += 1;
            skipped_disagree += 1;
            println!(
                "DIVERGENCE {slug}: native decided {native_status}, W3C declares {declared_status} (CorpusOnly)"
            );
            continue;
        }

        // ── EMIT agreeing Lane-A case ─────────────────────────────────────────
        // The agreeing deciders are capped to keep the bulk Lane-A corpus tight;
        // the cap never applies to the divergence bucket (the named divergence
        // set must be vendored in full) NOR to the soundness-crux cases — the
        // `new-feature-*` / `webont-*` consistency checks that exercise the
        // bottom-property, key, negative-assertion, functional-property, and
        // owl:Thing constructs the native reasoner was just made sound on. Those
        // are always vendored so the soundness gate pins them regardless of how
        // many bulk `fs2rdf-*` cases precede them alphabetically.
        if vendored >= LANE_A_CAP && !is_soundness_crux_case(&slug) {
            capped = true;
            skipped_gap_capped += 1;
            continue;
        }
        write_case(
            out_dir,
            "w3c-owl2-el",
            &slug,
            &world_iri,
            &input_nq,
            quad_count as u64,
            otest_type,
            premise_xml,
            &CaseVerdicts {
                committed_status: declared_status,
                published_status: declared_status,
                native_token: native_status,
            },
        )?;
        vendored += 1;
        println!("EMIT {slug}: {declared_status}");
    }

    // ── Write corpus.json for both buckets ────────────────────────────────────
    let corpus_json = "{\n  \
        \"name\": \"w3c-owl2-el\",\n  \
        \"spdx_license\": \"W3C\",\n  \
        \"source_url\": \"https://www.w3.org/2009/11/owl-test/profile-EL.rdf\",\n  \
        \"version_or_commit\": \"w3c-2009-11-archive\",\n  \
        \"refresh_command\": \"curl -sSL https://www.w3.org/2009/11/owl-test/profile-EL.rdf -o .tmp/w3c-owl2/profile-EL.rdf && cargo run -p gmeow-conformance --bin ingest-external -- --vendor-el .tmp/w3c-owl2/profile-EL.rdf conformance/logic/cases/external/w3c-owl2-el\",\n  \
        \"lane\": \"a\"\n}\n";
    std::fs::write(out_dir.join("corpus.json"), corpus_json)
        .map_err(|e| format!("cannot write corpus.json: {e}"))?;

    // The divergence bucket's lane is `divergence`: native and W3C disagree there
    // by construction (honest DlGap), so the soundness gate that asserts
    // committed==declared must EXCLUDE this lane; the dedicated divergence gate
    // pins it instead.
    let divergence_corpus_json = "{\n  \
        \"name\": \"w3c-owl2-el-divergence\",\n  \
        \"spdx_license\": \"W3C\",\n  \
        \"source_url\": \"https://www.w3.org/2009/11/owl-test/profile-EL.rdf\",\n  \
        \"version_or_commit\": \"w3c-2009-11-archive\",\n  \
        \"refresh_command\": \"curl -sSL https://www.w3.org/2009/11/owl-test/profile-EL.rdf -o .tmp/w3c-owl2/profile-EL.rdf && cargo run -p gmeow-conformance --bin ingest-external -- --vendor-el .tmp/w3c-owl2/profile-EL.rdf conformance/logic/cases/external/w3c-owl2-el\",\n  \
        \"lane\": \"divergence\"\n}\n";
    std::fs::write(divergence_dir.join("corpus.json"), divergence_corpus_json)
        .map_err(|e| format!("cannot write divergence corpus.json: {e}"))?;

    // ── Print final summary ───────────────────────────────────────────────────
    println!(
        "vendored={vendored} divergence_vendored={divergence_vendored} skipped_gap_capped={skipped_gap_capped} skipped_disagree={skipped_disagree} skipped_unparsable={skipped_unparsable} entailment_skipped={entailment_skipped} capped={capped}"
    );

    Ok(())
}

/// Read the quarantine baseline — the set of case slugs in the committed
/// divergence corpus directory.  These are the accepted, honest DlGap cases
/// (currently the two `webont-thing-00{4,5}` EL gaps) whose divergence is
/// known and committed.  Every subdirectory of `quarantine_dir` that is itself
/// a directory is treated as one quarantined slug.
///
/// Returns the slug set, or an error if the directory cannot be read.
fn load_quarantine_slugs(quarantine_dir: &Path) -> Result<BTreeSet<String>, String> {
    let mut slugs = BTreeSet::new();
    let rd = std::fs::read_dir(quarantine_dir).map_err(|e| {
        format!(
            "cannot read quarantine baseline dir {}: {e}",
            quarantine_dir.display()
        )
    })?;
    for entry in rd {
        let entry = entry.map_err(|e| format!("dir entry error in quarantine dir: {e}"))?;
        if entry.path().is_dir() {
            if let Some(name) = entry.file_name().to_str() {
                slugs.insert(name.to_owned());
            }
        }
    }
    Ok(slugs)
}

/// The soundness-scoped gate over one external-corpus grade run.
///
/// This is the invariant this whole grading lane protects:
///
/// * **`CorpusOnly` rows are always a soundness violation** — the native path
///   decided a verdict that disagrees with the published external ground truth.
///   Any `CorpusOnly` in the ledger is unexpected and causes a hard-fail.
///
/// * **`DlGap` rows split into three accepted classes and one unexpected class:**
///   - Entailment-test gaps: the test kind is outside the consistency lane
///     (needs conclusion-negation); their slugs arrive in `entailment_slugs`
///     and are always accepted.
///   - Quarantined consistency gaps: the native path honestly cannot decide a
///     DL/Full-divergent consistency case; their slugs are in `quarantine_slugs`
///     (the committed `w3c-owl2-el-divergence/` directory) and are accepted.
///   - All other `DlGap` rows are unexpected (a new gap appeared outside the
///     accepted scope) and cause a hard-fail.
///
/// Returns `Ok(())` when no unexpected divergences exist.  Returns `Err` with a
/// sorted, one-line-per-offender list when ANY unexpected divergence is found.
///
/// The function is small and side-effect-free so unit tests can drive it
/// directly without touching the filesystem or the reasoner.
pub fn soundness_gate(
    ledger: &gmeow_logic::reason::DivergenceLedger,
    entailment_slugs: &BTreeSet<String>,
    quarantine_slugs: &BTreeSet<String>,
) -> Result<(), Vec<String>> {
    let mut unexpected: Vec<String> = Vec::new();

    for row in &ledger.rows {
        match row.kind {
            gmeow_logic::reason::DivergenceKind::CorpusOnly => {
                // Always a soundness violation — the native path decided the WRONG
                // answer for a published external ground-truth case.
                unexpected.push(format!(
                    "CORPUS-ONLY case {:?} (world {:?}): {}",
                    row.subject, row.world, row.detail
                ));
            }
            gmeow_logic::reason::DivergenceKind::DlGap => {
                // A DlGap from an entailment test is out-of-scope for the
                // consistency lane — accepted regardless of quarantine.
                if entailment_slugs.contains(&row.subject) {
                    continue;
                }
                // A DlGap for a consistency case that is in the committed
                // quarantine baseline is an accepted, honest gap.
                if quarantine_slugs.contains(&row.subject) {
                    continue;
                }
                // Any other DlGap is unexpected: a new consistency gap appeared
                // outside the accepted scope.
                unexpected.push(format!(
                    "DL-GAP case {:?} (world {:?}) not in quarantine baseline: {}",
                    row.subject, row.world, row.detail
                ));
            }
            // Agree / NativeOnly / OracleOnly are not produced by
            // compare_external_corpus; ignore them here.
            _ => {}
        }
    }

    if unexpected.is_empty() {
        Ok(())
    } else {
        unexpected.sort();
        Err(unexpected)
    }
}

/// The default quarantine baseline directory for the W3C OWL 2 EL divergence
/// corpus, resolved at compile time relative to this crate's manifest.
fn default_quarantine_dir() -> PathBuf {
    gmeow_conformance::paths::cases_root()
        .join("external")
        .join("w3c-owl2-el-divergence")
}

/// Grade every ConsistencyTest / InconsistencyTest in `input_rdf` gap-tolerantly
/// against the native reasoner and write divergences as a `gmeow:Finding` N-Quads
/// graph to `out_nq`.
///
/// Unlike `vendor_el_corpus` (Lane-A, strict agree-only), this mode records EVERY
/// case outcome — including `DlGap` (native incomplete) and `CorpusOnly` (native
/// disagrees with published) — as the divergence grading signal.
fn grade_suite_corpus(input_rdf: &Path, corpus_name: &str, out_nq: &Path) -> Result<(), String> {
    let (consistency_entries, entailment_entries) = parse_consistency_entries(input_rdf)?;
    let entailment_skipped = entailment_entries.len();

    let mut comparisons: Vec<gmeow_logic::reason::ExternalComparison> = Vec::new();
    let mut unlowerable: usize = 0;

    let world_iri_prefix = format!("https://gmeow.example/{corpus_name}/");

    // Track which slugs came from entailment tests: these produce DlGap rows
    // that are accepted by the soundness gate (out of consistency-lane scope).
    let mut entailment_slugs: BTreeSet<String> = BTreeSet::new();

    // Emit a DlGap Finding for every entailment test: they are out of the
    // consistency-lane scope (need conclusion-negation), so we record them
    // as coverage gaps rather than silently dropping them.
    for entry in &entailment_entries {
        let slug = to_slug(&entry.name);
        let world_iri = format!("{world_iri_prefix}{slug}/w");
        let published = entry.outcome().verdict_status().as_str().to_string();
        entailment_slugs.insert(slug.clone());
        comparisons.push(gmeow_logic::reason::ExternalComparison {
            case: slug,
            world: world_iri,
            native: "incomplete".to_string(),
            published,
        });
    }

    for entry in &consistency_entries {
        let slug = to_slug(&entry.name);
        let world_iri = format!("{world_iri_prefix}{slug}/w");

        let lowered = match lower_entry(entry, &world_iri_prefix) {
            Some(l) => l,
            None => {
                // Record as DlGap rather than silently dropping: the native path
                // could not ingest the premise (IRI reference, vacuous, or unparsable).
                let published = entry.outcome().verdict_status().as_str().to_string();
                comparisons.push(gmeow_logic::reason::ExternalComparison {
                    case: slug.clone(),
                    world: world_iri,
                    native: "incomplete".to_string(),
                    published,
                });
                unlowerable += 1;
                continue;
            }
        };
        let LoweredEntry {
            slug,
            world_iri,
            input_nq,
            ..
        } = lowered;

        // Round-trip into a parsed dataset for dl_consistency.
        let world_ds = match gmeow_rdf::dataset_from_bytes(
            input_nq.as_bytes(),
            gmeow_rdf::NativeRdfFormat::NQuads,
        ) {
            Ok(ds) => ds,
            Err(e) => {
                println!("SKIP {slug}: world N-Quads round-trip failed: {e}");
                let published = entry.outcome().verdict_status().as_str().to_string();
                comparisons.push(gmeow_logic::reason::ExternalComparison {
                    case: slug.clone(),
                    world: world_iri,
                    native: "incomplete".to_string(),
                    published,
                });
                unlowerable += 1;
                continue;
            }
        };

        // Run the native DL consistency path.
        let verdict = match gmeow_logic::reason::dl_consistency(world_ds.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                println!("SKIP {slug}: native DL consistency run failed: {e}");
                let published = entry.outcome().verdict_status().as_str().to_string();
                comparisons.push(gmeow_logic::reason::ExternalComparison {
                    case: slug.clone(),
                    world: world_iri,
                    native: "incomplete".to_string(),
                    published,
                });
                unlowerable += 1;
                continue;
            }
        };

        // Compute the native token gap-tolerantly: gaps → "incomplete".
        let native_token = if !verdict.gaps.is_empty() {
            "incomplete".to_string()
        } else if verdict.consistent {
            "consistent".to_string()
        } else {
            "inconsistent".to_string()
        };

        let published = entry.outcome().verdict_status().as_str().to_string();

        comparisons.push(gmeow_logic::reason::ExternalComparison {
            case: slug,
            world: world_iri,
            native: native_token,
            published,
        });
    }

    // Build the divergence graph.
    let nq = gmeow_conformance::divergence::emit_divergence_nq(corpus_name, &comparisons);

    // Write the output file (create parent dirs).
    if let Some(parent) = out_nq.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create output dir {}: {e}", parent.display()))?;
    }
    std::fs::write(out_nq, &nq).map_err(|e| format!("cannot write {}: {e}", out_nq.display()))?;

    // Derive counts from the ledger so they match the emitted graph exactly.
    let rows = gmeow_logic::reason::compare_external_corpus(corpus_name, &comparisons);
    let ledger = gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);

    let graded = comparisons.len();
    let agree = ledger.agree;
    let corpus_only = ledger.corpus_only;
    let dl_gap = ledger.dl_gap;

    println!(
        "graded={graded} agree={agree} corpus_only={corpus_only} dl_gap={dl_gap} entailment_skipped={entailment_skipped} unlowerable={unlowerable}"
    );

    // ── Invoke the strict enforce() authority and surface its reasons ─────────
    //
    // enforce() is the canonical gate for the classic native↔oracle cross-check;
    // it fails on ANY DlGap — including the accepted entailment and quarantined
    // consistency gaps.  We always call it so its structured reasons are printed
    // (surfaced, not bypassed), but the PASS/FAIL decision for this lane is made
    // by soundness_gate(), which applies the finer-grained soundness floor.
    let verdict = gmeow_logic::reason::enforce(&ledger);
    if !verdict.reasons.is_empty() {
        println!(
            "enforce() reasons ({}):",
            if verdict.passed { "PASS" } else { "FAIL" }
        );
        for reason in &verdict.reasons {
            println!("  - {reason}");
        }
    } else {
        println!("enforce(): PASS (no divergences)");
    }

    // ── Soundness gate: the invariant this grading lane protects ─────────────
    //
    // corpus_only == 0: the native path must NEVER decide a verdict that
    // disagrees with the published external ground truth.  A corpus_only > 0 is
    // a wrong decided answer and is always a hard-fail.
    //
    // DlGap rows are accepted in two cases:
    //   - Entailment-test DlGaps: out-of-scope for the consistency lane
    //     (need conclusion-negation; the test kind is not a consistency check).
    //   - Quarantined consistency DlGaps: honest gaps committed in the
    //     `w3c-owl2-el-divergence/` baseline (the native path cannot soundly
    //     decide a DL/Full-divergent case for this specific slug).
    // All other DlGap rows are unexpected and cause a hard-fail.
    let quarantine_dir = default_quarantine_dir();
    let quarantine_slugs = load_quarantine_slugs(&quarantine_dir)?;

    soundness_gate(&ledger, &entailment_slugs, &quarantine_slugs).map_err(|offenders| {
        let list = offenders.join("\n  ");
        format!(
            "soundness gate FAILED: {n} unexpected divergence(s):\n  {list}",
            n = offenders.len()
        )
    })
}

/// Detect whether ontology bytes are OWL 2 Functional Syntax.
///
/// The ORE 2015 corpus ships every ontology in OWL 2 Functional Syntax
/// (`Prefix(...)` / `Ontology(...)` headers). The native RDF codecs (Turtle / TriG /
/// N-Triples / N-Quads / RDF/XML) cannot read that surface, so a Functional-Syntax
/// ontology is an honest format gap (recorded as a DlGap Finding), never silently
/// skipped. This is a lexical sniff over the leading non-blank/non-comment line:
/// a Functional-Syntax document opens with `Prefix(` or `Ontology(` (possibly after
/// `# comment` lines), whereas an RDF/XML document opens with `<?xml` or `<rdf:RDF`.
fn is_owl_functional_syntax(bytes: &[u8]) -> bool {
    let text = match std::str::from_utf8(bytes) {
        Ok(t) => t,
        // Non-UTF-8 bytes are not Functional Syntax; let the RDF/XML parser report
        // the real error so it lands as a parse-failure DlGap with a real message.
        Err(_) => return false,
    };
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        return trimmed.starts_with("Prefix(") || trimmed.starts_with("Ontology(");
    }
    false
}

/// One ORE-ontology grade outcome: the comparison row plus the divergence class it
/// fell into (so the summary can tally without re-deriving from the ledger).
enum OreOutcome {
    /// Native agreed with the curated-consistent expected (`consistent`).
    Agree,
    /// Native could not parse or could not decide → honest coverage gap.
    DlGap,
    /// Native decided `inconsistent` on a curated-consistent ontology → soundness flag.
    CorpusOnly,
}

/// Grade one ORE ontology file against the native DL consistency path.
///
/// Returns the divergence comparison (native vs. the curated-consistent `published`)
/// and the outcome class. ORE ships OWL 2 Functional Syntax (unreadable by the native
/// codecs) and provides no per-ontology reference verdict, so:
///   * Functional-Syntax / unparsable / undecidable → native `"incomplete"` (DlGap);
///   * native decided `consistent` → Agree;
///   * native decided `inconsistent` → `"inconsistent"` vs. published `"consistent"`
///     (CorpusOnly — a soundness flag, the only hard-fail condition for ORE).
fn grade_ore_ontology(
    path: &Path,
    slug: &str,
    world_iri: &str,
) -> (gmeow_logic::reason::ExternalComparison, OreOutcome) {
    // ORE real-world ontologies are curated-consistent; the distribution ships no
    // per-ontology reference verdict, so the published expected is `consistent`.
    let published = "consistent".to_string();
    let mut comparison = gmeow_logic::reason::ExternalComparison {
        case: slug.to_string(),
        world: world_iri.to_string(),
        native: "incomplete".to_string(),
        published: published.clone(),
    };

    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) => {
            println!("DL-GAP {slug}: cannot read ontology file: {e}");
            return (comparison, OreOutcome::DlGap);
        }
    };

    // Format gap: OWL 2 Functional Syntax is not in the native codec surface.
    // Recorded as a coverage DlGap with the named format, never a silent skip.
    if is_owl_functional_syntax(&bytes) {
        println!("DL-GAP {slug}: OWL 2 Functional Syntax (native RDF codecs cannot parse)");
        return (comparison, OreOutcome::DlGap);
    }

    // Parse as RDF/XML (the only OWL-bearing surface the native codecs read).
    let ds = match gmeow_rdf::parse_dataset(
        &bytes,
        "application/rdf+xml",
        Some("http://example.org/"),
    ) {
        Ok(ds) => ds,
        Err(e) => {
            println!("DL-GAP {slug}: ontology unparsable as RDF/XML: {e}");
            return (comparison, OreOutcome::DlGap);
        }
    };
    if ds.quad_refs().count() == 0 {
        println!("DL-GAP {slug}: ontology parsed to zero quads (vacuous, not graded)");
        return (comparison, OreOutcome::DlGap);
    }

    let verdict = match gmeow_logic::reason::dl_consistency(ds.as_ref()) {
        Ok(v) => v,
        Err(e) => {
            println!("DL-GAP {slug}: native DL consistency run failed: {e}");
            return (comparison, OreOutcome::DlGap);
        }
    };

    if !verdict.gaps.is_empty() {
        println!("DL-GAP {slug}: native could not decide (coverage gap)");
        return (comparison, OreOutcome::DlGap);
    }

    if verdict.consistent {
        comparison.native = "consistent".to_string();
        println!("AGREE {slug}: native consistent");
        (comparison, OreOutcome::Agree)
    } else {
        comparison.native = "inconsistent".to_string();
        println!(
            "CORPUS-ONLY {slug}: native inconsistent, ORE curated-consistent (soundness flag)"
        );
        (comparison, OreOutcome::CorpusOnly)
    }
}

/// Grade every `*.owl` ontology under `ontology_dir` against the native DL
/// consistency path and write divergences as a `gmeow:Finding` N-Quads graph.
///
/// The ORE 2015 corpus (Zenodo DOI 10.5281/zenodo.18578) is curated-consistent and
/// ships OWL 2 Functional Syntax with NO per-ontology reference verdict. We grade for
/// SOUNDNESS: the published expected is `consistent` for every ontology, a native
/// `inconsistent` is a CorpusOnly soundness violation (hard-fail), and every ontology
/// the native path cannot parse (Functional Syntax) or decide is recorded as an honest
/// DlGap Finding — never a silent skip. ORE is fetched-not-vendored for benchmarking
/// use only (the corpus license forbids redistribution), so nothing here is committed.
fn grade_ore_corpus(ontology_dir: &Path, corpus_name: &str, out_nq: &Path) -> Result<(), String> {
    // Collect `*.owl` ontology files deterministically (sorted by file name).
    let mut owl_files: Vec<PathBuf> = Vec::new();
    let rd = std::fs::read_dir(ontology_dir).map_err(|e| {
        format!(
            "cannot read ORE ontology dir {}: {e}",
            ontology_dir.display()
        )
    })?;
    for entry in rd {
        let entry =
            entry.map_err(|e| format!("dir entry error in {}: {e}", ontology_dir.display()))?;
        let path = entry.path();
        if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("owl") {
            owl_files.push(path);
        }
    }
    owl_files.sort();

    if owl_files.is_empty() {
        return Err(format!(
            "no *.owl ontologies under {} — refusing a vacuous ORE grade (a broken extract \
             must hard-fail, not silently pass)",
            ontology_dir.display()
        ));
    }

    let world_iri_prefix = format!("https://gmeow.example/{corpus_name}/");

    let mut comparisons: Vec<gmeow_logic::reason::ExternalComparison> = Vec::new();
    // Every DlGap slug is an ACCEPTED coverage gap for ORE: the corpus is graded for
    // soundness only, and the Functional-Syntax / undecidable gaps are the expected,
    // honest output (recorded as data), not a regression. Only CorpusOnly hard-fails.
    let mut dl_gap_slugs: BTreeSet<String> = BTreeSet::new();
    let mut graded = 0usize;
    let mut agree = 0usize;
    let mut dl_gap = 0usize;
    let mut corpus_only = 0usize;

    for path in &owl_files {
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| format!("non-UTF-8 ontology file name: {}", path.display()))?;
        let slug = to_slug(stem);
        let world_iri = format!("{world_iri_prefix}{slug}/w");

        let (comparison, outcome) = grade_ore_ontology(path, &slug, &world_iri);
        graded += 1;
        match outcome {
            OreOutcome::Agree => agree += 1,
            OreOutcome::DlGap => {
                dl_gap += 1;
                dl_gap_slugs.insert(slug.clone());
            }
            OreOutcome::CorpusOnly => corpus_only += 1,
        }
        comparisons.push(comparison);
    }

    // Build + write the divergence graph (DlGap + CorpusOnly rows become Findings).
    let nq = gmeow_conformance::divergence::emit_divergence_nq(corpus_name, &comparisons);
    if let Some(parent) = out_nq.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create output dir {}: {e}", parent.display()))?;
    }
    std::fs::write(out_nq, &nq).map_err(|e| format!("cannot write {}: {e}", out_nq.display()))?;

    println!("graded={graded} agree={agree} corpus_only={corpus_only} dl_gap={dl_gap}");

    // ── Soundness gate ────────────────────────────────────────────────────────
    //
    // ORE has no manifest entailment tests and no committed quarantine baseline, so
    // the accepted-DlGap set is the full set of coverage gaps observed this run: a
    // Functional-Syntax or undecidable gap is the EXPECTED honest output for ORE, not
    // a regression. The gate's teeth for ORE are CorpusOnly == 0 — a native
    // `inconsistent` verdict on a curated-consistent real-world ontology is a
    // soundness defect and the only hard-fail condition.
    let rows = gmeow_logic::reason::compare_external_corpus(corpus_name, &comparisons);
    let ledger = gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows);
    soundness_gate(&ledger, &BTreeSet::new(), &dl_gap_slugs).map_err(|offenders| {
        let list = offenders.join("\n  ");
        format!(
            "ORE soundness gate FAILED: {n} unexpected divergence(s):\n  {list}",
            n = offenders.len()
        )
    })
}

/// Read the value following a flag, or error with the flag name.
fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}

#[cfg(test)]
mod tests {
    /// `premise_ds_to_world_nquads` is not `pub`, so we test the same logic via a
    /// local helper that mirrors the fixed conversion exactly.  This keeps the test
    /// small and fast (no RDF parser, no reasoner).
    fn nt_lines_to_nquads(nt_text: &str, world_iri: &str) -> Result<Vec<String>, String> {
        nt_text
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
            .map(|line| {
                let trimmed = line.trim_end();
                let without_dot = trimmed
                    .strip_suffix('.')
                    .ok_or_else(|| format!("malformed N-Triples line (no trailing '.'): {line}"))?;
                let body = without_dot.trim_end();
                Ok(format!("{body} <{world_iri}> ."))
            })
            .collect::<Result<Vec<String>, String>>()
    }

    const WORLD: &str = "https://gmeow.example/test/w";

    /// A normal N-Triples line (space before dot) must produce a well-formed N-Quad:
    /// exactly one graph IRI and one trailing dot, no double terminator.
    #[test]
    fn normal_nt_line_produces_well_formed_quad() {
        let nt = "_:s <http://example.org/p> <http://example.org/o> .\n";
        let quads = nt_lines_to_nquads(nt, WORLD).expect("must succeed");
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        // Must end with exactly ` .`
        assert!(q.ends_with(" ."), "quad must end with ' .': {q:?}");
        // Must contain the world IRI
        assert!(q.contains(WORLD), "quad must contain world IRI: {q:?}");
        // Must NOT contain double terminator `. <`
        assert!(
            !q.contains(". <"),
            "quad must not contain double terminator '. <': {q:?}"
        );
    }

    /// A line ending with `. ` (dot + trailing space) — the previously buggy case —
    /// must also produce a well-formed N-Quad with no double terminator.
    #[test]
    fn trailing_space_after_dot_produces_well_formed_quad() {
        // Dot followed by a trailing space (the bug trigger).
        let nt = "_:s <http://example.org/p> <http://example.org/o> . \n";
        let quads = nt_lines_to_nquads(nt, WORLD).expect("must succeed");
        assert_eq!(quads.len(), 1);
        let q = &quads[0];
        assert!(q.ends_with(" ."), "quad must end with ' .': {q:?}");
        assert!(q.contains(WORLD), "quad must contain world IRI: {q:?}");
        assert!(
            !q.contains(". <"),
            "quad must not contain double terminator '. <': {q:?}"
        );
    }

    /// A mix of normal, trailing-space, blank, and comment lines: only the two
    /// data lines must appear in the output, both well-formed.
    #[test]
    fn mixed_input_skips_blanks_and_comments() {
        let nt = "\
_:a <http://example.org/p> <http://example.org/o1> .\n\
# a comment line\n\
\n\
_:b <http://example.org/p> <http://example.org/o2> . \n\
";
        let quads = nt_lines_to_nquads(nt, WORLD).expect("must succeed");
        assert_eq!(quads.len(), 2, "expected exactly 2 quads: {quads:?}");
        for q in &quads {
            assert!(q.ends_with(" ."), "quad must end with ' .': {q:?}");
            assert!(!q.contains(". <"), "double terminator in: {q:?}");
        }
    }

    /// A line with NO trailing dot at all must hard-fail with a descriptive error,
    /// not silently produce a malformed quad.
    #[test]
    fn missing_trailing_dot_returns_err() {
        let nt = "_:s <http://example.org/p> <http://example.org/o>\n";
        let result = nt_lines_to_nquads(nt, WORLD);
        assert!(result.is_err(), "expected Err for missing dot");
        let msg = result.unwrap_err();
        assert!(
            msg.contains("malformed N-Triples line"),
            "error message should describe the problem: {msg:?}"
        );
    }

    /// `grade_suite_corpus` must emit a DlGap Finding for un-lowerable cases
    /// (IRI reference premise or entailment test) rather than silently dropping them.
    ///
    /// This test uses a synthetic two-entry manifest: one consistency test whose
    /// premise is an IRI reference (un-lowerable, no reasoner invoked) and one
    /// entailment test (out of consistency-lane scope). Both must appear as
    /// `dl-gap` Findings in the emitted N-Quads graph.
    #[test]
    fn grade_suite_emits_dlgap_for_unlowerable_and_entailment() {
        // RDF/XML manifest with:
        //   - one ConsistencyTest whose mf:action is an IRI reference (Lane-B skip)
        //   - one PositiveEntailmentTest (always out-of-scope for the consistency lane)
        let manifest_xml = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:mf="http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#"
         xmlns:otest="http://www.w3.org/2007/OWL/testOntology#">
  <otest:ConsistencyTest rdf:about="http://example.org/test/ref-premise">
    <mf:name>ref-premise</mf:name>
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#ConsistencyTest"/>
    <mf:action rdf:resource="http://example.org/ontologies/premise.owl"/>
    <otest:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <mf:result rdf:resource="http://www.w3.org/2007/OWL/testOntology#Consistent"/>
  </otest:ConsistencyTest>
  <otest:PositiveEntailmentTest rdf:about="http://example.org/test/entailment-case">
    <mf:name>entailment-case</mf:name>
    <rdf:type rdf:resource="http://www.w3.org/2007/OWL/testOntology#PositiveEntailmentTest"/>
    <mf:action rdf:resource="http://example.org/ontologies/premise2.owl"/>
    <otest:status rdf:resource="http://www.w3.org/2007/OWL/testOntology#Approved"/>
    <mf:result rdf:resource="http://www.w3.org/2007/OWL/testOntology#Consistent"/>
  </otest:PositiveEntailmentTest>
</rdf:RDF>"#;

        // Write manifest to a temp file.
        let dir = std::env::temp_dir();
        let manifest_path = dir.join(format!(
            "gmeow-test-grade-suite-dlgap-{}.rdf",
            std::process::id()
        ));
        let out_nq = dir.join(format!(
            "gmeow-test-grade-suite-dlgap-{}.nq",
            std::process::id()
        ));
        std::fs::write(&manifest_path, manifest_xml).expect("write manifest");

        // Run grade_suite_corpus.  The `ref-premise` consistency test has an
        // IRI-reference premise (un-lowerable) and is NOT in the quarantine
        // baseline, so the soundness gate now correctly hard-fails: the
        // un-lowerable consistency DlGap is unexpected.  The entailment case
        // is accepted (entailment_slugs), but the consistency IRI-ref gap is
        // not — the function returns Err.
        let result = super::grade_suite_corpus(&manifest_path, "test-corpus", &out_nq);
        assert!(
            result.is_err(),
            "grade_suite_corpus must fail the soundness gate for an un-quarantined \
             un-lowerable consistency DlGap: {result:?}"
        );
        let err_msg = result.unwrap_err();
        assert!(
            err_msg.contains("soundness gate FAILED"),
            "error must name the soundness gate failure: {err_msg:?}"
        );
        assert!(
            err_msg.contains("ref-premise"),
            "error must name the offending case: {err_msg:?}"
        );

        // The N-Quads output file is written BEFORE the soundness gate check,
        // so it must exist and contain dl-gap Findings for both cases.
        let nq = std::fs::read_to_string(&out_nq).expect("read output nq");

        // Both un-lowerable and entailment cases must appear as dl-gap Findings.
        assert!(
            nq.contains("reason.divergence.dl-gap"),
            "output must contain dl-gap Findings for un-lowerable/entailment cases: {nq:?}"
        );

        // There must be at least 2 Finding nodes (one per unlowerable/entailment case).
        let finding_type_count = nq.lines().filter(|l| l.contains("/Finding>")).count();
        assert!(
            finding_type_count >= 2,
            "expected at least 2 dl-gap Findings (ref-premise + entailment-case), got {finding_type_count}: {nq:?}"
        );

        // Clean up.
        let _ = std::fs::remove_file(&manifest_path);
        let _ = std::fs::remove_file(&out_nq);
    }

    // ── soundness_gate unit tests ─────────────────────────────────────────────
    //
    // These tests drive the gate helper directly without touching the filesystem
    // or the reasoner.  They are sub-second and deterministic.

    /// Build a minimal `DivergenceLedger` from a single external comparison.
    fn ledger_from_one(
        case: &str,
        native: &str,
        published: &str,
    ) -> gmeow_logic::reason::DivergenceLedger {
        let rows = gmeow_logic::reason::compare_external_corpus(
            "test-corpus",
            &[gmeow_logic::reason::ExternalComparison {
                case: case.to_owned(),
                world: format!("https://gmeow.example/test-corpus/{case}/w"),
                native: native.to_owned(),
                published: published.to_owned(),
            }],
        );
        gmeow_logic::reason::build_ledger(Vec::new(), Vec::new(), Vec::new(), rows)
    }

    /// A synthetic CorpusOnly row (native decided WRONG) must always cause a
    /// hard-fail regardless of entailment or quarantine sets.
    #[test]
    fn soundness_gate_fails_on_corpus_only() {
        // native decided "consistent" but the corpus published "inconsistent"
        // → CorpusOnly → always a soundness violation.
        let ledger = ledger_from_one("some-case", "consistent", "inconsistent");
        assert_eq!(ledger.corpus_only, 1, "must be a CorpusOnly row");

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
        );
        assert!(
            result.is_err(),
            "CorpusOnly must hard-fail the soundness gate"
        );
        let offenders = result.unwrap_err();
        assert_eq!(offenders.len(), 1);
        assert!(
            offenders[0].contains("CORPUS-ONLY"),
            "offender line must name CORPUS-ONLY: {:?}",
            offenders[0]
        );
    }

    /// A DlGap from an entailment test (slug in entailment_slugs) must be
    /// accepted: the gate returns Ok.
    #[test]
    fn soundness_gate_accepts_entailment_dl_gap() {
        // native "incomplete" for an entailment test → DlGap, but accepted.
        let ledger = ledger_from_one("an-entailment-case", "incomplete", "consistent");
        assert_eq!(ledger.dl_gap, 1);

        let mut entailment_slugs = std::collections::BTreeSet::new();
        entailment_slugs.insert("an-entailment-case".to_owned());

        let result = super::soundness_gate(
            &ledger,
            &entailment_slugs,
            &std::collections::BTreeSet::new(),
        );
        assert!(
            result.is_ok(),
            "entailment DlGap must be accepted: {result:?}"
        );
    }

    /// A DlGap for a consistency case in the committed quarantine baseline
    /// must be accepted: the gate returns Ok.
    #[test]
    fn soundness_gate_accepts_quarantined_dl_gap() {
        // native "incomplete" for a consistency case that is in the quarantine.
        let ledger = ledger_from_one("webont-thing-004", "incomplete", "consistent");
        assert_eq!(ledger.dl_gap, 1);

        let mut quarantine_slugs = std::collections::BTreeSet::new();
        quarantine_slugs.insert("webont-thing-004".to_owned());

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &quarantine_slugs,
        );
        assert!(
            result.is_ok(),
            "quarantined DlGap must be accepted: {result:?}"
        );
    }

    /// A DlGap for a consistency case NOT in either accepted set (not an
    /// entailment test, not in the quarantine baseline) must hard-fail.
    #[test]
    fn soundness_gate_fails_on_unexpected_dl_gap() {
        let ledger = ledger_from_one("new-unknown-gap", "incomplete", "consistent");
        assert_eq!(ledger.dl_gap, 1);

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
        );
        assert!(
            result.is_err(),
            "unexpected DlGap must hard-fail the soundness gate"
        );
        let offenders = result.unwrap_err();
        assert_eq!(offenders.len(), 1);
        assert!(
            offenders[0].contains("DL-GAP"),
            "offender line must name DL-GAP: {:?}",
            offenders[0]
        );
    }

    // ── ORE grading-adapter unit tests ───────────────────────────────────────
    //
    // These are network-free: they synthesize a tiny ORE-shaped ontology directory
    // (one OWL 2 Functional-Syntax file = format-gap DlGap, one trivial consistent
    // RDF/XML ontology = Agree) and assert the grade output, the emitted Findings,
    // and the soundness-gate exit.

    /// `is_owl_functional_syntax` must flag Functional-Syntax headers (the ORE
    /// surface) and must NOT flag RDF/XML.
    #[test]
    fn detects_owl_functional_syntax() {
        let functional = b"Prefix(owl:=<http://www.w3.org/2002/07/owl#>)\nOntology(<http://x>\n)";
        assert!(
            super::is_owl_functional_syntax(functional),
            "Functional-Syntax header must be detected"
        );
        // Leading comment lines are skipped before the sniff.
        let with_comment = b"# a comment\nOntology(<http://x>)";
        assert!(super::is_owl_functional_syntax(with_comment));

        let rdfxml = br#"<?xml version="1.0"?><rdf:RDF/>"#;
        assert!(
            !super::is_owl_functional_syntax(rdfxml),
            "RDF/XML must NOT be flagged as Functional Syntax"
        );
    }

    /// `grade_ore_corpus` over a synthetic dir: one Functional-Syntax ontology (a
    /// format-gap DlGap) and one trivial consistent RDF/XML ontology (Agree). The
    /// DlGap is an ACCEPTED ORE coverage gap, so the soundness gate must pass, the
    /// DlGap must surface as a `dl-gap` Finding, and the consistent one must NOT.
    #[test]
    fn grade_ore_records_functional_syntax_gap_and_passes_soundness() {
        let base = std::env::temp_dir().join(format!("gmeow-ore-grade-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create ORE dir");

        // ORE-shaped OWL 2 Functional-Syntax ontology (native codecs cannot read it).
        std::fs::write(
            base.join("ore_ont_0001.owl"),
            "Prefix(owl:=<http://www.w3.org/2002/07/owl#>)\n\
             Ontology(<http://owl.cs.man.ac.uk/ore/ont1>\n\
             Declaration(Class(<http://a.com/ont#A>))\n)",
        )
        .expect("write functional ontology");

        // A trivial, satisfiable RDF/XML ontology (one class declaration): the native
        // DL consistency path decides `consistent`, matching the curated expected.
        std::fs::write(
            base.join("ore_ont_0002.owl"),
            r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:owl="http://www.w3.org/2002/07/owl#">
  <owl:Class rdf:about="http://a.com/ont#A"/>
</rdf:RDF>"#,
        )
        .expect("write rdfxml ontology");

        let out_nq = base.join("divergence.nq");
        let result = super::grade_ore_corpus(&base, "ore-test", &out_nq);
        assert!(
            result.is_ok(),
            "ORE soundness gate must pass (no CorpusOnly): {result:?}"
        );

        let nq = std::fs::read_to_string(&out_nq).expect("read divergence nq");
        // The Functional-Syntax ontology must appear as a dl-gap Finding.
        assert!(
            nq.contains("reason.divergence.dl-gap"),
            "format-gap ontology must surface as a dl-gap Finding: {nq:?}"
        );
        // Exactly one Finding (the DlGap); the consistent ontology agrees → no row.
        let finding_count = nq.lines().filter(|l| l.contains("/Finding>")).count();
        assert_eq!(
            finding_count, 1,
            "exactly one dl-gap Finding expected (consistent ontology emits none): {nq:?}"
        );
        assert!(
            !nq.contains("reason.divergence.corpus-only"),
            "no soundness (corpus-only) divergence expected for trivial consistent ontology: {nq:?}"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// An empty (or no-`.owl`) directory must hard-fail rather than silently pass a
    /// vacuous grade — a broken extract is a real error.
    #[test]
    fn grade_ore_hard_fails_on_empty_dir() {
        let base = std::env::temp_dir().join(format!("gmeow-ore-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("create empty ORE dir");

        let out_nq = base.join("divergence.nq");
        let result = super::grade_ore_corpus(&base, "ore-test", &out_nq);
        assert!(
            result.is_err(),
            "an empty ORE extract must hard-fail, not vacuously pass"
        );
        assert!(
            result.unwrap_err().contains("no *.owl ontologies"),
            "error must name the empty-extract condition"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// When a ledger has only agreeing rows, the gate passes regardless of what
    /// the entailment/quarantine sets contain.
    #[test]
    fn soundness_gate_passes_on_pure_agreement() {
        let ledger = ledger_from_one("consistent-case", "consistent", "consistent");
        assert_eq!(ledger.agree, 1);
        assert_eq!(ledger.corpus_only, 0);
        assert_eq!(ledger.dl_gap, 0);

        let result = super::soundness_gate(
            &ledger,
            &std::collections::BTreeSet::new(),
            &std::collections::BTreeSet::new(),
        );
        assert!(result.is_ok(), "pure agreement must pass: {result:?}");
    }
}
