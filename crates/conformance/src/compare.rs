// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The three comparison modes of the runner contract.
//!
//! These are the canonical runner comparators; the Python `logic_runner.py` they
//! replaced was retired. The implementation drives the native, oxigraph-free RDF
//! kernel (parse → frozen `RdfDataset` IR → native RDFC-1.0 canonicalization):
//!
//! * [`compare_rdf`] — blank-node-aware **graph isomorphism** via RDFC-1.0
//!   canonicalization. Two serialized RDF documents are equal iff their canonical
//!   quad lists match (`rdflib.compare.isomorphic` had the same verdict; the native
//!   codecs additionally read the RDF 1.2 `<< … >>` triple terms the
//!   `canonical-rdf12` projection emits). The four RDF projection goldens
//!   (`owl-dl`, `owl-el`, `gufo`, `canonical-rdf12`) additionally receive a
//!   **byte-exact banner-header check** via [`leading_comment_block`] — graph
//!   isomorphism alone is blind to Turtle `#`-comment banners, which caused stale
//!   `(logic_projections.py)` banners to survive undetected in goldens.
//! * [`compare_canonical_json`] — sorted-key **canonical JSON** equality for
//!   `verdicts.json`, `preservation-ledger.json`, `certification.json`,
//!   `budget.json`, and `answers/*.json`.
//! * [`compare_explanation_skeleton`] — **cited-IRI set** equality for
//!   `explanation/*.md`, NEVER surface prose.
//!
//! [`diff_case`] drives all three modes over a case's full committed `expected/`
//! tree (projections, materialized N-Quads, verdicts, certification, budget,
//! explanation skeletons, answers).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use gmeow_errors::Diag;
use purrdf::{RdfTerm, SerializeGraph};

use crate::error::{Io, JsonRead, RdfCompare};
use crate::run::CaseOutputs;

/// IANA media type for Turtle (the projection comparison surface).
const TURTLE: &str = "text/turtle";
/// IANA media type for N-Triples (the per-world materialized comparison).
const NTRIPLES: &str = "application/n-triples";
/// IANA media type for N-Quads (the materialized-corpus bucketing).
const NQUADS: &str = "application/n-quads";

/// Parse serialized RDF `text` of `media_type` into the frozen [`RdfDataset`] IR via
/// the native, oxigraph-free codecs (`purrdf::parse_dataset`).
fn parse_native_dataset(
    text: &str,
    media_type: &str,
) -> gmeow_errors::Result<std::sync::Arc<purrdf::RdfDataset>> {
    purrdf::parse_dataset(text.as_bytes(), media_type, None).map_err(|e| {
        Diag::of_kind(RdfCompare {
            detail: format!("RDF parse error: {e}"),
        })
    })
}

/// Canonicalize a serialized RDF document to a sorted list of canonical N-Quads
/// strings.
///
/// Parses `text` in `format`, applies RDFC-1.0 canonical blank-node labelling, and
/// returns the canonicalized quads as a sorted `Vec` of their N-Quads strings. Two
/// RDF documents are graph-isomorphic iff their canonical quad lists are equal — the
/// rdflib-free replacement for `rdflib.compare.isomorphic`.
fn canonical_quads(text: &str, media_type: &str) -> gmeow_errors::Result<Vec<String>> {
    // Native text ingress: parse via the gmeow-gts codecs, not oxigraph::io.
    let dataset = parse_native_dataset(text, media_type)?;
    // Native flat RDFC-1.0: the oxigraph-free canonical N-Quads document. This
    // is byte-identical to the prior oxigraph flat-canonical path (proven by the
    // `canonical_flat_nquads_byte_matches_oxigraph_path` gate in gmeow-rdf), so the
    // graph-isomorphism verdict is unchanged. Splitting into lines and sorting yields
    // the same canonical quad set the comparator compared before.
    let canonical = purrdf::canonical_flat_nquads(&dataset).map_err(|e| {
        Diag::of_kind(RdfCompare {
            detail: format!("RDF canonicalization error: {e}"),
        })
    })?;
    let mut quads: Vec<String> = canonical
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_owned)
        .collect();
    quads.sort();
    Ok(quads)
}

/// Compare two deterministic text projections (Datalog / N3) by exact
/// equality. Unlike the RDF targets these carry no blank-node ambiguity once the
/// front-end RDFC-1.0-canonicalizes blank labels, so a byte mismatch is a real
/// regression, not codec skew. On mismatch the line counts and the first differing
/// line are reported (capped, mirroring `compare_rdf`).
pub fn compare_text(actual: &str, expected: &str) -> Vec<String> {
    if actual == expected {
        return vec![];
    }
    let a: Vec<&str> = actual.lines().collect();
    let e: Vec<&str> = expected.lines().collect();
    let mut lines = vec![format!("text mismatch: {} vs {} lines", a.len(), e.len())];
    if let Some((i, (al, el))) = a
        .iter()
        .zip(e.iter())
        .enumerate()
        .find(|(_, (al, el))| al != el)
    {
        lines.push(format!(
            "  first diff at line {}: actual {al:?} vs expected {el:?}",
            i + 1
        ));
    }
    lines
}

