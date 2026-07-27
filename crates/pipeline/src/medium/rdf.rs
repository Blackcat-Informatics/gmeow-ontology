// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The projection of the medium pass's OUTPUT into the build-time
//! [`MEDIUM_REGISTRY_GRAPH`] named graph.
//!
//! Two record families are emitted, and both are GENERATED — never hand-authored:
//!
//! * `gmeow:CompressionDictionaryRealization`, the measured half of the
//!   definition/realization split: the content digest, the byte length, the zstd
//!   `Dictionary_ID` a frame header cites, and the strategy/target the trainer
//!   ACTUALLY produced. Requiring any of these on the AUTHORED
//!   `gmeow:CompressionDictionary` would be an unsatisfiable obligation — the digest
//!   cannot exist before the dictionary is trained;
//! * `gmeow:MediumEnvelope`, one per sealed frame.
//!
//! The MEASURED strategy is carried beside the authored one deliberately: a trainer
//! that fell back to raw content must say so, because the two produce different
//! decode-side expectations, and a realization silently claiming the authored intent
//! would be a second source of truth for what actually happened.

use std::collections::BTreeMap;

use purrdf::{RdfLiteral, RdfQuad, RdfTerm};

use super::envelope::MediumEnvelope;
use super::registry::{DictionaryDef, DictionaryStrategy, MediumRegistry};
use super::{MEDIUM_REGISTRY_GRAPH, blake3_digest, dictionary_regression, is_canonical_digest};

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_NON_NEGATIVE_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#nonNegativeInteger";

/// The GENERATED realization of one authored dictionary.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DictionaryRealization {
    /// `gmeow:realizesDictionary` — the authored definition's IRI.
    pub dictionary: String,
    /// The authored `gmeow:dictionaryVersion`, repeated so the retention constraint
    /// can compare an EMITTED version against what the definition still declares.
    pub version: String,
    /// The MEASURED strategy — what the trainer produced, not what was asked for.
    pub strategy: DictionaryStrategy,
    /// The MEASURED target length the trainer was given.
    pub target_length: usize,
    /// `gmeow:dictionaryContentDigest`, `blake3:<64 lowercase hex>`.
    pub content_digest: String,
    /// `gmeow:dictionaryByteLength` — what the trainer ACTUALLY returned, which may
    /// be under the requested target.
    pub byte_length: usize,
    /// `gmeow:zstdDictionaryId` — the `Dictionary_ID` every primed frame cites.
    pub zstd_dictionary_id: u32,
}

impl DictionaryRealization {
    /// The realization individual's IRI, derived from the dictionary id + version so
    /// two versions of one id never collide on one subject.
    #[must_use]
    pub fn iri(&self, def: &DictionaryDef) -> String {
        format!("{}realization/{}/{}", super::GMEOW, def.id, def.version)
    }
}

/// Measure a trained dictionary into its realization record.
///
/// `measured_strategy` is what the trainer ACTUALLY ran — the caller passes the
/// authored intent only when it genuinely honoured it.
///
/// # Errors
/// The bytes do not parse as a finalized zstd dictionary (so no `Dictionary_ID`
/// exists to cite).
pub fn realize(
    def: &DictionaryDef,
    bytes: &[u8],
    measured_strategy: DictionaryStrategy,
) -> Result<DictionaryRealization, gmeow_errors::Diag> {
    Ok(DictionaryRealization {
        dictionary: def.iri.clone(),
        version: def.version.clone(),
        strategy: measured_strategy,
        target_length: def.target_length,
        content_digest: blake3_digest(bytes),
        byte_length: bytes.len(),
        zstd_dictionary_id: super::train::zstd_dictionary_id(bytes)?,
    })
}

