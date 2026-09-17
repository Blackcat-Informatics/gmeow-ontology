// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// A minimal but structurally complete rubric: one tier, one axis with a
/// threshold, and one exemption whose `gmeow:exemptsAxis` is `exempts_axis`.
fn rubric_ttl(exempts_axis: &str) -> String {
    format!(
        r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
gmeow:exFoo a gmeow:AxisExemption ;
    gmeow:exemptsAxis {exempts_axis} ;
    gmeow:exemptionReason "unlanded" ;
    gmeow:exemptionDate "2026-07-08" ;
    gmeow:exemptionProducer "FooProducer" .
"#
    )
}

fn load(ttl: &str) -> gmeow_errors::Result<Rubric> {
    let ds = purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None)
        .map_err(|e| super::rubric_err(e.to_string()))?;
    let mut b = purrdf::RdfDatasetBuilder::new();
    b.push_dataset(&ds);
    let frozen = b.freeze().map_err(|e| super::rubric_err(e.to_string()))?;
    load_rubric(&frozen)
}

#[test]
/// Parsed multilingual tiers retain translations but resolve the carrier CLI label.
fn parsed_multilingual_tier_uses_carrier_label_for_cli_resolution() {
    let ttl = rubric_ttl("gmeow:axisFoo")
            .replacen(
                "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n",
                "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n",
                1,
            )
            .replacen(
                "gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .",
                "gmeow:tierRegistered a gmeow:QualityTier ; rdfs:label \"Inscrit\"@fr, \"Public registered\"@en, \"Registered\"@x-gmeow-english ; gmeow:tierRank 0 .",
                1,
            );
    let rubric = load(&ttl).expect("multilingual rubric loads");

    let by_label = crate::lint::resolve_min_tier(&rubric.standard, "registered")
        .expect("carrier label resolves");
    let by_local = crate::lint::resolve_min_tier(&rubric.standard, "tierREGISTERED")
        .expect("stable IRI local name resolves");
    assert_eq!(by_label.label, "Registered");
    assert_eq!(by_label.iri, by_local.iri);
    assert!(
        crate::lint::resolve_min_tier(&rubric.standard, "Inscrit").is_err(),
        "a retained translation must not replace the carrier CLI authority"
    );
}

#[test]
fn exemption_naming_a_real_axis_loads() {
    // Control: the same fixture with a valid axis_iri loads cleanly, proving the
    // negative test isolates the axis check (not a malformed fixture).
    let rubric = load(&rubric_ttl("gmeow:axisFoo")).expect("valid rubric loads");
    assert_eq!(rubric.floors.exemptions.len(), 1);
    assert_eq!(
        rubric.floors.exemptions[0].axis_iri,
        format!("{GMEOW_NS}axisFoo")
    );
}

#[test]
fn exemption_with_unknown_axis_hard_fails() {
    // (e) An exemption naming an axis the rubric never loaded is a hard fail.
    let err = load(&rubric_ttl("gmeow:axisNope")).unwrap_err();
    assert!(err.message().contains("exempts unknown axis"), "{err}");
    assert!(
        err.message().contains("axisNope"),
        "names the offending axis: {err}"
    );
}

#[test]
fn non_finite_axis_weight_hard_fails() {
    // A NaN gmeow:axisWeight parses fine as an f64 and would otherwise
    // silently collapse the advisory weight-rank comparator — G4 mandates a
    // hard fail at load time instead.
    let ttl = format!(
        r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisWeight "NaN" ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
"#
    );
    let err = load(&ttl).unwrap_err();
    assert!(
        err.message().contains("non-finite gmeow:axisWeight"),
        "{err}"
    );
    assert!(
        err.message().contains("axisFoo"),
        "names the offending axis: {err}"
    );
}

#[test]
fn non_numeric_axis_weight_hard_fails() {
    // A PRESENT but non-numeric gmeow:axisWeight must hard-fail, never
    // silently degrade to the missing-value default of 1.0 (.goals
    // no-optionality) — only an ABSENT predicate earns that default.
    let ttl = format!(
        r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisWeight "abc" ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
"#
    );
    let err = load(&ttl).unwrap_err();
    assert!(
        err.message().contains("non-numeric gmeow:axisWeight"),
        "{err}"
    );
    assert!(
        err.message().contains("axisFoo"),
        "names the offending axis: {err}"
    );
}

