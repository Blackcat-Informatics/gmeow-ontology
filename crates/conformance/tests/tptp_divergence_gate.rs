// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The TPTP honest-capability-gap gate.
//!
//! The `tptp-mini-divergence` corpus holds problems that are genuine first-order
//! theorems whose refutation the native EL/DL fragment cannot express. The
//! contract under test (LOW/NO-OPTIONALITY, HARD FAILS) is that such a problem
//! surfaces as an honest **capability gap** — the parser or the FOL lowerer
//! returns an error — and is NEVER silently decided (a wrong `consistent`) nor
//! quietly downgraded to `incomplete`.
//!
//! These cases are source-only (`source/problem.p` + `corpus.json`, no
//! `profile.json`/`input.nq`), so the consistency harness never runs them and the
//! external-soundness gate skips the `divergence` lane; this gate pins them
//! instead — it is where their honest-gap status is enforced.

use std::path::Path;

use gmeow_conformance::external::tptp::{TptpError, lower_and_decide, parse_tptp};
use gmeow_conformance::external::{outcome_from_szs, parse_szs_status};
use gmeow_conformance::paths::cases_root;

fn divergence_root() -> std::path::PathBuf {
    cases_root().join("external").join("tptp-mini-divergence")
}

fn subdirs(dir: &Path) -> Vec<std::path::PathBuf> {
    let mut v: Vec<_> = std::fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .map(|e| e.expect("dir entry").path())
        .filter(|p| p.is_dir())
        .collect();
    v.sort();
    v
}

#[test]
fn tptp_divergence_cases_are_honest_capability_gaps_never_a_wrong_verdict() {
    let root = divergence_root();
    assert!(
        root.is_dir(),
        "tptp-mini-divergence corpus missing: {}",
        root.display()
    );

    let mut checked = 0usize;
    for case_dir in subdirs(&root) {
        let problem = case_dir.join("source").join("problem.p");
        assert!(
            problem.is_file(),
            "{}: divergence case has no source/problem.p",
            case_dir.display()
        );
        let text = std::fs::read_to_string(&problem)
            .unwrap_or_else(|e| panic!("read {}: {e}", problem.display()));

        // The problem still carries a real SZS ground truth (provenance is not lost
        // just because we cannot decide it).
        let raw = parse_szs_status(&text)
            .unwrap_or_else(|e| panic!("{}: SZS token: {e}", problem.display()));
        outcome_from_szs(&text)
            .unwrap_or_else(|e| panic!("{}: SZS outcome: {e}", problem.display()));

        // The native path MUST refuse to decide it — an honest gap, either at parse
        // (Unsupported) or at lowering/decision (LoweringGap). A malformed source
        // (TptpError::Syntax) is a corpus-authoring error, not a capability gap.
        let world = format!(
            "https://gmeow.example/tptp-mini-divergence/{}/w",
            case_dir.file_name().and_then(|s| s.to_str()).unwrap_or("")
        );
        match parse_tptp(&text) {
            Err(TptpError::Syntax(m)) => {
                panic!(
                    "{}: malformed TPTP, not a capability gap: {m}",
                    problem.display()
                )
            }
            Err(TptpError::Unsupported(_)) => { /* honest parse-level gap */ }
            Ok(formulas) => {
                let decided = lower_and_decide(&formulas, &world);
                assert!(
                    decided.is_err(),
                    "{}: native engine DECIDED a divergence problem (SZS {raw}) as {:?} — it must \
                     be an honest capability gap, never a decided verdict",
                    problem.display(),
                    decided.map(|(o, _)| o)
                );
            }
        }
        checked += 1;
    }

    assert!(
        checked >= 1,
        "expected ≥1 tptp-mini-divergence case, found {checked}"
    );
}
