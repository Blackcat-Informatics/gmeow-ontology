// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The reasoner axis's NEGATIVE-space half: a slice's counter-example fixtures pin
//! DL-provable negative space. A counter-example whose SHACL shape declares a
//! class-disjointness (`sh:not [ sh:class B ]` on a shape targeting `A`) MUST, when
//! reasoned with the module, force a DL clash — because that SHACL disjointness is a
//! lossy projection of the canonical `A owl:disjointWith B` (Principle 17). A
//! co-typed counter-example the native reasoner finds CONSISTENT is a silent hole:
//! the negative space lives only in the SHACL projection, not the logic core.
//!
//! These tests use the REAL slice convention discovered under `slices/`:
//! `gmeow:ExampleConformance` cells with `gmeow:expectedOutcome gmeow:violates` +
//! `gmeow:exampleFile` (e.g. `slices/grounding/lang/tests/example-conformance.ttl`
//! → `tests/counter-examples/meaning-act-observation-conflation.ttl`, whose
//! `lang:ActObservationDisjointShape` mirrors `lang:InterpretationAct owl:disjointWith
//! gmeow:Observation`).

use std::path::PathBuf;

use gmeow_slice_quality::reasoner::reasoner_axis;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};

/// A read-only snapshot of the reasoner axis result.
struct View {
    score: f64,
    messages: Vec<String>,
    codes: Vec<String>,
}

/// A throwaway slice directory: `module.ttl`, `shapes.ttl`, an
/// `tests/example-conformance.ttl` binding one counter-example, and the
/// counter-example fixture itself under `tests/counter-examples/`.
struct Fixture {
    dir: PathBuf,
    conformance: PathBuf,
    fixture: PathBuf,
}

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
     @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
     @prefix sh: <http://www.w3.org/ns/shacl#> .\n\
     @prefix ex: <https://blackcatinformatics.ca/gmeow/examples/fixture/> .\n";