#[test]
fn non_finite_threshold_floor_hard_fails() {
    // Same defect class for gmeow:thresholdFloor: it feeds the ascending
    // floor sort and the `score + EPSILON >= floor` gate comparisons, so a
    // NaN/inf literal must hard-fail at load rather than silently break
    // tier ordering.
    let ttl = format!(
        r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor "inf" .
"#
    );
    let err = load(&ttl).unwrap_err();
    assert!(
        err.message().contains("non-finite gmeow:thresholdFloor"),
        "{err}"
    );
    assert!(
        err.message().contains("thrFoo"),
        "names the offending threshold: {err}"
    );
}

/// A structurally complete rubric (one tier, one axis, one threshold) with an
/// extra `body` block appended — used to exercise the floor-commitment loaders
/// without duplicating the required ladder/axis scaffolding.
fn rubric_with(body: &str) -> String {
    format!(
        r#"@prefix gmeow: <{GMEOW_NS}> .
gmeow:tierRegistered a gmeow:QualityTier ; gmeow:tierRank 0 .
gmeow:axisFoo a gmeow:QualityAxis ;
    gmeow:axisProducer "foo" ;
    gmeow:axisDimension gmeow:dimFoo ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrFoo .
gmeow:thrFoo a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
{body}
"#
    )
}

#[test]
fn axis_floor_commitment_loads_with_full_precision() {
    // (a) A well-formed gmeow:AxisFloorCommitment resolves to (slice, axis,
    // floor) carrying the full f64 precision the measured score commits.
    let rubric = load(&rubric_with(
        r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.9954337899543378 ."#,
    ))
    .expect("valid floor commitment loads");
    assert_eq!(rubric.floors.commitments.len(), 1);
    let c = &rubric.floors.commitments[0];
    assert_eq!(c.slice, format!("{GMEOW_NS}sliceFoo"));
    assert_eq!(c.axis, format!("{GMEOW_NS}axisFoo"));
    assert!((c.floor - 0.995_433_789_954_337_8).abs() < f64::EPSILON);
}

#[test]
fn slice_tier_floor_loads() {
    // (b) A well-formed gmeow:SliceTierFloor resolves to (slice, tier).
    let rubric = load(&rubric_with(
        r#"gmeow:tierFloorFoo a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierRegistered ."#,
    ))
    .expect("valid tier floor loads");
    assert_eq!(rubric.floors.tier_floors.len(), 1);
    let f = &rubric.floors.tier_floors[0];
    assert_eq!(f.slice, format!("{GMEOW_NS}sliceFoo"));
    assert_eq!(f.tier, format!("{GMEOW_NS}tierRegistered"));
}

#[test]
fn axis_floor_commitment_missing_value_hard_fails() {
    // (c) A commitment missing gmeow:floorValue is a hard fail — a floor with
    // no value cannot pin a regression bar, so we never silently skip it.
    let err = load(&rubric_with(
        r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ."#,
    ))
    .unwrap_err();
    assert!(
        err.message().contains("no decimal gmeow:floorValue"),
        "{err}"
    );
    assert!(
        err.message().contains("floorFooGrounding"),
        "names the offending commitment: {err}"
    );
}

#[test]
fn axis_floor_commitment_missing_axis_hard_fails() {
    // (c) A commitment missing gmeow:floorAxis is likewise a hard fail.
    let err = load(&rubric_with(
        r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorValue 0.5 ."#,
    ))
    .unwrap_err();
    assert!(err.message().contains("no gmeow:floorAxis"), "{err}");
}

#[test]
fn axis_floor_commitment_unknown_axis_hard_fails() {
    // A floor commitment naming an axis the rubric never loaded (a typo'd
    // gmeow:floorAxis) must hard-fail — otherwise it loads cleanly and then
    // silently never gates anything, leaving the ratchet dead.
    let err = load(&rubric_with(
        r#"gmeow:floorFooGrounding a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisNope ;
    gmeow:floorValue 0.5 ."#,
    ))
    .unwrap_err();
    assert!(err.message().contains("unknown axis"), "{err}");
    assert!(
        err.message().contains("axisNope"),
        "names the offending axis: {err}"
    );
}

#[test]
fn slice_tier_floor_unknown_tier_hard_fails() {
    // A tier floor naming a tier the rubric ladder never loaded (a typo'd
    // gmeow:floorTier) must hard-fail — otherwise it loads cleanly and then
    // silently never gates anything, leaving the ratchet dead.
    let err = load(&rubric_with(
        r#"gmeow:tierFloorFoo a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierNope ."#,
    ))
    .unwrap_err();
    assert!(err.message().contains("unknown tier"), "{err}");
    assert!(
        err.message().contains("tierNope"),
        "names the offending tier: {err}"
    );
}

