// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `lang:` bridge diagnostic kinds.
//!
//! The content-address collision guard and the functor-totality registry check
//! are HARD failure surfaces (no-optionality): a digest collision must not
//! silently alias two keys, and a dropped projection target must not silently
//! lose a surface. Each defect is a [`DiagKind`](gmeow_errors::DiagKind) minted
//! by [`define_diag_kind!`](gmeow_errors::define_diag_kind).

use gmeow_errors::{Code, FindingCategory, Grade, Severity, Standpoint, define_diag_kind};

use crate::gmn1_codec::Gmn1Error;

define_diag_kind! {
    /// Two DISTINCT full content keys map to the same [`digest16`](crate::digest16)
    /// short IRI segment. The full key is the identity, so a collision is a hard
    /// fail rather than a silent merge onto one short IRI.
    pub struct DigestCollision { prior: String, key: String, digest: String }
    code = "lang-bridge.emit.digest-collision";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "digest collision: distinct keys '{}' and '{}' both map to digest '{}'", prior, key, digest;
}

define_diag_kind! {
    /// A `lang:` class is not listed in `EMISSION_WORTHY_CLASSES`, so the registry
    /// cannot prove it maps to any projection target.
    pub struct ClassNotEmissionWorthy { lang_class: String }
    code = "lang-bridge.registry.class-not-listed";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:{} is not listed in EMISSION_WORTHY_CLASSES; add it with the target(s) that project it", lang_class;
}

define_diag_kind! {
    /// An emission-worthy `lang:` class declares projection target(s) that are not
    /// registered (functor totality breach). `missing` is the debug-rendered list
    /// of unregistered target names.
    pub struct MissingProjectionTargets { lang_class: String, missing: String }
    code = "lang-bridge.registry.missing-targets";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "emission-worthy class lang:{} declares projection target(s) {} that are not registered (functor totality: every declared target must be registered)", lang_class, missing;
}

define_diag_kind! {
    /// `lang:GmnUncoveredTerm` — a GMN-0 construct the GMN-1 codec cannot losslessly
    /// encode or decode: an IRI under no registered namespace, or GMN-1 text carrying a
    /// token/sigil outside the codec's covered fragment. The no-optionality hard fail
    /// behind `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic true` claim: an uncovered
    /// construct is named and reported here, never silently dropped. (RDF 1.2 triple terms
    /// are covered losslessly; a named-graph quad is the distinct
    /// [`GmnGraphOutOfDomain`] boundary, not this residual.)
    pub struct GmnUncoveredTerm { construct: String }
    code = "lang-bridge.gmn1.uncovered-term";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnUncoveredTerm: {}", construct;
}

define_diag_kind! {
    /// `lang:GmnNonCanonicalOrder` — a GMN-1 record's field keys are not in the canonical
    /// key order (`s p o v q st ev m ek bd it`), forfeiting byte-comparability. The typed
    /// read-side counterpart of [`crate::gmn1_codec::Gmn1Error::NonCanonicalOrder`].
    pub struct GmnNonCanonicalOrder { detail: String }
    code = "lang-bridge.gmn1.non-canonical-order";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnNonCanonicalOrder: {}", detail;
}

define_diag_kind! {
    /// `lang:GmnMalformedNumber` — a number-shaped GMN-1 value token outside the grammar's
    /// number production (scientific notation, or a non-two-digit fraction). The typed
    /// read-side counterpart of [`crate::gmn1_codec::Gmn1Error::MalformedNumber`].
    pub struct GmnMalformedNumber { token: String }
    code = "lang-bridge.gmn1.malformed-number";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnMalformedNumber: number-shaped token '{}' is not a canonical integer or two-digit decimal", token;
}

define_diag_kind! {
    /// `lang:GmnUndeclaredDialectVersion` — a GMN-1 document reaches the reader without a
    /// `@gmn` header pinning its dialect/dictionary version. The typed read-side counterpart
    /// of [`crate::gmn1_codec::Gmn1Error::UndeclaredDialectVersion`].
    pub struct GmnUndeclaredDialectVersion { detail: String }
    code = "lang-bridge.gmn1.undeclared-dialect-version";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnUndeclaredDialectVersion: {}", detail;
}

define_diag_kind! {
    /// `lang:GmnNonDecodableGrammar` — the residual GMN-1 grammar defect (unbalanced brace,
    /// unknown sigil/key, duplicate key, schema/row mismatch, or the codec's own round-trip
    /// mismatch). The typed read-side counterpart of
    /// [`crate::gmn1_codec::Gmn1Error::NonDecodableGrammar`].
    pub struct GmnNonDecodableGrammar { detail: String }
    code = "lang-bridge.gmn1.non-decodable-grammar";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnNonDecodableGrammar: {}", detail;
}

define_diag_kind! {
    /// `lang:GmnNonCanonicalCodepoint` — a GMN literal's lexical form is not NFC-normalized,
    /// the Unicode-canonicity failure raised by the codec's write-time literal NFC gate. The
    /// typed counterpart of [`crate::gmn1_codec::Gmn1Error::NonNfcLiteral`], reusing the one
    /// existing normalization/codepoint-canonicity conformance class.
    pub struct GmnNonNfcLiteral { lexical: String }
    code = "lang-bridge.gmn1.non-nfc-literal";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnNonCanonicalCodepoint: literal lexical form '{}' is not NFC-normalized", lexical;
}

