// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use gmeow_logic_compile::projections::correspondence::project_correspondence_dataset;

#[test]
fn fidelity_observation_rejects_an_altered_cell() {
    let expected = super::super::synthetic_affine_program();
    let projection = project_correspondence_dataset(&expected).unwrap();
    observe(&expected, &projection).unwrap();
    let mut changed = expected;
    changed.correspondences.clear();
    let mut artifacts = BTreeMap::new();
    record(&changed, &projection, &mut artifacts).unwrap();
    let result: Result<(), gmeow_errors::RecordedDiag> =
        serde_json::from_slice(&artifacts[CHANNEL]).unwrap();
    assert!(result.is_err());
}
