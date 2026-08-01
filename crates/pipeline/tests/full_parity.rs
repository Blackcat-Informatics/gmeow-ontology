// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The FULL-build parity gate (P6 integration): a single-pass `run_full(check)`
//! must reproduce EVERY declared `generated/` artifact — semantically — against
//! the freshly-materialized staged tree.
//!
//! `generated/` is NOT git-tracked: it is a git-ignored LOCAL PRODUCT that
//! `make check` materializes on disk (PIPELINE_SPINE §6 fanout). So the reference
//! this gate compares against is that MATERIALIZED staged tree (the bytes `make
//! sync` last wrote), never "historical Git bytes" and never the git index — the
//! test only ever reads the on-disk product. Its distinct value is the SEMANTIC
//! comparison a byte-only gate cannot do: it runs the whole dogfooded DAG
//! single-pass — the fold-reading export leaves consume THIS run's freshly-composed
//! `gmeow.gts` (the `stage-snapshot` product), not the on-disk file — and asserts a
//! fresh independent run reproduces the materialized tree by FOLD / graph
//! isomorphism, proving the staged materialization is itself a faithful projection
//! of the bundle.
//!
//! The must-exist authority (which paths a conforming build owes) is the DECLARED
//! inventory — the pipeline slice's `gmeow:expectsGeneratedOutput` rows, enforced
//! `⊆` by `project_bundle`'s completeness oracle — not a walk of this tree; this
//! gate's set is the paths a fresh run produces, each reconciled against the
//! materialized bytes.
//!
//! # What is compared (and what is NOT)
//!
//!  * `gmeow.gts` (materialized) — compared by the FOLD (per-named-graph quad set +
//!    reifier/annotation counts, the same comparator as `fold_parity.rs`).
//!    CBOR has encoding skew, so byte parity is not the contract; the fold
//!    is. A fold mismatch here is a real regression (modulo the self-describing
//!    pipeline-DAG triples the `fold_parity.rs` filter excludes for the
//!    stale-vs-fresh window before `make check` reruns the terminal).
//!  * Every other declared artifact (`generated/**`) — reconciled against the
//!    materialized bytes: byte-deterministic text/CSV/JSON/etc. by BYTES,
//!    RDF/Turtle leaves by GRAPH ISOMORPHISM (RDF text carries serializer skew, so
//!    the contract is the canonical quad set, not the bytes).
//!  * Ephemeral `dist/**` outputs carry NO materialized reference (the export
//!    leaves' on-disk-only outputs), so they NEVER gate parity. They are instead
//!    given DETERMINISM coverage: a second `run_full` must reproduce them
//!    byte-for-byte (a pure function of the inputs), proving the build is
//!    reproducible without pinning them to any on-disk reference.
//!
//! # The `KNOWN_SKEW` allowlist
//!
//! A tiny, justified set of artifacts whose materialized bytes were minted by an
//! EXTERNAL toolchain absent / differently-configured in this environment (not by
//! the snapshot rewiring). Each carries a one-line reason. A drift OUTSIDE this set
//! is a real regression and FAILS the gate.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_pipeline::{RunMode, run_full};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// The artifacts whose materialized bytes were minted by an external toolchain
/// that is absent / differently-configured in this environment, so the
/// freshly-produced bytes legitimately diverge. Minimal and individually
/// justified — NOT a place to hide real drift.
///
/// The last two parity gaps (P6) are now CLOSED:
///
///  * The three `generated/logic/*` artifacts (inferred-closure /
///    reasoning-explanations / dl-el-crosscheck-report) are produced by the
///    `stage-export-logic` leaf, which reasons over THIS run's FULL snapshot fold
///    through the SAME GTS-import → `reason_all` → `build_*_ttl` path that drives
///    the native reasoner over the materialized bundle. The native reasoner is
///    Docker-free and deterministic, so all three reproduce the materialized bytes
///    EXACTLY (the ledger is report-only / native-computed — no external oracle, so
///    no external dependency remains). They are NOT in this allowlist.
///  * `generated/projections/functions.fno.ttl` — the FnO emitter now applies the
///    `@x-gmeow-english` → `@en` projection-boundary retag (mirroring the native
///    EDOAL tag map), so it is
///    graph-isomorphic to the materialized `@en` projection. NOT in this allowlist.
///
/// The allowlist is EMPTY: the materialized `gmeow.gts` and every materialized leaf
/// are kept in lock-step with the dogfooded DAG, so the pipeline reproduces them all
/// with zero drift. New entries must be individually justified.
const KNOWN_SKEW: &[&str] = &[];

