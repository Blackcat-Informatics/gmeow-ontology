// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only extracted correspondence envelopes and original declaration inventories.

use std::collections::BTreeSet;
use std::sync::OnceLock;

pub use gmeow_validate::grounding_observation::{Cell, Observations};

pub fn observations() -> &'static Observations {
    static OBSERVED: OnceLock<
        gmeow_errors::Result<super::observation_reader::Selected<Observations>>,
    > = OnceLock::new();
    let observed = super::observation_reader::selected(
        &OBSERVED,
        "stage-conformance",
        "pipeline/grounding-catalog-observations.json",
        |bytes| purrdf_lex::json::record::from_slice(bytes).map_err(gmeow_errors::Diag::from),
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
