// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Full slice-example validation sweep (#700 Task 6).
//!
//! This integration test proves the closed-world fidelity of the SHACL→JSON
//! Schema projection over the WHOLE example corpus: for every `slices/*/*/
//! examples/*.ttl` data graph, the projected JSON-LD `@graph` instance form
//! validates against the JSON Schema the emitter derives from the SAME merged
//! shapes the live validator uses ([`gmeow_shacl::shape_union::load_shapes`]).
//!
//! # Soundness contract
//!
//! The JSON Schema is a CLOSED-WORLD projection of the SHACL shapes: it claims
//! to accept exactly the data the SHACL validator accepts (for the modeled
//! subset). Therefore:
//!
//! * If an example does NOT conform to its SHACL shapes, it is illustrative
//!   (not valid instance data) and OUT OF SCOPE for the schema sweep. Such
//!   examples are listed in [`NON_CONFORMANT`] with a one-line reason. The test
//!   asserts the excluded set is EXACTLY the set that fails native SHACL — so an
//!   exclusion can never silently mask a real schema bug.
//! * If an example DOES conform to SHACL but the JSON Schema REJECTS it, that is
//!   a soundness bug in the emitter/projector, surfaced as a test failure with a
//!   readable per-example violation report.

use std::path::{Path, PathBuf};

use gmeow_shacl::shapes::Shapes;
use gmeow_shacl::{engine, instance, json_schema, shape_union};
use gmeow_validate::instance::{validate_instance, InstanceFormat};
use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::store::Store;

/// Examples that do NOT conform to the merged SHACL shapes and are therefore
/// out of scope for the JSON-schema sweep (illustrative, not valid instance
/// data). The sweep asserts this set is EXACTLY the SHACL-failing set, so this
/// allowlist cannot hide a JSON-schema soundness bug.
///
/// Each entry is the repo-relative path; the trailing comment is the reason.
const NON_CONFORMANT: &[&str] = &[
    // Bucket A — `sh:class` (ClassConstraintComponent): the example references a
    // SHARED ontology individual (a method/status/kind/profile defined in the
    // vocabulary, not redeclared in the standalone fixture), so the referenced
    // node lacks its `rdf:type` when the file is loaded in isolation. The example
    // is meant to be read alongside the full ontology; standalone it is
    // illustrative, not valid instance data.
    "slices/core/ai/examples/grounded-claim.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/calendar/examples/recurring-meeting.ttl", // gmeow:invitationStatus → shared status individual untyped standalone
    "slices/core/deception/examples/blame-deflection.ttl", // gmeow:doxasticClaim → StandpointClaim not typed standalone
    "slices/core/diagnostics/examples/shacl-violation-finding.ttl", // gmeow:findingSeverity → shared DiagnosticSeverity untyped standalone
    "slices/core/epistemics/examples/belief-revision.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/epistemics/examples/flagship-epistemic-ledger.ttl", // gmeow:epistemicAgent → Agent not typed standalone
    "slices/core/epistemics/examples/justification-and-defeat.ttl", // gmeow:defeatedBy → JustificationStatus not typed standalone
    "slices/core/gts/examples/dist-package.ttl", // gmeow:gtsProfile → shared profile individual untyped standalone
    "slices/core/imagination/examples/reality-monitoring.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/abduction.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/analogy.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/belief-revision.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/deduction.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inference/examples/induction.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inquiry/examples/loaded-question.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/inquiry/examples/open-question-and-resolution.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/metacognition/examples/dunning-kruger.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/metacognition/examples/reflection-revision.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/observations/examples/temperature-reading.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/core/pipeline/examples/minimal-pipeline.ttl", // gmeow:stageKind → shared StageKind untyped standalone
    "slices/core/profiles/examples/named-profile-membership.ttl", // gmeow:profileAppliesTo → owl:Class target not typed standalone
    "slices/core/standpoint/examples/contested-authorship.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    "slices/extensions/aggregation/examples/spatial-bins.ttl", // gmeow:aggregationFunction → shared function individual untyped standalone
    "slices/extensions/dreaming/examples/ai-offline-replay.ttl", // gmeow:gtsProfile → shared profile individual untyped standalone
    "slices/extensions/finance/examples/double-entry.ttl", // gmeow:ledgerAccountHolder → Agent not typed standalone
    "slices/extensions/images/examples/photo-metadata.ttl", // gmeow:selectorType → shared selector-type individual untyped standalone
    "slices/extensions/music/examples/score-as-lossy-projection.ttl", // gmeow:realizes → Work not typed standalone
    "slices/extensions/notes/examples/annotations-and-notes.ttl", // gmeow:commentParent → Entity not typed standalone
    "slices/extensions/sensory/examples/sensor-reading.ttl", // gmeow:observationMethod → shared method individual untyped standalone
    // Bucket B — `sh:minCount` (MinCountConstraintComponent): the example omits a
    // P11-required reference / temporal frame on a value or interval. The frame
    // lives in the full ontology context; standalone the fixture is illustrative.
    "slices/core/creative-works/examples/wemi-novel.ttl", // Expression missing gmeow:hasReferenceFrame (P11)
    "slices/core/documents/examples/web-presence.ttl", // Expression missing gmeow:hasReferenceFrame (P11)
    "slices/core/learning/examples/skill-acquisition-trajectory.ttl", // TimeInterval missing gmeow:hasTemporalFrame (P11)
    "slices/extensions/affect/examples/two-critics.ttl", // Expression missing gmeow:hasReferenceFrame (P11)
    "slices/extensions/narrative/examples/flashback.ttl", // Event missing gmeow:eventTemporalFrame (P11)
];