/// The schema artifacts are emitted natively by `stage-export-schemas`; this
/// prefix is used only to isolate schema drifts in the ignored full-parity report.
const SCHEMAS_PREFIX: &str = "generated/schemas/";

// `#[ignore]` in CI: this runs `run_full(Check)` several times (full reasoning +
// every leaf + a fold regen + a dist determinism pass) — minutes of work. The CI
// `ontology` lane's `gmeow-dev sync --mode check --outputs generated` runs the IDENTICAL `run_full(Check)`
// parity gate (with the native-ext environment it needs) and is the
// authoritative cutover gate; duplicating it as a multi-minute Rust test would only
// double the cost. Run on demand with `cargo test -p gmeow-pipeline --test full_parity
// -- --ignored`. (The cheaper `fold_parity` fold gate still runs unconditionally.)
#[ignore = "redundant with the ontology lane's `gmeow-dev sync --mode check --outputs generated`; run with --ignored"]
#[test]
fn full_run_reproduces_every_materialized_artifact() {
    let root = repo_root();

    // ── 1. The `gmeow.gts` bundle: FOLD parity. ──
    //
    // The fold matches EXACTLY except the self-describing pipeline DAG triples
    // excluded by `is_pipeline_self_triple` (the SAME filter `fold_parity.rs`
    // applies); any OTHER fold mismatch is a real drift and HARD-fails here.
    let regen = regen_gts_fold(&root);
    let materialized_fold = fold_shape(
        &std::fs::read(root.join("generated/dist/gmeow.gts")).expect("materialized gmeow.gts"),
    );
    let fold_mismatches = compare_folds(&regen, &materialized_fold);

    // ── 2. Every non-schemas leaf: reconcile over the production DAG. ──
    //
    // Run the full production DAG and compare every artifact it produces
    // (excluding `generated/schemas/*`, the internal `pipeline/`
    // dataflow, the reference-less `dist/*`, and `gmeow.gts` itself — compared by
    // fold above) against the materialized bytes, classifying any drift against
    // KNOWN_SKEW.
    let leaf_drifts = non_schemas_leaf_drifts(&root);
    let mut unexpected: Vec<String> = Vec::new();
    let mut classified_skew: Vec<String> = Vec::new();
    for path in &leaf_drifts {
        if KNOWN_SKEW.contains(&path.as_str()) {
            classified_skew.push(path.clone());
        } else {
            unexpected.push(path.clone());
        }
    }
    // ── 3. `dist/**` determinism. ──
    let dist_nondeterministic = dist_determinism_mismatches(&root);

    // ── 4. The native `generated/schemas/*` byte comparisons. ──
    let mut schema_drifts: Vec<String> = Vec::new();
    let report = run_full(&root, 4, RunMode::Check).expect("run_full(check) completes");
    println!(
        "\nfull_parity: produced={} reproduced={} drifted={}",
        report.produced,
        report.reproduced,
        report.drifted.len(),
    );
    for path in &report.drifted {
        if path.starts_with(SCHEMAS_PREFIX) && !KNOWN_SKEW.contains(&path.as_str()) {
            schema_drifts.push(path.clone());
        }
    }

    assert!(
        unexpected.is_empty()
            && fold_mismatches.is_empty()
            && dist_nondeterministic.is_empty()
            && schema_drifts.is_empty(),
        "full-build parity FAILED:\n  \
         unexpected materialized-leaf drifts: {unexpected:?}\n  \
         known-skew (classified, non-failing): {classified_skew:?}\n  \
         schema drifts: {schema_drifts:?}\n  \
         fold drifts:\n  {}\n  \
         dist non-determinism:\n  {}",
        fold_mismatches.join("\n  "),
        dist_nondeterministic.join("\n  "),
    );
}

