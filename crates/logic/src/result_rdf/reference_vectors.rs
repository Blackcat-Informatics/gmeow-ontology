// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit reference production, separate from read-only codec checks.

use std::sync::Arc;

use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm, TermValue};

use super::{native, result_err};
use crate::physical::{DomainProfile, LogicalGraph, SelectedDomains, SelectedLogicalWorld};
use crate::reason::refute::{
    BoundKind, NativeBoundEvidence, NativeFamilyCompletion, NativeFamilyOutcome,
    NativeObligationScope, NativeProofOrigin, NativeRefutationFamily,
};
use crate::result::{InformationState, ReasoningResult};

#[derive(Clone, Copy)]
enum BoundaryProfile {
    Ordinary,
    Directional,
    WideCardinality,
}

impl BoundaryProfile {
    const ALL: [Self; 3] = [Self::Ordinary, Self::Directional, Self::WideCardinality];

    const fn wire(self) -> &'static str {
        match self {
            Self::Ordinary => "ordinary",
            Self::Directional => "directional",
            Self::WideCardinality => "ordinary-u128",
        }
    }
}

fn conflict_result() -> gmeow_errors::Result<ReasoningResult> {
    let mut source = RdfDatasetBuilder::new();
    source.push_owned_quad(&RdfQuad::new(
        RdfTerm::iri("urn:gmeow:test:conflicting-individual"),
        "https://blackcatinformatics.ca/logic/instanceOf",
        RdfTerm::iri("https://blackcatinformatics.ca/logic/Nothing"),
    ))?;
    let source = source.freeze()?;
    let input = crate::reason::prepare_reasoning_input(&source)?;
    let domains = SelectedDomains::new([SelectedLogicalWorld::new(
        LogicalGraph::Default,
        DomainProfile::NonemptyObjectDomainV1,
        "urn:gmeow:test:result-provenance-domain".into(),
        *input.ingress_contract(),
    )?])?;
    let result = crate::reason::reason_all(input, &domains)?;
    result.validate_native_closure()?;
    if result.information != InformationState::Both {
        return Err(result_err(
            "native reference source did not retain its asserted conflict".into(),
        ));
    }
    Ok(result)
}

/// Alter codec data while retaining the source assessment as opaque metadata.
/// These packets never acquire Original authority or semantic result admission.
fn boundary_packet(
    original: &native::ReceiptRecord,
    profile: BoundaryProfile,
    depth: usize,
) -> gmeow_errors::Result<native::ReceiptRecord> {
    let mut execution = original.execution.as_ref().clone();
    if matches!(profile, BoundaryProfile::WideCardinality) {
        execution
            .families
            .first_mut()
            .ok_or_else(|| result_err("native reference has no selected family ledger".into()))?
            .outcomes
            .push(NativeFamilyOutcome {
                family: NativeRefutationFamily::Cardinality,
                obligation: NativeObligationScope::World,
                bounds: vec![NativeBoundEvidence {
                    owner: TermValue::iri("urn:gmeow:test:capture-bound-owner"),
                    predicate: "urn:gmeow:test:capture-bound".into(),
                    value: TermValue::Literal {
                        lexical_form: "18446744073709551616".into(),
                        datatype: "http://www.w3.org/2001/XMLSchema#nonNegativeInteger".into(),
                        language: None,
                        direction: None,
                    },
                    kind: BoundKind::Max,
                    interpreted: Some(u128::from(u64::MAX) + 1),
                    qualifier: None,
                    capacity: Some(u128::MAX),
                    support: Vec::new(),
                }],
                conclusions: Vec::new(),
                obstructions: Vec::new(),
                publication_reads: Vec::new(),
                completion: NativeFamilyCompletion::NotEngaged,
            });
    }
    let directional = matches!(profile, BoundaryProfile::Directional);
    let mut object = TermValue::Literal {
        lexical_form: "receipt depth boundary".into(),
        datatype: if directional {
            "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
        } else {
            "http://www.w3.org/2001/XMLSchema#string"
        }
        .into(),
        language: directional.then(|| "ar".into()),
        direction: directional.then_some(purrdf::RdfTextDirection::Rtl),
    };
    for _ in 0..depth {
        object = TermValue::Triple {
            s: TermValue::iri("urn:gmeow:test:receipt-subject").into(),
            p: TermValue::iri("urn:gmeow:test:receipt-predicate").into(),
            o: object.into(),
        };
    }
    let source_object = execution
        .families
        .iter_mut()
        .flat_map(|ledger| &mut ledger.proofs)
        .find_map(|proof| match &mut proof.origin {
            NativeProofOrigin::Asserted { sources, .. } => {
                sources.first_mut().map(|source| &mut source.object)
            }
            _ => None,
        })
        .ok_or_else(|| {
            result_err("native reference source has no retained asserted premise".into())
        })?;
    *source_object = object;
    Ok(native::ReceiptRecord {
        execution: Arc::new(execution),
        assessment: original.assessment,
        closure: original.closure.clone(),
    })
}

/// Produce the one current native receipt reference family from a genuine tiny
/// constructor-owned conflict and twelve deliberately altered codec boundaries.
/// Only the original row is semantic execution evidence. Profile/depth rows
/// record strict codec emission and readback, never semantic re-admission.
///
/// # Errors
/// Refuses failed source execution, unexpected codec failures or changed readback.
pub fn native_receipt_reference_vectors() -> gmeow_errors::Result<String> {
    let result = conflict_result()?;
    let original = native::ReceiptRecord::original(&result, None)?;
    let wire = native::encode(&original)?;
    if native::decode(&wire)? != original {
        return Err(result_err(
            "native reference original changed during codec readback".into(),
        ));
    }
    let mut rows = format!("conflict|{}\n", purrdf_hash::hex::encode(&wire));
    for profile in BoundaryProfile::ALL {
        for depth in 25..=28 {
            let packet = boundary_packet(&original, profile, depth)?;
            rows.push_str(profile.wire());
            rows.push('|');
            rows.push_str(&depth.to_string());
            match native::encode(&packet) {
                Ok(encoded) => {
                    if native::decode(&encoded)? != packet {
                        return Err(result_err(
                            "native codec boundary changed during reference readback".into(),
                        ));
                    }
                    rows.push_str("|admitted|admitted|");
                    rows.push_str(&purrdf_hash::hex::encode(&encoded));
                    rows.push('|');
                }
                Err(error) if error.message().contains("typed admission bound") => {
                    rows.push_str("|refused|not-run||");
                    rows.push_str(error.message());
                }
                Err(error) => {
                    return Err(error.with_context("produce native receipt codec reference"));
                }
            }
            rows.push('\n');
        }
    }
    Ok(rows)
}
