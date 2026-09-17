// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::{
    ProjectionVocabulary, RelocationReason, relocation_reasons_over_texts,
    residue_constructs_over_texts, residue_over_texts,
};
use crate::model::CountKind;

const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const KERNEL: &str = "https://blackcatinformatics.ca/gmeow/slices/kernel";

fn prefixes() -> &'static str {
    "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n\
         @prefix gufo: <https://w3id.org/gufo#> .\n"
}

fn text(body: &str) -> Vec<String> {
    vec![format!("{}{body}", prefixes())]
}

fn gufo_vocab() -> ProjectionVocabulary {
    ProjectionVocabulary {
        prefix: "gufo".to_owned(),
        namespaces: vec!["https://w3id.org/gufo#".to_owned()],
        subsumed_by: LOGIC_NS.to_owned(),
        owner: LOGIC_NS.to_owned(),
        count_kind: CountKind::TypedAxiom,
        default_ceiling: 0,
        preservation: "SoundUnderApproximation".to_owned(),
        alignment_predicates: Vec::new(),
        counted_predicates: Vec::new(),
    }
}

/// A validated grounding correspondence: exempt on the vocabulary's OWNER surface,
/// plain residue anywhere else.
fn grounding_cell() -> Vec<String> {
    text(
        r#"
            gmeow:MyKind skos:exactMatch gufo:Kind {|
                a logic:GroundingCorrespondence ;
                gmeow:sssomFile "grounding.sssom.tsv" ;
                gmeow:justification gmeow:ManualMappingCuration ;
                logic:sourceEndpoint gmeow:MyKind ;
                logic:targetEndpoint gufo:Kind ;
                logic:morphismClass logic:WellBehavedLens ;
                logic:morphismKind logic:InstitutionMorphism ;
                logic:preservationKind logic:SoundUnderApproximation
            |} .
            "#,
    )
}

#[test]
fn count_over_texts_is_the_construct_sets_length() {
    let texts = text("gmeow:A a sh:NodeShape . gmeow:B a sh:NodeShape .");
    let vocab = crate::counting::shacl_vocab();
    let constructs = residue_constructs_over_texts(&texts, &vocab, &vocab.owner).unwrap();
    assert_eq!(constructs.len(), 2);
    assert_eq!(
        residue_over_texts(&texts, &vocab, &vocab.owner).unwrap(),
        constructs.len() as u64
    );
}

#[test]
fn base_bytes_can_be_measured_at_a_destination_surface() {
    // The SAME base bytes measured at the OWNER surface and at a destination slice
    // surface differ — residue is not conserved across the owner boundary, and the
    // existing `surface_iri` parameter is all it takes to see that.
    let base = grounding_cell();
    let vocab = gufo_vocab();
    assert_eq!(residue_over_texts(&base, &vocab, &vocab.owner).unwrap(), 0);
    assert_eq!(residue_over_texts(&base, &vocab, KERNEL).unwrap(), 1);
}

#[test]
fn relocation_reasons_over_texts_names_the_owner_boundary_shift() {
    let base = grounding_cell();
    let working = grounding_cell();
    let vocab = gufo_vocab();
    let reasons =
        relocation_reasons_over_texts(&base, &vocab.owner, &working, KERNEL, &vocab).unwrap();
    let codes: Vec<&str> = reasons
        .values()
        .flat_map(|set| set.iter().map(|r| r.code()))
        .collect();
    assert_eq!(codes, vec!["exemption-shift-owner-boundary"], "{reasons:?}");
    assert!(reasons.contains_key("https://blackcatinformatics.ca/gmeow/MyKind"));
}

#[test]
fn relocation_reasons_over_texts_names_an_orphaned_grounding() {
    let base = text(
        "gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .\n\
             logic:sAxiom a logic:Formula .",
    );
    let working = text("gmeow:S a sh:NodeShape ; logic:formalizes logic:sAxiom .");
    let vocab = crate::counting::shacl_vocab();
    let reasons =
        relocation_reasons_over_texts(&base, &vocab.owner, &working, KERNEL, &vocab).unwrap();
    assert_eq!(
        reasons
            .get("https://blackcatinformatics.ca/gmeow/S")
            .map(|set| set.iter().copied().collect::<Vec<_>>()),
        Some(vec![RelocationReason::GroundingOrphaned]),
        "{reasons:?}"
    );
}

#[test]
fn a_broken_base_surface_hard_fails_rather_than_measuring_zero() {
    let broken = vec!["this is not turtle {{{".to_owned()];
    let vocab = crate::counting::shacl_vocab();
    assert!(
        residue_constructs_over_texts(&broken, &vocab, &vocab.owner).is_err(),
        "an unparsable surface must HARD FAIL, never silently score as clean"
    );
}