/// Run the full production DAG over a fresh ephemeral cache and reconcile every
/// artifact it produces against the MATERIALIZED staged bytes on disk, returning the
/// drifted logical paths. Skips the gated `generated/schemas/*`, the internal
/// `pipeline/` dataflow, the reference-less `dist/*`, and `gmeow.gts` (folded above).
/// Byte-deterministic text compares by bytes; RDF leaves by graph isomorphism.
fn non_schemas_leaf_drifts(root: &Path) -> Vec<String> {
    use gmeow_pipeline::{CarrierRetention, RunContext, bind, default_registry, full_spec, run};
    // The FULL production DAG. The schemas leaf used to be filtered out of the spec
    // here; it cannot be any more — `stage-gts-sink` DECLARES it in `consumes()`, so
    // removing it leaves a dangling dependency and `validate()` HARD-fails. The filter
    // was also redundant: every comparison below already skips `generated/schemas/*` by
    // PATH, which is the exclusion that was ever actually wanted.
    let pre = full_spec();
    let graph = pre.validate().expect("production DAG validates");
    let bound = bind(&pre, &graph, &default_registry()).expect("binds");
    let mut ctx = RunContext::open_ephemeral(root, 4).expect("ctx");
    // The PRODUCER's retention profile, over the REAL DAG. `run_full` releases each
    // stage's carrier once its last declared consumer has run; this reconcile compares
    // exactly what that reconcile compares (every committed artifact, `pipeline/` skipped
    // below), so running it under the same profile is what proves the release changes no
    // produced byte on the real graph rather than only on a synthetic fixture.
    ctx.carrier_retention = CarrierRetention::DropAfterLastConsumer;
    let result = run(&graph, &bound, &mut ctx).expect("pipeline runs");

    let mut drifted: Vec<String> = Vec::new();
    for product in result.products.values() {
        for (path, bytes) in &product.artifacts() {
            if path.starts_with("pipeline/")
                || path.starts_with("dist/")
                || path.starts_with(SCHEMAS_PREFIX)
                || path == "generated/dist/gmeow.gts"
            {
                continue;
            }
            let materialized = match std::fs::read(root.join(path)) {
                Ok(c) => c,
                Err(_) => {
                    drifted.push(path.clone());
                    continue;
                }
            };
            if materialized == *bytes {
                continue;
            }
            // RDF/Turtle/N-Triples/N-Quads leaves carry serializer skew; compare them
            // by graph isomorphism (canonical quad set), not bytes.
            if is_rdf_leaf(path) && rdf_isomorphic(&materialized, bytes) {
                continue;
            }
            drifted.push(path.clone());
        }
    }
    drifted.sort();
    drifted.dedup();
    drifted
}

/// Whether `path` is an RDF text artifact compared by graph isomorphism.
fn is_rdf_leaf(path: &str) -> bool {
    path.ends_with(".ttl") || path.ends_with(".nt") || path.ends_with(".nq")
}

/// Whether two RDF documents are isomorphic (same RDFC-1.0 canonical quad set).
fn rdf_isomorphic(materialized: &[u8], produced: &[u8]) -> bool {
    canonical_quads(materialized)
        .zip(canonical_quads(produced))
        .map(|(c, p)| c == p)
        .unwrap_or(false)
}