#[test]
fn slice_tier_floor_missing_tier_hard_fails() {
    // A tier floor missing gmeow:floorTier is a hard fail: it names no rung.
    let err = load(&rubric_with(
        r#"gmeow:tierFloorFoo a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ."#,
    ))
    .unwrap_err();
    assert!(err.message().contains("no gmeow:floorTier"), "{err}");
}

#[test]
fn duplicate_axis_floor_commitment_hard_fails() {
    // Two AxisFloorCommitment individuals naming the SAME (slice, axis) pair
    // collapse silently in the downstream BTreeMap (last-writer-wins) — the
    // loader must hard-fail rather than let one commitment shadow the other.
    let err = load(&rubric_with(
        r#"gmeow:floorFooA a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.5 .
gmeow:floorFooB a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.9 ."#,
    ))
    .unwrap_err();
    assert!(err.message().contains("duplicate"), "{err}");
    assert!(
        err.message().contains("axisFoo"),
        "names the offending axis: {err}"
    );
    assert!(
        err.message().contains("sliceFoo"),
        "names the offending slice: {err}"
    );
}

#[test]
fn distinct_axis_floor_commitments_for_same_slice_load_cleanly() {
    // Positive control: two commitments for the SAME slice but DIFFERENT axes
    // are not duplicates — proves the guard keys on the (slice, axis) pair,
    // not the slice alone.
    let rubric = load(&rubric_with(
        r#"gmeow:axisBar a gmeow:QualityAxis ;
    gmeow:axisProducer "bar" ;
    gmeow:axisDimension gmeow:dimBar ;
    gmeow:axisContextScope gmeow:scopeSliceLocal ;
    gmeow:axisThreshold gmeow:thrBar .
gmeow:thrBar a gmeow:AxisThreshold ;
    gmeow:thresholdTier gmeow:tierRegistered ;
    gmeow:thresholdFloor 0.0 .
gmeow:floorFooA a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisFoo ;
    gmeow:floorValue 0.5 .
gmeow:floorFooB a gmeow:AxisFloorCommitment ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorAxis gmeow:axisBar ;
    gmeow:floorValue 0.9 ."#,
    ))
    .expect("distinct (slice, axis) commitments load cleanly");
    assert_eq!(rubric.floors.commitments.len(), 2);
}

#[test]
fn duplicate_slice_tier_floor_hard_fails() {
    // Two SliceTierFloor individuals naming the SAME slice collapse silently
    // in the downstream BTreeMap (last-writer-wins) — the loader must
    // hard-fail rather than let one tier floor shadow the other.
    let err = load(&rubric_with(
        r#"gmeow:tierFloorFooA a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierRegistered .
gmeow:tierFloorFooB a gmeow:SliceTierFloor ;
    gmeow:floorSlice gmeow:sliceFoo ;
    gmeow:floorTier gmeow:tierRegistered ."#,
    ))
    .unwrap_err();
    assert!(err.message().contains("duplicate"), "{err}");
    assert!(
        err.message().contains("sliceFoo"),
        "names the offending slice: {err}"
    );
}

// --- Projection vocabulary + ceiling loaders ---------------------------

/// A well-formed guarded vocabulary the ceiling tests reference.
const VOCAB_SH: &str = r#"gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularyNamespace "http://www.w3.org/ns/shacl#" ;
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;
    gmeow:vocabularyOwner gmeow:sliceLogic ;
    gmeow:vocabularyCountKind "countKindShape" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."#;

#[test]
fn projection_vocabulary_and_ceiling_load() {
    // Happy path: a guarded vocab plus a ceiling that references it resolve to
    // the expected (prefix, namespaces, kind) and (slice, vocab-prefix, count).
    let body = format!(
        "{VOCAB_SH}\n\
gmeow:pcc-foo-sh a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-sh ;\n\
    gmeow:ceilingCount 7 ."
    );
    let rubric = load(&rubric_with(&body)).expect("valid vocab + ceiling load");
    assert_eq!(rubric.floors.vocabularies.len(), 1);
    let v = &rubric.floors.vocabularies[0];
    assert_eq!(v.prefix, "sh");
    assert_eq!(v.namespaces, vec!["http://www.w3.org/ns/shacl#".to_owned()]);
    assert_eq!(v.count_kind, crate::model::CountKind::Shape);
    assert_eq!(v.default_ceiling, 0);
    assert_eq!(rubric.floors.ceilings.len(), 1);
    let c = &rubric.floors.ceilings[0];
    assert_eq!(c.slice, format!("{GMEOW_NS}sliceFoo"));
    assert_eq!(c.vocab_prefix, "sh");
    assert_eq!(c.count, 7);
}