/// The repo root (two levels up from this crate's manifest dir).
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

/// Load one Turtle data-graph file into a fresh oxigraph [`Store`], using the
/// SAME lenient driver the shape union uses ([`shape_union::load_shapes`]).
fn load_data_graph(path: &Path) -> Store {
    let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let store = Store::new().expect("create store");
    let mut parser = RdfParser::from_format(RdfFormat::Turtle)
        .lenient()
        .for_reader(bytes.as_slice());
    for quad in parser.by_ref() {
        let quad = quad.unwrap_or_else(|e| panic!("parse {}: {e}", path.display()));
        store.insert(&quad).expect("store insert");
    }
    store
}

/// Glob every `slices/*/*/examples/*.ttl`, sorted, as repo-relative paths.
fn example_files(repo: &Path) -> Vec<PathBuf> {
    let slices = repo.join("slices");
    let mut out: Vec<PathBuf> = Vec::new();
    for group in read_dirs(&slices) {
        for slice in read_dirs(&group) {
            let examples = slice.join("examples");
            if !examples.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&examples)
                .unwrap_or_else(|e| panic!("read {}: {e}", examples.display()))
            {
                let path = entry.expect("dir entry").path();
                if path.extension().and_then(|e| e.to_str()) == Some("ttl") && path.is_file() {
                    out.push(path);
                }
            }
        }
    }
    out.sort();
    out
}

/// Immediate subdirectories of `dir`, sorted (empty when `dir` is absent).
fn read_dirs(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        // Fail fast on an unreadable entry rather than silently dropping a
        // slice/example directory (which would let the sweep pass without covering
        // the full corpus) — matching the file-level `example_files()` behavior.
        .map(|e| {
            e.unwrap_or_else(|err| panic!("read {} entry: {err}", dir.display()))
                .path()
        })
        .filter(|p| p.is_dir())
        .collect();
    out.sort();
    out
}

