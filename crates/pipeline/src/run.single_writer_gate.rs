// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{GTS_PATH, assert_single_gts_writer};
use crate::node::StageProduct;
use std::collections::BTreeMap;

fn gts_writer(id: &str) -> StageProduct {
    let mut a: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    a.insert(GTS_PATH.to_string(), b"gts-bytes".to_vec());
    StageProduct::from_artifacts(id, a)
}

fn plain(id: &str) -> StageProduct {
    let mut a: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    a.insert("generated/other.ttl".to_string(), b"x".to_vec());
    StageProduct::from_artifacts(id, a)
}

fn products(items: Vec<StageProduct>) -> BTreeMap<String, StageProduct> {
    items.into_iter().map(|p| (p.stage_id.clone(), p)).collect()
}

#[test]
fn exactly_one_writer_passes() {
    let p = products(vec![gts_writer("stage-gts-sink"), plain("stage-export")]);
    assert!(assert_single_gts_writer(&p, "stage-gts-sink").is_ok());
}

#[test]
fn a_second_writer_is_rejected() {
    let p = products(vec![
        gts_writer("stage-gts-sink"),
        gts_writer("stage-rogue"),
    ]);
    let msg = format!(
        "{}",
        assert_single_gts_writer(&p, "stage-gts-sink").unwrap_err()
    );
    assert!(
        msg.contains("2 stages emit") && msg.contains("exactly one"),
        "a second GTS writer must hard-fail: got {msg}"
    );
    assert!(
        msg.contains("stage-gts-sink") && msg.contains("stage-rogue"),
        "the error must name both offending stages: got {msg}"
    );
}

#[test]
fn no_writer_is_rejected() {
    let p = products(vec![plain("stage-export")]);
    let msg = format!(
        "{}",
        assert_single_gts_writer(&p, "stage-gts-sink").unwrap_err()
    );
    assert!(
        msg.contains("no stage emits"),
        "zero GTS writers must hard-fail: got {msg}"
    );
}

#[test]
fn a_writer_that_is_not_the_declared_sink_is_rejected() {
    // A single writer exists, so the old count-only gate would have passed this —
    // but its stage_id does not match the declared sink (sinkCapability), so the
    // stronger identity gate must reject it as a rogue writer impersonating the
    // terminal (PIPELINE_SPINE §4/§7).
    let p = products(vec![gts_writer("stage-impostor"), plain("stage-export")]);
    let msg = format!(
        "{}",
        assert_single_gts_writer(&p, "stage-gts-sink").unwrap_err()
    );
    assert!(
        msg.contains("stage-impostor") && msg.contains("stage-gts-sink"),
        "the error must name both the actual writer and the declared sink: got {msg}"
    );
}
