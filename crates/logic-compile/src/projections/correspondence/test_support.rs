// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

pub(super) const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

// --------------------------------------------------------------------------- //
// The §14 affine-triangle worked example
// --------------------------------------------------------------------------- //

/// The §14 worked example (`docs/APPLIED_CATEGORY_THEORY/take1.md`): `foaf:Person`
/// (an agent) and `schema:ContactPoint` (a contact channel) **co-project onto the
/// contact-bearing facet of `gmeow:contact`** — not peers, not subsets, not equivalent.
/// The honest canonical object is a *vague affine overlap*, not a forced equality.
///
/// Builds the [`CorrespondenceProgram`] carrying exactly this one correspondence (with
/// its caveat) so the lane flows end-to-end through the bundle carrier. The generated
/// alignment surface MUST be `skos:relatedMatch` (NEVER `skos:exactMatch`, NEVER
/// `owl:equivalentClass`), and the lane declares its `SoundUnderApproximation`
/// preservation polarity in the loss ledger.
///
/// TEST-ONLY (`#[cfg(test)]`): the production correspondence lane no longer constructs
/// this Rust literal. It is a fixed synthetic program for codec and law tests;
/// authored-example fidelity is checked by the pipeline's authenticated corpus consumer.
pub fn affine_triangle_worked_example() -> CorrespondenceProgram {
    use crate::ir::{
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
            crate::ir::UnitInterval::new(purrdf::RdfLiteral::typed(
                "0.72".to_owned(),
                "http://www.w3.org/2001/XMLSchema#decimal".to_owned(),
            ))
            .expect("valid authored confidence"),
        ),
        None,
        None,
        None,
        None,
        Some(PreservationKind::SoundUnder),
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
                .expect("unique caveat")
                .with_loss_evidence(vec![purrdf::RdfLiteral::language_tagged(
                    "This contact-facet overlap is projected as skos:relatedMatch; the alignment does not carry the complete agent/person versus contact-channel distinction or assert their equivalence.",
                    "x-gmeow-english",
                )])
                .expect("authored overlap loss"),
        ],
        // A caveated overlap under-approximates the forced-equality reading it refuses.
        PreservationKind::SoundUnder,
    )
}