#[test]
fn ceiling_with_unknown_vocabulary_hard_fails() {
    // A ceiling naming a vocab the registry never loaded is a dead ratchet cell —
    // hard fail, never a silent skip.
    let body = "gmeow:pcc-foo-nope a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-nope ;\n\
    gmeow:ceilingCount 1 .";
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("unknown gmeow:ceilingVocabulary"),
        "{err}"
    );
    assert!(err.message().contains("projVocab-nope"), "names it: {err}");
}

#[test]
fn vocabulary_with_unknown_count_kind_hard_fails() {
    let body = r#"gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularyNamespace "http://www.w3.org/ns/shacl#" ;
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;
    gmeow:vocabularyOwner gmeow:sliceLogic ;
    gmeow:vocabularyCountKind "countKindBogus" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("unknown gmeow:vocabularyCountKind"),
        "{err}"
    );
    assert!(err.message().contains("countKindBogus"), "names it: {err}");
}

#[test]
fn vocabulary_with_no_namespace_hard_fails() {
    let body = r#"gmeow:projVocab-sh a gmeow:ProjectionVocabulary ;
    gmeow:vocabularyPrefix "sh" ;
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;
    gmeow:vocabularyOwner gmeow:sliceLogic ;
    gmeow:vocabularyCountKind "countKindShape" ;
    gmeow:vocabularyDefaultCeiling 0 ;
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("no gmeow:vocabularyNamespace"),
        "{err}"
    );
}

#[test]
fn duplicate_vocabulary_prefix_hard_fails() {
    let body = format!(
        "{VOCAB_SH}\n\
gmeow:projVocab-sh2 a gmeow:ProjectionVocabulary ;\n\
    gmeow:vocabularyPrefix \"sh\" ;\n\
    gmeow:vocabularyNamespace \"http://example.org/other#\" ;\n\
    gmeow:vocabularySubsumedBy gmeow:sliceLogic ;\n\
    gmeow:vocabularyOwner gmeow:sliceLogic ;\n\
    gmeow:vocabularyCountKind \"countKindShape\" ;\n\
    gmeow:vocabularyDefaultCeiling 0 ;\n\
    gmeow:vocabularyPreservation gmeow:presSoundUnder ."
    );
    let err = load(&rubric_with(&body)).unwrap_err();
    assert!(err.message().contains("duplicate"), "{err}");
    assert!(
        err.message().contains("prefix sh"),
        "names the prefix: {err}"
    );
}

#[test]
fn duplicate_ceiling_for_same_slice_vocab_hard_fails() {
    let body = format!(
        "{VOCAB_SH}\n\
gmeow:pcc-a a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-sh ;\n\
    gmeow:ceilingCount 2 .\n\
gmeow:pcc-b a gmeow:ProjectionCeilingCommitment ;\n\
    gmeow:ceilingSlice gmeow:sliceFoo ;\n\
    gmeow:ceilingVocabulary gmeow:projVocab-sh ;\n\
    gmeow:ceilingCount 3 ."
    );
    let err = load(&rubric_with(&body)).unwrap_err();
    assert!(err.message().contains("duplicate"), "{err}");
    assert!(err.message().contains("sliceFoo"), "names the slice: {err}");
}

// --- Ceiling relocation loaders -----------------------------------------
//
// `gmeow:CeilingRelocation logic:subClassOf [ a logic:Restriction ; ... ]` in
// `slices/core/slice-quality-rubric/module.ttl` authors the four required-binding
// axioms (relocationTerm/relocationFromSlice/relocationToSlice/relocationDate all
// minCardinality 1) as EL-safe declarative axioms; this loader is the DERIVED
// enforcement of those axioms, not a second, Rust-only source of truth. The
// `from_slice == to_slice` rejection and the unknown-vocabulary-reference
// rejection are genuinely procedural checks with no declarative cardinality/
// class/datatype form and remain enforced here only. These tests exercise the
// LOADER'S behavior, not that axiom authoring.

#[test]
fn relocation_with_no_term_hard_fails() {
    let body = r#"gmeow:reloc-noterm a gmeow:CeilingRelocation ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationDate "2026-07-08" ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("names no gmeow:relocationTerm"),
        "{err}"
    );
    assert!(
        err.message().contains("reloc-noterm"),
        "names the offending declaration: {err}"
    );
}

