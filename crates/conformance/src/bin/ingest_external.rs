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
  ingest-external --vendor-el <input.rdf> <out-dir>";

fn main() -> Result<(), String> {
    let mut szs: Option<PathBuf> = None;
    let mut manifest: Option<PathBuf> = None;
    let mut vendor_el: Option<(PathBuf, PathBuf)> = None;
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

    let mode_count = szs.is_some() as u8 + manifest.is_some() as u8 + vendor_el.is_some() as u8;
    if mode_count > 1 {
        return Err(format!(
            "--szs, --manifest, and --vendor-el are mutually exclusive\n{USAGE}"
        ));
    }

    match (szs, manifest, vendor_el) {
        (Some(path), None, None) => ingest_szs(&path, world.as_deref(), quads),
        (None, Some(path), None) => {
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
        (None, None, Some((input, out))) => vendor_el_corpus(&input, &out),
        _ => Err(format!(
            "one of --szs / --manifest / --vendor-el is required\n{USAGE}"
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
    let mut nq_lines: Vec<String> = nt_text
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.trim_start().starts_with('#'))
        .map(|line| {
            // N-Triples lines end with ` .`; strip the trailing ` .` and append graph.
            let body = line.trim_end_matches('.').trim_end();
            format!("{body} <{world_iri}> .")
        })
        .collect();
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

/// Vendor a curated Lane-A subset of the W3C OWL 2 EL conformance suite.
///
/// Reads `<input_rdf>` (RDF/XML), keeps only ConsistencyTest / InconsistencyTest
/// entries, runs the native DL consistency path on each, and emits the ones the
/// native path decides AND agrees with the W3C declared outcome.
fn vendor_el_corpus(input_rdf: &Path, out_dir: &Path) -> Result<(), String> {
    // ── Parse the EL manifest ─────────────────────────────────────────────────
    let raw_src = std::fs::read_to_string(input_rdf)
        .map_err(|e| format!("cannot read {}: {e}", input_rdf.display()))?;
    // Expand XML entities (the W3C EL suite uses a DOCTYPE internal subset with
    // entity references; expand them before handing to the parser).
    let src = expand_xml_entities(&raw_src);
    let abs = std::path::absolute(input_rdf)
        .map_err(|e| format!("cannot resolve {}: {e}", input_rdf.display()))?;
    let base = format!("file://{}", abs.display());
    let entries = parse_test_manifest_rdfxml(&src, Some(&base))?;

    // ── Filter: keep only Consistency / Inconsistency tests ───────────────────
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

        let slug = to_slug(&entry.name);
        let world_iri = format!("https://gmeow.example/w3c-owl2-el/{slug}/w");

        // ── Extract the inline premise RDF/XML ────────────────────────────────
        let premise_xml = match &entry.action {
            Some(OntologyDoc::InlineRdfXml(xml)) => xml.clone(),
            Some(OntologyDoc::Reference(_)) => {
                println!("SKIP {slug}: premise is an IRI reference, not inline RDF/XML (Lane-B)");
                skipped_unparsable += 1;
                continue;
            }
            None => {
                println!(
                    "SKIP {slug}: no recognized premise document (e.g. fsPremiseOntology only) — Lane-B"
                );
                skipped_unparsable += 1;
                continue;
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
                skipped_unparsable += 1;
                continue;
            }
        };
        if premise_ds.quad_refs().count() == 0 {
            println!("SKIP {slug}: premise parsed to zero quads (vacuous pass not permitted)");
            skipped_unparsable += 1;
            continue;
        }

        // ── Build world-scoped N-Quads: serialize premise as N-Triples, append world IRI ──
        let (input_nq, quad_count) =
            match premise_ds_to_world_nquads(premise_ds.as_ref(), &world_iri) {
                Ok(r) => r,
                Err(e) => {
                    println!("SKIP {slug}: premise N-Quads build failed: {e}");
                    skipped_unparsable += 1;
                    continue;
                }
            };

        if input_nq.trim().is_empty() {
            println!("SKIP {slug}: premise yields zero valid N-Quads (vacuous pass not permitted)");
            skipped_unparsable += 1;
            continue;
        }

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
        std::fs::write(source_dir.join("premise.rdf"), &premise_xml)
            .map_err(|e| format!("cannot write source/premise.rdf for {slug}: {e}"))?;

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

/// Read the value following a flag, or error with the flag name.
fn next(args: &mut impl Iterator<Item = String>, flag: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("{flag} requires a value"))
}
