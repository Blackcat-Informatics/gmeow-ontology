// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The linkage axis measures correspondence-calculus ADOPTION: of the identity-strength
//! correspondences a slice authors, the fraction routed through the calculus
//! (`gmeow:ProjectionMapping` mnemomorphic `=` cells / `logic:Correspondence` lenses /
//! validated grounding frontends) rather than hand-authored (`gmeow:TermEquivalence` rows /
//! hand-authored `dc:` alignments). These
//! tests pin the metric on purpose-built slice fixtures: a fully-calculus slice scores 1.0, a
//! hand-authored `dc:` slice scores below 1.0 with an advisory naming the record, and a slice
//! with no calculus-eligible surface is vacuous (a neutral 1.0 that says so).

use std::path::PathBuf;

use gmeow_slice_quality::axes;
use gmeow_slice_quality::score::{ScoreContext, ScoringEnv};

/// A read-only snapshot of an [`axes`] result, decoupled from the internal finding shape.
struct AxisScoreView {
    score: f64,
    messages: Vec<String>,
    codes: Vec<String>,
}

/// A throwaway slice directory: a `module.ttl` (so the surface is discoverable) plus a
/// `mappings/equivalences.ttl` carrying the correspondence records under test.
struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str, mappings: &str) -> Self {
        let mut dir = std::env::temp_dir();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!(
            "gmeow-linkage-{name}-{}-{nanos}",
            std::process::id()
        ));
        std::fs::create_dir_all(dir.join("mappings")).unwrap();
        // A real slice directory declares its identity in a manifest; the fixture is read
        // through the same `slice_files_from_dir` entry point production uses.
        std::fs::write(
            dir.join("manifest.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             <https://blackcatinformatics.ca/gmeow/slices/fixture> a gmeow:Slice .\n",
        )
        .unwrap();
        // A minimal explicitly-owned term so ScoreContext::new has a term set.
        std::fs::write(
            dir.join("module.ttl"),
            "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
             @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
             gmeow:Thing a owl:Class ;\n\
                rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/fixture> .\n",
        )
        .unwrap();
        std::fs::write(dir.join("mappings/equivalences.ttl"), mappings).unwrap();
        Self { dir }
    }

    fn score(&self) -> AxisScoreView {
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
        let primitive = axes::resolve("linkage_axis").unwrap();
        let s = primitive(&ctx);
        AxisScoreView {
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

const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
     @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
     @prefix schema: <https://schema.org/> .\n\
     @prefix dc: <http://purl.org/dc/elements/1.1/> .\n\
     @prefix semapv: <https://w3id.org/semapv/vocab/> .\n";

#[test]
fn fully_calculus_slice_scores_maximal() {
    // Two mnemomorphic `=` ProjectionMapping cells, no hand-authored records → adoption 1.0.
    let mappings = format!(
        "{PREFIXES}\n\
         gmeow:mapThing a gmeow:ProjectionMapping ;\n\
            gmeow:hasBinding [ gmeow:profile \"schema\" ; gmeow:toClass schema:Thing ; gmeow:relation \"=\" ; gmeow:mnemomorphic true ] .\n\
         gmeow:mapName a gmeow:ProjectionMapping ;\n\
            gmeow:hasBinding [ gmeow:profile \"schema\" ; gmeow:toPredicate schema:name ; gmeow:relation \"=\" ; gmeow:mnemomorphic true ] .\n"
    );
    let f = Fixture::new("full", &mappings);
    let r = f.score();
    assert_eq!(
        r.score, 1.0,
        "all correspondences routed through the calculus → 1.0"
    );
    assert!(
        r.messages.is_empty(),
        "a fully-calculus slice carries no migration advisory, got {:?}",
        r.messages
    );
}

#[test]
fn hand_authored_dc_alignment_scores_below_one_and_is_named() {
    // A hand-authored dc: (DCMI-elements-1.1) alignment the dumb-down calculus should derive
    // — flagged via the real lint_dc_refinement `dc-hand-authored` path. No calculus record →
    // adoption 0.0, and the advisory names the record IRI.
    let mappings = format!(
        "{PREFIXES}\n\
         gmeow:title skos:closeMatch dc:title {{| gmeow:sssomFile \"fixture.sssom.tsv\" ;\n\
            gmeow:justification semapv:ManualMappingCuration |}} .\n"
    );
    let f = Fixture::new("dc", &mappings);
    let r = f.score();
    assert!(
        r.score < 1.0,
        "a hand-authored dc: alignment is unadopted → below 1.0, got {}",
        r.score
    );
    assert_eq!(
        r.score, 0.0,
        "no calculus records, one hand-authored dc: record → 0/1"
    );
    assert!(
        r.messages.iter().any(|m| m.contains("title")),
        "an advisory names the hand-authored record as a migration target, got {:?}",
        r.messages
    );
    assert!(
        r.codes
            .iter()
            .any(|c| c == "slice-quality.linkage.uncalculated-correspondence"),
        "the advisory uses the migration-target code"
    );
}

#[test]
fn identity_hand_authored_term_equivalence_is_a_migration_target() {
    // An identity-strength (owl:equivalentClass) TermEquivalence authored by hand → a real
    // migration target; a fully-calculus slice must outscore it (adoption is comparative).
    let hand = format!(
        "{PREFIXES}\n\
         gmeow:Thing owl:equivalentClass schema:Thing {{| gmeow:sssomFile \"fixture.sssom.tsv\" ;\n\
            gmeow:justification semapv:ManualMappingCuration |}} .\n"
    );
    let hand_f = Fixture::new("hand", &hand);
    let hand_r = hand_f.score();
    assert_eq!(
        hand_r.score, 0.0,
        "one hand-authored identity record, no calculus → 0.0"
    );
    assert!(
        hand_r.messages.iter().any(|m| m.contains("Thing")),
        "the identity-strength hand-authored record is named, got {:?}",
        hand_r.messages
    );

    // A mixed slice: one calculus cell + one hand-authored identity record → 0.5, strictly
    // above the fully-hand-authored slice.
    let mixed = format!(
        "{PREFIXES}\n\
         gmeow:mapThing a gmeow:ProjectionMapping ;\n\
            gmeow:hasBinding [ gmeow:toClass schema:Thing ; gmeow:relation \"=\" ; gmeow:mnemomorphic true ] .\n\
         gmeow:name skos:exactMatch schema:name {{| gmeow:sssomFile \"fixture.sssom.tsv\" ;\n\
            gmeow:justification semapv:ManualMappingCuration |}} .\n"
    );
    let mixed_f = Fixture::new("mixed", &mixed);
    let mixed_r = mixed_f.score();
    assert_eq!(
        mixed_r.score, 0.5,
        "one of two identity correspondences is calculus-routed → 0.5"
    );
    assert!(
        mixed_r.score > hand_r.score,
        "a partly-calculus slice outscores a fully-hand-authored one"
    );
}

#[test]
fn validated_grounding_term_equivalence_is_calculus_routed() {
    let mappings = format!(
        "{PREFIXES}\n\
         gmeow:Thing owl:equivalentClass schema:Thing {{| a logic:GroundingCorrespondence ;\n\
            gmeow:sssomFile \"grounding.sssom.tsv\" ;\n\
            gmeow:justification semapv:ManualMappingCuration ;\n\
            logic:sourceEndpoint gmeow:Thing ; logic:targetEndpoint schema:Thing ;\n\
            logic:morphismClass logic:WellBehavedLens ;\n\
            logic:morphismKind logic:InstitutionMorphism ;\n\
            logic:preservationKind logic:SoundUnderApproximation |}} .\n"
    );
    let f = Fixture::new("grounding", &mappings);
    let r = f.score();
    assert_eq!(
        r.score, 1.0,
        "a complete grounding frontend is calculus-routed"
    );
    assert!(
        r.messages.is_empty(),
        "no migration debt remains: {:?}",
        r.messages
    );
}

#[test]
fn malformed_grounding_marker_remains_legacy_debt() {
    let mappings = format!(
        "{PREFIXES}\n\
         gmeow:Thing owl:equivalentClass schema:Thing {{| a logic:GroundingCorrespondence ;\n\
            gmeow:sssomFile \"fixture.sssom.tsv\" ;\n\
            gmeow:justification semapv:ManualMappingCuration |}} .\n"
    );
    let f = Fixture::new("malformed-grounding", &mappings);
    let r = f.score();
    assert_eq!(
        r.score, 0.0,
        "a type marker alone grants no calculus credit"
    );
    assert!(
        r.messages.iter().any(|message| message.contains("Thing")),
        "the incomplete cell remains named debt: {:?}",
        r.messages
    );
}

#[test]
fn no_calculus_eligible_surface_is_vacuous_but_says_so() {
    // Only a lossy closeMatch to a non-dc external term — the calculus cannot carry it, so the
    // population is empty. The axis is not applicable: a neutral 1.0 that CARRIES an advisory
    // (never a silent "fully linked").
    let mappings = format!(
        "{PREFIXES}\n\
         gmeow:Thing skos:closeMatch schema:Thing {{| gmeow:sssomFile \"fixture.sssom.tsv\" ;\n\
            gmeow:justification semapv:ManualMappingCuration |}} .\n"
    );
    let f = Fixture::new("vacuous", &mappings);
    let r = f.score();
    assert_eq!(
        r.score, 1.0,
        "no calculus-eligible correspondence → neutral vacuity score"
    );
    assert!(
        r.codes
            .iter()
            .any(|c| c == "slice-quality.linkage.no-calculus-eligible-correspondence"),
        "the vacuity is explicit, not silent, got {:?}",
        r.codes
    );
}