fn canonical_quads(bytes: &[u8]) -> Option<std::collections::BTreeSet<String>> {
    // Native text ingress + native full RDFC-1.0: no oxigraph::io
    // parse, no oxrdf `Dataset::canonicalize`.
    for media_type in ["text/turtle", "application/n-quads"] {
        let Ok(ir) = purrdf::parse_dataset(bytes, media_type, None) else {
            continue;
        };
        let quads = purrdf::flat_rdf_quads_from_dataset(&ir);
        if !quads.is_empty() {
            let flat = purrdf::flat_dataset_from_quads(&quads).ok()?;
            return Some(
                purrdf::canonicalize(&flat)
                    .nquads
                    .lines()
                    .map(str::to_owned)
                    .collect(),
            );
        }
    }
    None
}

/// Run the fold-reading export DAG twice over fresh ephemeral caches and assert
/// every `dist/**` artifact reproduces byte-for-byte across the two runs
/// (determinism), since they have no materialized reference to compare against.
fn dist_determinism_mismatches(root: &Path) -> Vec<String> {
    let a = run_dist_artifacts(root);
    let b = run_dist_artifacts(root);
    let mut bad: Vec<String> = Vec::new();
    let mut all: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
    all.extend(a.keys());
    all.extend(b.keys());
    for path in all {
        match (a.get(path), b.get(path)) {
            (Some(x), Some(y)) if x == y => {}
            (Some(_), Some(_)) => bad.push(format!("{path}: byte-differs across two runs")),
            _ => bad.push(format!("{path}: produced in only one of two runs")),
        }
    }
    bad
}

/// Run the pre-schemas full DAG over a fresh ephemeral cache and collect every
/// `dist/**` artifact (the export leaves' on-disk-only outputs).
fn run_dist_artifacts(root: &Path) -> BTreeMap<String, Vec<u8>> {
    use gmeow_pipeline::{RunContext, bind, default_registry, full_spec, run};
    // The FULL production DAG. The schemas leaf used to be filtered out of the spec
    // here; it cannot be any more — `stage-gts-sink` DECLARES it in `consumes()`, so
    // removing it leaves a dangling dependency and `validate()` HARD-fails. The filter
    // was also redundant: every comparison below already skips `generated/schemas/*` by
    // PATH, which is the exclusion that was ever actually wanted.
    let pre = full_spec();
    let graph = pre.validate().expect("production DAG validates");
    let bound = bind(&pre, &graph, &default_registry()).expect("binds");
    let mut ctx = RunContext::open_ephemeral(root, 4).expect("ctx");
    let result = run(&graph, &bound, &mut ctx).expect("runs");
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for product in result.products.values() {
        for (path, bytes) in &product.artifacts() {
            if path.starts_with("dist/") {
                out.insert(path.clone(), bytes.clone());
            }
        }
    }
    out
}

// ── gmeow.gts fold comparison (reuse the fold_parity approach) ───────────────

struct FoldShape {
    by_graph: BTreeMap<String, usize>,
    ground: std::collections::BTreeSet<String>,
    reifiers: usize,
    annotations: usize,
}

/// Whether a quad is a self-describing pipeline-DAG triple, mirroring
/// `fold_parity.rs::is_pipeline_self_triple`. Re-authoring the dogfooded build DAG
/// (e.g. re-adding `gmeow:stage-export-logic`) legitimately changes these triples
/// in the freshly-composed fold while the materialized bundle still carries the old
/// DAG (until `make check` reruns the terminal), so they are excluded from the fold
/// comparison.
fn is_pipeline_self_triple(s: &str, p: &str, o: &str) -> bool {
    const NS: &str = "https://blackcatinformatics.ca/gmeow/";
    let pipeline_iri = |t: &str| -> bool {
        t.strip_prefix(NS).is_some_and(|local| {
            local.starts_with("stage-")
                || local.starts_with("kind")
                || local.starts_with("pipeline-")
                || matches!(
                    local,
                    "Pipeline"
                        | "PipelineStage"
                        | "StageCapability"
                        | "Resource"
                        | "BuildDataFlow"
                        | "hasStage"
                        | "dataflowConsumes"
                        | "dataflowProduces"
                        | "hasCapability"
                        | "sinkCapability"
                        | "sourceOrigin"
                        | "stageImpl"
                        | "producesFormat"
                        | "requiresResource"
                        | "engineResource"
                        | "flowEntity"
                        | "buildFlowFrom"
                        | "buildFlowTo"
                )
        })
    };
    // The slice-analysis graph's `gmeow:bundleContentId` (a content hash over the
    // authored graph) and the pipeline slice's OWN slice-analysis row (subject
    // `gmeow:slices/pipeline`, carrying its `gmeow:termCoverage` count) shift with a
    // DAG re-authoring; exclude them for the same stale-vs-fresh reason.
    if p == format!("{NS}bundleContentId") || s == format!("{NS}slices/pipeline") {
        return true;
    }
    pipeline_iri(s) || pipeline_iri(p) || pipeline_iri(o)
}

