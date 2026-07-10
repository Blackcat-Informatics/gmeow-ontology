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
    /// encode or decode: an IRI under no registered namespace, a quoted RDF 1.2 triple
    /// term, a named-graph-scoped quad, or GMN-1 text carrying a token/sigil outside the
    /// codec's covered fragment. The no-optionality hard fail behind
    /// `gmeow:gmnCorrNormalToGmn`'s `logic:mnemomorphic true` claim: an uncovered
    /// construct is named and reported here, never silently dropped.
    pub struct GmnUncoveredTerm { construct: String }
    code = "lang-bridge.gmn1.uncovered-term";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "lang:GmnUncoveredTerm: {}", construct;
}

/// The complete `lang:` bridge diagnostic-code catalog, in registration order.
pub const LANG_BRIDGE_DIAG_CODES: &[&str] = &[
    DigestCollision::CODE,
    ClassNotEmissionWorthy::CODE,
    MissingProjectionTargets::CODE,
    GmnUncoveredTerm::CODE,
];

/// Eagerly intern every `lang:` bridge diagnostic code (idempotent).
pub fn register_all() -> Vec<Code> {
    vec![
        DigestCollision::register(),
        ClassNotEmissionWorthy::register(),
        MissingProjectionTargets::register(),
        GmnUncoveredTerm::register(),
    ]
}

/// Intern one `lang:GmnUncoveredTerm` finding into `ledger`, focused on `focus` (the
/// fixture/source name the uncovered construct came from) — the LossLedger/DiagLedger
/// identity discipline (finding_iri + anchor + antecedents) every hard-fail finding in
/// this codebase routes through, never a bespoke ad hoc error type. Mirrors
/// `crates/pipeline/src/run.rs`'s `attach_pipeline_finding` for the drift/superset
/// findings — the SAME mechanism, applied to the GMN-1 round-trip gate's findings.
pub fn attach_gmn_uncovered(
    ledger: &mut gmeow_errors::DiagLedger,
    stage_id: &str,
    focus: &str,
    construct: &str,
) {
    let diag = gmeow_errors::Diag::of_kind(GmnUncoveredTerm {
        construct: construct.to_owned(),
    })
    .with_focus(focus.to_owned());
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