/// Every emitted realization's version is still declared by the definition it
/// realizes (`logic:MediumDictionaryRetentionConstraint`).
///
/// Retiring a shipped version is a DATA-DESTROYING change: every artifact already
/// primed with it becomes undecodable the moment the version is dropped. So this is
/// never resolved by deleting the realization — the shipped bytes it describes still
/// exist.
///
/// # Errors
/// `MediumDictionaryRegression` for a realization whose definition no longer
/// declares its version (or no longer exists at all).
pub fn check_dictionary_retention(
    registry: &MediumRegistry,
    realizations: &[DictionaryRealization],
) -> Result<(), gmeow_errors::Diag> {
    for realization in realizations {
        let Some(def) = registry.dictionaries().get(&realization.dictionary) else {
            return Err(dictionary_regression(format!(
                "realization of <{}> version {:?} has no authored gmeow:CompressionDictionary at \
                 all — every artifact already primed with that version is now undecodable, and \
                 deleting the realization would not bring those bytes back",
                realization.dictionary, realization.version
            )));
        };
        if def.version != realization.version {
            return Err(dictionary_regression(format!(
                "realization of <{}> emits version {:?}, but its definition now declares only \
                 {:?} — retiring a shipped version orphans every artifact primed with it. Restore \
                 the dropped version rather than suppressing the check",
                realization.dictionary, realization.version, def.version
            )));
        }
    }
    Ok(())
}

/// Project realizations and envelopes into the [`MEDIUM_REGISTRY_GRAPH`] named
/// graph, as canonical quads in deterministic order.
///
/// # Errors
/// A realization whose authored definition is not registered, or a digest that is
/// not in canonical `blake3:<64 lowercase hex>` form (a malformed digest makes the
/// decoder's pre-priming comparison meaningless, so it is refused at emission rather
/// than shipped).
pub fn project(
    registry: &MediumRegistry,
    realizations: &[DictionaryRealization],
    envelopes: &[MediumEnvelope],
) -> Result<Vec<RdfQuad>, gmeow_errors::Diag> {
    let graph = RdfTerm::iri(MEDIUM_REGISTRY_GRAPH);
    let mut quads: Vec<RdfQuad> = Vec::new();
    let mut emit = |subject: &str, predicate: String, object: RdfTerm| {
        quads.push(RdfQuad::new(RdfTerm::iri(subject), predicate, object).in_graph(graph.clone()));
    };

    // Realizations, in canonical (id, version) order so the projection is a pure
    // function of the registry rather than of the caller's emission order.
    let ordered: BTreeMap<String, &DictionaryRealization> = realizations
        .iter()
        .map(|r| (format!("{}\u{0}{}", r.dictionary, r.version), r))
        .collect();
    for realization in ordered.into_values() {
        let def = registry
            .dictionaries()
            .get(&realization.dictionary)
            .ok_or_else(|| {
                super::invalid_declaration(format!(
                    "cannot project a realization of <{}>: it is not a registered \
                     gmeow:CompressionDictionary",
                    realization.dictionary
                ))
            })?;
        if !is_canonical_digest(&realization.content_digest) {
            return Err(super::digest_mismatch(format!(
                "realization of <{}> carries gmeow:dictionaryContentDigest {:?}, which is not \
                 written 'blake3:<64 lowercase hex>' — a decoder compares that literal against \
                 the dictionary it holds BEFORE priming, so a malformed value makes the primed \
                 decode unsafe",
                realization.dictionary, realization.content_digest
            )));
        }
        let subject = realization.iri(def);
        emit(
            &subject,
            RDF_TYPE.to_string(),
            RdfTerm::iri(gm("CompressionDictionaryRealization")),
        );
        emit(
            &subject,
            gm("realizesDictionary"),
            RdfTerm::iri(&realization.dictionary),
        );
        emit(
            &subject,
            gm("dictionaryVersion"),
            RdfTerm::literal(RdfLiteral::simple(&realization.version)),
        );
        emit(
            &subject,
            gm("dictionaryStrategy"),
            RdfTerm::iri(realization.strategy.iri()),
        );
        emit(
            &subject,
            gm("dictionaryTargetLength"),
            non_negative_integer(realization.target_length),
        );
        emit(
            &subject,
            gm("dictionaryContentDigest"),
            RdfTerm::literal(RdfLiteral::simple(&realization.content_digest)),
        );
        emit(
            &subject,
            gm("dictionaryByteLength"),
            non_negative_integer(realization.byte_length),
        );
        emit(
            &subject,
            gm("zstdDictionaryId"),
            non_negative_integer(realization.zstd_dictionary_id as usize),
        );
    }

    // Envelopes, in canonical frame order for the same reason.
    let ordered: BTreeMap<&str, &MediumEnvelope> =
        envelopes.iter().map(|e| (e.frame.as_str(), e)).collect();
    for envelope in ordered.into_values() {
        for (label, digest) in [
            ("gmeow:strataDigest", &envelope.strata_digest),
            ("gmeow:contentDigest", &envelope.content_digest),
        ] {
            if !is_canonical_digest(digest) {
                return Err(super::digest_mismatch(format!(
                    "envelope for frame <{}> carries {label} {digest:?}, which is not written \
                     'blake3:<64 lowercase hex>'",
                    envelope.frame
                )));
            }
        }
        let subject = envelope_iri(envelope);
        emit(
            &subject,
            RDF_TYPE.to_string(),
            RdfTerm::iri(gm("MediumEnvelope")),
        );
        emit(
            &subject,
            gm("envelopePayloadFrame"),
            RdfTerm::iri(&envelope.frame),
        );
        emit(
            &subject,
            gm("envelopeSchema"),
            RdfTerm::iri(&envelope.schema),
        );
        emit(
            &subject,
            gm("envelopeMedium"),
            RdfTerm::iri(&envelope.medium),
        );
        if let Some(dictionary) = &envelope.dictionary {
            emit(&subject, gm("envelopeDictionary"), RdfTerm::iri(dictionary));
        }
        emit(
            &subject,
            gm("envelopeDigestStratum"),
            RdfTerm::iri(envelope.stratum.iri()),
        );
        emit(
            &subject,
            gm("strataDigest"),
            RdfTerm::literal(RdfLiteral::simple(&envelope.strata_digest)),
        );
        emit(
            &subject,
            gm("contentDigest"),
            RdfTerm::literal(RdfLiteral::simple(&envelope.content_digest)),
        );
    }

    Ok(quads)
}