/// Compare two serialized RDF documents by blank-node-aware graph equality.
///
/// Both documents are parsed and RDFC-1.0 canonicalized, and their canonical
/// quad lists compared. Returns an empty `Vec` on match, or error strings
/// describing the canonical quads unique to each side (capped at 5 each, as the
/// Python runner did). A parse error on either side surfaces as a single diff
/// line (it still fails the case loudly).
pub fn compare_rdf(actual_text: &str, expected_text: &str, media_type: &str) -> Vec<String> {
    let actual = match canonical_quads(actual_text, media_type) {
        Ok(q) => q,
        Err(e) => return vec![format!("actual {e}")],
    };
    let expected = match canonical_quads(expected_text, media_type) {
        Ok(q) => q,
        Err(e) => return vec![format!("expected {e}")],
    };
    if actual == expected {
        return vec![];
    }

    let actual_set: BTreeSet<&String> = actual.iter().collect();
    let expected_set: BTreeSet<&String> = expected.iter().collect();
    let actual_only: Vec<&&String> = actual_set.difference(&expected_set).collect();
    let expected_only: Vec<&&String> = expected_set.difference(&actual_set).collect();

    let mut lines = vec![format!(
        "RDF graph mismatch: {} vs {} quads",
        actual.len(),
        expected.len()
    )];
    for quad in actual_only.iter().take(5) {
        lines.push(format!("  actual only: {quad}"));
    }
    for quad in expected_only.iter().take(5) {
        lines.push(format!("  expected only: {quad}"));
    }
    lines
}

/// Compare two JSON values by canonical form: sorted keys, no whitespace.
///
/// The [`serde_json::Value`] is BTreeMap-backed (no `preserve_order` feature), so
/// [`serde_json::to_string`] emits object keys in sorted order — exactly the
/// canonical form the Python runner produced via `json.dumps(sort_keys=True,
/// ensure_ascii=False)`. Lists remain order-sensitive. Returns an empty `Vec` on
/// match, or one error string with both canonical forms (truncated to 200 chars).
pub fn compare_canonical_json(
    actual: &serde_json::Value,
    expected: &serde_json::Value,
) -> Vec<String> {
    let actual_canon = serde_json::to_string(actual).unwrap_or_default();
    let expected_canon = serde_json::to_string(expected).unwrap_or_default();
    if actual_canon == expected_canon {
        return vec![];
    }
    vec![format!(
        "Canonical JSON mismatch:\n  actual:   {}\n  expected: {}",
        truncate_chars(&actual_canon, 200),
        truncate_chars(&expected_canon, 200)
    )]
}

/// Compare two explanation skeletons by their cited-IRI/rule-IRI sets.
///
/// The runner contract compares `explanation/<q>.md` on the cited-IRI skeleton,
/// NEVER on surface prose. Two skeletons are equal iff their cited-IRI sets are
/// identical. Returns an empty `Vec` on match, or the missing/extra IRIs (capped
/// at 10 each).
pub fn compare_explanation_skeleton(
    actual_cited_iris: &BTreeSet<String>,
    expected_cited_iris: &BTreeSet<String>,
) -> Vec<String> {
    if actual_cited_iris == expected_cited_iris {
        return vec![];
    }
    let missing: Vec<&String> = expected_cited_iris.difference(actual_cited_iris).collect();
    let extra: Vec<&String> = actual_cited_iris.difference(expected_cited_iris).collect();
    let mut lines = vec!["Explanation skeleton mismatch (cited-IRI sets differ):".to_string()];
    for iri in missing.iter().take(10) {
        lines.push(format!("  missing (expected but not produced): <{iri}>"));
    }
    for iri in extra.iter().take(10) {
        lines.push(format!("  extra   (produced but not expected): <{iri}>"));
    }
    lines
}

/// Parse the `cited-iri-skeleton` block from an explanation markdown file.
///
/// Reads every non-empty line between the `<!-- cited-iri-skeleton` opening
/// comment and its closing `-->` marker; lines are trimmed before collection.
pub fn parse_cited_iri_skeleton(text: &str) -> BTreeSet<String> {
    let mut in_block = false;
    let mut iris = BTreeSet::new();
    for line in text.lines() {
        if line.trim() == "<!-- cited-iri-skeleton" {
            in_block = true;
            continue;
        }
        if in_block {
            if line.trim() == "-->" {
                break;
            }
            let iri = line.trim();
            if !iri.is_empty() {
                iris.insert(iri.to_string());
            }
        }
    }
    iris
}

/// Parse the `target_quad_reifier` from the prose header of an explanation file.
///
/// Looks for the line `` # Explanation for `<REIFIER>` ``. Returns the reifier
/// IRI, or an empty string if the header is absent.
pub fn parse_explanation_reifier(text: &str) -> String {
    let prefix = "# Explanation for `<";
    let suffix = ">`";
    for line in text.lines() {
        if line.starts_with(prefix)
            && line.ends_with(suffix)
            && line.len() >= prefix.len() + suffix.len()
        {
            return line[prefix.len()..line.len() - suffix.len()].to_string();
        }
    }
    String::new()
}

/// Group an N-Quads document into per-named-graph N-Triples documents.
///
/// Buckets every quad by its named-graph IRI and re-serializes each world's
/// triples as an N-Triples string. Default-graph (and blank-node-graph) triples
/// are dropped — the materialized-corpus comparison asserts only over named
/// worlds.
pub fn nquads_by_named_graph(nquads_text: &str) -> gmeow_errors::Result<BTreeMap<String, String>> {
    if nquads_text.trim().is_empty() {
        return Ok(BTreeMap::new());
    }
    let dataset = parse_native_dataset(nquads_text, NQUADS)?;

    // Collect the set of named-graph IRIs. Default-graph (graph_name == None) and
    // blank-node-graph triples are NOT part of the world-indexed comparison surface,
    // so only IRI-named graphs are bucketed.
    let mut graph_iris: BTreeSet<String> = BTreeSet::new();
    for quad in dataset.owned_quads() {
        if let Some(RdfTerm::Iri(iri)) = &quad.graph_name {
            graph_iris.insert(iri.clone());
        }
    }

    let mut by_graph: BTreeMap<String, String> = BTreeMap::new();
    for iri in graph_iris {
        // Project the named graph into a fresh default-graph dataset, then serialize
        // it as N-Triples via the native codecs (oxigraph-free). The per-world bucket
        // is re-parsed downstream by `compare_rdf`, so any valid N-Triples document of
        // the graph's content suffices.
        let projected = dataset.project_named_graph(&iri);
        let bytes = purrdf::serialize_dataset(&projected, NTRIPLES, SerializeGraph::DefaultGraph)
            .map_err(|e| {
            Diag::of_kind(RdfCompare {
                detail: format!("named graph <{iri}> N-Triples serialize error: {e}"),
            })
        })?;
        let doc = String::from_utf8(bytes).map_err(|e| {
            Diag::of_kind(RdfCompare {
                detail: format!("named graph <{iri}> N-Triples not UTF-8: {e}"),
            })
        })?;
        by_graph.insert(iri, doc);
    }
    Ok(by_graph)
}

