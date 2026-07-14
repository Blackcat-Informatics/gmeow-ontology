// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Light behavioural tests over the `slices/grounding/logic` fixture — the one
//! slice that ships fr/zh `.po` catalogs AND external `gmeow:TermEquivalence`
//! alignments, so the cross-lingual JOIN, the explicit-absent record, the batch
//! hard-fail, exemplar shortfall, and determinism are all exercised on real data.

use std::collections::BTreeMap;
use std::path::PathBuf;

use gmeow_slice_brief::{BriefInputs, GroundingAttribute, assemble_packet};

const RULE: &str = "https://blackcatinformatics.ca/logic/Rule";

fn logic_slice_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../slices/grounding/logic")
}

fn empty_tiers() -> BTreeMap<String, i64> {
    BTreeMap::new()
}

#[test]
fn fr_translation_joins_as_present_cell() {
    let dir = logic_slice_dir();
    let tiers = empty_tiers();
    let inputs = BriefInputs {
        slice_dir: &dir,
        axis: Some("Rule"),
        batch: None,
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    };
    let packet = assemble_packet(&inputs).expect("assemble logic Rule packet");

    let cell = packet
        .grounding
        .iter()
        .find(|c| {
            c.term == RULE
                && c.attribute == GroundingAttribute::Fr
                && c.predicate.as_deref() == Some("rdfs:label")
        })
        .expect("a groundingFr rdfs:label cell for logic:Rule");
    assert!(cell.present, "logic:Rule fr label must be present");
    assert_eq!(cell.value.as_deref(), Some("Règle"));
}

#[test]
fn missing_zh_is_an_explicit_absent_cell() {
    // The first sorted batch of the slice covers terms the zh catalog does not
    // reach, so their zh cells are recorded present=false (never silently dropped).
    let dir = logic_slice_dir();
    let tiers = empty_tiers();
    let inputs = BriefInputs {
        slice_dir: &dir,
        axis: None,
        batch: Some(0),
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    };
    let packet = assemble_packet(&inputs).expect("assemble batch-0 packet");

    let absent = packet
        .grounding
        .iter()
        .find(|c| c.attribute == GroundingAttribute::Zh && !c.present)
        .expect("at least one explicit-absent groundingZh cell");
    assert!(
        absent.predicate.is_some(),
        "an absent language cell still names its annotation predicate"
    );
    assert!(absent.value.is_none(), "an absent cell carries no value");
}

#[test]
fn batch_out_of_range_is_a_hard_fail() {
    let dir = logic_slice_dir();
    let tiers = empty_tiers();
    let inputs = BriefInputs {
        slice_dir: &dir,
        axis: None,
        batch: Some(99_999),
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    };
    assert!(
        assemble_packet(&inputs).is_err(),
        "an out-of-range batch must be an Err, never a silent empty packet"
    );
}

#[test]
fn exemplar_shortfall_is_recorded() {
    // Inject a single positive tier against a target of 3: found=1, shortfall=2.
    let dir = logic_slice_dir();
    let mut tiers = BTreeMap::new();
    tiers.insert(RULE.to_string(), 5_i64);
    let inputs = BriefInputs {
        slice_dir: &dir,
        axis: Some("Rule"),
        batch: None,
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    };
    let packet = assemble_packet(&inputs).expect("assemble packet with injected tier");

    assert_eq!(packet.exemplars, vec![RULE.to_string()]);
    assert_eq!(packet.exemplar_shortfall, 2);
}

#[test]
fn to_turtle_is_byte_stable_across_identical_assemblies() {
    let dir = logic_slice_dir();
    let tiers = empty_tiers();
    let inputs = BriefInputs {
        slice_dir: &dir,
        axis: Some("Rule"),
        batch: None,
        exemplar_tiers: &tiers,
        exemplar_target: 3,
    };
    let a = assemble_packet(&inputs).expect("first assembly");
    let b = assemble_packet(&inputs).expect("second assembly");

    assert_eq!(a.digest, b.digest, "digest is deterministic");
    assert_eq!(
        a.to_turtle(),
        b.to_turtle(),
        "canonical turtle is byte-identical across identical inputs"
    );
}
