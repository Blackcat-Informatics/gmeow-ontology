// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read the two produced GMEOW-specific validation reports without building data.

use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Deserialize)]
struct Observations {
    profile: String,
    sources: BTreeMap<String, String>,
    controls: BTreeMap<String, Control>,
}

#[derive(Deserialize)]
pub struct Control {
    input_digest: String,
    results: Vec<ResultMessage>,
}

#[derive(Deserialize)]
struct ResultMessage {
    severity: String,
    message: Option<String>,
}

impl Control {
    pub fn violations(&self) -> Vec<&str> {
        self.results
            .iter()
            .filter(|result| result.severity == "http://www.w3.org/ns/shacl#Violation")
            .map(|result| result.message.as_deref().unwrap_or_default())
            .collect()
    }
}

pub fn report(name: &str) -> &'static Control {
    static OBSERVED: OnceLock<
        gmeow_errors::Result<super::observation_reader::Selected<Observations>>,
    > = OnceLock::new();
    let observed = super::observation_reader::selected(
        &OBSERVED,
        "stage-validate",
        "pipeline/inference-validation-observations.json",
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