/// Extract the contiguous leading block of `#`-comment lines from a serialized
/// RDF document (i.e. the generated banner header).
///
/// Returns the lines that form the unbroken prefix of `#`-starting lines joined
/// by `\n`. Scanning stops at the first line that neither starts with `#` nor is
/// blank — blank lines interleaved within the leading `#` block are included, but
/// a non-`#`, non-blank line terminates the block. This precisely captures the
/// two-line banner that `gmeow logic compile` prepends to every RDF projection:
///
/// ```text
/// # GENERATED by `gmeow logic compile` — DO NOT EDIT.
/// # OWL 2 DL projection of the canonical logic: program.
/// ```
///
/// Used by [`diff_case`] to byte-compare the banner headers of the four RDF
/// projection goldens against the canonical produced banner, guarding the blind
/// spot left by graph-isomorphism comparison (which is comment-transparent).
pub(crate) fn leading_comment_block(s: &str) -> String {
    let mut lines: Vec<&str> = Vec::new();
    for line in s.lines() {
        if line.starts_with('#') || (line.trim().is_empty() && !lines.is_empty()) {
            lines.push(line);
        } else {
            break;
        }
    }
    // Trim any trailing blank lines that were included in the block.
    while lines
        .last()
        .map(|l: &&str| l.trim().is_empty())
        .unwrap_or(false)
    {
        lines.pop();
    }
    lines.join("\n")
}

/// Truncate a string to at most `n` characters (UTF-8 safe), mirroring Python's
/// `s[:n]` slice used in the canonical-JSON mismatch message.
fn truncate_chars(s: &str, n: usize) -> String {
    s.chars().take(n).collect()
}

// ── Case-level diff: actual vs committed expected/ ──────────────────────────────

