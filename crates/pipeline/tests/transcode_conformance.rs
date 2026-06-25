// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! File-driven transcode conformance corpus harness (#671 Task 5).
//!
//! Discovers every subdirectory of `tests/transcode_corpus/` that contains a
//! `profile.json`, runs the universal transcoder on the input file, and
//! compares the output against committed expected files.
//!
//! Profile schema:
//! ```json
//! { "from": "<codec>", "to": "<codec>", "compare": "rdf" | "text",
//!   "input": "input.<ext>", "expected": "expected.<ext>" }
//! ```
//!
//! Comparison modes:
//! - `rdf`  — RDFC-1.0 canonical quad comparison via oxigraph + gmeow_rdf.
//! - `text` — exact UTF-8 trimmed equality.
//!
//! Also compares `loss.json` byte-for-byte against `realized_loss_json(&output.realized)`.

use std::path::{Path, PathBuf};

use gmeow_pipeline::transcode::{realized_loss_json, transcode, Codec};
use oxigraph::io::{RdfFormat, RdfParser};

// ── Minimum corpus size guard ──────────────────────────────────────────────────

const MIN_CASES: usize = 4;

// ── Profile ────────────────────────────────────────────────────────────────────

#[derive(serde::Deserialize, Debug)]
struct Profile {
    from: String,
    to: String,
    compare: String,
    input: String,
    expected: String,
}

// ── RDF canonicalization (mirrors crates/conformance/src/compare.rs) ──────────

fn canonical_quads(bytes: &[u8], fmt: RdfFormat) -> Result<Vec<String>, String> {
    let mut quads = Vec::new();
    for q in RdfParser::from_format(fmt).lenient().for_slice(bytes) {
        quads.push(q.map_err(|e| format!("RDF parse error: {e}"))?);
    }
    let canonical = gmeow_rdf::canonicalize_quads(quads)
        .map_err(|e| format!("RDF canonicalization error: {e}"))?;
    let mut strings: Vec<String> = canonical.iter().map(ToString::to_string).collect();
    strings.sort();
    Ok(strings)
}

fn rdf_format_for_codec(codec: &str) -> Option<RdfFormat> {
    match codec {
        "turtle" | "ttl" | "owl-rdf12" => Some(RdfFormat::Turtle),
        "ntriples" | "nt" => Some(RdfFormat::NTriples),
        "nquads" | "nq" => Some(RdfFormat::NQuads),
        "trig" => Some(RdfFormat::TriG),
        "jsonld" | "json-ld" => Some(RdfFormat::JsonLd {
            profile: Default::default(),
        }),
        "rdfxml" | "rdf-xml" | "xml" => Some(RdfFormat::RdfXml),
        _ => None,
    }
}

// ── Corpus discovery ───────────────────────────────────────────────────────────

fn discover_cases(corpus_dir: &Path) -> Vec<PathBuf> {
    let mut cases: Vec<PathBuf> = std::fs::read_dir(corpus_dir)
        .unwrap_or_else(|e| panic!("cannot open corpus dir {}: {e}", corpus_dir.display()))
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.is_dir() && p.join("profile.json").exists())
        .collect();
    cases.sort();
    cases
}

// ── Main test ─────────────────────────────────────────────────────────────────