fn fold_shape(bytes: &[u8]) -> FoldShape {
    let g = purrdf::gts::read_graph(bytes, true).expect("read_graph");
    let term = |id: usize| -> String {
        g.terms
            .get(id)
            .and_then(|t| t.value.clone())
            .unwrap_or_else(|| format!("<term {id}>"))
    };
    let mut by_graph: BTreeMap<String, usize> = BTreeMap::new();
    let mut ground: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for &(s, p, o, gname) in &g.quads {
        let key = match gname {
            Some(gid) => term(gid),
            None => "<default>".to_string(),
        };
        let (sv, pv, ov) = (term(s), term(p), term(o));
        // Exclude the pipeline-self-description triples (stale-vs-fresh pending the
        // bundle regen) — the SAME filter `fold_parity.rs` applies.
        if is_pipeline_self_triple(&sv, &pv, &ov) {
            continue;
        }
        *by_graph.entry(key.clone()).or_default() += 1;
        if !sv.contains("c14n") && !ov.contains("c14n") {
            ground.insert(format!("{key}\t{sv} {pv} {ov}"));
        }
    }
    FoldShape {
        by_graph,
        ground,
        reifiers: g.reifiers.len(),
        annotations: g.annotations.len(),
    }
}

/// Re-derive the snapshot fold the way `run_full` does (the pre-sink spine over a
/// fresh ephemeral cache), without writing into the repo tree.
fn regen_gts_fold(root: &Path) -> FoldShape {
    use gmeow_pipeline::{RunContext, bind, default_registry, full_spec, run};
    // The FULL production DAG. The schemas leaf used to be filtered out of the spec
    // here; it cannot be any more — `stage-gts-sink` DECLARES it in `consumes()`, so
    // removing it leaves a dangling dependency and `validate()` HARD-fails. The filter
    // was also redundant: every comparison below already skips `generated/schemas/*` by
    // PATH, which is the exclusion that was ever actually wanted.
    let pre = full_spec();
    let graph = pre.validate().expect("production DAG validates");
    let bound = bind(&pre, &graph, &default_registry()).expect("binds");
    let mut ctx = RunContext::open_ephemeral(root, 4).expect("ctx");
    // Same as above: the terminal bundle is folded under the producer's retention profile.
    ctx.carrier_retention = gmeow_pipeline::CarrierRetention::DropAfterLastConsumer;
    let result = run(&graph, &bound, &mut ctx).expect("pipeline runs");
    let sink = result.products.get("stage-gts-sink").expect("sink product");
    let bytes = sink
        .artifact("generated/dist/gmeow.gts")
        .expect("gmeow.gts");
    fold_shape(bytes)
}