impl Fixture {
    /// Build a slice with a `sh:not [ sh:class Observation ]` disjointness shape over
    /// `InterpretationAct` and a counter-example co-typing `ex:act` as both. When
    /// `with_axiom` is set the module also authors `owl:disjointWith` (the backing
    /// canonical logic); otherwise the negative space is SHACL-only (a silent hole).
    fn new(name: &str, with_axiom: bool) -> Self {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("gmeow-ce-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(dir.join("tests/counter-examples")).unwrap();
        // A real slice directory declares its identity in a manifest; the fixture is read
        // through the same `slice_files_from_dir` entry point production uses.
        std::fs::write(
            dir.join("manifest.ttl"),
            format!(
                "{PREFIXES}\n\
                 <https://blackcatinformatics.ca/gmeow/slices/fixture> a gmeow:Slice .\n"
            ),
        )
        .unwrap();

        let disjoint = if with_axiom {
            "gmeow:InterpretationAct owl:disjointWith gmeow:Observation .\n"
        } else {
            ""
        };
        std::fs::write(
            dir.join("module.ttl"),
            format!(
                "{PREFIXES}\n\
                 gmeow:InterpretationAct a owl:Class .\n\
                 gmeow:Observation a owl:Class .\n\
                 gmeow:InterpretationAct rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> .\n\
                 gmeow:Observation rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> .\n\
                 {disjoint}"
            ),
        )
        .unwrap();

        // The SHACL disjointness projection: an InterpretationAct is never an Observation.
        std::fs::write(
            dir.join("shapes.ttl"),
            format!(
                "{PREFIXES}\n\
                 gmeow:ActObservationDisjointShape a sh:NodeShape ;\n\
                    sh:targetClass gmeow:InterpretationAct ;\n\
                    sh:not [ a sh:NodeShape ; sh:class gmeow:Observation ] .\n"
            ),
        )
        .unwrap();

        // The counter-example fixture: co-types ex:act as both disjoint classes.
        let fixture = dir.join("tests/counter-examples/conflation.ttl");
        std::fs::write(
            &fixture,
            format!("{PREFIXES}\nex:act a gmeow:InterpretationAct , gmeow:Observation .\n"),
        )
        .unwrap();

        // The discovered convention: an ExampleConformance cell pinning it as a violation.
        let conformance = dir.join("tests/example-conformance.ttl");
        std::fs::write(
            &conformance,
            format!(
                "{PREFIXES}\n\
                 ex:ecConflation a gmeow:ExampleConformance ;\n\
                    gmeow:exampleFile \"tests/counter-examples/conflation.ttl\" ;\n\
                    gmeow:expectedOutcome gmeow:violates ;\n\
                    gmeow:expectedViolationCode \"shacl.NotConstraintComponent\" .\n"
            ),
        )
        .unwrap();

        Self {
            dir,
            conformance,
            fixture,
        }
    }

    fn score(&self) -> View {
        // Mirror the sweep's slice graph: module.ttl + tests/*.ttl merged.
        let module = self.dir.join("module.ttl");
        let ds = gmeow_slice_quality::dataset_from_paths(&[
            module.as_path(),
            self.conformance.as_path(),
            self.fixture.as_path(),
        ])
        .unwrap();
        let files = gmeow_slice_quality::report::slice_files_from_dir(&self.dir).unwrap();
        let ctx = ScoreContext::new(
            "https://blackcatinformatics.ca/gmeow/slices/fixture".to_owned(),
            &files,
            &ds,
            ScoringEnv::Repo {
                slice_dir: self.dir.clone(),
            },
        );
        let s = reasoner_axis(&ctx);
        View {
            score: s.score,
            messages: s.findings.iter().map(|f| f.message.clone()).collect(),
            codes: s.findings.iter().map(|f| f.code.clone()).collect(),
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn counterexample_that_clashes_is_credited_no_advisory() {
    // Module authors owl:disjointWith backing the SHACL shape → co-typed ex:act
    // clashes → the logical counter-example is met, and the axis is perfect.
    let f = Fixture::new("clash", true);
    let r = f.score();
    assert_eq!(
        r.score, 1.0,
        "a counter-example that genuinely clashes meets its reasoner obligation → 1.0, got {} ({:?})",
        r.score, r.messages
    );
    assert!(
        !r.codes
            .iter()
            .any(|c| c == "slice-quality.reasoner.counterexample-no-clash"),
        "a clashing counter-example raises no silent-hole advisory, got {:?}",
        r.messages
    );
}

#[test]
fn counterexample_that_fails_to_clash_is_reported_and_drops_score() {
    // Same SHACL disjointness shape + same co-typed counter-example, but the module
    // authors NO owl:disjointWith → the reasoner finds module + fixture CONSISTENT →
    // a silent hole: reported, named, and the axis drops below perfect.
    let f = Fixture::new("hole", false);
    let r = f.score();
    assert!(
        r.score < 1.0,
        "a counter-example that fails to clash drops the reasoner axis below 1.0, got {}",
        r.score
    );
    assert_eq!(
        r.score, 0.0,
        "no authored axioms and one non-clashing logical counter-example → 0/1"
    );
    let hole = r
        .messages
        .iter()
        .find(|m| m.contains("conflation.ttl"))
        .expect("the silent-hole advisory names the counter-example fixture");
    assert!(
        hole.contains("InterpretationAct") && hole.contains("Observation"),
        "the advisory names the co-typed disjoint pair, got {hole}"
    );
    assert!(
        r.codes
            .iter()
            .any(|c| c == "slice-quality.reasoner.counterexample-no-clash"),
        "the silent hole uses the dedicated code, got {:?}",
        r.codes
    );
}

/// A slice that authors the disjointness axiom BUT drops the SHACL projection shape
/// still clashes; conversely, this test pins the discovery contract: without a
/// SHACL-declared disjoint pair, a counter-example is NOT in the logical population
/// (its structural violation has no DL analogue), so it neither credits nor holes.
#[test]
fn structural_only_counterexample_is_not_a_logical_obligation() {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!(
        "gmeow-ce-structural-{}-{nanos}",
        std::process::id()
    ));
    std::fs::create_dir_all(dir.join("tests/counter-examples")).unwrap();
    // A real slice directory declares its identity in a manifest; the fixture is read
    // through the same `slice_files_from_dir` entry point production uses.
    std::fs::write(
        dir.join("manifest.ttl"),
        format!(
            "{PREFIXES}\n\
             <https://blackcatinformatics.ca/gmeow/slices/fixture> a gmeow:Slice .\n"
        ),
    )
    .unwrap();
    // No shapes.ttl at all → no declared class-disjointness.
    std::fs::write(
        dir.join("module.ttl"),
        format!(
            "{PREFIXES}\ngmeow:Widget a owl:Class ; rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> .\n"
        ),
    )
    .unwrap();
    let fixture = dir.join("tests/counter-examples/missing-field.ttl");
    std::fs::write(&fixture, format!("{PREFIXES}\nex:w a gmeow:Widget .\n")).unwrap();
    let conformance = dir.join("tests/example-conformance.ttl");
    std::fs::write(
        &conformance,
        format!(
            "{PREFIXES}\n\
             ex:ecMissing a gmeow:ExampleConformance ;\n\
                gmeow:exampleFile \"tests/counter-examples/missing-field.ttl\" ;\n\
                gmeow:expectedOutcome gmeow:violates ;\n\
                gmeow:expectedViolationCode \"shacl.MinCountConstraintComponent\" .\n"
        ),
    )
    .unwrap();

    let ds = gmeow_slice_quality::dataset_from_paths(&[
        dir.join("module.ttl").as_path(),
        conformance.as_path(),
        fixture.as_path(),
    ])
    .unwrap();
    let files = gmeow_slice_quality::report::slice_files_from_dir(&dir).unwrap();
    let ctx = ScoreContext::new(
        "https://blackcatinformatics.ca/gmeow/slices/fixture".to_owned(),
        &files,
        &ds,
        ScoringEnv::Repo {
            slice_dir: dir.clone(),
        },
    );
    let s = reasoner_axis(&ctx);

    assert!(
        !s.findings
            .iter()
            .any(|f| f.code == "slice-quality.reasoner.counterexample-no-clash"),
        "a structural (minCount) counter-example is not a logical obligation → no hole advisory"
    );
    // No axioms, no logical counter-examples → vacuously perfect with the informational note.
    assert_eq!(s.score, 1.0, "no reasoner obligations → vacuous 1.0");
    assert!(
        s.findings
            .iter()
            .any(|f| f.code == "slice-quality.reasoner.no-obligations"),
        "the vacuity is explicit, got {:?}",
        s.findings.iter().map(|f| &f.code).collect::<Vec<_>>()
    );

    let _ = std::fs::remove_dir_all(&dir);
}
