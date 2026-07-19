// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Parity acceptance tests for the consumer `gmeow` binary.
//!
//! These mirror the behaviors pinned by the Python `tests/test_cli.py` and
//! `tests/test_validate_rdf.py` against the same embedded/bundled snapshot an
//! installed wheel uses. Each test drives the built binary through `assert_cmd`,
//! so the split (product → stdout, diagnostics → stderr) and the `0`/`1`/`2` exit
//! convention are exercised end to end.
//!
//! (The `gmeow-dev logic … / reason …` surface belongs to the repo-maintenance
//! `gmeow-dev` bin and is covered by that crate's parity tests, so it is not
//! mirrored here.)

use std::path::{Path, PathBuf};
use std::process::Command as StdCommand;

use assert_cmd::Command;
use predicates::prelude::*;

/// The repo-root path of a committed validate fixture.
fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures/validate")
        .join(name)
}

/// A fresh, unique, empty scratch directory under the system temp dir.
fn scratch(tag: &str) -> PathBuf {
    let mut dir = std::env::temp_dir();
    dir.push(format!(
        "gmeow-cli-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

// ── version / describe (test_cli.py) ─────────────────────────────────────────

#[test]
fn version_prints_the_package_version() {
    gmeow()
        .arg("version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn describe_known_term_renders_prose() {
    // Mirrors the describe prose expectations: a known kernel term resolves to a
    // rendered card on stdout with exit 0.
    gmeow()
        .args(["describe", "Entity"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Entity"));
}

#[test]
fn describe_unknown_language_fails_with_available_list() {
    // test_describe_unknown_language_fails: a bad --lang exits non-zero and lists
    // the available languages.
    gmeow()
        .args(["describe", "Person", "--lang", "notatag"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("unknown language tag")
                .and(predicate::str::contains("Available languages")),
        );
}

#[test]
fn describe_env_language_rejected_if_unknown() {
    // test_describe_env_language_rejected_if_unknown: GMEOW_LANG feeds the same
    // resolution and a bad value hard-fails.
    gmeow()
        .args(["describe", "Person"])
        .env("GMEOW_LANG", "notatag")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown language tag"));
}

#[test]
fn describe_resolves_grounding_curies() {
    // The headline: grounding-namespace terms resolve from the embedded bundle via
    // their registered CURIE, on the shipped binary.
    for (term, needle) in [
        ("lang:Denotation", "lang:Denotation"),
        ("math:Function", "math:Function"),
        ("logic:Formula", "logic:Formula"),
    ] {
        gmeow()
            .args(["describe", term])
            .assert()
            .success()
            .stdout(predicate::str::contains(needle));
    }
}

#[test]
fn describe_resolves_grounding_full_iris() {
    for iri in [
        "https://blackcatinformatics.ca/lang/Denotation",
        "https://blackcatinformatics.ca/math/Function",
    ] {
        gmeow()
            .args(["describe", iri])
            .assert()
            .success()
            .stdout(predicate::str::contains("category: Class"));
    }
}

#[test]
fn describe_json_format_is_valid_card_json() {
    let out = gmeow()
        .args(["describe", "lang:Denotation", "--format", "json"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let value: serde_json::Value =
        serde_json::from_slice(&out).expect("--format json must emit valid JSON");
    assert_eq!(
        value["iri"], "https://blackcatinformatics.ca/lang/Denotation",
        "the card JSON must carry the term IRI: {value}"
    );
    assert_eq!(value["category"], "Class");
    assert!(
        value["definition"].is_string(),
        "definition present: {value}"
    );
}

#[test]
fn describe_toon_format_emits_toon() {
    // TOON carries the same fields in the compact token-oriented form (the IRI value
    // contains `:` so it is quoted).
    gmeow()
        .args(["describe", "math:Function", "-f", "toon"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("iri: \"https://blackcatinformatics.ca/math/Function\"")
                .and(predicate::str::contains("category: Class")),
        );
}

#[test]
fn describe_help_documents_curie_prefixes() {
    gmeow()
        .args(["describe", "--help"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("logic:")
                .and(predicate::str::contains("math:"))
                .and(predicate::str::contains("lang:")),
        );
}

#[test]
fn describe_ambiguous_bare_name_emits_typed_code() {
    // A bare local name present in more than one namespace HARD-FAILS with the typed
    // `gmeow-cli.describe.ambiguous` code and the candidate CURIEs — no silent pick.
    // Driven through the shipped binary via the supported `--gts` flag on a real,
    // deterministic collision bundle.
    let dir = tempfile::TempDir::new().expect("temp dir");
    let path = dir.path().join("collision.gts");
    let mut nt = String::new();
    for prefix in ["math", "logic"] {
        let iri = format!("https://blackcatinformatics.ca/{prefix}/Widget");
        nt.push_str(&format!(
            "<{iri}> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Class> .\n"
        ));
        nt.push_str(&format!(
            "<{iri}> <http://www.w3.org/2000/01/rdf-schema#label> \"Widget\"@x-gmeow-english .\n"
        ));
        nt.push_str(&format!(
            "<{iri}> <http://www.w3.org/2000/01/rdf-schema#isDefinedBy> <https://blackcatinformatics.ca/gmeow/slices/{prefix}> .\n"
        ));
    }
    let ds = purrdf::parse_dataset(nt.as_bytes(), "application/n-triples", None)
        .expect("collision fixture parses");
    let bytes = purrdf::gts_write::to_gts(&ds, &purrdf::RdfLookaside::default(), "purrdf-test")
        .expect("collision fixture serializes");
    std::fs::write(&path, bytes).expect("write collision fixture");

    gmeow()
        .args(["describe", "--gts", path.to_str().unwrap(), "Widget"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("gmeow-cli.describe.ambiguous")
                .and(predicate::str::contains("logic:Widget"))
                .and(predicate::str::contains("math:Widget")),
        );
}

// ── validate (test_validate_rdf.py) ──────────────────────────────────────────

#[test]
fn validate_clean_file_passes() {
    // test_validate_rdf_clean_file_passes.
    gmeow()
        .arg("validate")
        .arg(fixture("clean.ttl"))
        .assert()
        .success()
        .stdout(predicate::str::contains("validation passed"));
}

#[test]
fn validate_fail_file_exits_one_human() {
    // test_validate_rdf_human_format_exits_nonzero: names the offending constraints
    // on stderr.
    gmeow()
        .arg("validate")
        .arg(fixture("fail.nq"))
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("identity axis")
                .or(predicate::str::contains("reference frame")),
        );
}

#[test]
fn validate_fail_file_json_is_well_formed() {
    // test_validate_rdf_reports_two_errors_one_warning_with_locations: JSON on
    // stdout carries the findings; exit 1.
    let output = gmeow()
        .arg("validate")
        .arg(fixture("fail.nq"))
        .args(["--format", "json"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 json");
    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON report");
    let findings = parsed["findings"].as_array().expect("findings array");
    // The closed-world validation-shape derivation adds disjointness shapes, so
    // fail.nq trips 4 errors + 1 warning: the original P9 disjointness
    // (IdentityAxisDisjointnessConstraintShape), the P17 sh:not pair
    // (Honorific-shape / PronounSet-shape), the under-mediated Commitment
    // (CommitmentShape), and the frame-relativity warning
    // (EventFrameRequirementShape). rdfs:domain/range are open-world by default
    // (inference axioms, no ClosedWorldClosure opt-in), so no domain/range shape
    // fires.
    assert_eq!(findings.len(), 5, "four errors + one warning: {findings:?}");
    let errors = findings.iter().filter(|f| f["severity"] == "error").count();
    let warnings = findings
        .iter()
        .filter(|f| f["severity"] == "warning")
        .count();
    assert_eq!(errors, 4, "{findings:?}");
    assert_eq!(warnings, 1, "{findings:?}");
}

#[test]
fn validate_fail_file_sarif_is_well_formed() {
    // test_validate_rdf_sarif_is_well_formed.
    let output = gmeow()
        .arg("validate")
        .arg(fixture("fail.nq"))
        .args(["--format", "sarif"])
        .assert()
        .code(1)
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(output).expect("utf-8 sarif");
    let sarif: serde_json::Value = serde_json::from_str(&text).expect("valid SARIF");
    let results = sarif["runs"][0]["results"]
        .as_array()
        .expect("sarif results array");
    assert_eq!(results.len(), 5, "four errors + one warning");
}

#[test]
fn validate_unknown_extension_hard_fails() {
    // test_validate_unknown_extension_hard_fails.
    let dir = scratch("badext");
    let bogus = dir.join("data.csv");
    std::fs::write(&bogus, "a,b,c\n").unwrap();
    gmeow()
        .arg("validate")
        .arg(&bogus)
        .assert()
        .failure()
        .stderr(predicate::str::contains("cannot infer format"));
}

// ── export / project / convert ───────────────────────────────────────────────

#[test]
fn export_respects_language_selector() {
    // test_export_respects_language_selector: --lang fr yields fr-keyed labels /
    // definitions in the JSONL term records. purrdf's CSVW package (dist/csvw/*, see
    // stages::export's module doc) is now a generic lossless RDF-1.2-in-CSV encoding
    // with no per-language columns, so the selector's effect is asserted against
    // gmeow-terms.jsonl (the flattened Term surface still carries a
    // language-tag-keyed `labels`/`definitions` map) instead of the retired
    // gmeow-classes.csv.
    let out = scratch("export");
    gmeow()
        .arg("export")
        .arg("--out")
        .arg(&out)
        .args(["--lang", "fr"])
        .assert()
        .success();
    let jsonl = out.join("gmeow-terms.jsonl");
    assert!(jsonl.exists(), "gmeow-terms.jsonl written");
    let text = std::fs::read_to_string(&jsonl).unwrap();
    assert!(
        text.contains("\"fr\":"),
        "french label/definition key present"
    );
    assert!(
        text.contains("labelFallback") || text.contains("definitionFallback"),
        "fallback flag present"
    );
}

#[test]
fn project_schema_org_view_filter() {
    // Mirrors `gmeow project --profile schema.org`: the schema.org VIEW filter over
    // the bundle (the registry name is `schema-org`). Writes a Turtle projection.
    let out = scratch("project");
    gmeow()
        .args(["project", "--profile", "schema-org"])
        .arg("--out")
        .arg(&out)
        .assert()
        .success()
        .stdout(predicate::str::contains("schema-org.ttl"));
    assert!(out.join("schema-org.ttl").exists());
}

#[test]
fn project_unknown_view_fails() {
    let out = scratch("project-bad");
    gmeow()
        .args(["project", "--profile", "definitely-not-a-view"])
        .arg("--out")
        .arg(&out)
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown view"));
}

#[test]
fn convert_turtle_to_ntriples() {
    // `gmeow convert --from turtle --to ntriples` round-trips a triple to stdout.
    let dir = scratch("convert");
    let src = dir.join("in.ttl");
    std::fs::write(
        &src,
        "@prefix ex: <http://example.org/> .\nex:a ex:p ex:b .\n",
    )
    .unwrap();
    gmeow()
        .arg("convert")
        .arg(&src)
        .args(["--from", "turtle", "--to", "ntriples"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "<http://example.org/a> <http://example.org/p> <http://example.org/b> .",
        ));
}

#[test]
fn convert_unknown_codec_fails() {
    let dir = scratch("convert-bad");
    let src = dir.join("in.ttl");
    std::fs::write(
        &src,
        "@prefix ex: <http://example.org/> .\nex:a ex:p ex:b .\n",
    )
    .unwrap();
    gmeow()
        .arg("convert")
        .arg(&src)
        .args(["--from", "turtle", "--to", "not-a-codec"])
        .assert()
        .failure();
}

// ── info / mcp / passthrough ─────────────────────────────────────────────────

#[test]
fn info_summarizes_the_bundle() {
    gmeow()
        .arg("info")
        .assert()
        .success()
        .stdout(predicate::str::contains("terms").and(predicate::str::contains("quads")));
}

#[test]
fn mcp_serves_a_native_jsonrpc_initialize() {
    // The consumer `gmeow mcp` runs the native stdio MCP server over the
    // embedded bundle: a JSON-RPC `initialize` request returns a well-formed
    // response identifying the server, and the transport closes cleanly on EOF.
    gmeow()
        .arg("mcp")
        .write_stdin("{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"initialize\",\"params\":{}}\n")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("\"jsonrpc\":\"2.0\"")
                .and(predicate::str::contains("protocolVersion"))
                .and(predicate::str::contains("serverInfo")),
        );
}

#[test]
fn gts_shim_hard_fails_when_binary_missing() {
    // test_gts_shim_fails_when_binary_missing: with no `gts` on PATH and no
    // GMEOW_GTS_BIN, the shim hard-fails with an install hint.
    let bin = assert_cmd::cargo::cargo_bin("gmeow");
    let output = StdCommand::new(bin)
        .args(["gts", "info"])
        .env_clear()
        .env("PATH", "/nonexistent-path-for-tests")
        .output()
        .expect("run gmeow gts info");
    assert!(!output.status.success(), "must hard-fail without gts");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("gts binary not found"),
        "install hint on stderr: {stderr}"
    );
}

// ── affect intensity (Q10 production surface) ────────────────────────────────

/// Absolute path of a committed slice file, relative to this crate.
fn slice_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices")
        .join(relative)
}

/// Build a fixture `.gts` at test time from the REAL committed affect slice
/// files: the canonical `module.ttl` (carries `gmeow:coreAffectGram` +
/// `gmeow:coreAxisIndex` AND the shipped schadenfreude worked A-Box) merged with
/// the intensity-discriminating worked example. This exercises the shipped instance
/// end to end, proving intensity is COMPUTED from the Gram matrix and appraisal
/// vectors — not read from a hand-authored magnitude.
fn affect_fixture_gts() -> PathBuf {
    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};
    use purrdf::{NativeRdfFormat, parse_dataset};

    let mut builder = SnapshotBuilder::default();
    for relative in [
        "core/affect/module.ttl",
        "core/affect/examples/intensity-discriminating.ttl",
    ] {
        let text = std::fs::read(slice_path(relative)).expect("read slice file");
        let dataset = parse_dataset(&text, NativeRdfFormat::Turtle.media_type(), None)
            .unwrap_or_else(|e| panic!("parse {relative}: {e}"));
        builder.add_dataset(&dataset).expect("add dataset");
    }
    let bytes = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .expect("emit gts");

    let dir = scratch("affect-fixture");
    let path = dir.join("affect.gts");
    std::fs::write(&path, bytes).expect("write fixture gts");
    path
}

/// Build a fixture `.gts` from the canonical `module.ttl` (carries the core-affect
/// axis indices) merged with the committed nearest-prototype worked example.
fn affect_nearest_fixture_gts() -> PathBuf {
    use purrdf::gts_compose::{DEFAULT_RSYNCABLE_THRESHOLD, SnapshotBuilder, emit_gts};
    use purrdf::{NativeRdfFormat, parse_dataset};

    let mut builder = SnapshotBuilder::default();
    for relative in [
        "core/affect/module.ttl",
        "core/affect/examples/nearest-prototype-metric.ttl",
    ] {
        let text = std::fs::read(slice_path(relative)).expect("read slice file");
        let dataset = parse_dataset(&text, NativeRdfFormat::Turtle.media_type(), None)
            .unwrap_or_else(|e| panic!("parse {relative}: {e}"));
        builder.add_dataset(&dataset).expect("add dataset");
    }
    let bytes = emit_gts(
        &builder,
        "dist",
        None,
        Vec::new(),
        Vec::new(),
        None,
        None,
        None,
        DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .expect("emit gts");

    let dir = scratch("affect-nearest-fixture");
    let path = dir.join("affect-nearest.gts");
    std::fs::write(&path, bytes).expect("write fixture gts");
    path
}

#[test]
fn affect_nearest_selects_metric_nearest_prototype() {
    // Under the valence-dominant metric diag(2, 1) the state (0.5, 0.0) is classified
    // to ELATION (exact squared 19/50) — NOT the raw-L²-nearest contentment (0.34).
    // Selection is by exact Rational squared distance; the CLI prints the winner, the
    // exact squared distance, and the display √ decimal.
    let fixture = affect_nearest_fixture_gts();
    let ns = "https://blackcatinformatics.ca/gmeow/examples/affect/nearest/";
    gmeow()
        .args(["affect", "nearest"])
        .arg(&fixture)
        .args(["--observation", &format!("{ns}stateObservation")])
        .args(["--prototype", &format!("{ns}contentmentPrototype")])
        .args(["--prototype", &format!("{ns}elationPrototype")])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(format!("nearest {ns}elationPrototype"))
                .and(predicate::str::contains("squared-distance 19/50")),
        );
}

#[test]
fn affect_nearest_missing_source_is_a_runtime_error() {
    gmeow()
        .args([
            "affect",
            "nearest",
            "/nonexistent-affect-fixture.gts",
            "--observation",
            "urn:x",
            "--prototype",
            "urn:p",
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}

#[test]
fn affect_nearest_without_prototype_is_a_clap_usage_error() {
    // `--prototype` is `required = true`, so omitting it fails with a usage error
    // (exit 2) before the source is read.
    gmeow()
        .args(["affect", "nearest", "/dev/null", "--observation", "urn:x"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--prototype").and(predicate::str::contains("Usage:")));
}

#[test]
fn affect_intensity_schadenfreude_is_computed_from_the_metric() {
    // Q10: the CLI computes √(xᵀGx) over the canonical coreAffectGram
    // (diagonal 1, valence–arousal coupling 1/4) for the schadenfreude vector
    // (valence 0.7, arousal 0.4): Q = 79/100, intensity 0.888819, dominant
    // valence. The metric-tensor norm is the load-bearing computation.
    let fixture = affect_fixture_gts();
    gmeow()
        .args(["affect", "intensity"])
        .arg(&fixture)
        .args([
            "--observation",
            "https://blackcatinformatics.ca/gmeow/schadenfreudeIntensity",
        ])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("intensity 0.888819").and(predicate::str::contains(
                "dominant-axis https://blackcatinformatics.ca/gmeow/dimensionValence",
            )),
        );
}

#[test]
fn affect_intensity_discriminating_dominant_axis_is_metric_aware() {
    // The discriminating case: raw-max axis is arousal (0.6 > 0.5), but the
    // computed G-weighted dominant is valence (diag(2,1): 2·0.5² = 0.5 >
    // 1·0.6² = 0.36) — the compute is load-bearing, not a raw-max read.
    let fixture = affect_fixture_gts();
    gmeow()
        .args(["affect", "intensity"])
        .arg(&fixture)
        .args([
            "--observation",
            "https://blackcatinformatics.ca/gmeow/examples/affect/tests/vdIntensity",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "dominant-axis https://blackcatinformatics.ca/gmeow/dimensionValence",
        ));
}

// ── affect goemotions ingestion (the production surface) ─────────────────────

/// Absolute path of a committed crate fixture, relative to this crate.
fn crate_fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(relative)
}

#[test]
fn affect_goemotions_ingest_recover_round_trips_through_the_cli() {
    let capture = crate_fixture("affect-ingest/fixtures/goemotions-sample.json");

    // Put leg: the captured GoEmotions JSON becomes attributed evidence Turtle,
    // resolving the label set + reviewed closeMatch from the EMBEDDED bundle. The
    // adapter is dispatched from the capture's declared label set — one generic
    // `ingest` subcommand, no per-model variant.
    let put = gmeow()
        .args(["affect", "ingest"])
        .arg(&capture)
        .assert()
        .success();
    let ttl = String::from_utf8(put.get_output().stdout.clone()).expect("utf-8 turtle");
    assert!(ttl.contains("ModelInferenceRun"), "run emitted");
    // lossless: an output for every one of the 28 GoEmotions labels, on every
    // captured target (the real fixture carries three).
    let outputs = ttl.matches("AffectClassifierOutput").count();
    assert!(
        outputs >= 28 && outputs.is_multiple_of(28),
        "28 labels per target: {outputs}"
    );
    // claim routing via the bundle's SSSOM correspondence: the joyful target's
    // `joy` crosses threshold AND closeMatches gmeow:emotionJoy.
    assert!(ttl.contains("the text expresses joy"));

    // Get leg: pipe the evidence back in; the label set is auto-detected from the
    // emitted labels and the reconstructed capture round-trips through the real CLI
    // (pinned model revision + real model identity).
    gmeow()
        .args(["affect", "recover", "-"])
        .write_stdin(ttl)
        .assert()
        .success()
        .stdout(
            predicate::str::contains("d75048347613a25d77de8cf6412eaae9fa7b26be")
                .and(predicate::str::contains("SamLowe/roberta-base-go_emotions")),
        );
}

#[test]
fn affect_goemotions_ingest_missing_source_is_a_runtime_error() {
    gmeow()
        .args(["affect", "ingest", "/nonexistent-capture.json"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}

#[test]
fn affect_intensity_missing_source_is_a_runtime_error() {
    // Mirror the music runtime-error path: an unreadable source → exit 1 with an
    // `Error:` prefix on stderr.
    gmeow()
        .args(["affect", "intensity", "/nonexistent-affect-fixture.gts"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}

#[test]
fn affect_intensity_to_without_observation_is_a_clap_usage_error() {
    // `--to` declares `requires = "observation"` at the clap layer, so omitting
    // `--observation` fails fast with a usage error (exit 2) before the source is
    // ever read — no runtime `Error:` fallback.
    gmeow()
        .args(["affect", "intensity", "/dev/null", "--to", "urn:x"])
        .assert()
        .failure()
        .code(2)
        .stderr(predicate::str::contains("--observation").and(predicate::str::contains("Usage:")));
}

#[test]
fn music_render_missing_source_is_a_runtime_error() {
    // The music passthrough maps a runtime failure (unreadable source) to exit 1
    // with an `Error:` prefix; the unsupported-format → exit 2 mapping is pinned by
    // the gmeow-music crate's own tests (it needs a valid piece to reach the format
    // check).
    let dir = scratch("music");
    let missing = dir.join("nope.gts");
    gmeow()
        .args(["music", "render"])
        .arg(&missing)
        .args(["--to", "midi"])
        .arg("--out")
        .arg(dir.join("out.mid"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("Error:"));
}

// ── conjecture test (Task 5b production surface) ─────────────────────────────

const LOGIC_NS_TEST: &str = "https://blackcatinformatics.ca/logic/";

/// A universally-quantified Horn candidate whose head fires `rdf:type(x, ex:B)` —
/// the same shape the pipeline `conjecture_test` fixtures use.
fn forall_horn_candidate_ttl() -> String {
    format!(
        "@prefix logic: <{LOGIC_NS_TEST}> .\n\
         @prefix ex:  <http://ex/> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         ex:cand a logic:Formula ;\n\
             logic:forall ex:body ;\n\
             logic:quantifiedVariable [ logic:termIndex 0 ; logic:termVariable \"x\" ] .\n\
         ex:body a logic:Formula ;\n\
             logic:antecedent ex:ant ;\n\
             logic:consequent ex:con .\n\
         ex:ant a logic:Formula ;\n\
             logic:relation ex:trigger ;\n\
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
             logic:argument [ logic:termIndex 1 ; logic:termIri ex:mark ] .\n\
         ex:con a logic:Formula ;\n\
             logic:relation rdf:type ;\n\
             logic:argument [ logic:termIndex 0 ; logic:termVariable \"x\" ] ;\n\
             logic:argument [ logic:termIndex 1 ; logic:termIri ex:B ] .\n"
    )
}

/// A KB where the head class `ex:B` is DISJOINT with `ex:a`'s asserted type, so
/// firing the candidate forces an `owl:Nothing` clash ⇒ refutation + witness.
fn refuting_kb_ttl() -> String {
    "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
     ex:a ex:trigger ex:mark .\n\
     ex:a rdf:type ex:A .\n\
     ex:A owl:disjointWith ex:B .\n"
        .to_owned()
}

/// A refuting KB persists a `refuted-in-standpoint` verdict + witness and GROWS
/// the append-only library; the standard (non-dry) run commits.
#[test]
fn conjecture_test_refutes_persists_and_grows_the_library() {
    let dir = scratch("conjecture-commit");
    let formula = dir.join("candidate.ttl");
    let kb = dir.join("kb.ttl");
    let lib = dir.join("conjectures.gts");
    std::fs::write(&formula, forall_horn_candidate_ttl()).expect("write formula");
    std::fs::write(&kb, refuting_kb_ttl()).expect("write kb");

    assert!(
        !lib.exists(),
        "library must not exist before the first persist"
    );
    gmeow()
        .env("GMEOW_CONJECTURE_PATH", &lib)
        .args(["conjecture", "test"])
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args(["--standpoint", "http://ex/standpoint/alice"])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("lifecycle refuted-in-standpoint")
                .and(predicate::str::contains("witness-individual http://ex/a"))
                .and(predicate::str::contains("witness-premise"))
                .and(predicate::str::contains("conjecture "))
                .and(predicate::str::contains("persisted committed")),
        );
    // The append-only library was written and is non-empty.
    let grown = std::fs::metadata(&lib).map(|m| m.len()).unwrap_or(0);
    assert!(grown > 0, "the committed run must have grown the library");
    std::fs::remove_dir_all(&dir).ok();
}

/// A `--dry-run` computes the SAME verdict but WRITES NOTHING to the library.
#[test]
fn conjecture_test_dry_run_writes_nothing() {
    let dir = scratch("conjecture-dry");
    let formula = dir.join("candidate.ttl");
    let kb = dir.join("kb.ttl");
    let lib = dir.join("conjectures.gts");
    std::fs::write(&formula, forall_horn_candidate_ttl()).expect("write formula");
    std::fs::write(&kb, refuting_kb_ttl()).expect("write kb");

    gmeow()
        .env("GMEOW_CONJECTURE_PATH", &lib)
        .args(["conjecture", "test"])
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args(["--standpoint", "http://ex/standpoint/alice"])
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("lifecycle refuted-in-standpoint").and(
                predicate::str::contains("persisted dry-run (nothing written)"),
            ),
        );
    // Nothing written: the library does not exist / is zero bytes.
    let written = lib.exists() && std::fs::metadata(&lib).map(|m| m.len()).unwrap_or(0) > 0;
    assert!(!written, "a dry run must write nothing to the library");
    std::fs::remove_dir_all(&dir).ok();
}

/// A KB whose `ex:trigger` fires the candidate on SEVERAL individuals, so the
/// candidate's derived (non-EDB) closure is strictly larger than a bound of 1.
fn multi_trigger_kb_ttl() -> String {
    "@prefix ex:  <http://ex/> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     ex:a ex:trigger ex:mark .\n\
     ex:b ex:trigger ex:mark .\n\
     ex:c ex:trigger ex:mark .\n\
     ex:a rdf:type ex:A .\n"
        .to_owned()
}

/// GAP G1: `--max-steps` is reachable from the SHIPPED CLI. A bound of 1 truncates
/// the multi-fact derived closure ⇒ evaluation budget-exhausted → lifecycle open →
/// discharge ObligationUnknown, and (being inconclusive) nothing is persisted-open
/// beyond the Open verdict.
#[test]
fn conjecture_test_max_steps_bound_forces_open() {
    let dir = scratch("conjecture-budget");
    let formula = dir.join("candidate.ttl");
    let kb = dir.join("kb.ttl");
    let lib = dir.join("conjectures.gts");
    std::fs::write(&formula, forall_horn_candidate_ttl()).expect("write formula");
    std::fs::write(&kb, multi_trigger_kb_ttl()).expect("write kb");

    gmeow()
        .env("GMEOW_CONJECTURE_PATH", &lib)
        .args(["conjecture", "test"])
        .arg("--formula")
        .arg(&formula)
        .arg("--kb")
        .arg(&kb)
        .args(["--standpoint", "http://ex/standpoint/alice"])
        .args(["--max-steps", "1"])
        .arg("--dry-run")
        .assert()
        .success()
        .stdout(
            predicate::str::contains("lifecycle open")
                .and(predicate::str::contains("evaluation budget-exhausted"))
                .and(predicate::str::contains("discharge ObligationUnknown")),
        );
    std::fs::remove_dir_all(&dir).ok();
}

// ── hybrid-query (Gap E2/E3: external-relation provider on the shipped CLI) ──

/// Ordinary asserted RDF facts: two documents, one active and one inactive.
/// The hard RDF constraint `ex:status(D, ex:active)` filters the provider's
/// candidate join down to exactly `doc/one`.
fn hybrid_query_facts_ttl() -> String {
    "<https://example.org/doc/one> <https://example.org/status> <https://example.org/active> .\n\
     <https://example.org/doc/two> <https://example.org/status> <https://example.org/inactive> .\n"
        .to_owned()
}

/// A provider/RDF join program: `eligible(Q, D)` holds when the lexical
/// provider relation returns `D` for `Q` AND `D` is asserted `ex:active` —
/// the same join shape as `crates/logic/tests/external_relations.rs`'s
/// `bound_lexical_candidates_join_hard_rdf_constraints_in_one_fixpoint`.
fn hybrid_query_program() -> String {
    ":- prefix(ex, 'https://example.org/').\n\
     ex:eligible(Q, D) :- ex:relation/lexical(Q, D), ex:status(D, ex:active).\n\
     ?- ex:eligible(ex:cat, D).\n"
        .to_owned()
}

/// Three lexical candidate tuples: two for `cat` (one active, one inactive
/// document) and one for `dog` (irrelevant to the goal). Comments and blank
/// lines are deliberately included to exercise the candidate-file grammar.
fn hybrid_query_candidates_txt() -> String {
    "# query        document                        annotation order-key\n\
     \n\
     <https://example.org/cat> <https://example.org/doc/one> 7 001\n\
     <https://example.org/cat> <https://example.org/doc/two> 5 002\n\
     <https://example.org/dog> <https://example.org/doc/three> 3 003\n"
        .to_owned()
}

const HYBRID_QUERY_RELATION: &str = "https://example.org/relation/lexical";
const HYBRID_QUERY_PROVIDER_IRI: &str = "https://example.org/hybrid-query-test/provider";

/// The headline E3 acceptance test: `gmeow hybrid-query` registers a real
/// `TableRelationProvider` and drives the query end-to-end on the SHIPPED
/// binary, printing both the resolved answer binding (the RDF-constrained
/// join keeps only the active document) and the query receipt naming the
/// contributing provider — the observable proof this capability is reachable
/// outside `crates/logic`'s own test binary.
#[test]
fn hybrid_query_prints_answer_binding_and_provider_lineage_receipt() {
    let dir = scratch("hybrid-query");
    let facts = dir.join("facts.ttl");
    let program = dir.join("query.logic");
    let candidates = dir.join("candidates.txt");
    std::fs::write(&facts, hybrid_query_facts_ttl()).expect("write facts");
    std::fs::write(&program, hybrid_query_program()).expect("write program");
    std::fs::write(&candidates, hybrid_query_candidates_txt()).expect("write candidates");

    gmeow()
        .arg("hybrid-query")
        .arg("--facts")
        .arg(&facts)
        .arg("--program")
        .arg(&program)
        .arg("--candidates")
        .arg(&candidates)
        .args(["--relation", HYBRID_QUERY_RELATION])
        .args(["--provider-iri", HYBRID_QUERY_PROVIDER_IRI])
        .assert()
        .success()
        .stdout(
            // The RDF join admits ONLY the active document, with the provider's
            // ZWeight annotation (7) composed against the asserted-fact identity
            // (the CLI supplies no asserted-RDF scoring function).
            predicate::str::contains("answer D=<https://example.org/doc/one> annotation=7")
                // The excluded, inactive candidate must never appear as an answer.
                .and(predicate::str::contains("doc/two").not())
                .and(predicate::str::contains("status Ok"))
                // The receipt must name the contributing provider by IRI — the
                // acceptance criterion that provider lineage is observable.
                .and(predicate::str::contains(format!(
                    "receipt contributing-provider {HYBRID_QUERY_PROVIDER_IRI}"
                )))
                .and(predicate::str::contains(format!(
                    "relation={HYBRID_QUERY_RELATION}"
                )))
                .and(predicate::str::contains(format!(
                    "provider={HYBRID_QUERY_PROVIDER_IRI}"
                )))
                .and(predicate::str::contains("status=Complete"))
                .and(predicate::str::contains("contributed=true")),
        );
    std::fs::remove_dir_all(&dir).ok();
}

/// A malformed `--candidates` line (only 3 fields, missing the order-key) is a
/// specific, honest parse failure — never a silently empty/degraded provider.
#[test]
fn hybrid_query_malformed_candidates_line_fails_with_a_specific_diagnostic() {
    let dir = scratch("hybrid-query-bad-candidates");
    let facts = dir.join("facts.ttl");
    let program = dir.join("query.logic");
    let candidates = dir.join("candidates.txt");
    std::fs::write(&facts, hybrid_query_facts_ttl()).expect("write facts");
    std::fs::write(&program, hybrid_query_program()).expect("write program");
    std::fs::write(
        &candidates,
        "<https://example.org/cat> <https://example.org/doc/one> 7\n",
    )
    .expect("write malformed candidates");

    gmeow()
        .arg("hybrid-query")
        .arg("--facts")
        .arg(&facts)
        .arg("--program")
        .arg(&program)
        .arg("--candidates")
        .arg(&candidates)
        .args(["--relation", HYBRID_QUERY_RELATION])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("line 1").and(predicate::str::contains(
                "expected 4 whitespace-separated fields",
            )),
        );
    std::fs::remove_dir_all(&dir).ok();
}

/// A provider that is registered but never referenced by the program is
/// legitimate (mirrors the `unused_vector` fixture in
/// `crates/logic/tests/external_relations.rs`): the query still completes
/// normally, simply without touching the unused provider — a syntactically
/// valid, disjoint program is NOT an error path, and every asserted-RDF-only
/// answer carries the multiplicative identity annotation (`1`).
#[test]
fn hybrid_query_relation_not_referenced_by_program_still_succeeds() {
    let dir = scratch("hybrid-query-unused-provider");
    let facts = dir.join("facts.ttl");
    let program = dir.join("query.logic");
    let candidates = dir.join("candidates.txt");
    std::fs::write(&facts, hybrid_query_facts_ttl()).expect("write facts");
    std::fs::write(
        &program,
        ":- prefix(ex, 'https://example.org/').\n\
         ?- ex:status(S, D).\n",
    )
    .expect("write program");
    std::fs::write(&candidates, hybrid_query_candidates_txt()).expect("write candidates");

    gmeow()
        .arg("hybrid-query")
        .arg("--facts")
        .arg(&facts)
        .arg("--program")
        .arg(&program)
        .arg("--candidates")
        .arg(&candidates)
        .args(["--relation", HYBRID_QUERY_RELATION])
        .assert()
        .success()
        .stdout(
            predicate::str::contains(
                "answer D=<https://example.org/active>, S=<https://example.org/doc/one> \
                 annotation=1",
            )
            .and(predicate::str::contains(
                "answer D=<https://example.org/inactive>, S=<https://example.org/doc/two> \
                 annotation=1",
            ))
            .and(predicate::str::contains("status Ok")),
        );
    std::fs::remove_dir_all(&dir).ok();
}

// ── logic backward (Task 8: interactive backward-engine CLI surface) ────────

/// The repo-committed goal-directed demonstrator corpus — the SAME
/// `logic:ReasoningProgram` cell `stage-goal-directed` compiles into
/// `gmeow.gts`'s `graph/goal-directed`.
fn reasoning_programs_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../slices/grounding/logic/examples/reasoning-programs.ttl")
}

/// The repo-committed `math:` grounding module, whose told `rdfs:subClassOf`
/// chain (`math:Integer ⊑ math:RationalNumber ⊑ math:RealNumber ⊑ …`) seeds
/// the order-sorted `ex:mathSubsort` demonstrator's unification lattice.
fn math_module_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../slices/grounding/math/module.ttl")
}

/// The example corpus's namespace (`@prefix ex:` in `reasoning-programs.ttl`).
const LOGIC_BACKWARD_EX: &str = "https://blackcatinformatics.ca/gmeow/examples/logic/";

/// `gmeow logic backward` over the full shipped demonstrator corpus (no
/// `--program-iri`, no `--subsort-source`) drives the SAME
/// `evaluate_reasoning_programs` production path `stage-goal-directed` folds
/// into `gmeow.gts`, and prints the Peano-addition proof-checked answer, the
/// reachability answers, and the three-valued WFS verdicts. The order-sorted
/// `mathSubsort`/`mathSubsortControl` pair correctly yields zero answers here —
/// no subsort edges are supplied, which is an honest gap, never a silent
/// fallback to a hardcoded math tower.
#[test]
fn logic_backward_evaluates_the_shipped_demonstrator_corpus() {
    const EX: &str = LOGIC_BACKWARD_EX;
    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg(reasoning_programs_fixture())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("program peanoAdd")
                .and(predicate::str::contains(format!(
                    "answer atom={EX}add({EX}s({EX}s({EX}zero)),{EX}s({EX}zero),\
                     {EX}s({EX}s({EX}s({EX}zero))))"
                )))
                .and(predicate::str::contains(format!(
                    "binding R = {EX}s({EX}s({EX}s({EX}zero)))"
                )))
                .and(predicate::str::contains("proof-checked=true"))
                .and(predicate::str::contains("program reachability"))
                .and(predicate::str::contains(format!(
                    "answer atom={EX}reach({EX}a,{EX}b)"
                )))
                .and(predicate::str::contains(format!(
                    "answer atom={EX}reach({EX}a,{EX}c)"
                )))
                .and(predicate::str::contains("program memberCons"))
                .and(predicate::str::contains(format!("binding M = {EX}a")))
                .and(predicate::str::contains(format!("binding M = {EX}b")))
                .and(predicate::str::contains(format!("binding M = {EX}c")))
                .and(predicate::str::contains("program winWfs"))
                .and(predicate::str::contains(format!(
                    "verdict atom={EX}win({EX}a) verdict=undefined"
                )))
                .and(predicate::str::contains(format!(
                    "verdict atom={EX}win({EX}c) verdict=true"
                )))
                .and(predicate::str::contains(format!(
                    "verdict atom={EX}win({EX}d) verdict=false"
                )))
                .and(predicate::str::contains("program mathSubsort"))
                .and(predicate::str::contains("program mathSubsortControl")),
        );

    // Narrowed to just `mathSubsort`, with no `--subsort-source` supplied,
    // the order-sorted lattice is empty and the demonstrator honestly
    // produces zero answers — proving the positive case exercised elsewhere
    // (`logic_backward_subsort_source_seeds_the_order_sorted_lattice`) is
    // reasoned-closure-driven from the told `math:` subsort chain, never a
    // hardcoded math tower baked into the engine.
    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg(reasoning_programs_fixture())
        .args(["--program-iri", &format!("{EX}mathSubsort")])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("program mathSubsort")
                .and(predicate::str::contains("answer atom=").not()),
        );
}

/// `--program-iri` narrows evaluation to exactly the named program: the output
/// carries only `peanoAdd` and neither `reachability` nor `winWfs` appears.
#[test]
fn logic_backward_program_iri_narrows_to_one_program() {
    const EX: &str = LOGIC_BACKWARD_EX;
    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg(reasoning_programs_fixture())
        .args(["--program-iri", &format!("{EX}peanoAdd")])
        .assert()
        .success()
        .stdout(
            predicate::str::contains("program peanoAdd")
                .and(predicate::str::contains("program reachability").not())
                .and(predicate::str::contains("program winWfs").not()),
        );
}

/// `--subsort-source` seeds the order-sorted unification lattice from the
/// `math:` module's TOLD `rdfs:subClassOf` chain: the engine composes its own
/// reflexive-transitive closure, so `ex:mathSubsort` (whose query variable
/// carries `logic:variableSort math:RealNumber`) accepts the `math:Integer`
/// constant `ex:one` (ℤ ⊑ ℝ), while the negative control `ex:mathSubsortControl`
/// (an incomparable `math:Set` sort) still correctly refuses it.
#[test]
fn logic_backward_subsort_source_seeds_the_order_sorted_lattice() {
    const EX: &str = LOGIC_BACKWARD_EX;
    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg(reasoning_programs_fixture())
        .args(["--program-iri", &format!("{EX}mathSubsort")])
        .arg("--subsort-source")
        .arg(math_module_fixture())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("program mathSubsort")
                .and(predicate::str::contains(format!(
                    "answer atom={EX}p({EX}one)"
                )))
                .and(predicate::str::contains(format!("binding X = {EX}one"))),
        );

    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg(reasoning_programs_fixture())
        .args(["--program-iri", &format!("{EX}mathSubsortControl")])
        .arg("--subsort-source")
        .arg(math_module_fixture())
        .assert()
        .success()
        .stdout(
            predicate::str::contains("program mathSubsortControl")
                .and(predicate::str::contains("answer atom=").not()),
        );
}

/// A missing `--program-file` is a hard fail (exit 1), never a silent empty
/// success.
#[test]
fn logic_backward_missing_program_file_hard_fails() {
    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg("/nonexistent-reasoning-programs.ttl")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

/// An unknown `--program-iri` is a hard fail listing the known program IRIs —
/// never a silently empty result set.
#[test]
fn logic_backward_unknown_program_iri_hard_fails() {
    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg(reasoning_programs_fixture())
        .args(["--program-iri", "https://example.org/nope"])
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("names no logic:ReasoningProgram")
                .and(predicate::str::contains("known:")),
        );
}

/// A cell carrying zero `logic:ReasoningProgram` individuals is a hard fail,
/// never a silent empty success.
#[test]
fn logic_backward_program_free_cell_hard_fails() {
    let dir = scratch("logic-backward-empty");
    let empty = dir.join("empty.ttl");
    std::fs::write(&empty, "@prefix ex: <http://ex/> .\nex:a ex:knows ex:b .\n")
        .expect("write program-free cell");

    gmeow()
        .args(["logic", "backward"])
        .arg("--program-file")
        .arg(&empty)
        .assert()
        .failure()
        .stderr(predicate::str::contains("zero logic:ReasoningProgram"));
    std::fs::remove_dir_all(&dir).ok();
}