/// Render a path relative to the repo root (forward slashes) for reports/allowlist.
fn rel(repo: &Path, path: &Path) -> String {
    path.strip_prefix(repo)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Whether `store` conforms to the merged `shapes` per the native SHACL engine.
fn conforms_to_shacl(store: &Store, shapes: &Shapes) -> bool {
    engine::validate(store, shapes).conforms
}

#[test]
fn example_corpus_validates_against_closed_world_schema() {
    let repo = repo_root();

    // The merged shape union + the JSON Schema derived from those same shapes.
    let (_shapes_store, shapes) =
        shape_union::load_shapes(&repo).expect("load merged SHACL shapes");
    let compiled = json_schema::compile(&shapes);
    let schema_bytes = compiled.schema_json.as_bytes();

    let non_conformant: std::collections::BTreeSet<&str> = NON_CONFORMANT.iter().copied().collect();

    let examples = example_files(&repo);
    assert!(
        !examples.is_empty(),
        "no example fixtures found under slices/*/*/examples/*.ttl"
    );

    // Per-example outcomes.
    let mut schema_failures: Vec<(String, Vec<String>)> = Vec::new();
    let mut shacl_failing: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut excluded_count = 0usize;
    let mut passed_count = 0usize;

    for path in &examples {
        let relpath = rel(&repo, path);
        let store = load_data_graph(path);

        // (A) Does the example conform to its SHACL shapes? If not, it is
        //     illustrative, not valid instance data → out of scope.
        let shacl_ok = conforms_to_shacl(&store, &shapes);
        if !shacl_ok {
            shacl_failing.insert(relpath.clone());
        }

        if non_conformant.contains(relpath.as_str()) {
            excluded_count += 1;
            continue;
        }

        // (B) Project to JSON-LD and validate against the closed-world schema.
        let instance_value = instance::project_graph(&store);
        let instance_bytes = serde_json::to_vec(&instance_value).expect("serialize instance");
        let violations = validate_instance(&instance_bytes, InstanceFormat::Json, schema_bytes)
            .unwrap_or_else(|e| panic!("validate_instance hard error for {relpath}: {e}"));

        if violations.is_empty() {
            passed_count += 1;
        } else {
            schema_failures.push((relpath, violations));
        }
    }

    // Log a sweep summary (visible with --nocapture).
    eprintln!(
        "example sweep: {} total, {} passed, {} excluded (non-conformant), {} schema failures",
        examples.len(),
        passed_count,
        excluded_count,
        schema_failures.len()
    );
    if !non_conformant.is_empty() {
        eprintln!("excluded (SHACL-non-conformant, out of scope):");
        for ex in NON_CONFORMANT {
            eprintln!("  - {ex}");
        }
    }

    // Invariant 1: the allowlist must be EXACTLY the SHACL-failing set, so an
    // exclusion can never silently mask a JSON-schema soundness bug.
    let allowlisted: std::collections::BTreeSet<String> =
        non_conformant.iter().map(|s| (*s).to_owned()).collect();
    if allowlisted != shacl_failing {
        let only_allowlist: Vec<&String> = allowlisted.difference(&shacl_failing).collect();
        let only_shacl: Vec<&String> = shacl_failing.difference(&allowlisted).collect();
        panic!(
            "NON_CONFORMANT allowlist drifted from the SHACL-failing set.\n\
             listed but actually SHACL-CONFORMANT (remove from allowlist): {only_allowlist:#?}\n\
             SHACL-NON-CONFORMANT but not listed (add to allowlist with a reason, \
             or fix the example): {only_shacl:#?}"
        );
    }

    // Invariant 2: every in-scope (SHACL-conformant, non-excluded) example must
    // validate against the closed-world JSON Schema.
    if !schema_failures.is_empty() {
        let mut report = String::from(
            "closed-world JSON Schema REJECTED SHACL-conformant example data \
             (soundness bug in emitter/projector):\n",
        );
        for (path, violations) in &schema_failures {
            report.push_str(&format!("\n{path}:\n"));
            for v in violations.iter().take(5) {
                report.push_str(&format!("  - {v}\n"));
            }
            if violations.len() > 5 {
                report.push_str(&format!("  … and {} more\n", violations.len() - 5));
            }
        }
        panic!("{report}");
    }
}