define_diag_kind! {
    /// `lang:GmnGraphOutOfDomain` — a quad carries a named graph, which is OUTSIDE the
    /// default-graph GMN-0 normal-form domain (the GMN-1 record shape has no graph slot).
    /// An HONEST domain boundary, not a term-coverage residual: no larger dictionary could
    /// bring a named-graph quad in-domain. The typed counterpart of
    /// [`crate::gmn1_codec::Gmn1Error::NamedGraphOutOfDomain`].
    pub struct GmnGraphOutOfDomain { graph: String }
    code = "lang-bridge.gmn1.graph-out-of-domain";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnGraphOutOfDomain: quad in named graph '{}' is outside the default-graph GMN-0 normal-form domain", graph;
}

/// The complete `lang:` bridge diagnostic-code catalog, in registration order.
pub const LANG_BRIDGE_DIAG_CODES: &[&str] = &[
    DigestCollision::CODE,
    ClassNotEmissionWorthy::CODE,
    MissingProjectionTargets::CODE,
    GmnUncoveredTerm::CODE,
    GmnNonCanonicalOrder::CODE,
    GmnMalformedNumber::CODE,
    GmnUndeclaredDialectVersion::CODE,
    GmnNonDecodableGrammar::CODE,
    GmnNonNfcLiteral::CODE,
    GmnGraphOutOfDomain::CODE,
];

/// Eagerly intern every `lang:` bridge diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        DigestCollision::register(),
        ClassNotEmissionWorthy::register(),
        MissingProjectionTargets::register(),
        GmnUncoveredTerm::register(),
        GmnNonCanonicalOrder::register(),
        GmnMalformedNumber::register(),
        GmnUndeclaredDialectVersion::register(),
        GmnNonDecodableGrammar::register(),
        GmnNonNfcLiteral::register(),
        GmnGraphOutOfDomain::register(),
    ]
}

/// Intern ONE typed GMN-1 codec failure into `ledger` with DiagLedger finding identity
/// (finding_iri + anchor), focused on `focus` (the source/artifact the failure came from) —
/// the SAME interning mechanism [`attach_pipeline_finding`](../../pipeline/src/run.rs) uses,
/// generalized across every [`Gmn1Error`] variant so a reasoner/meta-fold over the finding
/// graph can join any GMN validator-tier failure by its class, never a hand-built Finding.
///
/// Dispatch is driven off the codec's ONE canonical classifier ([`Gmn1Error::failure_class`]):
/// the variant selects the matching typed [`gmeow_errors::DiagKind`], so there is a single
/// class→finding mapping, not a second classifier.
pub fn attach_gmn_failure(
    ledger: &mut gmeow_errors::DiagLedger,
    stage_id: &str,
    focus: &str,
    error: &Gmn1Error,
) {
    let diag = match error {
        Gmn1Error::Uncovered(term) => gmeow_errors::Diag::of_kind(GmnUncoveredTerm {
            construct: term.0.clone(),
        }),
        Gmn1Error::NonCanonicalOrder { detail } => {
            gmeow_errors::Diag::of_kind(GmnNonCanonicalOrder {
                detail: detail.clone(),
            })
        }
        Gmn1Error::MalformedNumber { token } => gmeow_errors::Diag::of_kind(GmnMalformedNumber {
            token: token.clone(),
        }),
        Gmn1Error::UndeclaredDialectVersion { detail } => {
            gmeow_errors::Diag::of_kind(GmnUndeclaredDialectVersion {
                detail: detail.clone(),
            })
        }
        Gmn1Error::NonDecodableGrammar { detail } => {
            gmeow_errors::Diag::of_kind(GmnNonDecodableGrammar {
                detail: detail.clone(),
            })
        }
        Gmn1Error::NonNfcLiteral { lexical } => gmeow_errors::Diag::of_kind(GmnNonNfcLiteral {
            lexical: lexical.clone(),
        }),
        Gmn1Error::NamedGraphOutOfDomain { graph } => {
            gmeow_errors::Diag::of_kind(GmnGraphOutOfDomain {
                graph: graph.clone(),
            })
        }
        // A per-claim mismatch routes through the SAME `GmnNonDecodableGrammar` finding as
        // the whole-model round-trip failure (its `failure_class` is
        // `CLASS_NON_DECODABLE_GRAMMAR`), naming the offending canonical subject in the
        // detail so a meta-fold joins it by class without a second classifier.
        Gmn1Error::PerClaimMismatch { subject } => {
            gmeow_errors::Diag::of_kind(GmnNonDecodableGrammar {
                detail: format!("per-claim round-trip mismatch at canonical subject {subject}"),
            })
        }
    };
    let diag = diag.with_focus(focus.to_owned());
    ledger.attach(diag, gmeow_errors::StageId::new(stage_id.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_errors::intern_code;
    use std::collections::HashSet;

    #[test]
    fn every_lang_bridge_code_interns_with_no_collision() {
        let handles = register_all();
        assert_eq!(
            handles.len(),
            LANG_BRIDGE_DIAG_CODES.len(),
            "register_all() and LANG_BRIDGE_DIAG_CODES must enumerate the same kinds"
        );
        for code in LANG_BRIDGE_DIAG_CODES {
            assert!(
                intern_code(code).is_ok(),
                "lang-bridge code `{code}` did not intern after register_all()"
            );
        }
        let distinct_strings: HashSet<&&str> = LANG_BRIDGE_DIAG_CODES.iter().collect();
        assert_eq!(
            distinct_strings.len(),
            LANG_BRIDGE_DIAG_CODES.len(),
            "duplicate lang-bridge diagnostic code string detected"
        );
        let distinct_handles: HashSet<Code> = handles.iter().copied().collect();
        assert_eq!(distinct_handles.len(), handles.len());
    }
}