#[test]
fn relocation_with_no_from_slice_hard_fails() {
    let body = r#"gmeow:reloc-nofrom a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationDate "2026-07-08" ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("has no gmeow:relocationFromSlice"),
        "{err}"
    );
    assert!(
        err.message().contains("reloc-nofrom"),
        "names the offending declaration: {err}"
    );
}

#[test]
fn relocation_with_no_to_slice_hard_fails() {
    let body = r#"gmeow:reloc-noto a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationDate "2026-07-08" ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("has no gmeow:relocationToSlice"),
        "{err}"
    );
    assert!(
        err.message().contains("reloc-noto"),
        "names the offending declaration: {err}"
    );
}

#[test]
fn relocation_naming_the_same_slice_twice_hard_fails() {
    let body = r#"gmeow:reloc-same a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceFoo ;
    gmeow:relocationDate "2026-07-08" ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("as both source and destination"),
        "{err}"
    );
    assert!(
        err.message().contains("sliceFoo"),
        "names the offending slice: {err}"
    );
}

#[test]
fn relocation_with_unknown_vocabulary_hard_fails() {
    let body = r#"gmeow:reloc-badvocab a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationVocabulary gmeow:projVocab-nope ;
    gmeow:relocationDate "2026-07-08" ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(
        err.message().contains("unknown gmeow:relocationVocabulary"),
        "{err}"
    );
    assert!(err.message().contains("projVocab-nope"), "names it: {err}");
}

#[test]
fn relocation_with_no_date_hard_fails() {
    let body = r#"gmeow:reloc-nodate a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(err.message().contains("undated"), "{err}");
    assert!(
        err.message().contains("reloc-nodate"),
        "names the offending declaration: {err}"
    );
}

#[test]
fn relocation_with_blank_date_hard_fails() {
    // A PRESENT but whitespace-only gmeow:relocationDate must fail exactly like a
    // missing one (the loader's `date.trim().is_empty()` check) — a blank date
    // dates nothing, and a regression that dropped the `.trim()` would let this
    // one slip through as "present" while `relocation_with_no_date_hard_fails`
    // above stays green.
    let body = r#"gmeow:reloc-blankdate a gmeow:CeilingRelocation ;
    gmeow:relocationTerm gmeow:termFoo ;
    gmeow:relocationFromSlice gmeow:sliceFoo ;
    gmeow:relocationToSlice gmeow:sliceBar ;
    gmeow:relocationDate "   " ."#;
    let err = load(&rubric_with(body)).unwrap_err();
    assert!(err.message().contains("undated"), "{err}");
    assert!(
        err.message().contains("reloc-blankdate"),
        "names the offending declaration: {err}"
    );
}

#[test]
fn well_formed_relocation_loads_with_sorted_deduped_terms_and_resolved_vocabulary() {
    // (a) A well-formed gmeow:CeilingRelocation: repeated and out-of-order
    // gmeow:relocationTerm values collapse to a SORTED, DEDUPED `terms` vec, and
    // the optional gmeow:relocationVocabulary resolves to the registered prefix.
    let body = format!(
        "{VOCAB_SH}\n\
gmeow:reloc-good a gmeow:CeilingRelocation ;\n\
    gmeow:relocationTerm gmeow:termB, gmeow:termA, gmeow:termA ;\n\
    gmeow:relocationFromSlice gmeow:sliceFoo ;\n\
    gmeow:relocationToSlice gmeow:sliceBar ;\n\
    gmeow:relocationVocabulary gmeow:projVocab-sh ;\n\
    gmeow:relocationDate \"2026-07-08\" ."
    );
    let rubric = load(&rubric_with(&body)).expect("valid relocation loads");
    assert_eq!(rubric.floors.relocations.len(), 1);
    let r = &rubric.floors.relocations[0];
    assert_eq!(
        r.terms,
        vec![format!("{GMEOW_NS}termA"), format!("{GMEOW_NS}termB")],
        "terms are sorted and deduped: {r:?}"
    );
    assert_eq!(r.from_slice, format!("{GMEOW_NS}sliceFoo"));
    assert_eq!(r.to_slice, format!("{GMEOW_NS}sliceBar"));
    assert_eq!(r.vocabulary, Some("sh".to_owned()));
    assert_eq!(r.date, "2026-07-08");
}

use gmeow_ns::GMEOW_NS;