#[test]
fn transcode_corpus() {
    let corpus_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("transcode_corpus");

    let cases = discover_cases(&corpus_dir);
    assert!(
        cases.len() >= MIN_CASES,
        "corpus has {} cases but requires at least {MIN_CASES}; \
         check that tests/transcode_corpus/ is populated",
        cases.len()
    );

    let mut failures: Vec<String> = Vec::new();

    for case_dir in &cases {
        let case_name = case_dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("<unknown>");

        // Read profile.
        let profile_text = std::fs::read_to_string(case_dir.join("profile.json"))
            .unwrap_or_else(|e| panic!("[{case_name}] cannot read profile.json: {e}"));
        let profile: Profile = serde_json::from_str(&profile_text)
            .unwrap_or_else(|e| panic!("[{case_name}] cannot parse profile.json: {e}"));

        // Resolve codecs.
        let from_codec = match Codec::from_cli_str(&profile.from) {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("[{case_name}] unknown `from` codec: {e}"));
                continue;
            }
        };
        let to_codec = match Codec::from_cli_str(&profile.to) {
            Ok(c) => c,
            Err(e) => {
                failures.push(format!("[{case_name}] unknown `to` codec: {e}"));
                continue;
            }
        };

        // Read input.
        let input_bytes = match std::fs::read(case_dir.join(&profile.input)) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!(
                    "[{case_name}] cannot read input `{}`: {e}",
                    profile.input
                ));
                continue;
            }
        };

        // Run the transcoder.
        let output = match transcode(&input_bytes, from_codec, to_codec, None) {
            Ok(o) => o,
            Err(e) => {
                failures.push(format!("[{case_name}] transcode error: {e}"));
                continue;
            }
        };

        // Read expected output.
        let expected_bytes = match std::fs::read(case_dir.join(&profile.expected)) {
            Ok(b) => b,
            Err(e) => {
                failures.push(format!(
                    "[{case_name}] cannot read expected `{}`: {e}",
                    profile.expected
                ));
                continue;
            }
        };

        // Compare output.
        let compare_err: Option<String> = match profile.compare.as_str() {
            "rdf" => {
                let fmt = match rdf_format_for_codec(&profile.to) {
                    Some(f) => f,
                    None => {
                        failures.push(format!(
                            "[{case_name}] no RDF format mapping for `to` codec `{}`",
                            profile.to
                        ));
                        continue;
                    }
                };
                match (
                    canonical_quads(&output.bytes, fmt),
                    canonical_quads(&expected_bytes, fmt),
                ) {
                    (Ok(actual), Ok(expected)) if actual == expected => None,
                    (Ok(actual), Ok(expected)) => {
                        let actual_set: std::collections::BTreeSet<_> = actual.iter().collect();
                        let expected_set: std::collections::BTreeSet<_> = expected.iter().collect();
                        let only_actual: Vec<_> =
                            actual_set.difference(&expected_set).take(5).collect();
                        let only_expected: Vec<_> =
                            expected_set.difference(&actual_set).take(5).collect();
                        let mut msg = format!(
                            "RDF mismatch: {} actual quads vs {} expected quads",
                            actual.len(),
                            expected.len()
                        );
                        for q in only_actual {
                            msg.push_str(&format!("\n  actual only: {q}"));
                        }
                        for q in only_expected {
                            msg.push_str(&format!("\n  expected only: {q}"));
                        }
                        Some(msg)
                    }
                    (Err(e), _) => Some(format!("actual RDF parse error: {e}")),
                    (_, Err(e)) => Some(format!("expected RDF parse error: {e}")),
                }
            }
            "text" => {
                let actual = String::from_utf8_lossy(&output.bytes);
                let expected = String::from_utf8_lossy(&expected_bytes);
                if actual.trim() == expected.trim() {
                    None
                } else {
                    let a_lines: Vec<_> = actual.trim().lines().collect();
                    let e_lines: Vec<_> = expected.trim().lines().collect();
                    let mut msg = format!(
                        "text mismatch: {} actual lines vs {} expected lines",
                        a_lines.len(),
                        e_lines.len()
                    );
                    if let Some((i, (al, el))) = a_lines
                        .iter()
                        .zip(e_lines.iter())
                        .enumerate()
                        .find(|(_, (al, el))| al != el)
                    {
                        msg.push_str(&format!(
                            "\n  first diff at line {}: actual {al:?} vs expected {el:?}",
                            i + 1
                        ));
                    }
                    Some(msg)
                }
            }
            other => {
                failures.push(format!(
                    "[{case_name}] unknown compare mode `{other}`; expected `rdf` or `text`"
                ));
                continue;
            }
        };

        if let Some(msg) = compare_err {
            failures.push(format!("[{case_name}] output mismatch: {msg}"));
        }

        // Compare loss.json.
        let loss_path = case_dir.join("loss.json");
        match std::fs::read_to_string(&loss_path) {
            Err(e) => {
                failures.push(format!("[{case_name}] cannot read loss.json: {e}"));
            }
            Ok(expected_loss) => {
                // `realized_loss_json` emits no trailing newline; the committed
                // loss.json carries one (repo end-of-file convention). Compare
                // trim-end so both forms agree.
                let actual_loss = realized_loss_json(&output.realized);
                if actual_loss.trim_end() != expected_loss.trim_end() {
                    failures.push(format!(
                        "[{case_name}] loss.json mismatch:\n  actual:   {actual_loss}\n  expected: {expected_loss}"
                    ));
                }
            }
        }
    }

    if !failures.is_empty() {
        let msg = failures.join("\n");
        panic!(
            "{} corpus case(s) failed out of {}:\n{msg}",
            failures.len(),
            cases.len()
        );
    }
}
