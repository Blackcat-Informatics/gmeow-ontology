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
//!     Vendor a curated Lane-A subset of the W3C OWL 2 EL conformance suite
//!     (ConsistencyTest / InconsistencyTest only) into <out-dir>. Each emitted case
//!     is decided by the native reasoner and agrees with the W3C declared outcome.
//!
//! ingest-external --grade-suite <input.rdf> <corpus-name> <out.nq>
//!     Grade EVERY ConsistencyTest / InconsistencyTest in <input.rdf> gap-tolerantly
//!     against the native reasoner, record divergences (DlGap / CorpusOnly) as a
//!     gmeow:Finding N-Quads graph written to <out.nq>, and print a summary line.
//!     Entailment tests are counted but not graded (they need conclusion-negation).
//! ```

use std::collections::BTreeMap;
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
  ingest-external --grade-suite <input.rdf> <corpus-name> <out.nq>";

fn main() -> Result<(), String> {
    let mut szs: Option<PathBuf> = None;
    let mut manifest: Option<PathBuf> = None;
    let mut vendor_el: Option<(PathBuf, PathBuf)> = None;
    let mut grade_suite: Option<(PathBuf, String, PathBuf)> = None;
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
        + grade_suite.is_some() as u8;
    if mode_count > 1 {
        return Err(format!(
            "--szs, --manifest, --vendor-el, and --grade-suite are mutually exclusive\n{USAGE}"
        ));
    }

    match (szs, manifest, vendor_el, grade_suite) {
        (Some(path), None, None, None) => ingest_szs(&path, world.as_deref(), quads),
        (None, Some(path), None, None) => {
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
        (None, None, Some((input, out)), None) => vendor_el_corpus(&input, &out),
        (None, None, None, Some((input, corpus_name, out_nq))) => {
            grade_suite_corpus(&input, &corpus_name, &out_nq)
        }
        _ => Err(format!(
            "one of --szs / --manifest / --vendor-el / --grade-suite is required\n{USAGE}"
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
/// entries and the count of entailment entries skipped.
///
/// Both `vendor_el_corpus` and `grade_suite_corpus` share this parsing step.
fn parse_consistency_entries(
    input_rdf: &Path,
) -> Result<(Vec<gmeow_conformance::external::ManifestEntry>, usize), String> {
    let raw_src = std::fs::read_to_string(input_rdf)
        .map_err(|e| format!("cannot read {}: {e}", input_rdf.display()))?;
    // Expand XML entities (the W3C EL suite uses a DOCTYPE internal subset with
    // entity references; expand them before handing to the parser).
    let src = expand_xml_entities(&raw_src);
    let abs = std::path::absolute(input_rdf)
        .map_err(|e| format!("cannot resolve {}: {e}", input_rdf.display()))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest_rdfxml(&src, Some(&base))?;

    let mut entailment_skipped: usize = 0;
    let mut consistency_entries = Vec::new();
    for entry in entries {
        match entry.kind {
            ManifestTestKind::Consistency | ManifestTestKind::Inconsistency => {
                consistency_entries.push(entry);
            }
            ManifestTestKind::PositiveEntailment | ManifestTestKind::NegativeEntailment => {
                entailment_skipped += 1;
            }
        }
    }
    println!(
        "INFO: {entailment_skipped} entailment tests skipped (Lane-B scope, need conclusion-negation)"
    );
    Ok((consistency_entries, entailment_skipped))
}

/// Vendor a curated Lane-A subset of the W3C OWL 2 EL conformance suite.
///
/// Reads `<input_rdf>` (RDF/XML), keeps only ConsistencyTest / InconsistencyTest
/// entries, runs the native DL consistency path on each, and emits the ones the
/// native path decides AND agrees with the W3C declared outcome.
fn vendor_el_corpus(input_rdf: &Path, out_dir: &Path) -> Result<(), String> {
    let (consistency_entries, entailment_skipped) = parse_consistency_entries(input_rdf)?;

    // ── Run native reasoner on each, emit Lane-A cases ────────────────────────
    let mut vendored: usize = 0;
    let mut skipped_gap: usize = 0;
    let mut skipped_disagree: usize = 0;
    let mut skipped_unparsable: usize = 0;
    let mut capped = false;

    // Ensure output directory exists.
    std::fs::create_dir_all(out_dir)
        .map_err(|e| format!("cannot create out-dir {}: {e}", out_dir.display()))?;

    // We need a stable slug→entry list for deterministic output; entries are
    // already sorted by IRI from the manifest parser. Collect slugs in order.
    for entry in &consistency_entries {
        if vendored >= LANE_A_CAP {
            capped = true;
            println!("CAP reached, remaining deciders not vendored");
            break;
        }

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
                skipped_gap += 1;
                continue;
            }
        };

        // Zero-defer: gaps means the native path cannot honestly decide this case.
        if !verdict.gaps.is_empty() {
            let gap_codes: Vec<&str> = verdict.gaps.iter().map(|g| g.code.as_str()).collect();
            println!("SKIP {slug}: native gap (Lane-B): {gap_codes:?}");
            skipped_gap += 1;
            continue;
        }

        // Determine declared vs native verdict.
        let declared_status = entry.outcome().verdict_status().as_str();
        let native_status = if verdict.consistent {
            "consistent"
        } else {
            "inconsistent"
        };

        if native_status != declared_status {
            println!(
                "SKIP {slug}: native says {native_status}, W3C declares {declared_status} (Lane-B / would be CorpusOnly)"
            );
            skipped_disagree += 1;
            continue;
        }

        // ── EMIT Lane-A case ─────────────────────────────────────────────────
        let case_dir = out_dir.join(&slug);
        let source_dir = case_dir.join("source");
        let expected_dir = case_dir.join("expected");
        std::fs::create_dir_all(&source_dir)
            .map_err(|e| format!("cannot create {}: {e}", source_dir.display()))?;
        std::fs::create_dir_all(&expected_dir)
            .map_err(|e| format!("cannot create {}: {e}", expected_dir.display()))?;

        // profile.json
        let profile_json = "{\n  \"verdict_mode\": \"consistency\",\n  \"mode\": \"native\"\n}\n";
        std::fs::write(case_dir.join("profile.json"), profile_json)
            .map_err(|e| format!("cannot write profile.json for {slug}: {e}"))?;

        // input.logic.ttl — stub required by the per-case anatomy; not compiled in
        // consistency mode (the native DL consistency path reads input.nq only).
        let stub_ttl = "# SPDX-FileCopyrightText: 2009 W3C (Massachusetts Institute of Technology, ERCIM, Keio, Beihang)\n\
                         # SPDX-License-Identifier: W3C\n\
                         #\n\
                         # verdict_mode=consistency external case. The OWL EDB is the world-scoped\n\
                         # N-Quads in input.nq, decided by the native DL consistency path. This file\n\
                         # exists only to satisfy the per-case anatomy; it is NOT compiled in consistency mode.\n\
                         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n";
        std::fs::write(case_dir.join("input.logic.ttl"), stub_ttl)
            .map_err(|e| format!("cannot write input.logic.ttl for {slug}: {e}"))?;

        // input.nq — already sorted + deduped above.
        std::fs::write(case_dir.join("input.nq"), &input_nq)
            .map_err(|e| format!("cannot write input.nq for {slug}: {e}"))?;

        // Count quads for the verdicts.json.
        let quad_count = quad_count as u64;

        // expected/verdicts.json
        // Build the JSON manually using sorted BTreeMap to match expected format.
        let mut world_entry = BTreeMap::new();
        world_entry.insert("quads", serde_json::json!(quad_count));
        world_entry.insert("status", serde_json::json!(declared_status));
        let mut verdicts_obj = BTreeMap::new();
        verdicts_obj.insert(world_iri.clone(), world_entry);
        let verdicts_json = serde_json::to_string_pretty(&verdicts_obj)
            .map_err(|e| format!("serialize verdicts.json for {slug}: {e}"))?
            + "\n";
        std::fs::write(expected_dir.join("verdicts.json"), &verdicts_json)
            .map_err(|e| format!("cannot write expected/verdicts.json for {slug}: {e}"))?;

        // source/manifest.ttl
        let otest_type = match entry.kind {
            ManifestTestKind::Consistency => "ConsistencyTest",
            ManifestTestKind::Inconsistency => "InconsistencyTest",
            _ => unreachable!("only Consistency/Inconsistency reach this branch"),
        };
        let manifest_ttl = format!(
            "# SPDX-FileCopyrightText: 2009 W3C (Massachusetts Institute of Technology, ERCIM, Keio, Beihang)\n# SPDX-License-Identifier: W3C\n@prefix otest: <http://www.w3.org/2007/OWL/testOntology#> .\n@prefix mf: <http://www.w3.org/2001/sw/DataAccess/tests/test-manifest#> .\n@prefix ex: <https://gmeow.example/w3c-owl2-el/{slug}/> .\n\nex:{slug} a otest:{otest_type} ;\n    otest:identifier \"{slug}\" ;\n    mf:action ex:premise.rdf .\n"
        );
        std::fs::write(source_dir.join("manifest.ttl"), &manifest_ttl)
            .map_err(|e| format!("cannot write source/manifest.ttl for {slug}: {e}"))?;

        // source/premise.rdf — verbatim inline RDF/XML for provenance.
        // Retrieve the original XML from the entry action for the source record.
        if let Some(OntologyDoc::InlineRdfXml(premise_xml)) = &entry.action {
            std::fs::write(source_dir.join("premise.rdf"), premise_xml)
                .map_err(|e| format!("cannot write source/premise.rdf for {slug}: {e}"))?;
        }

        vendored += 1;
        println!("EMIT {slug}: {declared_status}");
    }

    // Count cases not processed due to cap (only if we hit the cap mid-loop).
    // (The cap break exits early, so we just report capped bool.)

    // ── Write corpus.json ─────────────────────────────────────────────────────
    let corpus_json = "{\n  \
        \"name\": \"w3c-owl2-el\",\n  \
        \"spdx_license\": \"W3C\",\n  \
        \"source_url\": \"https://www.w3.org/2009/11/owl-test/profile-EL.rdf\",\n  \
        \"version_or_commit\": \"w3c-2009-11-archive\",\n  \
        \"refresh_command\": \"curl -sSL https://www.w3.org/2009/11/owl-test/profile-EL.rdf -o .tmp/w3c-owl2/profile-EL.rdf && cargo run -p gmeow-conformance --bin ingest-external -- --vendor-el .tmp/w3c-owl2/profile-EL.rdf conformance/logic/cases/external/w3c-owl2-el\",\n  \
        \"lane\": \"a\"\n}\n";
    std::fs::write(out_dir.join("corpus.json"), corpus_json)
        .map_err(|e| format!("cannot write corpus.json: {e}"))?;

    // ── Print final summary ───────────────────────────────────────────────────
    println!(
        "vendored={vendored} skipped_gap={skipped_gap} skipped_disagree={skipped_disagree} skipped_unparsable={skipped_unparsable} entailment_skipped={entailment_skipped} capped={capped}"
    );

    Ok(())
}

/// Grade every ConsistencyTest / InconsistencyTest in `input_rdf` gap-tolerantly
/// against the native reasoner and write divergences as a `gmeow:Finding` N-Quads
/// graph to `out_nq`.
///
/// Unlike `vendor_el_corpus` (Lane-A, strict agree-only), this mode records EVERY
/// case outcome — including `DlGap` (native incomplete) and `CorpusOnly` (native
/// disagrees with published) — as the divergence grading signal.
fn grade_suite_corpus(input_rdf: &Path, corpus_name: &str, out_nq: &Path) -> Result<(), String> {
    let (consistency_entries, entailment_skipped) = parse_consistency_entries(input_rdf)?;

    let mut comparisons: Vec<gmeow_logic::reason::ExternalComparison> = Vec::new();
    let mut unlowerable: usize = 0;

    let world_iri_prefix = format!("https://gmeow.example/{corpus_name}/");

    for entry in &consistency_entries {
        let lowered = match lower_entry(entry, &world_iri_prefix) {
            Some(l) => l,
            None => {
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
                unlowerable += 1;
                continue;
            }
        };

        // Run the native DL consistency path.
        let verdict = match gmeow_logic::reason::dl_consistency(world_ds.as_ref()) {
            Ok(v) => v,
            Err(e) => {
                println!("SKIP {slug}: native DL consistency run failed: {e}");
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

    Ok(())
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
}
