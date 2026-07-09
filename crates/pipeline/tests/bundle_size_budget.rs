// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Size-budget gate over the committed `generated/dist/gmeow.gts`.
//!
//! ## Why this gate exists
//!
//! The `gmeow` deliverable ships ONE artifact — `gmeow.gts` — that self-contains
//! every projection surface, including the full documentation site, the mdbook
//! source tree, and the print PDF. Because those projections fold *into* the
//! bundle, careless growth (an accidental full re-embedding, a duplicated blob, a
//! debug artifact folded by mistake) inflates the single shippable file with no
//! other signal. This gate turns unbounded growth into a hard, named failure:
//!
//! * a TOTAL committed-file ceiling of 48 MiB, and
//! * a per-representation DECODED-archive ceiling table.
//!
//! Both ceilings are DELIBERATE, REVIEWED constants. Raising one is a considered
//! edit — bump the number here in the same change that legitimately grows a blob,
//! and record why in the PR. A silent bump defeats the gate.
//!
//! ## The per-rep ceilings
//!
//! Each ceiling below was pinned by measuring the actual decoded archive size of
//! the committed bundle (sum of member byte lengths from `Bundle::archive(rep)`)
//! and adding ~15% headroom, rounded to a clean number. The
//! `per_rep_decoded_sizes_within_pinned_ceilings` test prints the full measured
//! table on every run (visible under `--nocapture`), so re-pinning after an
//! intentional growth is a matter of reading the printed actuals. On ANY breach
//! the failure message prints the full table so the offending blob is named.

use std::path::PathBuf;

use gmeow_pipeline::bundle_blobs::{
    Bundle, REP_AXIOMS, REP_CELLS, REP_DOCS_BOOK, REP_DOCS_PRINT, REP_MAPPINGS, REP_ONTOLOGY_DOCS,
    REP_QUERIES, REP_REASONING, REP_SCHEMAS, REP_SHAPES, REP_TESTS, REP_YAMLLD,
};

/// The total committed-file ceiling: the single `gmeow.gts` must stay at or below
/// 48 MiB. Raising this is a deliberate, reviewed edit.
const TOTAL_CEILING: u64 = 48 * 1024 * 1024;

/// A pinned per-representation decoded-archive ceiling.
struct RepCeiling {
    /// The `REP_*` constant name (for the failure table).
    name: &'static str,
    /// The rep label.
    rep: &'static str,
    /// The maximum decoded archive size (sum of member byte lengths), in bytes.
    ceiling: usize,
}

/// The pinned ceiling table. Each ceiling ≈ measured decoded size + ~15% headroom,
/// rounded to a clean number. Covers the two documentation-projection blobs
/// (`docs-book`, `docs-print`) plus every other major representation.
const REP_CEILINGS: &[RepCeiling] = &[
    // measured 130_779_951 → ×1.15 ≈ 150_396_944 → pinned 152_000_000
    RepCeiling {
        name: "REP_ONTOLOGY_DOCS",
        rep: REP_ONTOLOGY_DOCS,
        ceiling: 152_000_000,
    },
    // measured 182_331_410 → ×1.15 ≈ 209_681_122 → pinned 210_000_000
    RepCeiling {
        name: "REP_YAMLLD",
        rep: REP_YAMLLD,
        ceiling: 210_000_000,
    },
    // measured 13_021_873 → ×1.15 ≈ 14_975_154 → pinned 15_500_000
    RepCeiling {
        name: "REP_DOCS_BOOK",
        rep: REP_DOCS_BOOK,
        ceiling: 15_500_000,
    },
    // measured 7_806_543 → ×1.15 ≈ 8_977_524 → pinned 9_500_000
    RepCeiling {
        name: "REP_DOCS_PRINT",
        rep: REP_DOCS_PRINT,
        ceiling: 9_500_000,
    },
    // measured 8_887_022 (grown by the reasoner-safe shape grounding: the injected owl:someValuesFrom
    // / owl:allValuesFrom restrictions enrich the inferred closure + reasoning explanations)
    // → ×1.15 ≈ 10_220_075 → pinned 10_500_000
    RepCeiling {
        name: "REP_REASONING",
        rep: REP_REASONING,
        ceiling: 10_500_000,
    },
    // measured 2_381_714 → ×1.15 ≈ 2_739_471 → pinned 2_800_000
    RepCeiling {
        name: "REP_SCHEMAS",
        rep: REP_SCHEMAS,
        ceiling: 2_800_000,
    },
    // measured 1_808_153 → ×1.15 ≈ 2_079_376 → pinned 2_100_000
    RepCeiling {
        name: "REP_TESTS",
        rep: REP_TESTS,
        ceiling: 2_100_000,
    },
    // measured 1_649_328 → ×1.15 ≈ 1_896_727 → pinned 2_000_000
    RepCeiling {
        name: "REP_CELLS",
        rep: REP_CELLS,
        ceiling: 2_000_000,
    },
    // measured 1_180_671 → ×1.15 ≈ 1_357_772 → pinned 1_400_000
    RepCeiling {
        name: "REP_SHAPES",
        rep: REP_SHAPES,
        ceiling: 1_400_000,
    },
    // measured 501_179 → ×1.15 ≈ 576_356 → pinned 600_000
    RepCeiling {
        name: "REP_AXIOMS",
        rep: REP_AXIOMS,
        ceiling: 600_000,
    },
    // measured 467_554 → ×1.15 ≈ 537_687 → pinned 550_000
    RepCeiling {
        name: "REP_MAPPINGS",
        rep: REP_MAPPINGS,
        ceiling: 550_000,
    },
    // measured 254_340 → ×1.15 ≈ 292_491 → pinned 300_000
    RepCeiling {
        name: "REP_QUERIES",
        rep: REP_QUERIES,
        ceiling: 300_000,
    },
];

