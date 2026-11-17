// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Governed summary values under the two production snapshot namespaces.

use purrdf::{BlankScope, RdfDatasetBuilder, RdfTerm, RdfTriple, TermValue};

use super::result_err;
use crate::result::{ReasoningResult, ResultPayload};

/// The native snapshot authorship path whose blank identities a summary will carry.
#[derive(Clone, Copy)]
pub enum ReasoningSnapshotScope {
    /// A standalone runtime artifact, isolated with its fixed ingest prefix.
    Runtime,
    /// The assembled pipeline carrier, ingested without an additional prefix.
    Carrier,
}

impl ReasoningSnapshotScope {
    pub(super) const fn prefix(self) -> Option<&'static str> {
        match self {
            Self::Runtime => Some("gmeow-reasoning-artifact-v2"),
            Self::Carrier => None,
        }
    }
}

/// Normalize only typed inferred-object summary slots through the selected native
/// snapshot's wire labels and the native importer's first-segment scope. Original
/// native execution evidence is retained unchanged; this is a transport summary.
///
/// # Errors
/// Refuses invalid quoted predicates, malformed composite literals or native storage failure.
pub fn reasoning_snapshot_summary(
    mut result: ReasoningResult,
    scope: ReasoningSnapshotScope,
) -> gmeow_errors::Result<ReasoningResult> {
    let ResultPayload::Inferred(rows) = &mut result.payload else {
        return Ok(result);
    };
    for row in rows {
        // Native snapshot ingestion and import retain an IRI object verbatim;
        // it cannot contain a direct or composite-literal blank identity.
        if matches!(&row.object, TermValue::Iri(_)) {
            continue;
        }
        // SnapshotBuilder qualifies direct blanks, including quoted components,
        // but retains literal lexical bytes. The native importer then applies
        // Ambient(scope 1) to both direct and composite-literal blank identities.
        let wire = row.object.try_fold(
            |leaf| match leaf {
                TermValue::Blank {
                    label,
                    scope: source,
                } => {
                    let qualified = source.qualify_label(label);
                    Ok(RdfTerm::BlankNode(match scope.prefix() {
                        Some(prefix) => format!("{prefix}-{qualified}"),
                        None => qualified.into_owned(),
                    }))
                }
                _ => leaf.to_rdf_term().map_err(|cause| {
                    gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(
                        "normalize snapshot summary object".into(),
                        cause,
                    ))
                }),
            },
            |subject, predicate, object| {
                let RdfTerm::Iri(predicate) = predicate else {
                    return Err(result_err(
                        "snapshot summary quoted predicate must be an IRI".into(),
                    ));
                };
                Ok(RdfTerm::triple(RdfTriple::new(subject, predicate, object)))
            },
        )?;
        // Release each native checking dictionary before the next object; the
        // summary does not need a second dictionary of every inferred object.
        let mut builder = RdfDatasetBuilder::new();
        let object = builder
            .intern_owned_term_bound(&wire, BlankScope(1))
            .map_err(|cause| {
                gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(
                    "admit transported snapshot summary object".into(),
                    cause,
                ))
            })?;
        let dataset = builder.freeze()?;
        row.object = dataset.as_ref().term_value(object);
    }
    Ok(result)
}
