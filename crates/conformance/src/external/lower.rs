// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Lowering external problems into the conformance per-case anatomy (#753).
//!
//! Two pure surfaces:
//!
//! * [`runner_verdict_json`] — the AC1 deliverable: given an external problem's
//!   declared [`ExternalOutcome`], produce the runner's `verdicts.json` value (the
//!   same world-indexed shape the engine emits). This is "the runner ingests a
//!   manifest / SZS problem and produces a runner verdict".
//! * [`lower_consistency_inputs`] — scaffold the INPUT files of a `verdict_mode =
//!   consistency` case from a world-scoped RDF EDB. The *expected* verdict is then
//!   produced by blessing the real engine; the soundness gate cross-checks that
//!   engine verdict against [`runner_verdict_json`] (the external ground truth).
//!
//! The split keeps `lower_consistency_inputs` a pure value transform (no filesystem,
//! no license policy — those are the binary's / Task-3's job), so it is unit-testable
//! without a tempdir.

use std::collections::BTreeMap;

use crate::external::status::ExternalOutcome;

/// The lowered INPUT anatomy of a consistency case (no `expected/` tree — that is
/// blessed from the engine). Filenames are fixed by the per-case anatomy contract.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoweredInputs {
    /// `input.logic.ttl` — a minimal honest stub (consistency mode does not compile it).
    pub input_logic_ttl: String,
    /// `input.nq` — the world-scoped RDF EDB the native DL path decides.
    pub input_nq: String,
    /// `profile.json` — declares `verdict_mode = consistency`.
    pub profile_json: String,
}

/// The `input.logic.ttl` stub written for a consistency case.
///
/// Present only to satisfy the per-case anatomy (discovery requires it); the
/// consistency run path never compiles it — the OWL EDB lives in `input.nq`.
fn consistency_logic_stub() -> String {
    "\
# SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>\n\
# SPDX-License-Identifier: CC-BY-4.0\n\
#\n\
# verdict_mode=consistency external case (#753). The OWL EDB is the world-scoped\n\
# N-Quads in input.nq, decided by the native DL consistency path. This file exists\n\
# only to satisfy the per-case anatomy; it is NOT compiled in consistency mode.\n\
@prefix logic: <https://blackcatinformatics.ca/logic/> .\n"
        .to_string()
}

/// The `profile.json` written for a consistency case.
fn consistency_profile_json() -> String {
    "{\n  \"verdict_mode\": \"consistency\",\n  \"mode\": \"native\"\n}\n".to_string()
}

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

/// Lower a world-scoped RDF EDB into the consistency-case input anatomy.
///
/// `edb_nquads` is copied verbatim as `input.nq`; the stub and consistency profile
/// are fixed. Pure: no filesystem, no license check.
pub fn lower_consistency_inputs(edb_nquads: &str) -> LoweredInputs {
    let input_nq = if edb_nquads.ends_with('\n') || edb_nquads.is_empty() {
        edb_nquads.to_string()
    } else {
        format!("{edb_nquads}\n")
    };
    LoweredInputs {
        input_logic_ttl: consistency_logic_stub(),
        input_nq,
        profile_json: consistency_profile_json(),
    }
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

    #[test]
    fn lower_consistency_inputs_scaffolds_anatomy() {
        let edb = "<https://w/s> <https://w/p> <https://w/o> <https://w/g> .";
        let lowered = lower_consistency_inputs(edb);
        assert!(lowered.input_nq.ends_with(".\n"));
        assert!(lowered
            .profile_json
            .contains("\"verdict_mode\": \"consistency\""));
        assert!(lowered.input_logic_ttl.contains("verdict_mode=consistency"));
    }

    #[test]
    fn lower_preserves_already_newline_terminated_edb() {
        let edb = "<https://w/s> <https://w/p> <https://w/o> <https://w/g> .\n";
        assert_eq!(lower_consistency_inputs(edb).input_nq, edb);
    }
}
