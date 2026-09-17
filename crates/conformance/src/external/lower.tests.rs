// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn runner_verdict_reflects_the_mapped_status() {
    let v = runner_verdict_json("https://w/x", 4, ExternalOutcome::Inconsistent);
    assert_eq!(v["https://w/x"]["status"], "inconsistent");
    assert_eq!(v["https://w/x"]["quads"], 4);

    let v = runner_verdict_json("https://w/x", 2, ExternalOutcome::Consistent);
    assert_eq!(v["https://w/x"]["status"], "consistent");

    let v = runner_verdict_json("https://w/x", 0, ExternalOutcome::Incomplete);
    assert_eq!(v["https://w/x"]["status"], "incomplete");
}
