// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::model::{
    DocCompetency, DocExample, DocFixture, DocFixtureKind, DocLinkage, DocLossTarget,
    DocTermCategory,
};

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

/// The [`DIMENSIONS`] indices of the four applicability-conditioned dimensions,
/// pinned so the tests below fail loudly if the stable order ever shifts.
const IDX_ALIGNMENT: usize = 5;
const IDX_LOSS_LEDGER_ROW: usize = 9;
const IDX_LINKAGE_COVERAGE: usize = 10;
const IDX_LOSS_JUDGMENT_SOUND: usize = 16;

fn bare(local: &str) -> DocTerm {
    DocTerm {
        iri: format!("{GMEOW}{local}"),
        curie: format!("gmeow:{local}"),
        category: DocTermCategory::Class,
        owner_slice: format!("{GMEOW}slice/zoo"),
        ..Default::default()
    }
}

fn dim_index(dim: Dimension) -> usize {
    DIMENSIONS
        .iter()
        .position(|d| d.dimension == dim)
        .expect("per-term dimension is in DIMENSIONS")
}

#[test]
fn empty_model_bare_term_covers_only_vacuous_dimensions() {
    let model = DocsModel::default();
    let ctx = CoverageContext::new(&model);
    let term = bare("Cat");
    let cov = term_coverage(&term, &ctx);
    // A bare term with no annotations: definition/label/... all absent. The
    // three vacuously-true dimensions with no configured universe are translation
    // (no non-English langs), provenance honesty (no rationale to be dishonest),
    // and loss-judgment-sound (no loss rows to be unsound).
    assert!(!cov.has_definition);
    assert!(
        cov.has_translation_coverage,
        "vacuous with no non-English langs"
    );
    assert!(cov.has_provenance_honesty, "vacuous with no rationale");
    assert!(cov.has_loss_judgment_sound, "vacuous with no loss rows");
    assert!(!cov.has_prose_quality);
    assert_eq!(cov.flags().len(), TermCoverage::TOTAL);
}

#[test]
fn loss_judgment_sound_reads_the_preservation_ordering() {
    assert!(is_sound_or_stronger("ExactPreservation"));
    assert!(is_sound_or_stronger("SoundUnderApproximation"));
    // Weaker-than-sound kinds fail; an unrecognized kind is not provably sound.
    assert!(!is_sound_or_stronger("ValidationOnly"));
    assert!(!is_sound_or_stronger("CompleteOverApproximation"));
    assert!(!is_sound_or_stronger("Unsupported"));
    assert!(!is_sound_or_stronger("NotAKind"));
}

#[test]
fn dimensions_and_flags_agree_in_length_and_order() {
    // The three-way order contract lives in tests/coverage_dimensions.rs; here we
    // pin the local invariant that flags() and DIMENSIONS are the same length and
    // that each DIMENSIONS entry's dimension is the matching per-term variant.
    assert_eq!(DIMENSIONS.len(), TermCoverage::TOTAL);
    let term = bare("Cat");
    let model = DocsModel::default();
    let ctx = CoverageContext::new(&model);
    assert_eq!(term_coverage(&term, &ctx).flags().len(), DIMENSIONS.len());
}

#[test]
fn dimension_index_constants_pin_the_stable_order() {
    // If DIMENSIONS is reordered, these constants (used by the applicability
    // tests) must move with it — pin them to the canonical positions.
    assert_eq!(dim_index(Dimension::Alignment), IDX_ALIGNMENT);
    assert_eq!(dim_index(Dimension::LossLedgerRow), IDX_LOSS_LEDGER_ROW);
    assert_eq!(dim_index(Dimension::LinkageCoverage), IDX_LINKAGE_COVERAGE);
    assert_eq!(
        dim_index(Dimension::LossJudgmentSound),
        IDX_LOSS_JUDGMENT_SOUND
    );
}

#[test]
fn superset_native_term_is_covered_for_the_conditional_dimensions() {
    // The CORE FIX: a novel, superset-native term — no `gmeow:adoptionTarget`,
    // not the subject of any alignment / linkage, and not a lossy-projection
    // source — is COVERED (never counted MISSING) for all four
    // applicability-conditioned dimensions. GMEOW guarantees such terms, so
    // penalizing them for having no external equivalent is exactly the flaw.
    let model = DocsModel::default();
    let ctx = CoverageContext::new(&model);
    let term = bare("NovelNative");
    let cov = term_coverage(&term, &ctx);

    assert!(
        !cov.applicable_external,
        "no external-correspondence intent"
    );
    assert!(!cov.applicable_lossy, "not a lossy-projection source");
    // Raw present detectors correctly report ABSENCE …
    assert!(!cov.has_alignment);
    assert!(!cov.has_linkage_coverage);
    assert!(!cov.has_loss_ledger_row);
    // … yet coverage (flags = !applicable ∨ present) reports COVERED.
    let flags = cov.flags();
    assert!(
        flags[IDX_ALIGNMENT],
        "novel term covers alignment vacuously"
    );
    assert!(flags[IDX_LINKAGE_COVERAGE], "…and linkage coverage");
    assert!(flags[IDX_LOSS_LEDGER_ROW], "…and loss ledger row");
    assert!(flags[IDX_LOSS_JUDGMENT_SOUND], "…and loss judgment sound");
    // None of the four appear in the MISSING facet.
    let missing = cov.missing_keys();
    for key in [
        "alignment",
        "linkage_coverage",
        "loss_ledger_row",
        "loss_judgment_sound",
    ] {
        assert!(!missing.contains(&key), "novel term must not miss `{key}`");
    }
}