/// Diff a [`CaseOutputs`] against the committed `expected/` files for `case_dir`.
///
/// Every committed expected artifact is checked with the appropriate comparison
/// mode. Missing goldens are treated as mismatches only when the corresponding
/// output is non-trivial (verification honesty); the certification / budget
/// opt-in rules follow the runner contract. Returns an empty `Vec` when the case
/// matches all of its goldens.
///
/// `witnesses.json` is intentionally NOT compared (it is a bless-only side file).
/// The retired Python `logic_runner.diff_case` that this replaced has since been
/// removed.
pub fn diff_case(case_dir: &Path, out: &CaseOutputs) -> Vec<String> {
    let case_id = &out.case_id;
    let mut diffs: Vec<String> = Vec::new();
    let expected = case_dir.join("expected");
    let proj = expected.join("projections");

    // Re-read the raw profile for the opt-in flags (lenient: an unreadable/non-object
    // profile is treated as "no opt-in").
    let profile_val = read_profile_value(case_dir);
    let cert_opt_in = profile_val
        .get("certify")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let declares_budget = profile_val.get("budget_params").is_some();

    // ── Projection RDF targets ────────────────────────────────────────────────
    if proj.is_dir() {
        const RDF: [(&str, &str); 4] = [
            ("owl-dl", "owl-dl.ttl"),
            ("owl-el", "owl-el.ttl"),
            ("gufo", "gufo.ttl"),
            ("canonical-rdf12", "canonical-rdf12.ttl"),
        ];
        for (target, filename) in RDF {
            let expected_path = proj.join(filename);
            let produced = out.projections.rdf.get(target);
            if !expected_path.exists() {
                // Missing golden + produced RDF ⇒ hard failure (verification honesty).
                if produced.is_some() {
                    diffs.push(format!(
                        "[{case_id}] projection {target}: golden {filename} is missing from \
                         expected/projections/ — run the bless mode to generate it"
                    ));
                }
                continue;
            }
            match (produced, read_text(&expected_path)) {
                (Some(content), Ok(expected_text)) => {
                    // Banner byte-check: graph isomorphism is comment-transparent, so a
                    // stale or wrong banner in the golden would pass undetected. Compare
                    // the leading `#`-comment block byte-exactly before the isomorphism
                    // check. Both checks always run (fail-fast is the isomorphism side).
                    let produced_hdr = leading_comment_block(content);
                    let golden_hdr = leading_comment_block(&expected_text);
                    if produced_hdr != golden_hdr {
                        diffs.push(format!(
                            "[{case_id}] {target}: banner drift — golden header does not \
                             byte-match the canonical generated banner \
                             (produced: {produced_hdr:?}, golden: {golden_hdr:?}); \
                             re-bless this golden"
                        ));
                    }
                    for d in compare_rdf(content, &expected_text, TURTLE) {
                        diffs.push(format!("[{case_id}] {target}: {d}"));
                    }
                }
                (None, _) => diffs.push(format!(
                    "[{case_id}] projection {target}: no graph produced"
                )),
                (_, Err(e)) => diffs.push(format!("[{case_id}] {target}: {e}")),
            }
        }

        // Projection report (graph-isomorphism).
        let report_path = proj.join("projection-report.ttl");
        if report_path.exists() {
            match read_text(&report_path) {
                Ok(expected_text) => {
                    for d in compare_rdf(&out.projections.report_turtle, &expected_text, TURTLE) {
                        diffs.push(format!("[{case_id}] projection-report: {d}"));
                    }
                }
                Err(e) => diffs.push(format!("[{case_id}] projection-report: {e}")),
            }
        }

        // Preservation ledger (canonical JSON).
        let ledger_path = proj.join("preservation-ledger.json");
        if ledger_path.exists() {
            match read_json(&ledger_path) {
                Ok(expected_ledger) => {
                    for d in compare_canonical_json(&out.projections.ledger, &expected_ledger) {
                        diffs.push(format!("[{case_id}] preservation-ledger: {d}"));
                    }
                }
                Err(e) => diffs.push(format!(
                    "[{case_id}] cannot parse expected preservation-ledger.json: {e}"
                )),
            }
        }

        // ── Validation-shape projections (SHACL Core / ShEx / residue) ─────────
        // The closed-world SHACL Core projection of the program's `logic:ValidationShape`s,
        // compared by graph isomorphism (like every other RDF target). Opt-in like the report:
        // a shape-free case commits no golden and produces the empty document, so absence is a
        // no-op; a validation case pins the derived shape document.
        let shacl_core_path = proj.join("shacl-core.ttl");
        if shacl_core_path.exists() {
            match read_text(&shacl_core_path) {
                Ok(expected_text) => {
                    for d in compare_rdf(&out.projections.shacl_core, &expected_text, TURTLE) {
                        diffs.push(format!("[{case_id}] shacl-core: {d}"));
                    }
                }
                Err(e) => diffs.push(format!("[{case_id}] shacl-core: {e}")),
            }
        }

        // The ShEx projection (ShExC), compared by exact text — ShExC carries no blank-node
        // ambiguity once the front-end canonicalizes, so a byte mismatch is a real regression.
        let shex_path = proj.join("shapes.shex");
        if shex_path.exists() {
            match read_text(&shex_path) {
                Ok(expected_text) => {
                    for d in compare_text(&out.projections.shex, &expected_text) {
                        diffs.push(format!("[{case_id}] shex: {d}"));
                    }
                }
                Err(e) => diffs.push(format!("[{case_id}] shex: {e}")),
            }
        }

        // The per-target validation-shape residue set (canonical JSON): the constructs each
        // shape surface (SHACL Core / ShEx) cannot faithfully hold, carried in the canonical
        // logic: layer. The ShEx residue is a strict superset of the SHACL one.
        let residue_path = proj.join("residue.json");
        if residue_path.exists() {
            match read_json(&residue_path) {
                Ok(expected) => {
                    for d in compare_canonical_json(&out.projections.residue, &expected) {
                        diffs.push(format!("[{case_id}] residue: {d}"));
                    }
                }
                Err(e) => diffs.push(format!(
                    "[{case_id}] cannot parse expected residue.json: {e}"
                )),
            }
        }

        // ── Projection text targets (Datalog / N3) ───────────────────────────
        // Deterministic since the front-end RDFC-1.0-canonicalizes blank labels, so
        // these are gated by exact text equality (parity with the RDF block's
        // missing-golden hard-fail). Every committed `projections/` dir carries all
        // two text files, so a missing-but-produced target is a real regression.
        const TEXT: [(&str, &str); 2] = [("datalog", "datalog.dl"), ("n3", "n3.n3")];
        for (target, filename) in TEXT {
            let expected_path = proj.join(filename);
            let produced = out.projections.text.get(target);
            if !expected_path.exists() {
                if produced.is_some() {
                    diffs.push(format!(
                        "[{case_id}] projection {target}: golden {filename} is missing from \
                         expected/projections/ — run the bless mode to generate it"
                    ));
                }
                continue;
            }
            match (produced, read_text(&expected_path)) {
                (Some(content), Ok(expected_text)) => {
                    for d in compare_text(content, &expected_text) {
                        diffs.push(format!("[{case_id}] {target}: {d}"));
                    }
                }
                (None, _) => {
                    diffs.push(format!("[{case_id}] projection {target}: no text produced"));
                }
                (_, Err(e)) => diffs.push(format!("[{case_id}] {target}: {e}")),
            }
        }

        // ── CL dialect projections (cl-roundtrip cases only) ───────────────────
        // A `cl-roundtrip` case pins the three ISO 24707 dialect renderings
        // (`gmeow.{clif,cgif,xcl}`) byte-exactly. Opt-in like certification: compared
        // only when the golden exists — a non-`cl-roundtrip` case produces none and
        // pins none, so `!exists && !produced` is correctly a no-op.
        const DIALECTS: [(&str, &str); 3] = [
            ("clif", "gmeow.clif"),
            ("cgif", "gmeow.cgif"),
            ("xcl", "gmeow.xcl"),
        ];
        for (target, filename) in DIALECTS {
            let expected_path = proj.join(filename);
            let produced = out.projections.text.get(target);
            if !expected_path.exists() {
                if produced.is_some() {
                    diffs.push(format!(
                        "[{case_id}] projection {target}: golden {filename} is missing from \
                         expected/projections/ — run the bless mode to generate it"
                    ));
                }
                continue;
            }
            match (produced, read_text(&expected_path)) {
                (Some(content), Ok(expected_text)) => {
                    for d in compare_text(content, &expected_text) {
                        diffs.push(format!("[{case_id}] {target}: {d}"));
                    }
                }
                (None, _) => diffs.push(format!(
                    "[{case_id}] projection {target}: golden {filename} exists but no dialect \
                     text produced (is this a cl-roundtrip case?)"
                )),
                (_, Err(e)) => diffs.push(format!("[{case_id}] {target}: {e}")),
            }
        }
    }

    // ── Verdicts (canonical JSON) ─────────────────────────────────────────────
    diff_json_golden(
        &expected.join("verdicts.json"),
        &out.verdicts,
        case_id,
        "verdicts",
        &mut diffs,
    );

    // ── Certification (opt-in canonical JSON) ─────────────────────────────────
    let cert_path = expected.join("certification.json");
    if cert_path.exists() {
        diff_json_golden(
            &cert_path,
            &out.certification,
            case_id,
            "certification",
            &mut diffs,
        );
    } else if cert_opt_in {
        diffs.push(format!(
            "[{case_id}] certification: golden certification.json is missing from expected/ \
             but profile.json declares \"certify\": true — run the bless mode to generate it"
        ));
    }

    // ── Runtime preservation judgment (opt-in canonical JSON) ─────────────────
    // The materialization's preservation claim: `{exact}` for the faithful chase,
    // `{sound-under}` naming the dropped derivation rules for the non-stratifiable
    // EDB-echo path. Distinct from the compile-time `preservation-ledger.json`
    // above (per-target lowering classes); this pins what a given run disclosed.
    let runtime_preservation_path = expected.join("runtime-preservation.json");
    if runtime_preservation_path.exists() {
        diff_json_golden(
            &runtime_preservation_path,
            &out.preservation,
            case_id,
            "runtime-preservation",
            &mut diffs,
        );
    }

    // ── Correspondence gates (authors-correspondences ⇒ require-golden) ────────
    // A case whose input authors `logic:Correspondence` individuals MUST commit the
    // `correspondence-gates.json` golden — the five-gate verdict report. A missing golden
    // is a hard failure (like budget / certification), never a silent pass.
    if let Some(actual_gates) = &out.correspondence_gates {
        let gates_path = expected.join("correspondence-gates.json");
        if gates_path.exists() {
            diff_json_golden(
                &gates_path,
                actual_gates,
                case_id,
                "correspondence-gates",
                &mut diffs,
            );
        } else {
            diffs.push(format!(
                "[{case_id}] correspondence-gates: golden correspondence-gates.json is missing \
                 from expected/ but the program authors logic:Correspondence individuals — run \
                 the bless mode to generate it"
            ));
        }
    }

    // ── Common Logic round-trip verdict (cl-roundtrip ⇒ require-golden) ────────
    // A `cl-roundtrip` case MUST commit the `cl-dialects.json` golden — the per-dialect
    // round-trip + cross-dialect verdict. A missing golden is a hard failure, never a
    // silent pass.
    if let Some(actual_cl) = &out.cl_dialects {
        let cl_path = expected.join("cl-dialects.json");
        if cl_path.exists() {
            diff_json_golden(&cl_path, actual_cl, case_id, "cl-dialects", &mut diffs);
        } else {
            diffs.push(format!(
                "[{case_id}] cl-dialects: golden cl-dialects.json is missing from expected/ but \
                 this is a cl-roundtrip case — run the bless mode to generate it"
            ));
        }
    }

    // ── Budget governor markers (declares-budget ⇒ require-golden) ─────────────
    let budget_path = expected.join("budget.json");
    let actual_budget = budget_json(out, &profile_val);
    if budget_path.exists() {
        diff_json_golden(&budget_path, &actual_budget, case_id, "budget", &mut diffs);
    } else if declares_budget {
        diffs.push(format!(
            "[{case_id}] budget: golden budget.json is missing from expected/ but profile.json \
             declares budget_params — run the bless mode to generate it"
        ));
    }

    // ── Per-quad budget stamps (frontier-aware; step-budget forward ⇒ require) ─
    // The frontier-aware PER-QUAD stamp is invisible in `materialized.nq` (graph
    // isomorphism, no status column), so a dedicated golden carries it. A case that
    // declares a step/derivation budget AND materializes quads MUST pin it — that is
    // exactly where a saturated-stratum quad (`ok`) differs from a cut-stratum one
    // (`exhausted`).
    let quad_status_path = expected.join("quad-status.json");
    if quad_status_path.exists() {
        diff_json_golden(
            &quad_status_path,
            &out.materialized_quad_status,
            case_id,
            "quad-status",
            &mut diffs,
        );
    } else if declares_step_budget(&profile_val) && !out.materialized_nquads.is_empty() {
        diffs.push(format!(
            "[{case_id}] quad-status: golden quad-status.json is missing from expected/ but \
             profile.json declares a step/derivation budget and the case materializes quads — \
             run the bless mode to generate it"
        ));
    }

    // ── Materialized N-Quads (per-world graph isomorphism) ────────────────────
    let mat_path = expected.join("materialized.nq");
    if mat_path.exists() {
        match read_text(&mat_path) {
            Err(e) => diffs.push(format!("[{case_id}] materialized.nq: {e}")),
            Ok(expected_nq) => {
                let actual_by_graph = nquads_by_named_graph(&out.materialized_nquads);
                let expected_by_graph = nquads_by_named_graph(&expected_nq);
                match (actual_by_graph, expected_by_graph) {
                    (Err(e), _) | (_, Err(e)) => {
                        diffs.push(format!("[{case_id}] materialized.nq parse error: {e}"))
                    }
                    (Ok(actual_by_graph), Ok(expected_by_graph)) => {
                        let actual_iris: BTreeSet<&String> = actual_by_graph.keys().collect();
                        let expected_iris: BTreeSet<&String> = expected_by_graph.keys().collect();
                        for extra in actual_iris.difference(&expected_iris) {
                            diffs.push(format!(
                                "[{case_id}] materialized.nq: named graph present in actual but \
                                 not expected: <{extra}>"
                            ));
                        }
                        for missing in expected_iris.difference(&actual_iris) {
                            diffs.push(format!(
                                "[{case_id}] materialized.nq: named graph present in expected but \
                                 not actual: <{missing}>"
                            ));
                        }
                        for g in actual_iris.intersection(&expected_iris) {
                            let g = *g;
                            for d in
                                compare_rdf(&actual_by_graph[g], &expected_by_graph[g], NTRIPLES)
                            {
                                diffs.push(format!("[{case_id}] materialized.nq [<{g}>]: {d}"));
                            }
                        }
                    }
                }
            }
        }
    }

    // ── Explanation skeletons (cited-IRI set, matched by reifier) ─────────────
    let expl_dir = expected.join("explanation");
    if expl_dir.is_dir() {
        let produced: BTreeMap<&str, &BTreeSet<String>> = out
            .explanations
            .iter()
            .map(|e| (e.target_quad_reifier.as_str(), &e.cited_iris))
            .collect();
        for md_path in sorted_files_with_ext(&expl_dir, "md") {
            let name = md_path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>.md")
                .to_string();
            let md_text = match read_text(&md_path) {
                Ok(t) => t,
                Err(e) => {
                    diffs.push(format!("[{case_id}] explanation {name}: {e}"));
                    continue;
                }
            };
            let committed = parse_cited_iri_skeleton(&md_text);
            let reifier = parse_explanation_reifier(&md_text);
            match produced.get(reifier.as_str()) {
                Some(cited) => {
                    for d in compare_explanation_skeleton(cited, &committed) {
                        diffs.push(format!("[{case_id}] explanation {name}: {d}"));
                    }
                }
                None => diffs.push(format!(
                    "[{case_id}] explanation {name}: golden cites reifier <{reifier}> but the \
                     runner produced no explanation for it"
                )),
            }
        }
    }

    // ── Answers (backward goals) ──────────────────────────────────────────────
    let queries_dir = case_dir.join("queries");
    let answers_dir = expected.join("answers");
    if queries_dir.is_dir() {
        for qfile in sorted_files_with_ext(&queries_dir, "logic") {
            let stem = qfile
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("<unknown>")
                .to_string();
            let expected_path = answers_dir.join(format!("{stem}.json"));
            if !expected_path.exists() {
                diffs.push(format!(
                    "[{case_id}] answers/{stem}: golden expected/answers/{stem}.json is missing"
                ));
                continue;
            }
            let expected_answer = match read_json(&expected_path) {
                Ok(v) => v,
                Err(e) => {
                    diffs.push(format!(
                        "[{case_id}] cannot parse expected answers/{stem}.json: {e}"
                    ));
                    continue;
                }
            };
            match out.answers.get(&stem) {
                Some(actual) => {
                    for d in compare_canonical_json(actual, &expected_answer) {
                        diffs.push(format!("[{case_id}] answers/{stem}: {d}"));
                    }
                }
                None => diffs.push(format!(
                    "[{case_id}] answers/{stem}: run produced no answer set"
                )),
            }
        }
    }

    diffs
}

