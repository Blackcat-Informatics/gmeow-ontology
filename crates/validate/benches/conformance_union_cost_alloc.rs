// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! DETERMINISTIC allocation cost-partition for the whole-ontology-union conformance
//! path — the always-available half of the gate-policy measurement (needs no
//! valgrind; the sibling `conformance_union_cost_iai` bench adds the retired-
//! instruction corroboration when valgrind is present).
//!
//! It partitions the cost a single whole-ontology-union twin pays into one-time
//! setup `S`, dataset projection `P`, and already-projected constraint execution
//! `V_projected`, using `gmeow-cost-measure`'s `CountingAllocator` (total bytes +
//! alloc count + peak-live, each deterministic as a per-region delta). It also
//! attributes constraint work between the generated procedural projection and
//! the authored/non-procedural shape corpus. This distinguishes cacheable setup,
//! GMEOW compiler work, and the shared purrdf evaluator before an optimization is
//! chosen.
//!
//! `harness = false`: this is a hand-written `fn main()`, not a criterion/libtest
//! harness. Run via `make maint-bench-instructions`; NOT wired into `make check`.

#[path = "cost_common/mod.rs"]
mod cost_common;

use std::sync::Arc;

use gmeow_cost_measure::{AllocSample, CountingAllocator, measure};

// Installing the counting allocator on THIS bench binary is what turns `measure`
// on. It never reaches the shipped `gmeow` CLI (a leaf bench target only).
#[global_allocator]
static ALLOC: CountingAllocator = CountingAllocator::new();

/// Print one region's sample as a stable, greppable line.
fn report(label: &str, s: AllocSample) {
    println!(
        "cost-partition\t{label}\tbytes={}\tcount={}\tpeak_live={}",
        s.bytes, s.count, s.peak_live
    );
}

fn main() {
    // Pin Rayon to a single thread so any parallel section inside the SHACL engine
    // does not fan allocations onto a worker thread concurrently with a measured
    // region (the total bytes/count stay deterministic sums either way, but this
    // keeps the measured region strictly sequential per the cost-measure contract).
    let _ = rayon::ThreadPoolBuilder::new()
        .num_threads(1)
        .build_global();

    // WARM PASS (outside every `measure`): pay all first-touch lazy-init in purrdf's
    // parser / SHACL engine now, so the measured deltas below attribute solely to the
    // region's own work, not to one-time process warmup.
    {
        let onto = cost_common::build_merged_ontology();
        let shapes = cost_common::load_production_shapes();
        let shapes_without_procedural = cost_common::load_shapes_without_procedural();
        let procedural_shapes = cost_common::load_procedural_shapes();
        let full = cost_common::ontology_plus_fixture(&onto);
        let fixture = cost_common::fixture_only_dataset();
        let projected_onto = cost_common::project(&onto);
        let projected_full = cost_common::project(&full);
        let projected_fixture = cost_common::project(&fixture);
        let _ = cost_common::validate_projected(Arc::clone(&projected_onto), &shapes);
        let _ = cost_common::validate_projected(
            Arc::clone(&projected_onto),
            &shapes_without_procedural,
        );
        let _ = cost_common::validate_projected(projected_onto, &procedural_shapes);
        let _ = cost_common::validate_projected(projected_full, &shapes);
        let _ = cost_common::validate_projected(projected_fixture, &shapes);
        let _ = cost_common::validate(&onto, &shapes);
        let _ = cost_common::validate(&full, &shapes);
        let _ = cost_common::validate(&fixture, &shapes);
    }

    // ── Setup S ────────────────────────────────────────────────────────────────
    let (onto, s_onto) = measure(cost_common::build_merged_ontology);
    report("S_build_merged_ontology", s_onto);

    let (shapes, s_shapes) = measure(cost_common::load_production_shapes);
    report("S_load_production_shapes", s_shapes);

    // ── Per-twin scans against the SAME production shape corpus ──────────────────
    // V_ontology_only: the fixture-independent whole-graph scan a disk cache can NEVER
    // remove. This is the load-bearing number for the amortization-vs-off-gate call.
    let (_r1, v_ontology_only) = measure(|| cost_common::validate(&onto, &shapes));
    report("V_ontology_only", v_ontology_only);

    let (projected_onto, p_ontology_only) = measure(|| cost_common::project(&onto));
    report("P_ontology_only", p_ontology_only);
    let (_r1_projected, v_projected_ontology_only) =
        measure(|| cost_common::validate_projected(Arc::clone(&projected_onto), &shapes));
    report("V_projected_ontology_only", v_projected_ontology_only);

    let shapes_without_procedural = cost_common::load_shapes_without_procedural();
    let (_r1_without_procedural, v_without_procedural) = measure(|| {
        cost_common::validate_projected(Arc::clone(&projected_onto), &shapes_without_procedural)
    });
    report("V_projected_without_procedural", v_without_procedural);

    let procedural_shapes = cost_common::load_procedural_shapes();
    let (_r1_procedural, v_procedural) = measure(|| {
        cost_common::validate_projected(Arc::clone(&projected_onto), &procedural_shapes)
    });
    report("V_projected_procedural_only", v_procedural);

    // V_full: ontology unioned with the representative twin fixture.
    let full = cost_common::ontology_plus_fixture(&onto);
    let (_r2, v_full) = measure(|| cost_common::validate(&full, &shapes));
    report("V_full", v_full);

    let (projected_full, p_full) = measure(|| cost_common::project(&full));
    report("P_full", p_full);
    let (_r2_projected, v_projected_full) =
        measure(|| cost_common::validate_projected(Arc::clone(&projected_full), &shapes));
    report("V_projected_full", v_projected_full);

    // V_fixture: the tiny fixture alone — the cheap on-gate anchor (~0.05 s path).
    let fixture = cost_common::fixture_only_dataset();
    let (_r3, v_fixture) = measure(|| cost_common::validate(&fixture, &shapes));
    report("V_fixture", v_fixture);

    // Derived partition summary (bytes as the headline scalar).
    let v_marginal_bytes = v_full.bytes.saturating_sub(v_ontology_only.bytes);
    let s_bytes = s_onto.bytes.saturating_add(s_shapes.bytes);
    println!(
        "cost-partition\tDERIVED\tS_bytes={s_bytes}\tV_ontology_only_bytes={}\t\
         V_marginal_bytes={v_marginal_bytes}\tV_fixture_bytes={}\t\
         scan_over_fixture_ratio={:.1}",
        v_ontology_only.bytes,
        v_fixture.bytes,
        (v_ontology_only.bytes as f64) / (v_fixture.bytes.max(1) as f64),
    );
    println!(
        "cost-partition\tVERDICT\tdisk_cache_removes_only_S={s_bytes}_bytes\t\
         per_twin_scan_it_cannot_remove={}_bytes",
        v_ontology_only.bytes,
    );
    println!(
        "cost-partition\tATTRIBUTION\tprojection_bytes={}\tconstraint_bytes={}\t\
         projection_share_pct={:.3}\tnon_procedural_constraint_bytes={}\t\
         procedural_constraint_bytes={}\tprocedural_share_pct={:.1}",
        p_ontology_only.bytes,
        v_projected_ontology_only.bytes,
        100.0 * (p_ontology_only.bytes as f64)
            / ((p_ontology_only.bytes + v_projected_ontology_only.bytes).max(1) as f64),
        v_without_procedural.bytes,
        v_procedural.bytes,
        100.0 * (v_procedural.bytes as f64) / (v_projected_ontology_only.bytes.max(1) as f64),
    );
}