#[test]
fn declared_adoption_without_a_mapping_is_still_missing_alignment() {
    // The DEFECT-STILL-CAUGHT guard: a term that DECLARES an external
    // correspondence (`gmeow:adoptionTarget`) but has no documented alignment /
    // linkage is applicable ∧ ¬present → genuinely MISSING both dimensions.
    let model = DocsModel::default();
    let ctx = CoverageContext::new(&model);
    let term = DocTerm {
        adoption_targets: vec!["schema".to_string(), "foaf".to_string()],
        ..bare("DeclaresButUnmapped")
    };
    let cov = term_coverage(&term, &ctx);

    assert!(
        cov.applicable_external,
        "declaring adoptionTarget is an intent"
    );
    assert!(!cov.has_alignment);
    assert!(!cov.has_linkage_coverage);
    let flags = cov.flags();
    assert!(
        !flags[IDX_ALIGNMENT],
        "declared-but-unmapped misses alignment"
    );
    assert!(!flags[IDX_LINKAGE_COVERAGE], "…and linkage coverage");
    let missing = cov.missing_keys();
    assert!(missing.contains(&"alignment"));
    assert!(missing.contains(&"linkage_coverage"));
}

#[test]
fn a_mapping_subject_is_applicable_and_present() {
    // A term that already participates in an alignment + mapping-set linkage is
    // applicable AND present → covered (unchanged from before the fix).
    let iri = format!("{GMEOW}Mapped");
    let model = DocsModel {
        linkages: vec![DocLinkage {
            mapping_set: Some(format!("{GMEOW}mappingSet/1")),
            subject: iri.clone(),
            subject_curie: "gmeow:Mapped".to_string(),
            predicate: "skos:closeMatch".to_string(),
            object: "http://example.org/Mapped".to_string(),
            justification: None,
            confidence: None,
            owner_slice: format!("{GMEOW}slice/zoo"),
        }],
        ..Default::default()
    };
    let ctx = CoverageContext::new(&model);
    let cov = term_coverage(&bare("Mapped"), &ctx);
    assert!(cov.applicable_external);
    assert!(cov.has_alignment);
    assert!(cov.has_linkage_coverage);
    assert!(cov.flags()[IDX_ALIGNMENT]);
    assert!(cov.flags()[IDX_LINKAGE_COVERAGE]);
}

#[test]
fn a_lossy_projection_source_is_applicable_for_the_loss_dimensions() {
    // A term with an authored static loss target is a lossy-projection source →
    // applicable for both loss dimensions. A SOUND judgment is present (covered);
    // an UNSOUND judgment is applicable ∧ ¬present → MISSING (defect caught).
    let sound_model = DocsModel {
        loss_targets: vec![DocLossTarget {
            target: "SoundProj".to_string(),
            label: None,
            preservation_kind: "SoundUnderApproximation".to_string(),
            complexity_class: "PTIME".to_string(),
            slice: format!("{GMEOW}slice/zoo"),
        }],
        ..Default::default()
    };
    let ctx = CoverageContext::new(&sound_model);
    let cov = term_coverage(&bare("SoundProj"), &ctx);
    assert!(cov.applicable_lossy, "carries a static loss target");
    assert!(cov.has_loss_ledger_row);
    assert!(
        cov.has_loss_judgment_sound,
        "SoundUnder is sound-or-stronger"
    );
    assert!(cov.flags()[IDX_LOSS_LEDGER_ROW]);
    assert!(cov.flags()[IDX_LOSS_JUDGMENT_SOUND]);

    let unsound_model = DocsModel {
        loss_targets: vec![DocLossTarget {
            target: "UnsoundProj".to_string(),
            label: None,
            preservation_kind: "ValidationOnly".to_string(),
            complexity_class: "PTIME".to_string(),
            slice: format!("{GMEOW}slice/zoo"),
        }],
        ..Default::default()
    };
    let ctx = CoverageContext::new(&unsound_model);
    let cov = term_coverage(&bare("UnsoundProj"), &ctx);
    assert!(cov.applicable_lossy);
    assert!(cov.has_loss_ledger_row, "present: it is a lossy source");
    assert!(
        !cov.has_loss_judgment_sound,
        "ValidationOnly is weaker than sound"
    );
    assert!(cov.flags()[IDX_LOSS_LEDGER_ROW], "row present → covered");
    assert!(
        !cov.flags()[IDX_LOSS_JUDGMENT_SOUND],
        "unsound judgment → MISSING"
    );
    assert!(cov.missing_keys().contains(&"loss_judgment_sound"));
}