/// The committed bundle path (`generated/dist/gmeow.gts`), resolved off the crate
/// manifest so the test runs from any cwd.
fn committed_gts_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("generated/dist/gmeow.gts")
}

/// The decoded archive size of one rep: the sum of its member byte lengths.
fn decoded_archive_size(bundle: &Bundle, rep: &str) -> usize {
    bundle
        .archive(rep)
        .unwrap_or_else(|e| panic!("resolve archive {rep}: {e}"))
        .values()
        .map(Vec::len)
        .sum()
}

/// Render `n` bytes as an integer plus a MiB approximation, for the tables.
fn human(n: u64) -> String {
    format!("{n} bytes (~{:.2} MiB)", n as f64 / (1024.0 * 1024.0))
}

#[test]
fn total_bundle_size_within_48_mib() {
    let bytes = std::fs::read(committed_gts_path()).expect("read committed gmeow.gts");
    let total = bytes.len() as u64;
    assert!(
        total <= TOTAL_CEILING,
        "committed gmeow.gts exceeds the total size budget.\n  \
         actual:   {}\n  ceiling:  {}\n  OVER by:  {}\n\
         Raising the 48 MiB ceiling is a deliberate reviewed edit — do it in the \
         same change that legitimately grows the bundle, and record why.",
        human(total),
        human(TOTAL_CEILING),
        human(total.saturating_sub(TOTAL_CEILING)),
    );
    // Report the remaining headroom so a near-breach is visible under --nocapture.
    println!(
        "committed gmeow.gts: {} of {} ({} headroom remaining)",
        human(total),
        human(TOTAL_CEILING),
        human(TOTAL_CEILING.saturating_sub(total)),
    );
}

#[test]
fn per_rep_decoded_sizes_within_pinned_ceilings() {
    let bytes = std::fs::read(committed_gts_path()).expect("read committed gmeow.gts");
    let bundle = Bundle::from_snapshot(&bytes).expect("fold committed gmeow.gts");

    // Measure every rep, then format one full table. Printed unconditionally so
    // `--nocapture` shows the live actuals for re-pinning after intentional growth.
    let mut rows: Vec<(&'static str, usize, usize, bool)> = Vec::new();
    for rc in REP_CEILINGS {
        let actual = decoded_archive_size(&bundle, rc.rep);
        rows.push((rc.name, actual, rc.ceiling, actual > rc.ceiling));
    }

    let table = |rows: &[(&'static str, usize, usize, bool)]| -> String {
        let mut s = String::from(
            "  rep                    decoded bytes        ceiling              status\n",
        );
        for (name, actual, ceiling, over) in rows {
            s.push_str(&format!(
                "  {name:<22} {actual:<20} {ceiling:<20} {}\n",
                if *over {
                    format!("OVER by {} bytes", actual.saturating_sub(*ceiling))
                } else {
                    "ok".to_string()
                },
            ));
        }
        s
    };

    println!("per-rep decoded archive sizes:\n{}", table(&rows));

    let breaches: Vec<_> = rows.iter().filter(|(_, _, _, over)| *over).collect();
    assert!(
        breaches.is_empty(),
        "one or more bundle representations exceed their pinned decoded-size ceiling.\n{}\n\
         Each pinned ceiling is a deliberate reviewed constant (measured size + ~15%). \
         Raising one is an intentional edit made in the same change that grows the blob.",
        table(&rows),
    );
}