/// The envelope individual's IRI: derived from the frame it describes, so an
/// envelope is addressable from the frame and two envelopes over one frame collapse
/// onto one subject rather than accumulating.
fn envelope_iri(envelope: &MediumEnvelope) -> String {
    format!(
        "{}envelope/{}",
        super::GMEOW,
        blake3_digest(envelope.frame.as_bytes())
            .strip_prefix("blake3:")
            .expect("blake3_digest always carries the prefix")
    )
}

fn gm(local: &str) -> String {
    format!("{}{local}", super::GMEOW)
}

fn non_negative_integer(value: usize) -> RdfTerm {
    RdfTerm::literal(RdfLiteral::typed(
        value.to_string(),
        XSD_NON_NEGATIVE_INTEGER,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::medium::envelope::{DigestStratum, FrameFacts, seal};
    use crate::medium::registry::{fixture, gm as reg_gm};
    use crate::medium::train;

    fn registry() -> MediumRegistry {
        MediumRegistry::from_dataset(&fixture::dataset("")).expect("fixture registry")
    }

    fn trained_bytes() -> Vec<u8> {
        let owned: Vec<Vec<u8>> = (0..400u32)
            .map(|i| {
                format!(
                    "<https://blackcatinformatics.ca/gmeow/t{}> <https://e/p> \"v{i}\" .\n",
                    i % 31
                )
                .into_bytes()
            })
            .collect();
        let corpus: Vec<&[u8]> = owned.iter().map(Vec::as_slice).collect();
        train::build(DictionaryStrategy::Trained, &corpus, 4096).expect("train")
    }

    #[test]
    fn a_realization_carries_every_measured_field() {
        let registry = registry();
        let def = registry
            .dictionary_by_id("gmeow-core-v1")
            .expect("declared");
        let bytes = trained_bytes();
        let realization = realize(def, &bytes, DictionaryStrategy::Trained).expect("realize");
        assert_eq!(realization.byte_length, bytes.len());
        assert!(is_canonical_digest(&realization.content_digest));
        assert_ne!(realization.zstd_dictionary_id, 0);
        // The MEASURED strategy is its own field: a trainer that fell back must be
        // able to say so without touching the authored definition.
        let fell_back = realize(def, &bytes, DictionaryStrategy::RawContent).expect("realize");
        assert_eq!(fell_back.strategy, DictionaryStrategy::RawContent);
        assert_eq!(def.strategy, DictionaryStrategy::Trained);
    }

    #[test]
    fn the_projection_lands_entirely_in_the_medium_registry_graph() {
        let registry = registry();
        let def = registry
            .dictionary_by_id("gmeow-core-v1")
            .expect("declared");
        let realization =
            realize(def, &trained_bytes(), DictionaryStrategy::Trained).expect("realize");
        let envelope = seal(
            &registry,
            &FrameFacts {
                frame: "https://e/frame7",
                rep: "cells-archive",
                payload: b"payload",
                stratum_bytes: b"stratum",
                stratum: DigestStratum::PayloadExcludingMediumEnvelope,
                dictionary_id: Some("gmeow-core-v1"),
            },
        )
        .expect("seal");

        let quads = project(&registry, &[realization], &[envelope]).expect("project");
        assert!(!quads.is_empty());
        assert!(
            quads
                .iter()
                .all(|q| q.graph_name == Some(RdfTerm::iri(MEDIUM_REGISTRY_GRAPH))),
            "every projected quad lives in the build-time registry graph"
        );
        let predicates: Vec<&str> = quads.iter().map(|q| q.predicate.as_str()).collect();
        for required in [
            "realizesDictionary",
            "dictionaryContentDigest",
            "dictionaryByteLength",
            "zstdDictionaryId",
            "envelopeSchema",
            "envelopeMedium",
            "envelopeDictionary",
            "envelopeDigestStratum",
            "strataDigest",
        ] {
            assert!(
                predicates.contains(&reg_gm(required).as_str()),
                "the projection must carry gmeow:{required}"
            );
        }
    }

    /// The projection is a pure function of the registry, not of the caller's
    /// emission order — two shuffles of the same records produce identical quads.
    #[test]
    fn the_projection_is_emission_order_independent() {
        let registry = registry();
        let core = registry
            .dictionary_by_id("gmeow-core-v1")
            .expect("declared");
        let terms = registry
            .dictionary_by_id("gmeow-terms-v1")
            .expect("declared");
        let bytes = trained_bytes();
        let a = realize(core, &bytes, DictionaryStrategy::Trained).expect("realize");
        let b = realize(terms, &bytes, DictionaryStrategy::TermTable).expect("realize");

        let forward = project(&registry, &[a.clone(), b.clone()], &[]).expect("project");
        let reversed = project(&registry, &[b, a], &[]).expect("project");
        assert_eq!(forward, reversed);
    }

    /// Retiring a shipped version orphans every artifact primed with it.
    #[test]
    fn a_retired_dictionary_version_is_a_regression() {
        let registry = registry();
        let def = registry
            .dictionary_by_id("gmeow-core-v1")
            .expect("declared");
        let mut realization =
            realize(def, &trained_bytes(), DictionaryStrategy::Trained).expect("realize");
        check_dictionary_retention(&registry, &[realization.clone()])
            .expect("the emitted version is still declared");

        realization.version = "0".to_string();
        let diag = check_dictionary_retention(&registry, &[realization])
            .expect_err("a dropped version must be a regression");
        assert_eq!(
            diag.code(),
            crate::error::MediumDictionaryRegression::register(),
            "{diag}"
        );
    }

    /// A malformed digest is refused at EMISSION: it makes the decoder's
    /// pre-priming comparison meaningless, so shipping it would be shipping an
    /// uncheckable claim.
    #[test]
    fn a_malformed_realization_digest_is_refused_at_emission() {
        let registry = registry();
        let def = registry
            .dictionary_by_id("gmeow-core-v1")
            .expect("declared");
        let mut realization =
            realize(def, &trained_bytes(), DictionaryStrategy::Trained).expect("realize");
        realization.content_digest = "not-a-digest".to_string();
        let diag = project(&registry, &[realization], &[])
            .expect_err("a malformed digest must not be projected");
        assert_eq!(
            diag.code(),
            crate::error::MediumDigestMismatch::register(),
            "{diag}"
        );
    }
}
