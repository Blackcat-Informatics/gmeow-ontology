// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Fixed synthetic expectations for correspondence routing and cache tests.
//! No repository source or authenticated corpus is used to construct this value.
//! The separate corpus consumer compares the producer's authored value to it.

use gmeow_logic_compile::ir::{Correspondence, CorrespondenceCaveat, PreservationKind};
use gmeow_logic_compile::projections::correspondence::CorrespondenceProgram;

pub(crate) fn synthetic_affine_program() -> CorrespondenceProgram {
    use gmeow_logic_compile::ir::{
        CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeVerdict, LawClaimIr,
        MorphismClass, MorphismKind,
    };

    const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
    let corr_iri = format!("{GMEOW}example/gmeowContactCorrespondence");
    // The two legs are affine optics onto the shared apex `gmeow:contact`.
    let get_leg = format!("{GMEOW}example/foafPersonToGmeowContactFacet");
    let put_leg = format!("{GMEOW}example/schemaContactPointToGmeowContactFacet");
    let caveat_iri = format!("{corr_iri}/caveat");

    let correspondence = Correspondence::new(
        corr_iri.clone(),
        CorrespondenceRelation::Overlaps,
        MorphismClass::AffineCorrespondence,
        MorphismKind::InstitutionMorphism,
        false,
        Some(Determinacy::Vague),
        Some(get_leg),
        Some(put_leg),
        // An affine co-projection claims GetPut (acquisition stability) but the law is
        // left unverified (honest unknown), never asserted discharged on a vague overlap.
        vec![LawClaimIr {
            law: CorrespondenceLaw::GetPut,
            verdict: DischargeVerdict::ObligationUnknown,
            condition: None,
        }],
        Some(
            gmeow_logic_compile::ir::UnitInterval::new(purrdf::RdfLiteral::typed(
                "0.72".to_owned(),
                "http://www.w3.org/2001/XMLSchema#decimal".to_owned(),
            ))
            .expect("valid authored confidence"),
        ),
        None,
        None,
        None,
        None,
        // The lane-level preservation polarity lives on the CorrespondenceProgram; this
        // worked-example cell authors no per-correspondence rung.
        None,
    )
    .expect("the §14 affine-triangle correspondence is well-formed");

    let caveat = CorrespondenceCaveat {
        iri: caveat_iri,
        comments: vec![purrdf::RdfLiteral::language_tagged(
            "foaf:Person denotes an agent/person; schema:ContactPoint denotes a contact \
               channel/role. Both project through the contact-bearing facet of gmeow:contact; \
               they are not equivalent and neither subsumes the other.",
            "x-gmeow-english",
        )],
    };

    CorrespondenceProgram::new(
        vec![
            correspondence
                .with_caveats(vec![caveat])
                .expect("unique caveat"),
        ],
        // A caveated overlap under-approximates the forced-equality reading it refuses.
        PreservationKind::SoundUnder,
    )
}
