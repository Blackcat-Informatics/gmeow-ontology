// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independently reusable oracle and TPTP producer observations.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use gmeow_action_cache::{ActionStore, STORE_FORMAT_VERSION, StoreLimits};
use gmeow_conformance::external::tptp::{AnnotatedFormula, TptpError, parse_tptp};
use gmeow_conformance::observations::{
    self, NumericOutcome, OntoumlOutcome, OracleVerdict, TptpObservation,
};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

pub(super) const CHANNEL: &str = "pipeline/native-oracle-observations.json";
const DL: &str = "coverage/external/697-dl-oracle-gold";
const NUMERIC: &str = "coverage/external/1428-numeric-builtin-oracle-gold";
const TSTP_FIXTURE: &str = "crates/math-lift/fixtures/theorem-subclass.tstp";

#[derive(Serialize, Deserialize)]
pub(super) struct Observations {
    pub dl: BTreeMap<String, Result<OracleVerdict, gmeow_errors::RecordedDiag>>,
    pub numeric: BTreeMap<String, Result<NumericOutcome, gmeow_errors::RecordedDiag>>,
    pub tptp:
        BTreeMap<String, Result<Result<TptpObservation, TptpError>, gmeow_errors::RecordedDiag>>,
    pub ontouml: BTreeMap<String, Result<OntoumlOutcome, gmeow_errors::RecordedDiag>>,
    pub tstp_fixture: Result<Vec<AnnotatedFormula>, gmeow_errors::RecordedDiag>,
}

pub(super) fn input_files(root: &Path) -> gmeow_errors::Result<Vec<PathBuf>> {
    let mut files = vec![root.join(TSTP_FIXTURE)];
    for directory in [DL, NUMERIC] {
        super::execution::collect_files(&root.join(directory), &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn fail(error: impl std::fmt::Display) -> gmeow_errors::Diag {
    super::stage_err(&error.to_string())
}

pub(super) fn record(
    root: &Path,
    artifacts: &mut BTreeMap<String, Vec<u8>>,
) -> gmeow_errors::Result<()> {
    let store = ActionStore::open(
        ActionStore::default_root(root),
        STORE_FORMAT_VERSION,
        StoreLimits::default(),
    )
    .map_err(fail)?;
    let mut files = input_files(root)?;
    files.extend(super::execution::input_files(root)?);
    let selected = |directory: &str, extension: &str| -> Vec<_> {
        files
            .iter()
            .filter(|path| {
                path.starts_with(root.join(directory))
                    && path.extension().is_some_and(|suffix| suffix == extension)
            })
            .cloned()
            .collect()
    };
    let dl = selected(&format!("{DL}/datasets"), "ttl")
        .par_iter()
        .map(|path| {
            Ok((
                key(root, path)?,
                super::execution::cached_observation(
                    root,
                    path,
                    &store,
                    "dl-oracle",
                    observations::dl_oracle,
                )
                .map_err(super::record_failure),
            ))
        })
        .collect::<gmeow_errors::Result<BTreeMap<_, _>>>()?;
    let numeric = selected(&format!("{NUMERIC}/cases"), "json")
        .par_iter()
        .map(|path| {
            Ok((
                key(root, path)?,
                super::execution::cached_observation(
                    root,
                    path,
                    &store,
                    "numeric-oracle",
                    observations::numeric_oracle,
                )
                .map_err(super::record_failure),
            ))
        })
        .collect::<gmeow_errors::Result<BTreeMap<_, _>>>()?;
    let tptp = files
        .par_iter()
        .filter(|path| path.ends_with("source/problem.p"))
        .map(|path| {
            let case = path
                .parent()
                .and_then(Path::parent)
                .ok_or_else(|| fail("TPTP case path"))?;
            let slug = case
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| fail("TPTP case name"))?;
            let corpus = case
                .parent()
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .ok_or_else(|| fail("TPTP corpus name"))?;
            let world = format!("https://gmeow.example/{corpus}/{slug}/w");
            Ok((
                key(root, path)?,
                super::execution::cached_observation(
                    root,
                    path,
                    &store,
                    "tptp-native-selections",
                    |bytes| observations::tptp(bytes, &world),
                )
                .map_err(super::record_failure),
            ))
        })
        .collect::<gmeow_errors::Result<BTreeMap<_, _>>>()?;
    let ontouml = files
        .par_iter()
        .filter(|path| {
            path.ends_with("source/model.ttl")
                && path.starts_with(
                    root.join("conformance/logic/cases/external/ontouml-mini-divergence"),
                )
        })
        .map(|path| {
            let slug = path
                .parent()
                .and_then(Path::parent)
                .and_then(Path::file_name)
                .and_then(|name| name.to_str())
                .ok_or_else(|| fail("OntoUML case name"))?;
            let world = format!("https://gmeow.example/ontouml-mini-divergence/{slug}/w");
            Ok((
                key(root, path)?,
                super::execution::cached_observation(
                    root,
                    path,
                    &store,
                    "ontouml-native-divergence",
                    |bytes| observations::ontouml(bytes, &world),
                )
                .map_err(super::record_failure),
            ))
        })
        .collect::<gmeow_errors::Result<BTreeMap<_, _>>>()?;
    if ontouml.is_empty() {
        return Err(fail("required OntoUML observation inventory is empty"));
    }
    if dl.is_empty() || numeric.is_empty() || tptp.is_empty() {
        return Err(fail("required oracle observation inventory is empty"));
    }
    let tstp_fixture = super::execution::cached_observation(
        root,
        &root.join(TSTP_FIXTURE),
        &store,
        "tstp-fixture-parse",
        |bytes| {
            let text = std::str::from_utf8(bytes).map_err(gmeow_errors::Diag::from)?;
            parse_tptp(text).map_err(gmeow_errors::Diag::from)
        },
    )
    .map_err(super::record_failure);
    artifacts.insert(
        CHANNEL.to_owned(),
        serde_json::to_vec(&Observations {
            dl,
            numeric,
            tptp,
            tstp_fixture,
            ontouml,
        })
        .map_err(fail)?,
    );
    Ok(())
}

fn key(root: &Path, path: &Path) -> gmeow_errors::Result<String> {
    Ok(path
        .strip_prefix(root)
        .map_err(fail)?
        .to_string_lossy()
        .into_owned())
}
