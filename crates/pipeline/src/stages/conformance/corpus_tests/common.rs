// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only native verdict admission shared by dedicated corpus assertions.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use gmeow_conformance::consistency::Observation;
use gmeow_conformance::paths::{cases_root, repo_root};
use gmeow_logic::reason::DlVerdict;

type Observations = BTreeMap<String, Result<Observation, gmeow_errors::RecordedDiag>>;

pub fn native_verdict(input_nq: &Path) -> Result<DlVerdict, gmeow_errors::RecordedDiag> {
    static OBSERVATIONS: OnceLock<Result<Observations, gmeow_errors::Diag>> = OnceLock::new();
    let observations = OBSERVATIONS
        .get_or_init(|| {
            let bytes = gmeow_action_cache::selection::source_artifacts::load(
                &repo_root(),
                "stage-conformance",
                super::super::execution::CONSISTENCY_CHANNEL,
            )
            .map_err(gmeow_errors::Diag::from)?;
            serde_json::from_slice(&bytes).map_err(gmeow_errors::Diag::from)
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("authenticated consistency observations: {error}"));
    let path = input_nq.canonicalize().expect("selected consistency input");
    let root = repo_root();
    let key = path
        .strip_prefix(&root)
        .expect("selected consistency input is inside the repository")
        .to_string_lossy();
    let observation = observations
        .get(key.as_ref())
        .unwrap_or_else(|| panic!("no producer-selected consistency observation for {key}"))
        .as_ref()
        .map_err(Clone::clone)?;
    Ok(observation.verdict.clone())
}

pub fn native_token(input_nq: &Path) -> String {
    let verdict = native_verdict(input_nq)
        .unwrap_or_else(|error| panic!("authenticated consistency: {error}"));
    match verdict.information_state() {
        gmeow_logic::result::InformationState::Both => "inconsistent",
        gmeow_logic::result::InformationState::Supported => "consistent",
        _ => "incomplete",
    }
    .to_owned()
}

pub fn native_full_root() -> PathBuf {
    cases_root().join("external/w3c-owl2-full-native")
}

pub fn divergence_root() -> PathBuf {
    cases_root().join("external/w3c-owl2-full-divergence")
}

pub fn case_slugs(root: &Path) -> BTreeMap<String, PathBuf> {
    assert!(root.is_dir(), "corpus root missing: {}", root.display());
    let mut cases = BTreeMap::new();
    for entry in std::fs::read_dir(root).expect("case inventory") {
        let path = entry.expect("case inventory entry").path();
        if !path.is_dir() || !path.join("input.nq").is_file() {
            continue;
        }
        let slug = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("UTF-8 case name");
        cases.insert(slug.to_owned(), path);
    }
    cases
}

pub(super) fn supplemental() -> &'static super::super::supplemental::Observations {
    static OBSERVATIONS: OnceLock<super::super::supplemental::Observations> = OnceLock::new();
    OBSERVATIONS.get_or_init(|| {
        let bytes = crate::fixture::authenticated_artifact(
            &repo_root(),
            "stage-conformance",
            super::super::supplemental::CHANNEL,
        )
        .expect("authenticated oracle observations");
        serde_json::from_slice(&bytes).expect("typed oracle observations")
    })
}

pub(super) fn input_key(path: &Path) -> String {
    path.canonicalize()
        .expect("selected source exists")
        .strip_prefix(repo_root())
        .expect("selected source inside repository")
        .to_string_lossy()
        .into_owned()
}

/// Exact producer-selected source admission, with no test-side native execution.
pub(super) fn native_admission(
    input_nq: &Path,
) -> &'static gmeow_conformance::native_observation::SourceAdmissionObservation {
    static OBSERVATIONS: OnceLock<super::super::native_cases::Admissions> = OnceLock::new();
    let values = OBSERVATIONS.get_or_init(|| {
        let bytes = crate::fixture::authenticated_artifact(
            &repo_root(),
            "stage-conformance",
            super::super::native_cases::CLASS_ADMISSION_CHANNEL,
        )
        .expect("authenticated source-admission observations");
        serde_json::from_slice(&bytes).expect("typed source-admission observations")
    });
    let key = input_key(input_nq);
    let observed = values
        .get(&key)
        .unwrap_or_else(|| panic!("missing selected source-admission observation for {key}"))
        .as_ref()
        .unwrap_or_else(|error| panic!("source admission failed for {key}: {error}"));
    let bytes = std::fs::read(input_nq).expect("exact selected case bytes");
    assert_eq!(
        observed.input_blake3,
        *blake3::hash(&bytes).as_bytes(),
        "source-admission input identity"
    );
    observed
        .admission
        .validate()
        .expect("native source-admission scope");
    observed
}
