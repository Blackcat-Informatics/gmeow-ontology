// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The fold-parity gate: the executor's composed fold must be
//! fold-isomorphic to the committed `generated/dist/gmeow.gts`.
//!
//! It runs the executor spine over the real repo, reads the sink's `gmeow.gts`,
//! folds BOTH the sink output and the committed reference through the kernel GTS
//! reader (`purrdf::gts::read_graph`), and compares the
//! per-named-graph quad counts plus the reifier/annotation counts. Per the
//! semantic gts gate, full CBOR byte-identity is NOT required: the FOLD
//! (quads + reifiers + annotations per named graph) is the contract.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_pipeline::{PipelineCache, RunContext, bind, default_registry, full_spec, run};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

/// Per-named-graph quad counts + reifier/annotation counts of a folded snapshot.
struct FoldShape {
    by_graph: BTreeMap<String, usize>,
    /// Ground (blank-node-free) quads, keyed `graph\ts p o`, as a set: blank-node
    /// canonical labels differ across runs, so only blank-free quads compare as a
    /// stable set (the count comparison covers the blank-bearing remainder).
    ground: std::collections::BTreeSet<String>,
    reifiers: usize,
    annotations: usize,
}

fn fold_shape(bytes: &[u8]) -> FoldShape {
    // Read the folded GTS graph and count over the raw `Graph` quad table. We do
    // Count graph slots directly (resolving only the graph-name term id). This is
    // robust when the committed snapshot carries a triple-term quad whose binding
    // lives only in `reifies`, and is exactly the per-named-graph fold the parity
    // gate measures.
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
        *by_graph.entry(key.clone()).or_default() += 1;
        // The pipeline DAG is kept regenerated in lock-step with `module.ttl`, so the
        // freshly-composed fold matches the committed bundle quad-for-quad — with ONE
        // exception: `gmeow:bundleContentId` is the bundle's OWN content-address,
        // stored INSIDE the slice-analysis graph it hashes. It is a self-referential
        // digest subject to a two-pass/fixpoint settle (the committed value is one
        // build's; an in-memory rebuild produces its own), so it can never equal the
        // committed value by construction. Every OTHER quad (the content it hashes)
        // is compared exactly; excluding only the self-hash is correct, not masking.
        if pv == "https://blackcatinformatics.ca/gmeow/bundleContentId" {
            continue;
        }
        // A blank node's canonical label is run-specific; only blank-free quads
        // compare across runs (the count covers the blank-bearing remainder).
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

fn run_sink() -> Vec<u8> {
    let root = repo_root();
    // Drive the REAL production DAG (`full_spec`, the same spec `run_full` / `make check`
    // build) rather than a hand-maintained copy: the executor's composed fold is then proven
    // fold-isomorphic to the committed bundle over the ACTUAL shipped stage set, and the spine
    // can never silently drift from the production wiring again.
    let spec = full_spec();
    let graph = spec.validate().expect("production DAG validates");
    let bound = bind(&spec, &graph, &default_registry()).expect("every spine stage binds");
    let cache_dir = tempfile::tempdir().unwrap();
    let mut ctx = RunContext::open(&root, 4).expect("ctx");
    ctx.cache = PipelineCache::open(cache_dir.path()).unwrap();
    // The PRODUCER's retention profile, for the same reason the spec is the production
    // one: `run_full` releases each stage's carrier at its drop-after-last-consumer point,
    // so folding the terminal bundle under any other profile would prove the fold of a run
    // the producer never performs. The sink's `gmeow.gts` artifact is a committed output
    // and survives every release untouched.
    ctx.carrier_retention = gmeow_pipeline::CarrierRetention::DropAfterLastConsumer;
    let result = run(&graph, &bound, &mut ctx).expect("pipeline runs end-to-end");
    let sink = result.products.get("stage-gts-sink").expect("sink product");
    sink.artifact("generated/dist/gmeow.gts")
        .expect("gmeow.gts artifact")
        .to_vec()
}

#[test]
fn sink_fold_matches_committed_per_named_graph() {
    let root = repo_root();
    let committed =
        std::fs::read(root.join("generated/dist/gmeow.gts")).expect("committed gmeow.gts");
    let committed_shape = fold_shape(&committed);

    let sink_bytes = run_sink();
    let sink_shape = fold_shape(&sink_bytes);

    // Print the comparison table (the iterate-until-green oracle).
    let mut all_graphs: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    all_graphs.extend(committed_shape.by_graph.keys().cloned());
    all_graphs.extend(sink_shape.by_graph.keys().cloned());
    println!("\n{:55} {:>10} {:>10}", "named-graph", "sink", "committed");
    let mut mismatches: Vec<String> = Vec::new();
    for g in &all_graphs {
        let s = sink_shape.by_graph.get(g).copied().unwrap_or(0);
        let c = committed_shape.by_graph.get(g).copied().unwrap_or(0);
        let label = g.rsplit('/').next().unwrap_or(g);
        let flag = if s == c { "" } else { "  <-- MISMATCH" };
        println!("{label:55} {s:>10} {c:>10}{flag}");
        if s != c {
            mismatches.push(format!("{label}: sink={s} committed={c}"));
        }
    }
    println!(
        "{:55} {:>10} {:>10}{}",
        "reifiers",
        sink_shape.reifiers,
        committed_shape.reifiers,
        if sink_shape.reifiers == committed_shape.reifiers {
            ""
        } else {
            "  <-- MISMATCH"
        }
    );
    println!(
        "{:55} {:>10} {:>10}{}",
        "annotations",
        sink_shape.annotations,
        committed_shape.annotations,
        if sink_shape.annotations == committed_shape.annotations {
            ""
        } else {
            "  <-- MISMATCH"
        }
    );
    if sink_shape.reifiers != committed_shape.reifiers {
        mismatches.push(format!(
            "reifiers: sink={} committed={}",
            sink_shape.reifiers, committed_shape.reifiers
        ));
    }
    if sink_shape.annotations != committed_shape.annotations {
        mismatches.push(format!(
            "annotations: sink={} committed={}",
            sink_shape.annotations, committed_shape.annotations
        ));
    }

    assert!(
        mismatches.is_empty(),
        "fold-parity mismatches:\n  {}",
        mismatches.join("\n  ")
    );

    // Stronger than counts: every blank-free quad set must match exactly, per
    // named graph. (Blank-bearing quads are covered by the per-graph counts above
    // — their canonical labels are run-specific and cannot compare as a set.)
    let missing: Vec<&String> = committed_shape
        .ground
        .difference(&sink_shape.ground)
        .collect();
    let extra: Vec<&String> = sink_shape
        .ground
        .difference(&committed_shape.ground)
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "ground-quad set drift: {} missing, {} extra\n  first missing: {:?}\n  first extra: {:?}",
        missing.len(),
        extra.len(),
        missing.first(),
        extra.first()
    );
}
