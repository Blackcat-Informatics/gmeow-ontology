// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read the two produced GMEOW-specific validation reports without building data.

use gmeow_validate::inference_observation::{Control, Observations};
use std::collections::BTreeSet;
use std::sync::OnceLock;

pub fn report(name: &str) -> &'static Control {
    static OBSERVED: OnceLock<
        gmeow_errors::Result<super::observation_reader::Selected<Observations>>,
    > = OnceLock::new();
    let observed = super::observation_reader::selected(
        &OBSERVED,
        "stage-validate",
        "pipeline/inference-validation-observations.json",
        |bytes| purrdf_lex::json::record::from_slice(bytes).map_err(gmeow_errors::Diag::from),
    );
    assert_eq!(
        observed.profile,
        "inference-slice-plus-constraint-shapes:flattened-default:v1"
    );
    assert_eq!(
        observed
            .sources
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([
            "slices/core/inference/module.ttl",
            "slices/core/inference/shapes.ttl",
            "generated/shapes/constraint-shapes.ttl",
        ])
    );
    assert!(observed.sources.values().all(|digest| digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())));
    assert_eq!(
        observed
            .controls
            .keys()
            .map(String::as_str)
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["wellformed", "malformed"])
    );
    let control = &observed.controls[name];
    assert_eq!(
        control.input_digest.len(),
        64,
        "exact selected control identity"
    );
    control
}
