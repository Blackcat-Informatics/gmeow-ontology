// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repo root")
}

fn inventory() -> ProjectionInventory {
    let bytes = gmeow_action_cache::selection::source_artifacts::load(
        &repo_root(),
        "stage-mappings",
        CHANNEL,
    )
    .expect("producer-selected projection inventory; no source construction in tests");
    serde_json::from_slice(&bytes).expect("native projection inventory")
}

#[test]
fn the_live_inventory_matches_the_live_projections_tree() {
    let measured = inventory().check().expect("live inventory is exact");
    assert!(
        measured.len() >= 34,
        "the consumer projection-profile inventory shrank: {} profiles",
        measured.len()
    );
    assert_eq!(
        measured
            .get("dsl/mappings/projections/schema-org-procedures.ttl")
            .copied(),
        Some(8),
        "the schema.org HowTo/Recipe consumer profile must declare its eight cells"
    );
}

#[test]
fn deleting_a_profile_reds_and_adding_an_undeclared_one_reds() {
    let mut observed = inventory();
    observed.check().expect("the original inventory is exact");
    let victim = "dsl/mappings/projections/schema-org-procedures.ttl";
    let count = observed
        .measured
        .remove(victim)
        .expect("profile exists in producer observation");
    let error = observed
        .check()
        .expect_err("deleting a consumer profile must red");
    assert!(
        error.to_string().contains("no longer on disk")
            && error.to_string().contains("schema-org-procedures.ttl"),
        "unexpected message: {error}"
    );
    observed.measured.insert(victim.to_owned(), count);
    observed.check().expect("restored inventory is exact again");
    observed
        .measured
        .insert(format!("{PROJECTIONS_DIR}/undeclared.ttl"), count);
    let error = observed
        .check()
        .expect_err("an undeclared profile must red");
    assert!(
        error.to_string().contains("not declared in the inventory"),
        "unexpected message: {error}"
    );
}

#[test]
fn hollowing_a_profile_out_reds_on_the_cell_floor() {
    let mut observed = inventory();
    *observed
        .measured
        .get_mut("dsl/mappings/projections/schema-org-procedures.ttl")
        .expect("selected profile") = 0;
    let error = observed.check().expect_err("an emptied profile must red");
    assert!(
        error.to_string().contains("cell-count ratchet"),
        "unexpected message: {error}"
    );
}
