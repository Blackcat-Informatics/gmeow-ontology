// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only extracted correspondence envelopes and original declaration inventories.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

use serde::Deserialize;

#[derive(Deserialize)]
pub struct Observations {
    pub sources: BTreeMap<String, String>,
    pub catalogs: BTreeMap<String, Vec<Cell>>,
    pub transpilation: BTreeMap<String, Result<usize, gmeow_errors::RecordedDiag>>,
    pub declared_subjects: BTreeMap<String, BTreeSet<String>>,
    pub imported_classes: BTreeMap<String, BTreeSet<String>>,
    pub bfo_labels: BTreeMap<String, BTreeSet<String>>,
}

#[derive(Clone, Deserialize)]
pub struct Cell {
    pub iri: String,
    pub subject: String,
    pub predicate: String,
    pub obj: String,
    pub source_endpoint: Option<String>,
    pub target_endpoint: Option<String>,
    pub sssom_file: String,
    pub morphism_class: Option<String>,
    pub morphism_kind: Option<String>,
    pub preservation: Option<String>,
    pub confidence: Option<String>,
    pub grounding: bool,
}

pub fn observations() -> &'static Observations {
    static OBSERVED: OnceLock<
        gmeow_errors::Result<super::observation_reader::Selected<Observations>>,
    > = OnceLock::new();
    let observed = super::observation_reader::selected(
        &OBSERVED,
        "stage-conformance",
        "pipeline/grounding-catalog-observations.json",
    );
    let expected = [
        "slices/grounding/logic/mappings/grounding-bridges.ttl",
        "slices/grounding/logic/mappings/foundation-bridges.ttl",
        "slices/grounding/math/mappings/quantity-bridges.ttl",
        "slices/core/observations/mappings/equivalences.ttl",
        "slices/grounding/logic/tests/conformance-fixtures/grounding-bridge-wellformed.ttl",
        "slices/grounding/logic/tests/counter-examples/grounding-bridge-missing-preservation.ttl",
        "slices/grounding/logic/module.ttl",
        "slices/grounding/math/module.ttl",
        "imports/gufo.ttl",
        "imports/targets/bfo.ttl",
    ];
    assert_eq!(
        observed
            .sources
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        expected.into_iter().collect()
    );
    assert!(observed.sources.values().all(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())), "exact original source digests");
    observed
}

pub fn cells(path: &str) -> &'static [Cell] {
    observations()
        .catalogs
        .get(path)
        .unwrap_or_else(|| panic!("missing original catalog {path}"))
        .as_slice()
}

pub fn transpilation(path: &str) -> &'static Result<usize, gmeow_errors::RecordedDiag> {
    observations()
        .transpilation
        .get(path)
        .unwrap_or_else(|| panic!("missing original transpilation {path}"))
}
