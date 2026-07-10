// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The projection axis's constraints component must never punish migration. Under
//! Principle 17 the PROJECTING source for a slice's structural constraints
//! (`owl:Restriction` / `owl:disjointWith`) is the `module.ttl` OWL/RDFS axioms
//! themselves — they derive `generated/shapes/*` via
//! `derive_validation_shapes` — never a hand-authored `shapes.ttl`, which is a
//! second source of truth and migration debt (Principle 17). These tests pin
//! the objective reading: a slice that migrates (module.ttl axioms present,
//! `shapes.ttl` deleted) scores full constraints credit and raises no penalty
//! finding, while a slice that still ships `shapes.ttl` scores no higher and
//! additionally carries the debt finding. The `links_out` / `no-mappings` half
//! of the axis is untouched by this fix and is pinned unaffected.

use std::path::PathBuf;

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::ScoreContext;

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
        let ctx = ScoreContext::new(
            "https://blackcatinformatics.ca/gmeow/slices/fixture".to_owned(),
            self.dir.clone(),
            &ds,
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
fn unmigrated_slice_shipping_shapes_ttl_scores_no_higher_and_carries_debt_finding() {
    // Same constraints, but the slice still ships the hand-authored shapes.ttl:
    // this must raise the migration-debt advisory and must NOT score any higher
    // than the migrated case above (it may only ever score the same or lower).
    let migrated = Fixture::new("baseline", false, false).score();
    let unmigrated = Fixture::new("debt", true, false).score();

    assert!(
        unmigrated.score <= migrated.score,
        "shipping shapes.ttl must never score higher than the migrated state: {} vs {}",
        unmigrated.score,
        migrated.score
    );
    assert_eq!(
        unmigrated.score, migrated.score,
        "the constraints component is credited from the module.ttl axioms alone, so keeping \
         shapes.ttl neither helps nor hurts the score, got {} vs {}",
        unmigrated.score, migrated.score
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