fn compare_folds(regen: &FoldShape, materialized: &FoldShape) -> Vec<String> {
    let mut mismatches: Vec<String> = Vec::new();
    let mut all_graphs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    all_graphs.extend(regen.by_graph.keys().cloned());
    all_graphs.extend(materialized.by_graph.keys().cloned());
    for gname in &all_graphs {
        let r = regen.by_graph.get(gname).copied().unwrap_or(0);
        let c = materialized.by_graph.get(gname).copied().unwrap_or(0);
        if r != c {
            let label = gname.rsplit('/').next().unwrap_or(gname);
            mismatches.push(format!("{label}: regen={r} materialized={c}"));
        }
    }
    if regen.reifiers != materialized.reifiers {
        mismatches.push(format!(
            "reifiers: regen={} materialized={}",
            regen.reifiers, materialized.reifiers
        ));
    }
    if regen.annotations != materialized.annotations {
        mismatches.push(format!(
            "annotations: regen={} materialized={}",
            regen.annotations, materialized.annotations
        ));
    }
    let missing: Vec<&String> = materialized.ground.difference(&regen.ground).collect();
    let extra: Vec<&String> = regen.ground.difference(&materialized.ground).collect();
    if !missing.is_empty() || !extra.is_empty() {
        mismatches.push(format!(
            "ground-quad drift: {} missing, {} extra (first missing: {:?}, first extra: {:?})",
            missing.len(),
            extra.len(),
            missing.first(),
            extra.first()
        ));
    }
    mismatches
}

// ── Two independent cold generations are deterministic (the reproducibility gate) ────────

/// Two independent cold generations from the SAME repo sources must be byte-identical on
/// every materialized flat output and validated-equivalent on the GTS bundle — the
/// clean-clone reproducibility gate. A DETERMINISTIC dropped output (a producing stage stops
/// emitting a file) is byte-identical across both runs and is caught ELSEWHERE, by the
/// completeness oracle (`superset::check_expected_completeness`, exercised by
/// `superset::tests::project_bundle_hard_fails_when_a_declared_output_is_never_produced`); a
/// NONDETERMINISTIC output (a timestamp / absolute path / hash-map iteration leak) is caught
/// HERE. Heavy (two full cold generations); `#[ignore]`d like the full-parity gate — CI's
/// producer lane invokes it with `--run-ignored`.
#[ignore = "heavy: two full cold generations; run with --ignored (CI producer lane invokes it)"]
#[test]
fn two_cold_generations_are_deterministic() {
    let root = repo_root();
    const BUNDLE: &str = "generated/dist/gmeow.gts";

    // Two independent in-process generations from the real sources into fresh ephemeral caches.
    let a = run_all_products(&root);
    let b = run_all_products(&root);

    // (a) BYTE-IDENTICAL flat outputs: every materialized path (the CBOR bundle excepted —
    //     compared by fold below) present in both runs with equal bytes.
    let mut flat_diffs: Vec<String> = Vec::new();
    let mut all: std::collections::BTreeSet<&String> = std::collections::BTreeSet::new();
    all.extend(a.keys());
    all.extend(b.keys());
    for path in all {
        if path == BUNDLE {
            continue;
        }
        match (a.get(path), b.get(path)) {
            (Some(x), Some(y)) if x == y => {}
            (Some(_), Some(_)) => {
                flat_diffs.push(format!("{path}: byte-differs across two cold runs"));
            }
            _ => flat_diffs.push(format!("{path}: produced in only one of two cold runs")),
        }
    }

    // (b) GTS bundle equivalence: assert the STRONGEST true invariant. Byte-identity first
    //     (the producer is deterministic, so the CBOR MAY be identical); on any CBOR encoding
    //     skew, fall back to FOLD equivalence (the same per-named-graph quad/reifier/annotation
    //     comparator the full-parity gate uses) plus the mandated frame-PROFILE check on BOTH
    //     bundles — never a naive CBOR byte compare masquerading as the contract.
    let gts_a = a.get(BUNDLE).expect("run A emits the gmeow.gts bundle");
    let gts_b = b.get(BUNDLE).expect("run B emits the gmeow.gts bundle");
    let mut bundle_diffs: Vec<String> = Vec::new();
    if gts_a != gts_b {
        let mismatches = compare_folds(&fold_shape(gts_a), &fold_shape(gts_b));
        if !mismatches.is_empty() {
            bundle_diffs.push(format!(
                "bundle FOLD differs across the two cold runs: {}",
                mismatches.join("; ")
            ));
        }
        for (label, bytes) in [("A", gts_a), ("B", gts_b)] {
            if !mandated_frame_profile_ok(bytes) {
                bundle_diffs.push(format!(
                    "bundle {label} does not satisfy the mandated GTS frame profile"
                ));
            }
        }
    }

    assert!(
        flat_diffs.is_empty() && bundle_diffs.is_empty(),
        "two cold generations diverged:\n  flat outputs:\n    {}\n  bundle:\n    {}",
        flat_diffs.join("\n    "),
        bundle_diffs.join("\n    "),
    );
}

