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
//! { "from": "<codec>", "to": "<codec>", "compare": "rdf" | "text" | "star",
//!   "input": "input.<ext>", "expected": "expected.<ext>" }
//! ```
//!
//! Comparison modes:
//! - `rdf`  — RDFC-1.0 canonical quad comparison via oxigraph + gmeow_rdf.
//! - `text` — exact UTF-8 trimmed equality.
//! - `star` — round-trip via `parse_jsonld_star` / `yaml_ld_star_to_json` +
//!   RDFC-1.0 comparison; used for JSON-LD-star and YAML-LD-star targets that
//!   oxigraph cannot parse directly.
//!
//! Also compares `loss.json` byte-for-byte against `realized_loss_json(&output.realized)`.

use std::path::{Path, PathBuf};

use gmeow_pipeline::stages::yaml_ld::{parse_jsonld_star, yaml_ld_star_to_json};
use gmeow_pipeline::transcode::{realized_loss_json, transcode, Codec};
use gmeow_rdf::NativeRdfFormat;

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

fn canonical_quads(bytes: &[u8], fmt: NativeRdfFormat) -> Result<Vec<String>, String> {
    // Native text ingress (#909) + native full RDFC-1.0 (#910): parse into the IR and
    // canonicalize via the flattened path (`canonical_flat_nquads`), byte-identical to
    // the prior oxigraph parse + `canonicalize_quads`.
    let dataset = gmeow_rdf::parse_dataset(bytes, fmt.media_type(), None)
        .map_err(|e| format!("RDF parse error: {e}"))?;
    let canonical = gmeow_rdf::canonical_flat_nquads(&dataset)
        .map_err(|e| format!("RDF canonicalization error: {e}"))?;
    let mut strings: Vec<String> = canonical.lines().map(str::to_owned).collect();
    strings.sort();
    Ok(strings)
}

/// Parse star-FREE JSON-LD bytes via the native JSON-LD codec
/// ([`gmeow_rdf::native_codecs::jsonld::parse_jsonld`]) and return RDFC-1.0
/// canonical quad strings.
///
/// Used for the `"rdf"` compare mode when the `to` codec is plain `jsonld`:
/// JSON-LD has no flat [`NativeRdfFormat`] variant (it is not a line/Turtle-family
/// format), so the flat `canonical_quads` path cannot ingest it. The JSON-LD
/// output of a star-dropping transcode is star-free, so the base parser suffices.
fn canonical_quads_jsonld(bytes: &[u8]) -> Result<Vec<String>, String> {
    let dataset = gmeow_rdf::native_codecs::jsonld::parse_jsonld(bytes)
        .map_err(|e| format!("jsonld parse error: {e}"))?;
    let canonical = gmeow_rdf::canonical_flat_nquads(&dataset)
        .map_err(|e| format!("RDF canonicalization error: {e}"))?;
    let mut strings: Vec<String> = canonical.lines().map(str::to_owned).collect();
    strings.sort();
    Ok(strings)
}

/// Parse JSON-LD-star or YAML-LD-star bytes via the pipeline's own round-trip
/// path (the inverse of the emitter) and return RDFC-1.0 canonical quad strings.
///
/// Used for the `"star"` compare mode: oxigraph cannot parse jsonld-star /
/// yaml-ld-star directly, so we decode through `parse_jsonld_star` (which
/// understands the `@annotation` idiom emitted by the GMEOW serializer) and
/// then canonicalize via gmeow_rdf.
fn canonical_quads_star(bytes: &[u8], to_codec: &str) -> Result<Vec<String>, String> {
    let json_bytes = match to_codec {
        "jsonld-star" | "json-ld-star" => bytes.to_vec(),
        "yaml-ld-star" | "yamlld-star" => {
            let json_str =
                yaml_ld_star_to_json(bytes).map_err(|e| format!("yaml-ld-star to json: {e}"))?;
            json_str.into_bytes()
        }
        other => return Err(format!("canonical_quads_star: unknown codec {other:?}")),
    };
    // `parse_jsonld_star` returns the frozen native carrier (RDF 1.2 statement layer
    // already folded). `canonical_flat_nquads` un-folds it back to flat `rdf:reifies`
    // / annotation rows before RDFC-1.0 canonicalizing — byte-identical to the prior
    // oxigraph-quad canonicalize path.
    let dataset = parse_jsonld_star(&json_bytes).map_err(|e| format!("parse jsonld-star: {e}"))?;
    let canonical = gmeow_rdf::canonical_flat_nquads(&dataset)
        .map_err(|e| format!("RDF canonicalization error: {e}"))?;
    let mut strings: Vec<String> = canonical.lines().map(str::to_owned).collect();
    strings.sort();
    Ok(strings)
}

fn rdf_format_for_codec(codec: &str) -> Option<NativeRdfFormat> {
    match codec {
        "turtle" | "ttl" | "owl-rdf12" => Some(NativeRdfFormat::Turtle),
        "ntriples" | "nt" => Some(NativeRdfFormat::NTriples),
        "nquads" | "nq" => Some(NativeRdfFormat::NQuads),
        "trig" => Some(NativeRdfFormat::TriG),
        "rdfxml" | "rdf-xml" | "xml" => Some(NativeRdfFormat::RdfXml),
        // JSON-LD/YAML-LD are compared via the `star` path (`canonical_quads_star`),
        // never the flat `rdf` path, so they have no native flat-format mapping here.
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
                // JSON-LD has no flat `NativeRdfFormat` variant (it is not a
                // line/Turtle-family format), so it is canonicalized through the
                // native JSON-LD codec; every other `rdf`-compare codec maps to a
                // flat format.
                let (actual_canon, expected_canon) =
                    if matches!(profile.to.as_str(), "jsonld" | "json-ld") {
                        (
                            canonical_quads_jsonld(&output.bytes),
                            canonical_quads_jsonld(&expected_bytes),
                        )
                    } else {
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
                        (
                            canonical_quads(&output.bytes, fmt),
                            canonical_quads(&expected_bytes, fmt),
                        )
                    };
                match (actual_canon, expected_canon) {
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
            "star" => {
                match (
                    canonical_quads_star(&output.bytes, &profile.to),
                    canonical_quads_star(&expected_bytes, &profile.to),
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
                            "RDF mismatch (star round-trip): {} actual quads vs {} expected quads",
                            actual.len(),
                            expected.len()
                        );
                        for q in only_actual {
                            msg.push_str(&format!(
                                "
  actual only: {q}"
                            ));
                        }
                        for q in only_expected {
                            msg.push_str(&format!(
                                "
  expected only: {q}"
                            ));
                        }
                        Some(msg)
                    }
                    (Err(e), _) => Some(format!("actual star parse error: {e}")),
                    (_, Err(e)) => Some(format!("expected star parse error: {e}")),
                }
            }
            other => {
                failures.push(format!(
                    "[{case_name}] unknown compare mode `{other}`; expected `rdf`, `text`, or `star`"
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
