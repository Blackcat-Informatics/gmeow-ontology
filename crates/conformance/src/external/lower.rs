// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lowering an external problem's declared outcome into the runner verdict shape (#753).
//!
//! [`runner_verdict_json`] is the AC1 deliverable: given an external problem's
//! declared [`ExternalOutcome`], produce the runner's `verdicts.json` value (the
//! same world-indexed shape the engine emits). This is "the runner ingests a
//! manifest / SZS problem and produces a runner verdict".
//!
//! Lowering an external *source* into the on-disk case anatomy (`input.nq`) is NOT
//! done here: for the TPTP/W3C entailment seeds it requires the general FOL-negation
//! reduction that is explicitly X2/X3 scope (#754–#755). The seed `input.nq` files
//! are authored by hand with the negated conclusion pre-baked; `runner_verdict_json`
//! is the pure surface the `ingest-external` binary uses to re-derive the verdict.

use std::collections::BTreeMap;

use crate::external::status::ExternalOutcome;

/// Build the runner's `verdicts.json` value for a single-world external problem.
///
/// `{ world_iri: { quads, status } }` — the same shape the engine emits, with the
/// status taken from the external declaration. `quads` is the EDB quad count in the
/// world (so the value matches the engine's blessed output for a decided case).
pub fn runner_verdict_json(
    world_iri: &str,
    quads: u64,
    outcome: ExternalOutcome,
) -> serde_json::Value {
    let mut counts: BTreeMap<String, u64> = BTreeMap::new();
    counts.insert(world_iri.to_string(), quads);
    crate::serialize::build_verdicts(&counts, |_| outcome.verdict_status())
}

#[cfg(test)]
mod tests {
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
}