#[test]
fn demonstration_dims_are_existential_per_term_dims_universal() {
    // The DEMONSTRATION vs PER-TERM split. A slice with three terms where only
    // ONE (`gmeow:One`) demonstrates the slice-level practices — a fixture pair,
    // a competency question with a rationale, and a worked instance — yet a
    // per-term quality (`scope_note`) is absent on one of the three.
    let slice = format!("{GMEOW}slice/zoo");
    let one_iri = format!("{GMEOW}One");
    let terms = vec![
        DocTerm {
            scope_notes: vec!["a boundary note".to_string()],
            ..bare("One")
        },
        DocTerm {
            scope_notes: vec!["another boundary note".to_string()],
            ..bare("Two")
        },
        // The third term is missing its scope note → the ∀ dimension fails.
        bare("Three"),
    ];
    let model = DocsModel {
        terms,
        fixtures: vec![
            DocFixture {
                slice: slice.clone(),
                logical_path: "tests/conformance-fixtures/one-ok.ttl".to_string(),
                title: "one ok".to_string(),
                text: String::new(),
                kind: DocFixtureKind::Wellformed,
                terms_referenced: vec!["gmeow:One".to_string()],
                expected_outcome: None,
                violation_code: None,
                rationale: None,
                catalog_slug: None,
            },
            DocFixture {
                slice: slice.clone(),
                logical_path: "tests/counter-examples/one-bad.ttl".to_string(),
                title: "one bad".to_string(),
                text: String::new(),
                kind: DocFixtureKind::CounterExample,
                terms_referenced: vec!["gmeow:One".to_string()],
                expected_outcome: None,
                violation_code: None,
                rationale: None,
                catalog_slug: None,
            },
        ],
        competencies: vec![DocCompetency {
            iri: format!("{GMEOW}cq/one"),
            rationale: Some("why the ontology must answer this".to_string()),
            exercises: vec![one_iri.clone()],
            owner_slice: slice.clone(),
            ..Default::default()
        }],
        examples: vec![DocExample {
            slice: slice.clone(),
            logical_path: "examples/one-scene.ttl".to_string(),
            title: "one scene".to_string(),
            text: String::new(),
            terms_referenced: vec!["gmeow:One".to_string()],
        }],
        ..Default::default()
    };
    let ctx = CoverageContext::new(&model);

    // Only `gmeow:One` carries the three demonstration facts per-term …
    let one = term_coverage(&model.terms[0], &ctx);
    let three = term_coverage(&model.terms[2], &ctx);
    assert!(one.has_fixture_pair && one.has_competency_rationale && one.has_worked_instance);
    assert!(
        !three.has_fixture_pair && !three.has_competency_rationale && !three.has_worked_instance,
        "the per-term diagnostic still records each term's individual gaps"
    );

    let covered = slice_covered_dims(&slice, &model, &ctx);
    // ∃: one demonstrating term is enough for the SLICE to cover all three.
    assert!(covered.contains(&Dimension::FixturePair), "∃ fixture pair");
    assert!(
        covered.contains(&Dimension::CompetencyRationale),
        "∃ competency rationale"
    );
    assert!(
        covered.contains(&Dimension::WorkedInstance),
        "∃ worked instance"
    );
    // ∀: one term missing its scope note → the SLICE does NOT cover it.
    assert!(
        !covered.contains(&Dimension::ScopeNote),
        "∀ scope note fails when one term lacks it"
    );
    // A per-term dim every term covers (translation, vacuous with no non-English
    // languages configured) is still slice-covered under ∀.
    assert!(
        covered.contains(&Dimension::TranslationCoverage),
        "∀ translation coverage holds vacuously for every term"
    );
}

#[test]
fn prose_heuristics_are_conservative() {
    // The boundary / worked-triple predicates themselves are owned and tested by
    // `crate::prose`; what this module still owns is the test-artifact detector.
    assert!(names_test_artifact("see test_foo_bar for evidence"));
    assert!(names_test_artifact("Mirrors the fixture behaviour"));
    assert!(!names_test_artifact("a genuine ontological rationale"));
}