/// Compare a JSON `actual` against the golden at `path` when it exists, pushing
/// any diffs (prefixed `[case_id] label:`). A missing golden is a no-op here —
/// the caller decides whether absence is a failure.
fn diff_json_golden(
    path: &Path,
    actual: &serde_json::Value,
    case_id: &str,
    label: &str,
    diffs: &mut Vec<String>,
) {
    if !path.exists() {
        return;
    }
    match read_json(path) {
        Ok(expected) => {
            for d in compare_canonical_json(actual, &expected) {
                diffs.push(format!("[{case_id}] {label}: {d}"));
            }
        }
        Err(e) => diffs.push(format!(
            "[{case_id}] cannot parse expected {label}.json: {e}"
        )),
    }
}

/// Read a case's `profile.json` as a JSON value, returning `{}` on any error
/// (lenient diff-phase read: an unreadable or non-object profile is treated as
/// "no opt-in").
pub(crate) fn read_profile_value(case_dir: &Path) -> serde_json::Value {
    std::fs::read_to_string(case_dir.join("profile.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .filter(serde_json::Value::is_object)
        .unwrap_or_else(|| serde_json::Value::Object(serde_json::Map::new()))
}

/// Whether the case declares a step/derivation budget (`max_steps` or
/// `max_rule_firings`) — the two governor knobs the completion frontier tracks. A pure
/// `max_answers` cap (a post-fixpoint truncation) or a `time_ms` demotion does not run
/// the native semi-naive governor, so it carries no frontier and its `budget.json`
/// keeps the two-key status/incomplete form.
pub(crate) fn declares_step_budget(profile_val: &serde_json::Value) -> bool {
    profile_val
        .get("budget_params")
        .and_then(|b| b.as_object())
        .is_some_and(|b| {
            ["max_steps", "max_rule_firings"]
                .iter()
                .any(|k| b.get(*k).is_some_and(|v| !v.is_null()))
        })
}

/// Build the `budget.json` value: the always-present `budget_status` / `incomplete`
/// markers, plus the completion-frontier markers (`strata_completed` / `strata_total` /
/// `saturated`, deterministically sorted) when the case declares a step/derivation
/// budget. Shared by the bless writer and the diff reader so the two never drift.
pub(crate) fn budget_json(out: &CaseOutputs, profile_val: &serde_json::Value) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    obj.insert(
        "budget_status".to_string(),
        serde_json::json!(out.budget_status),
    );
    obj.insert("incomplete".to_string(), serde_json::json!(out.incomplete));
    if declares_step_budget(profile_val) {
        obj.insert(
            "strata_completed".to_string(),
            serde_json::json!(out.frontier.completed),
        );
        obj.insert(
            "strata_total".to_string(),
            serde_json::json!(out.frontier.total),
        );
        obj.insert(
            "saturated".to_string(),
            serde_json::json!(out.frontier.saturated_preds.iter().collect::<Vec<_>>()),
        );
    }
    serde_json::Value::Object(obj)
}

/// Read a UTF-8 text file, mapping I/O errors to a short diff string.
fn read_text(path: &Path) -> gmeow_errors::Result<String> {
    std::fs::read_to_string(path).map_err(|e| {
        Diag::of_kind(Io {
            detail: format!("cannot read {}: {e}", path.display()),
        })
    })
}

/// Read and parse a JSON file.
fn read_json(path: &Path) -> gmeow_errors::Result<serde_json::Value> {
    let text = read_text(path)?;
    serde_json::from_str(&text).map_err(|e| {
        Diag::of_kind(JsonRead {
            detail: format!("{e}"),
        })
    })
}

/// The files directly under `dir` with extension `ext`, sorted by path.
fn sorted_files_with_ext(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == ext))
            .collect(),
        Err(_) => Vec::new(),
    };
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- compare_rdf tests ----

    #[test]
    fn rdf_identical_graphs_match() {
        let nt = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/B> .\n";
        assert_eq!(compare_rdf(nt, nt, NTRIPLES), Vec::<String>::new());
    }

    #[test]
    fn rdf_isomorphic_blank_node_graphs_match() {
        let g1 = "<https://example.org/A> <https://example.org/rel> _:x .\n_:x <https://example.org/label> <https://example.org/val> .\n";
        let g2 = "<https://example.org/A> <https://example.org/rel> _:y .\n_:y <https://example.org/label> <https://example.org/val> .\n";
        assert_eq!(compare_rdf(g1, g2, NTRIPLES), Vec::<String>::new());
    }

    #[test]
    fn rdf_differing_graphs_fail() {
        let g1 = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/B> .\n";
        let g2 = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/C> .\n";
        assert!(!compare_rdf(g1, g2, NTRIPLES).is_empty());
    }

    #[test]
    fn rdf_empty_vs_nonempty_fails() {
        let g2 = "<https://example.org/A> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.org/B> .\n";
        assert!(!compare_rdf("", g2, NTRIPLES).is_empty());
    }

    #[test]
    fn rdf_empty_vs_empty_passes() {
        assert_eq!(compare_rdf("", "", NTRIPLES), Vec::<String>::new());
    }

    #[test]
    fn rdf12_triple_terms_compare() {
        let ttl = "@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n<https://example.org/r> rdf:reifies <<( <https://example.org/s> <https://example.org/p> <https://example.org/o> )>> .\n";
        assert_eq!(compare_rdf(ttl, ttl, TURTLE), Vec::<String>::new());
    }

    // ---- compare_canonical_json (ports TestCompareCanonicalJson) ----

    #[test]
    fn json_identical_match() {
        let d = serde_json::json!({"a": 1, "b": "hello"});
        assert_eq!(compare_canonical_json(&d, &d), Vec::<String>::new());
    }

    #[test]
    fn json_key_order_independent() {
        let d1 = serde_json::json!({"z": 3, "a": 1, "m": "foo"});
        let d2 = serde_json::json!({"a": 1, "m": "foo", "z": 3});
        assert_eq!(compare_canonical_json(&d1, &d2), Vec::<String>::new());
    }

    #[test]
    fn json_nested_key_order_independent() {
        let d1 = serde_json::json!({"x": {"b": 2, "a": 1}});
        let d2 = serde_json::json!({"x": {"a": 1, "b": 2}});
        assert_eq!(compare_canonical_json(&d1, &d2), Vec::<String>::new());
    }

    #[test]
    fn json_value_difference_fails() {
        let d1 = serde_json::json!({"a": 1});
        let d2 = serde_json::json!({"a": 2});
        assert!(!compare_canonical_json(&d1, &d2).is_empty());
    }

    #[test]
    fn json_missing_key_fails() {
        let d1 = serde_json::json!({"a": 1, "b": 2});
        let d2 = serde_json::json!({"a": 1});
        assert!(!compare_canonical_json(&d1, &d2).is_empty());
    }

    #[test]
    fn json_string_normalization_unchanged() {
        let d1 = serde_json::json!({"k": "SoundUnderApproximation"});
        let d2 = serde_json::json!({"k": "SoundUnderApproximation"});
        assert_eq!(compare_canonical_json(&d1, &d2), Vec::<String>::new());
    }

    #[test]
    fn json_list_order_matters() {
        let d1 = serde_json::json!({"arr": [1, 2, 3]});
        let d2 = serde_json::json!({"arr": [3, 2, 1]});
        assert!(!compare_canonical_json(&d1, &d2).is_empty());
    }

    // ---- compare_explanation_skeleton (ports TestCompareExplanationSkeleton) ----

    const IRI_A: &str = "https://example.org/rule/A";
    const IRI_B: &str = "https://example.org/term/B";
    const IRI_C: &str = "https://example.org/reifier/abc";

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn skeleton_identical_match() {
        let iris = set(&[IRI_A, IRI_B]);
        assert_eq!(
            compare_explanation_skeleton(&iris, &iris),
            Vec::<String>::new()
        );
    }

    #[test]
    fn skeleton_different_fail() {
        let actual = set(&[IRI_A, IRI_B]);
        let expected = set(&[IRI_A, IRI_C]);
        assert!(!compare_explanation_skeleton(&actual, &expected).is_empty());
    }

    #[test]
    fn skeleton_extra_iri_flagged() {
        let actual = set(&[IRI_A, IRI_B, IRI_C]);
        let expected = set(&[IRI_A, IRI_B]);
        let result = compare_explanation_skeleton(&actual, &expected);
        assert!(!result.is_empty());
        let combined = result.join("\n");
        assert!(combined.to_lowercase().contains("extra") || combined.contains(IRI_C));
    }

    #[test]
    fn skeleton_missing_iri_flagged() {
        let actual = set(&[IRI_A]);
        let expected = set(&[IRI_A, IRI_B]);
        let result = compare_explanation_skeleton(&actual, &expected);
        assert!(!result.is_empty());
        let combined = result.join("\n");
        assert!(combined.to_lowercase().contains("missing") || combined.contains(IRI_B));
    }

    // ---- skeleton/reifier parsing (ports the diff_case explanation-test parsing) ----

    #[test]
    fn parse_skeleton_block_collects_iris() {
        let md = "# Explanation for `<https://example.org/reifier/x>`\n\n\
                  <!-- cited-iri-skeleton\n  https://example.org/rule/A\n  https://example.org/term/B\n-->\n\n\
                  <!-- step-skeleton\n  step ...\n-->\nProse here is ignored.\n";
        let iris = parse_cited_iri_skeleton(md);
        assert_eq!(
            iris,
            set(&["https://example.org/rule/A", "https://example.org/term/B"])
        );
    }

    #[test]
    fn parse_skeleton_stops_at_close_marker() {
        // IRIs after the `-->` (e.g. inside the step-skeleton block) must NOT leak in.
        let md = "<!-- cited-iri-skeleton\n  https://example.org/only\n-->\n  https://example.org/leaked\n";
        assert_eq!(
            parse_cited_iri_skeleton(md),
            set(&["https://example.org/only"])
        );
    }

    #[test]
    fn parse_reifier_from_header() {
        let md = "# Explanation for `<https://example.org/reifier/abc>`\nbody\n";
        assert_eq!(
            parse_explanation_reifier(md),
            "https://example.org/reifier/abc"
        );
    }

    #[test]
    fn parse_reifier_absent_returns_empty() {
        assert_eq!(parse_explanation_reifier("no header here\n"), "");
    }

    // ---- leading_comment_block (banner helper) ----

    #[test]
    fn banner_identical_headers_produce_no_drift() {
        // Two documents with the SAME two-line banner but different (isomorphic) graphs:
        // the banner check must pass.
        let banner = "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
                      # OWL 2 DL projection of the canonical logic: program.";
        let body_a = "<https://example.org/A> <https://example.org/p> <https://example.org/B> .\n";
        let body_b = "<https://example.org/A> <https://example.org/p> <https://example.org/B> .\n";
        let doc_a = format!("{banner}\n{body_a}");
        let doc_b = format!("{banner}\n{body_b}");
        let produced_hdr = leading_comment_block(&doc_a);
        let golden_hdr = leading_comment_block(&doc_b);
        assert_eq!(produced_hdr, golden_hdr, "same banner must not flag drift");
    }

    #[test]
    fn banner_stale_python_module_suffix_detected() {
        // The headline defect: old golden had `(logic_projections.py)` in the GENERATED
        // line; produced banner no longer has it. The helper must detect the mismatch.
        let produced_banner = "# GENERATED by `gmeow logic compile` — DO NOT EDIT.\n\
                               # OWL 2 DL projection of the canonical logic: program.";
        let stale_golden_banner = "# GENERATED by `gmeow logic compile` (logic_projections.py) — DO NOT EDIT.\n\
             # OWL 2 DL projection of the canonical logic: program.";
        let body = "<https://example.org/A> <https://example.org/p> <https://example.org/B> .\n";
        let produced_doc = format!("{produced_banner}\n{body}");
        let stale_doc = format!("{stale_golden_banner}\n{body}");
        let produced_hdr = leading_comment_block(&produced_doc);
        let golden_hdr = leading_comment_block(&stale_doc);
        assert_ne!(
            produced_hdr, golden_hdr,
            "stale (logic_projections.py) banner must be detected as drift"
        );
    }

    #[test]
    fn banner_helper_stops_at_first_non_comment_non_blank_line() {
        let doc = "# line one\n# line two\n<https://example.org/s> <https://example.org/p> <https://example.org/o> .\n# not in banner\n";
        assert_eq!(leading_comment_block(doc), "# line one\n# line two");
    }

    #[test]
    fn banner_helper_no_comments_returns_empty() {
        let doc = "<https://example.org/s> <https://example.org/p> <https://example.org/o> .\n";
        assert_eq!(leading_comment_block(doc), "");
    }

    #[test]
    fn banner_helper_only_comments_returns_all() {
        let doc = "# line one\n# line two\n";
        assert_eq!(leading_comment_block(doc), "# line one\n# line two");
    }

    // ---- nquads_by_named_graph ----

    #[test]
    fn nquads_buckets_by_named_graph_and_drops_default() {
        let nq = "<https://example.org/s> <https://example.org/p> <https://example.org/o> <https://example.org/w1> .\n\
                  <https://example.org/s2> <https://example.org/p> <https://example.org/o> <https://example.org/w2> .\n\
                  <https://example.org/sd> <https://example.org/p> <https://example.org/o> .\n";
        let by_graph = nquads_by_named_graph(nq).expect("parse");
        assert_eq!(by_graph.len(), 2);
        assert!(by_graph.contains_key("https://example.org/w1"));
        assert!(by_graph.contains_key("https://example.org/w2"));
        // Each bucket re-parses as valid N-Triples and round-trips through compare_rdf.
        let w1 = &by_graph["https://example.org/w1"];
        assert_eq!(compare_rdf(w1, w1, NTRIPLES), Vec::<String>::new());
    }

    #[test]
    fn nquads_empty_input_is_empty_map() {
        assert!(nquads_by_named_graph("   \n").expect("parse").is_empty());
    }
}