/// Run the FULL producer DAG (the schemas tail included) over a fresh ephemeral cache and
/// collect every product artifact keyed by its repo-relative path. Two calls are two
/// independent cold generations from the same on-disk sources — the SAME `full_spec()` +
/// ephemeral-`run` machinery the full-parity determinism helpers use, not a reimplementation.
fn run_all_products(root: &Path) -> BTreeMap<String, Vec<u8>> {
    use gmeow_pipeline::{RunContext, bind, default_registry, full_spec, run};
    let spec = full_spec();
    let graph = spec.validate().expect("full DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("binds");
    let mut ctx = RunContext::open_ephemeral(root, 4).expect("ctx");
    let result = run(&graph, &bound, &mut ctx).expect("pipeline runs");
    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    for product in result.products.values() {
        for (path, bytes) in &product.artifacts() {
            out.insert(path.clone(), bytes.clone());
        }
    }
    out
}

/// Whether `bytes` satisfies the mandated GMEOW GTS frame profile — every payload-bearing
/// frame carries exactly the one `zstd-rsyncable` transform. This is the SAME invariant
/// `gmeow_pipeline::gts_profile`'s `assert_mandated_frames` pins as the single frame
/// authority (a crate-private unit-test helper, unreachable from an integration test),
/// mirrored here through purrdf's public `gts::wire` API so the two-generation fallback proves
/// each bundle is a VALID mandated-profile bundle, not merely fold-equivalent to the other.
fn mandated_frame_profile_ok(bytes: &[u8]) -> bool {
    use ciborium::value::Value;
    use purrdf::gts::wire::{iter_items, map_get, unwrap_header};

    fn as_map(v: &Value) -> Option<&[(Value, Value)]> {
        match v {
            Value::Map(entries) => Some(entries),
            _ => None,
        }
    }
    fn as_int(v: &Value) -> Option<i128> {
        match v {
            Value::Integer(i) => Some(i128::from(*i)),
            _ => None,
        }
    }

    let (items, torn) = iter_items(bytes);
    if torn.is_some() {
        return false;
    }
    let Some((_, header_item)) = items.first() else {
        return false;
    };
    let Ok(header) = unwrap_header(header_item) else {
        return false;
    };
    // The codec id of the mandated transform, resolved from the header catalog.
    let Some(catalog) = map_get(header, "cat").and_then(as_map) else {
        return false;
    };
    let Some(required) = catalog.iter().find_map(|(id, descriptor)| {
        let descriptor = as_map(descriptor)?;
        match map_get(descriptor, "name") {
            Some(Value::Text(name)) if name == "zstd-rsyncable" => as_int(id),
            _ => None,
        }
    }) else {
        return false;
    };
    let mut payload_frames = 0usize;
    for (_, item) in items.iter().skip(1) {
        let Some(frame) = as_map(item) else {
            return false;
        };
        if map_get(frame, "d").is_none() {
            // A metadata-only transport-key frame carries no payload and no transform chain.
            if map_get(frame, "x").is_some() {
                return false;
            }
            continue;
        }
        payload_frames += 1;
        let Some(Value::Array(transforms)) = map_get(frame, "x") else {
            return false;
        };
        if transforms.len() != 1 || as_int(&transforms[0]) != Some(required) {
            return false;
        }
    }
    payload_frames > 0
}
