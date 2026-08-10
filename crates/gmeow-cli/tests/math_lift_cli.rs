// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Acceptance tests for the `gmeow math lift-*` ingestion surface.
//!
//! These are the END-TO-END evidence that the three `math:` ingestion bridges are
//! reachable as a shipped capability rather than a library the tests alone call: every
//! case here drives the BUILT `gmeow` binary through `assert_cmd`, over the REAL committed
//! artifacts in `crates/math-lift/fixtures/` — an actual R statistics script, an actual
//! binary `.onnx` protobuf export, and an actual TSTP derivation produced by our own
//! reasoner.
//!
//! What each case pins:
//!
//! | case | claim |
//! |---|---|
//! | the three success cases | a real artifact of each kind lifts, exit `0`, and the Turtle carries that bridge's run class plus genuinely bridge-specific codomain classes |
//! | the two failure cases | a malformed and an unliftable artifact BOTH hard-fail carrying the ENGINE's own `math.lift.*` code on stderr and no product on stdout |
//! | the fingerprint case | two different engine kinds mint two different `finding_iri`s, so `gmeow explain` and the diagnostics substrate can tell them apart |
//! | `--out` | the file receives byte-for-byte what stdout emits — one product, two sinks |
//! | stdin (`-`) | the source is a stream, not a path, so the bridges compose in a pipeline |
//! | idempotence | a re-lift of the same bytes is byte-identical, the property the fixed mint base exists to guarantee |
//! | `--help` | the group and all three leaves are discoverable and exit `0` |

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use predicates::prelude::*;

/// The built `gmeow` binary.
fn gmeow() -> Command {
    Command::cargo_bin("gmeow").expect("gmeow binary builds")
}

/// Absolute path of a committed `math-lift` fixture — a REAL artifact of its format,
/// not a synthesized stub.
fn lift_fixture(name: &str) -> PathBuf {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../math-lift/fixtures")
        .join(name);
    path.canonicalize()
        .unwrap_or_else(|e| panic!("canonicalize math-lift fixture {}: {e}", path.display()))
}

/// A fresh, empty scratch directory owned by the returned [`tempfile::TempDir`].
///
/// The guard must be bound to a live local (`let (_tmp, dir) = scratch("tag");`)
/// for the duration of the test: dropping it removes the directory and its
/// contents, on success, on early return, and on panic alike.
fn scratch(tag: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::Builder::new()
        .prefix(&format!("gmeow-math-lift-{tag}-"))
        .tempdir()
        .expect("create scratch dir");
    let dir = tmp.path().to_path_buf();
    (tmp, dir)
}

/// Run one lift leaf over a committed fixture, assert exit `0`, and return the Turtle
/// the binary wrote to stdout.
fn lift_ok(leaf: &str, fixture: &str) -> String {
    let assertion = gmeow()
        .args(["math", leaf])
        .arg(lift_fixture(fixture))
        .assert()
        .success();
    String::from_utf8(assertion.get_output().stdout.clone()).expect("the lift emits UTF-8 Turtle")
}

/// Assert `ttl` mentions every one of `required` — used to pin the codomain classes a
/// bridge is claiming to have carried across, not merely that "some triples" appeared.
fn assert_mentions(ttl: &str, required: &[&str]) {
    for term in required {
        assert!(
            ttl.contains(term),
            "the lifted graph is missing `{term}`; it carried:\n{ttl}"
        );
    }
}

/// The `math:` term IRI for `local`.
fn math(local: &str) -> String {
    format!("https://blackcatinformatics.ca/math/{local}")
}

// ── the three bridges, over real artifacts ───────────────────────────────────

#[test]
fn lift_r_carries_a_real_statistics_script_into_the_math_codomain() {
    let ttl = lift_ok("lift-r", "mtcars.R");

    // The run itself, with the four mandatory frame edges and its law spine.
    assert_mentions(
        &ttl,
        &[
            &math("RIngestRun"),
            &math("parseSource"),
            &math("ingestCorrespondence"),
            "https://blackcatinformatics.ca/logic/instantiatesSchema",
            "https://blackcatinformatics.ca/logic/instantiatesPlan",
            "https://blackcatinformatics.ca/logic/Correspondence",
        ],
    );
    // The R rung is a LOSSY lens over a VAGUE source — never silently upgraded.
    assert_mentions(
        &ttl,
        &[
            "https://blackcatinformatics.ca/logic/LossyLens",
            "https://blackcatinformatics.ca/logic/Vague",
        ],
    );
    // The statistical codomain the R bridge exists to produce: the model and its
    // formula (both of the `math:FittedModel` OWL restrictions discharged), the
    // dataset held by reference, and the estimate/residual layer.
    assert_mentions(
        &ttl,
        &[
            &math("FittedModel"),
            &math("modelFormula"),
            &math("fittedToData"),
            &math("ModelFormula"),
            &math("ArgumentSlot"),
            &math("DatasetMatrix"),
            &math("Distribution"),
            &math("Estimate"),
            &math("Residual"),
        ],
    );
    // Non-statistical constructs route to `logic:`, and the lowering declares its
    // denotation kind and preservation in the same breath (the trap
    // `math:UndeclaredLogicLowering` guards).
    assert_mentions(
        &ttl,
        &[
            &math("compilesToLogicFormula"),
            &math("denotationKind"),
            &math("logicLoweringPreservation"),
        ],
    );
    // Every codomain node is enumerable by query, via the back-edge the native
    // `math:UnliftableIngest` lint looks for.
    assert!(ttl.contains("https://blackcatinformatics.ca/gmeow/wasGeneratedBy"));
}

