// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The projection axis's constraints component measures whether a slice has
//! expressed its validation as a PURE projection. Under Principle 17 a slice's
//! structural constraints (`owl:Restriction` / `owl:disjointWith`) are maximally
//! projected only when they live SOLELY as the `module.ttl` OWL/RDFS axioms
//! (which derive `generated/shapes/*` via `derive_validation_shapes`) with the
//! hand-authored `shapes.ttl` — a second source of truth — retired. These tests
//! pin the objective, falsifiable reading: a migrated slice (module.ttl axioms
//! present, `shapes.ttl` deleted) earns full constraints credit and raises no
//! penalty finding, while a slice still shipping `shapes.ttl` scores STRICTLY
//! LOWER (the credit is unearned until the second source is retired) and
//! additionally carries the debt finding. Migration is the only way to earn the
//! credit and can never lose it. The `links_out` / `no-mappings` half of the
//! axis is untouched by this fix and is pinned unaffected.

use std::path::PathBuf;

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};

/// A read-only snapshot of the projection axis result.
struct View {
    score: f64,
    codes: Vec<String>,
}

/// A throwaway slice directory with a `module.ttl` authoring an
/// `owl:disjointWith` constraint (so `has_constraints` is true), optionally a
/// hand-authored `shapes.ttl`, and optionally a triple linking a slice term to
/// an external (non-native) namespace (so `links_out` is true).
struct Fixture {
    dir: PathBuf,
}

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
     @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";

impl Fixture {
    fn new(name: &str, with_shapes_ttl: bool, with_external_link: bool) -> Self {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("gmeow-proj-{name}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // A real slice directory declares its identity in a manifest; the scorer reads
        // the fixture through the same `slice_files_from_dir` entry point production
        // uses, so the fixture ships one rather than being a manifest-less stand-in.
        std::fs::write(
            dir.join("manifest.ttl"),
            format!(
                "{PREFIXES}\n\
                 <https://blackcatinformatics.ca/gmeow/slices/fixture> a gmeow:Slice .\n"
            ),
        )
        .unwrap();

        let external = if with_external_link {
            "gmeow:Widget rdfs:seeAlso <http://example.org/external-vocab/Widget> .\n"
        } else {
            ""
        };
        std::fs::write(
            dir.join("module.ttl"),
            format!(
                "{PREFIXES}\n\
                 gmeow:Widget a owl:Class .\n\
                 gmeow:Gadget a owl:Class .\n\
                 gmeow:Widget rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> .\n\
                 gmeow:Gadget rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> .\n\
                 gmeow:Widget owl:disjointWith gmeow:Gadget .\n\
                 {external}"
            ),
        )
        .unwrap();

        if with_shapes_ttl {
            std::fs::write(
                dir.join("shapes.ttl"),
                "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
                 @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
                 gmeow:WidgetShape a sh:NodeShape ;\n\
                    sh:targetClass gmeow:Widget ;\n\
                    sh:not [ a sh:NodeShape ; sh:class gmeow:Gadget ] .\n",
            )
            .unwrap();
        }

        Self { dir }
    }

    fn score(&self) -> View {
        let module = self.dir.join("module.ttl");
        let ds = gmeow_slice_quality::dataset_from_paths(&[module.as_path()]).unwrap();
        let files = gmeow_slice_quality::report::slice_files_from_dir(&self.dir).unwrap();
        let ctx = ScoreContext::new(
            "https://blackcatinformatics.ca/gmeow/slices/fixture".to_owned(),
            &files,
            &ds,
            ScoringEnv::Repo {
                slice_dir: self.dir.clone(),
            },
        );
        let s = axes::resolve("projection_axis").unwrap()(&ctx);
        View {
            score: s.score,
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
fn migrated_slice_scores_full_constraints_credit_and_no_penalty_finding() {
    // module.ttl authors the disjointness axiom (the projecting source); no
    // hand-authored shapes.ttl at all — this is the desired, fully-migrated
    // end state and must score perfectly with no penalty finding of any kind.
    let f = Fixture::new("migrated", false, false);
    let r = f.score();
    assert_eq!(
        r.score, 1.0,
        "a migrated (axioms present, no shapes.ttl) slice scores full projection credit, got {} ({:?})",
        r.score, r.codes
    );
    assert!(
        !r.codes.iter().any(|c| c.contains("no-shapes")),
        "the no-shapes penalty must be fully retired, got {:?}",
        r.codes
    );
    assert!(
        !r.codes
            .iter()
            .any(|c| c == "slice-quality.projection.hand-authored-shapes"),
        "no shapes.ttl on disk → no debt finding, got {:?}",
        r.codes
    );
}

#[test]
fn unmigrated_slice_shipping_shapes_ttl_scores_strictly_lower_and_carries_debt_finding() {
    // Same constraints, but the slice still ships the hand-authored shapes.ttl:
    // its validation is NOT yet a pure projection, so the constraints credit is
    // unearned. This must raise the migration-debt advisory AND score strictly
    // lower than the migrated case — retiring shapes.ttl is the only way to earn
    // the credit, so keeping it is a real, measurable shortfall (not a no-op).
    let migrated = Fixture::new("baseline", false, false).score();
    let unmigrated = Fixture::new("debt", true, false).score();

    assert_eq!(
        migrated.score, 1.0,
        "the migrated slice earns full projection credit, got {}",
        migrated.score
    );
    assert_eq!(
        unmigrated.score, 0.0,
        "a constraint-bearing slice still shipping shapes.ttl has not projected its validation \
         (1 obligation, 0 met), got {} ({:?})",
        unmigrated.score, unmigrated.codes
    );
    assert!(
        unmigrated.score < migrated.score,
        "shipping shapes.ttl must score STRICTLY lower than the migrated state (the axis rewards \
         migration, never the second source of truth): {} vs {}",
        unmigrated.score,
        migrated.score
    );
    assert!(
        unmigrated
            .codes
            .iter()
            .any(|c| c == "slice-quality.projection.hand-authored-shapes"),
        "a slice still shipping shapes.ttl must carry the migration-debt finding, got {:?}",
        unmigrated.codes
    );
}

#[test]
fn no_mappings_path_is_unaffected_by_the_constraints_fix() {
    // Constraints present (full credit) AND the slice links out to an external
    // namespace without shipping mappings/equivalences.ttl: the links_out half
    // of the axis must still flag no-mappings and still drag the score down —
    // this fix must not touch that branch at all.
    let f = Fixture::new("links-out", false, true);
    let r = f.score();
    assert_eq!(
        r.score, 0.5,
        "constraints component full credit (1/1) + links_out unmet (0/1) → 1/2, got {} ({:?})",
        r.score, r.codes
    );
    assert!(
        r.codes
            .iter()
            .any(|c| c == "slice-quality.projection.no-mappings"),
        "the links_out branch still flags no-mappings, got {:?}",
        r.codes
    );
    assert!(
        !r.codes.iter().any(|c| c.contains("no-shapes")),
        "no-shapes must never re-appear, got {:?}",
        r.codes
    );
}
