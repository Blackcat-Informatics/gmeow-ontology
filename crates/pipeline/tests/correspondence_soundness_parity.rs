// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Equivalence-before-deletion parity gate for the correspondence soundness migration
//! (#1092 F5 Task 3a).
//!
//! The seven correspondence-stack semantic checks (the five alignment checks + the two
//! FnO back-end checks — including the SOLE native enforcer of Constitution Principle 5,
//! the equivalence-collapse gate) moved from the oxigraph-coupled `gmeow_slice` lints
//! (`alignment_lint` + `projection_lint`) into the wasm-clean
//! `gmeow_logic_compile::projections::correspondence_soundness` pass, driven by the
//! oxigraph-free pipeline edge `stages::correspondence_soundness::lint_correspondence_soundness`.
//!
//! This harness runs BOTH over the REAL committed repo corpus and asserts the finding sets
//! are byte-identical (same severity / check / code / message / instance / subject /
//! predicate / object). If they differ, the migration dropped or changed coverage — a hard
//! failure. The old `gmeow_slice::lint_projection` is NOT deleted by this task; this proof
//! is the gate that authorizes Task 3b's deletion.

use gmeow_pipeline::stages::correspondence_soundness::lint_correspondence_soundness;
use gmeow_slice::lint_projection;

/// The normalized comparison key for one finding — every field the diagnostic carries, in a
/// stable tuple so the two crates' distinct `ProjectionDiagnostic` types compare directly.
type Key = (
    String,         // severity
    String,         // check
    String,         // code
    String,         // message
    Option<String>, // instance
    Option<String>, // subject_id
    Option<String>, // predicate_id
    Option<String>, // object_id
);

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("canonicalize repo root")
}

#[test]
fn new_soundness_pass_is_identical_to_retired_lint_projection() {
    let root = repo_root();

    // 1. The retired oxigraph-coupled lint (alignment + FnO checks), allow_network=false
    //    (the on-gate path).
    let old = lint_projection(&root, false).expect("retired lint_projection should not error");

    // 2. The new oxigraph-free soundness pass over the same root.
    let new = lint_correspondence_soundness(&root, false)
        .expect("native correspondence-soundness pass should not error");

    // 3. Normalize both to the full-field key tuple and sort.
    let mut old_keys: Vec<Key> = old
        .iter()
        .map(|f| {
            (
                f.severity.clone(),
                f.check.clone(),
                f.code.clone(),
                f.message.clone(),
                f.instance.clone(),
                f.subject_id.clone(),
                f.predicate_id.clone(),
                f.object_id.clone(),
            )
        })
        .collect();
    let mut new_keys: Vec<Key> = new
        .iter()
        .map(|f| {
            (
                f.severity.clone(),
                f.check.clone(),
                f.code.clone(),
                f.message.clone(),
                f.instance.clone(),
                f.subject_id.clone(),
                f.predicate_id.clone(),
                f.object_id.clone(),
            )
        })
        .collect();
    old_keys.sort();
    new_keys.sort();

    // 4. Identical count.
    assert_eq!(
        old_keys.len(),
        new_keys.len(),
        "finding COUNT differs: retired lint_projection produced {} findings, native \
         soundness produced {} — the migration changed coverage",
        old_keys.len(),
        new_keys.len()
    );

    // 5. Identical sets — report the first divergence in full for debuggability.
    if old_keys != new_keys {
        let only_old: Vec<&Key> = old_keys.iter().filter(|k| !new_keys.contains(k)).collect();
        let only_new: Vec<&Key> = new_keys.iter().filter(|k| !old_keys.contains(k)).collect();
        panic!(
            "finding SETS differ ({} findings each).\n  only in retired lint ({}): {:#?}\n  \
             only in native soundness ({}): {:#?}",
            old_keys.len(),
            only_old.len(),
            only_old.iter().take(5).collect::<Vec<_>>(),
            only_new.len(),
            only_new.iter().take(5).collect::<Vec<_>>(),
        );
    }

    // 6. A floor: the committed corpus exercises real findings (the alignment checks emit
    //    INFO findings for unavailable targets at minimum), so an empty result on BOTH sides
    //    would be a silent no-op rather than a real parity proof.
    assert!(
        !old_keys.is_empty(),
        "expected the committed corpus to exercise at least one finding (sanity floor)"
    );

    eprintln!(
        "correspondence-soundness parity: {} findings, IDENTICAL (retired lint_projection vs \
         native soundness pass)",
        old_keys.len()
    );
}