#[test]
fn lift_onnx_carries_a_real_binary_export_into_the_math_codomain() {
    // `.onnx` is BINARY protobuf: this case is also the proof that the source is read
    // as bytes and never through a UTF-8-validating path.
    let ttl = lift_ok("lift-onnx", "mlp.onnx");

    assert_mentions(
        &ttl,
        &[
            &math("ONNXIngestRun"),
            &math("parseSource"),
            &math("ingestCorrespondence"),
        ],
    );
    // A lossy lens over a CRISP source: operator types and shapes are exact, weight
    // payloads are held by reference and do not cross.
    assert_mentions(
        &ttl,
        &[
            "https://blackcatinformatics.ca/logic/LossyLens",
            "https://blackcatinformatics.ca/logic/Crisp",
        ],
    );
    // The tensor codomain: the graph, its computation nodes, the layers, the weight
    // tensors held by reference, and the parameter space.
    assert_mentions(
        &ttl,
        &[
            &math("TensorComputationGraph"),
            &math("computationNode"),
            &math("NeuralLayer"),
            &math("WeightTensor"),
            &math("weightOf"),
            &math("ParameterSpace"),
            &math("LearnedModel"),
            &math("MathematicalSymbol"),
        ],
    );
    // The one operator this fixture's graph grounds in a `math:` individual.
    assert!(ttl.contains(&math("matrixProduct")));
}

#[test]
fn lift_proof_carries_a_real_tstp_derivation_into_the_math_proof_layer() {
    let ttl = lift_ok("lift-proof", "theorem-subclass.tstp");

    assert_mentions(
        &ttl,
        &[
            &math("ProofIngestRun"),
            &math("parseSource"),
            &math("ingestCorrespondence"),
        ],
    );
    // The ONLY bridge claiming a section/retraction at exact preservation — the
    // derivation reconstructs from the lift plus its retained witness.
    assert_mentions(
        &ttl,
        &[
            "https://blackcatinformatics.ca/logic/SectionRetraction",
            "https://blackcatinformatics.ca/logic/ExactPreservation",
            "https://blackcatinformatics.ca/logic/Equiv",
        ],
    );
    // The proof codomain: the DAG, its steps and axiom leaves, the parent/premise
    // edges, and the verification triangle.
    assert_mentions(
        &ttl,
        &[
            &math("ProofDependencyGraph"),
            &math("Proof"),
            &math("ProofStep"),
            &math("proofStep"),
            &math("Axiom"),
            &math("dependsOnAxiom"),
            &math("hasPremise"),
            &math("hasConclusion"),
            &math("FormalVerificationResult"),
            &math("verifiedByEngine"),
        ],
    );
}

// ── hard failures: malformed vs unliftable ───────────────────────────────────

#[test]
fn an_unliftable_r_script_hard_fails_and_emits_no_product() {
    // `unliftable.R` PARSES — it is well-formed R. It simply carries no statistical
    // content, so there is nothing the `math:` codomain can honestly claim to hold.
    // The bridge refuses rather than emitting a string-valued placeholder.
    let assertion = gmeow()
        .args(["math", "lift-r"])
        .arg(lift_fixture("unliftable.R"))
        .assert()
        .failure()
        // The ENGINE's own code, not a CLI re-coding of it: `math.lift.r.unliftable` names
        // exactly which of the eight kinds refused.
        .stderr(
            predicate::str::contains("math.lift.r.unliftable")
                .and(predicate::str::contains("no statistical content")),
        );
    assert!(
        assertion.get_output().stdout.is_empty(),
        "a refused lift must emit NO triples at all, not a partial graph"
    );
}

