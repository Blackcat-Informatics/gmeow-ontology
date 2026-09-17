// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shared helpers for the `*_decides_w3c_divergence` acceptance suites
//! (`casesplit_decides_w3c_divergence`, `counting_decides_w3c_divergence`,
//! `datatype_value_space_decides_w3c_divergence`).
//!
//! Each suite reads the exact native consistency observation selected by the
//! explicit producer. The two frozen corpora partition the original W3C-full set;
//! source paths identify receipts but never trigger parsing or reasoning here.

#![allow(dead_code)] // not every binary uses every helper

use std::path::{Path, PathBuf};

use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(serde::Deserialize)]
struct Observation {
    verdict: gmeow_logic::reason::DlVerdict,
}

type Observations = BTreeMap<String, Result<Observation, gmeow_errors::RecordedDiag>>;

/// Resolve exactly one source member from the two disjoint native-profiled and
/// divergence inventories. Missing or duplicate ownership fails before loading
/// an authenticated observation; corpus location never selects another engine.
pub fn case_input(slug: &str) -> PathBuf {
    let external =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/logic/cases/external");
    let candidates: Vec<_> = ["w3c-owl2-full-native", "w3c-owl2-full-divergence"]
        .into_iter()
        .map(|corpus| external.join(corpus).join(slug).join("input.nq"))
        .filter(|input| input.is_file())
        .collect();
    let [input] = candidates.as_slice() else {
        panic!("source case {slug} must have exactly one corpus owner");
    };
    input.clone()
}

/// Project the shared native consistency information state. Positive supported
/// conflict remains inconsistent even when independent completeness gaps remain.
pub fn native_token(slug: &str) -> String {
    static OBSERVATIONS: OnceLock<Observations> = OnceLock::new();
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root");
    let observations = OBSERVATIONS.get_or_init(|| {
        let bytes = gmeow_action_cache::selection::source_artifacts::load(
            &root,
            "stage-conformance",
            "pipeline/native-consistency-observations.json",
        )
        .expect("exact producer-selected consistency observations");
        serde_json::from_slice(&bytes).expect("typed consistency observations")
    });
    let input = case_input(slug)
        .canonicalize()
        .expect("selected case input");
    let key = input
        .strip_prefix(&root)
        .expect("case inside repository")
        .to_string_lossy();
    let verdict = &observations
        .get(key.as_ref())
        .unwrap_or_else(|| panic!("missing selected observation for {slug}"))
        .as_ref()
        .unwrap_or_else(|error| panic!("native observation failed for {slug}: {error}"))
        .verdict;
    match verdict.information_state() {
        gmeow_logic::result::InformationState::Both => "inconsistent",
        gmeow_logic::result::InformationState::Supported => "consistent",
        _ => "incomplete",
    }
    .to_owned()
}
