// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The OntoUML honest-gap gate.
//!
//! The `ontouml-mini-divergence` corpus holds valid OntoUML models that carry a
//! documented anti-pattern the five native foundation disciplines CANNOT reproduce.
//! The contract under test (NO-OPTIONALITY, HARD FAILS) is that such a model is an
//! honest gap — either an out-of-fragment **capability gap** (the lowerer returns
//! `Unsupported`) or a **coverage gap** (the model lowers cleanly and the disciplines
//! fire, but never the documented anti-pattern) — and is NEVER silently reproduced as
//! a wrong verdict. A malformed model (`Syntax`) is a corpus-authoring error, not a gap.
//!
//! These cases are source-only (`source/model.ttl` + `corpus.json`, no
//! `profile.json`/`input.nq`), so the consistency harness never runs them and the
//! external-soundness gate skips the `divergence` lane; this gate pins them instead.

use std::path::Path;

use gmeow_conformance::external::ontouml::{
    fired_disciplines, lower_and_evaluate, parse_ontouml_model, OntoumlError,
};
use gmeow_conformance::paths::cases_root;
use gmeow_logic::foundation::AntiRigidityPolicy;

fn divergence_root() -> std::path::PathBuf {
    cases_root()
        .join("external")
        .join("ontouml-mini-divergence")
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

/// The documented anti-pattern label the model header records
/// (`# documented-antipattern: <label> (...)`). The label is the first
/// whitespace-delimited token after the marker; the parenthetical gloss is dropped.
fn documented_antipattern(source: &str) -> Option<String> {
    for line in source.lines() {
        let line = line.trim_start_matches('#').trim();
        if let Some(rest) = line.strip_prefix("documented-antipattern:") {
            return rest.split_whitespace().next().map(str::to_owned);
        }
    }
    None
}

#[test]
fn ontouml_divergence_cases_are_honest_gaps_never_a_reproduced_verdict() {
    let root = divergence_root();
    assert!(
        root.is_dir(),
        "ontouml-mini-divergence corpus missing: {}",
        root.display()
    );

    let mut checked = 0usize;
    for case_dir in subdirs(&root) {
        let model_path = case_dir.join("source").join("model.ttl");
        assert!(
            model_path.is_file(),
            "{}: divergence case has no source/model.ttl",
            case_dir.display()
        );
        let text = std::fs::read_to_string(&model_path)
            .unwrap_or_else(|e| panic!("read {}: {e}", model_path.display()));

        // Every divergence case must declare the documented anti-pattern it abstracts
        // (provenance is not lost just because the fragment cannot reproduce it).
        let documented = documented_antipattern(&text).unwrap_or_else(|| {
            panic!(
                "{}: no `# documented-antipattern:` header",
                model_path.display()
            )
        });

        // A valid OntoUML model must PARSE (a malformed model is a corpus defect).
        let model = match parse_ontouml_model(&text, None) {
            Ok(m) => m,
            Err(OntoumlError::Syntax(m)) => panic!(
                "{}: malformed OntoUML, not an honest gap: {m}",
                model_path.display()
            ),
            Err(OntoumlError::Unsupported(_)) => {
                // A parse-level capability gap is already honest; nothing to reproduce.
                checked += 1;
                continue;
            }
        };

        let world = format!(
            "https://gmeow.example/ontouml-mini-divergence/{}/w",
            case_dir.file_name().and_then(|s| s.to_str()).unwrap_or("")
        );
        match lower_and_evaluate(&model, &world, AntiRigidityPolicy::WitnessObligation) {
            // A lowering capability gap (an out-of-fragment stereotype/construct) is honest.
            Err(OntoumlError::Unsupported(_)) => {}
            Err(OntoumlError::Syntax(m)) => panic!(
                "{}: lowering produced malformed output, not an honest gap: {m}",
                model_path.display()
            ),
            // A clean lowering is a COVERAGE gap only if the documented anti-pattern is
            // NOT among the fired disciplines. If a native discipline reproduced it, the
            // case is not a divergence and belongs in the Lane-A corpus instead.
            Ok((fq, _nq, _n)) => {
                let fired = fired_disciplines(&fq);
                assert!(
                    !fired.contains(&documented),
                    "{}: native disciplines REPRODUCED the documented anti-pattern \
                     {documented:?} (fired: {fired:?}) — this is not a divergence; move it \
                     to the Lane-A ontouml-mini corpus",
                    model_path.display()
                );
            }
        }
        checked += 1;
    }

    assert!(
        checked >= 2,
        "expected ≥2 ontouml-mini-divergence cases, found {checked}"
    );
}