#[test]
fn a_truncated_onnx_stream_hard_fails_and_emits_no_product() {
    // `truncated.onnx` is the other class: the artifact is not a well-formed instance
    // of its own format, caught by the wire decoder at a byte offset.
    let assertion = gmeow()
        .args(["math", "lift-onnx"])
        .arg(lift_fixture("truncated.onnx"))
        .assert()
        .failure()
        .stderr(
            predicate::str::contains("math.lift.onnx.wire")
                .and(predicate::str::contains("byte offset")),
        );
    assert!(
        assertion.get_output().stdout.is_empty(),
        "a refused lift must emit NO triples at all, not a partial graph"
    );
}

/// Drive one failing lift with `--console jsonl` and return its single parsed `finding`
/// object — the machine channel an agent sweeping a corpus actually consumes.
fn failing_lift_finding(leaf: &str, source: &Path) -> serde_json::Value {
    let assertion = gmeow()
        .args(["--console", "jsonl", "math", leaf])
        .arg(source)
        .assert()
        .code(1);
    let stdout = String::from_utf8(assertion.get_output().stdout.clone()).expect("utf-8 stdout");
    let line = stdout
        .lines()
        .find(|line| line.contains("\"event\":\"finding\""))
        .unwrap_or_else(|| panic!("`gmeow math {leaf}` emitted no NDJSON finding:\n{stdout}"));
    let event: serde_json::Value = serde_json::from_str(line).expect("the finding line is JSON");
    event["finding"].clone()
}

/// A scratch R script that is a SYNTAX error (an unclosed call) — the malformed twin of the
/// committed, well-formed-but-unliftable `unliftable.R`.
///
/// The returned [`tempfile::TempDir`] owns the script: hold it for as long as the
/// path is used, and the whole directory is removed when it drops.
fn malformed_r_script() -> (tempfile::TempDir, PathBuf) {
    let (tmp, dir) = scratch("malformed-r");
    let path = dir.join("unclosed-call.R");
    std::fs::write(&path, "x <- lm(mpg ~ wt\n").expect("write the malformed R fixture");
    (tmp, path)
}

#[test]
fn the_two_failure_classes_are_distinguished_structurally_not_by_exit_code() {
    // Both classes exit 1 (a handled failure of a correctly-spelled invocation over a
    // file that was found and read); exit 2 stays reserved for clap usage errors. The
    // distinction travels on the DIAGNOSTIC channel as the ENGINE's own code and category —
    // that is the channel a corpus sweep should branch on.
    let malformed = failing_lift_finding("lift-onnx", &lift_fixture("truncated.onnx"));
    let unliftable = failing_lift_finding("lift-r", &lift_fixture("unliftable.R"));

    assert_eq!(malformed["code"], "math.lift.onnx.wire");
    assert_eq!(unliftable["code"], "math.lift.r.unliftable");
    // A truncated byte stream is a DATA-SHAPE violation; a well-formed script with nothing
    // to carry is a MODELING-DISCIPLINE violation. The grading is the engine's, not a CLI
    // approximation of it.
    assert_eq!(malformed["category"], "data-shape-violation");
    assert_eq!(unliftable["category"], "modeling-discipline-violation");
}

#[test]
fn distinct_lift_failures_mint_distinct_finding_fingerprints() {
    // The regression this pins: the handler used to re-wrap the engine's typed `Diag` with a
    // rendered `format!("Error: {diag}")` under one of two CLI codes, discarding the code,
    // the category, and the detail. A truncated protobuf and an R SYNTAX error then shared a
    // single `finding_iri`, so the diagnostics substrate could not tell them apart and
    // `gmeow explain` conflated them.
    let wire = failing_lift_finding("lift-onnx", &lift_fixture("truncated.onnx"));
    let (_malformed_tmp, malformed_r) = malformed_r_script();
    let r_parse = failing_lift_finding("lift-r", &malformed_r);
    let r_unliftable = failing_lift_finding("lift-r", &lift_fixture("unliftable.R"));

    assert_eq!(wire["code"], "math.lift.onnx.wire");
    assert_eq!(r_parse["code"], "math.lift.r.parse");
    assert_eq!(r_unliftable["code"], "math.lift.r.unliftable");

    let iris: Vec<&str> = [&wire, &r_parse, &r_unliftable]
        .iter()
        .map(|f| {
            f["finding_iri"]
                .as_str()
                .expect("every lift failure is a ledger-borne witness with a finding_iri")
        })
        .collect();
    assert_ne!(
        iris[0], iris[1],
        "an ONNX wire failure and an R parse failure must be different witnesses"
    );
    assert_ne!(
        iris[1], iris[2],
        "an R parse failure and an R unliftable refusal must be different witnesses"
    );
    assert_ne!(iris[0], iris[2]);

    // And each finding carries the engine's `detail`, plus the source anchor naming which
    // artifact refused — the two things the collapsed path dropped.
    for (finding, leaf) in [(&wire, "lift-onnx"), (&r_parse, "lift-r")] {
        assert!(
            finding["detail"]
                .as_str()
                .is_some_and(|d| d.contains(&format!("gmeow math {leaf}"))),
            "the finding lost its context detail: {finding}"
        );
        assert_eq!(finding["anchor_non_trivial"], true);
    }
}

#[test]
fn a_missing_source_is_a_read_failure_not_a_lift_failure() {
    gmeow()
        .args(["math", "lift-proof", "/nonexistent-derivation.tstp"])
        .assert()
        .code(1)
        .stderr(
            predicate::str::contains("Error: cannot read")
                .and(predicate::str::contains("gmeow-cli.io.read")),
        );
}

// ── the product sinks: stdout, --out, stdin ──────────────────────────────────

#[test]
fn out_receives_exactly_the_bytes_stdout_emits() {
    let (_tmp, dir) = scratch("out");
    let out = dir.join("mtcars.ttl");
    gmeow()
        .args(["math", "lift-r"])
        .arg(lift_fixture("mtcars.R"))
        .arg("--out")
        .arg(&out)
        .assert()
        .success();
    let written = std::fs::read_to_string(&out).expect("--out wrote the product file");
    assert_eq!(
        written,
        lift_ok("lift-r", "mtcars.R"),
        "one product, two sinks: --out and stdout must be byte-identical"
    );
}

#[test]
fn a_dash_source_lifts_from_standard_input() {
    let script = std::fs::read_to_string(lift_fixture("mtcars.R")).expect("read the R fixture");
    let assertion = gmeow()
        .args(["math", "lift-r", "-"])
        .write_stdin(script)
        .assert()
        .success();
    let piped = String::from_utf8(assertion.get_output().stdout.clone()).expect("utf-8 turtle");
    assert_eq!(
        piped,
        lift_ok("lift-r", "mtcars.R"),
        "the source is BYTES: reading them from a pipe and from a path must agree"
    );
}

// ── idempotence and the consumer-surface razor ───────────────────────────────

#[test]
fn re_lifting_the_same_artifact_is_byte_identical() {
    // The property the fixed mint base exists to guarantee: with a content-addressed
    // run IRI and a constant base, a lift is a pure function of the artifact. A clock,
    // a counter, or a cwd-derived base would make every re-lift a new, incomparable
    // graph.
    for (leaf, fixture) in [
        ("lift-r", "mtcars.R"),
        ("lift-onnx", "mlp.onnx"),
        ("lift-proof", "theorem-subclass.tstp"),
    ] {
        assert_eq!(
            lift_ok(leaf, fixture),
            lift_ok(leaf, fixture),
            "`gmeow math {leaf}` over {fixture} is not idempotent"
        );
    }
}

#[test]
fn no_internal_private_use_language_tag_leaks_onto_the_lifted_surface() {
    // The consumer razor `tests/self_sufficiency.rs` pins repo-wide: an `x-gmeow-*`
    // private-use tag is an INTERNAL carrier and must never reach a consumer artifact.
    // The lift sinks emit plain literals precisely so this holds.
    for (leaf, fixture) in [
        ("lift-r", "mtcars.R"),
        ("lift-onnx", "mlp.onnx"),
        ("lift-proof", "theorem-subclass.tstp"),
    ] {
        let ttl = lift_ok(leaf, fixture);
        assert!(
            !ttl.contains("x-gmeow-"),
            "`gmeow math {leaf}` leaked an internal x-gmeow-* tag onto the consumer surface"
        );
    }
}

// ── discoverability ──────────────────────────────────────────────────────────

#[test]
fn the_math_group_and_every_leaf_are_discoverable() {
    gmeow().args(["math", "--help"]).assert().success().stdout(
        predicate::str::contains("lift-r")
            .and(predicate::str::contains("lift-onnx"))
            .and(predicate::str::contains("lift-proof")),
    );
    for leaf in ["lift-r", "lift-onnx", "lift-proof"] {
        gmeow()
            .args(["math", leaf, "--help"])
            .assert()
            .success()
            .stdout(predicate::str::contains("--out"));
    }
}

#[test]
fn a_lift_leaf_with_no_source_is_a_clap_usage_error() {
    // Exit 2 belongs to the argument vector, and stays there: it is what a MISSING
    // operand yields, never what an unreadable or unliftable artifact yields.
    gmeow().args(["math", "lift-r"]).assert().code(2);
}
