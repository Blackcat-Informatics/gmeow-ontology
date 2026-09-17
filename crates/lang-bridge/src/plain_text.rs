// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`PlainTextBridge`]: the total prose lift.
//!
//! Every external byte stream that is valid UTF-8 lifts into exactly ONE
//! [`SurfaceForm`] — the raw text, framed by its declared script, encoding, Unicode
//! normalization, and collation locale — held at [`AnalysisLevel::Raw`](gmeow_lang_form::AnalysisLevel::Raw):
//! honest `lang:UnanalyzedProse`. No segmentation, no tokenization, no structure is
//! invented; the "unanalyzed" status is a RECORDED graded level, never a projection loss.
//!
//! The lift CARRIES a `logic:Correspondence` (never a bespoke round-trip harness) that is
//! **exact on the surface stratum**: [`Bridge::emit`] re-emits the lifted bytes verbatim,
//! so the surface round-trip is an [`Isomorphism`](MorphismClass::Isomorphism) whose
//! `GetPut`/`PutGet`/`SectionLaw` claims are conclusively discharged (the identity map
//! trivially satisfies them). [`is_exact_correspondence`](crate::is_exact_correspondence)
//! reads that fact off the landed correspondence.
//!
//! Non-UTF-8 input is a HARD FAIL ([`LangFailure::NonUtf8Surface`]): a lossy repair would
//! corrupt the surface material a stable hash is taken over, so the bridge refuses it
//! rather than silently dropping or mangling bytes.

use gmeow_lang_form::SurfaceForm;
use gmeow_logic_compile::ir::{
    Correspondence, CorrespondenceLaw, CorrespondenceRelation, Determinacy, DischargeCondition,
    DischargeVerdict, LawClaimIr, MorphismClass, MorphismKind, PreservationKind,
};

use crate::bridge::{Bridge, IngestDiagnostic, LangFailure, Lifted};
use crate::emit::digest16;

/// The ISO 15924 script code a plain-text lift declares for its surface: `Zyyy`
/// (undetermined / common). A raw plain-text lift does NOT analyze script, so it declares
/// the undetermined code honestly rather than guessing a specific script; the byte
/// round-trip it proves is script-independent. A caller that KNOWS the script (the prose
/// corpus resolves it from the language tag) builds its own [`SurfaceForm`] frame.
pub const UNDETERMINED_SCRIPT: &str = "Zyyy";

/// The example-instance base the carried surface-round-trip correspondence IRI lives under,
/// matching the base every other `lang:` producer content-addresses its minted individuals
/// under.
const PLAIN_TEXT_CORR_BASE: &str = "http://example.org/lang/plain-text-correspondence/";

/// The plain-text bridge: lift any UTF-8 byte stream into one raw `lang:UnanalyzedProse`
/// surface under an exact surface-round-trip `logic:Correspondence`.
pub struct PlainTextBridge;

/// The HONEST Unicode normalization-form label for `text` — the form the bytes are
/// ACTUALLY in, decided by a UAX #15 quick-check, never an assumed `NFC`. ASCII and most
/// prose are simultaneously NFC and NFD, and `NFC` is the honest label there; a string that
/// is in no standard normal form is declared `unnormalized` rather than mislabeled.
///
/// This is a DECLARED FRAME label only: it never transforms the text. The surface hash is
/// taken over the raw bytes, so the label describes them rather than reshaping them.
pub fn normalization_label(text: &str) -> &'static str {
    use unicode_normalization::{is_nfc, is_nfd, is_nfkc, is_nfkd};
    if is_nfc(text) {
        "NFC"
    } else if is_nfd(text) {
        "NFD"
    } else if is_nfkc(text) {
        "NFKC"
    } else if is_nfkd(text) {
        "NFKD"
    } else {
        "unnormalized"
    }
}

/// Build the EXACT surface-round-trip `logic:Correspondence` a plain-text lift carries for
/// `surface`: an [`Isomorphism`](MorphismClass::Isomorphism) on the satisfaction-preserving
/// rung whose `GetPut`, `PutGet`, and `SectionLaw` claims are conclusively discharged (the
/// identity surface map trivially satisfies them, decidable by syntactic reachability). The
/// IRI is content-addressed on the surface's material key, so the same surface always
/// carries the same correspondence.
pub fn exact_surface_correspondence(
    surface: &SurfaceForm,
) -> Result<Correspondence, IngestDiagnostic> {
    let iri = format!(
        "{PLAIN_TEXT_CORR_BASE}{}",
        digest16("lang-plain-text-corr", &surface.surface_key())
    );
    let discharged = |law: CorrespondenceLaw| LawClaimIr {
        law,
        verdict: DischargeVerdict::ObligationDischarged,
        condition: Some(DischargeCondition::DischargeSyntacticReachability),
    };
    Correspondence::new(
        iri,
        CorrespondenceRelation::Equiv,
        MorphismClass::Isomorphism,
        MorphismKind::InstitutionMorphism,
        // The identity forward leg retains the full source witness.
        true,
        Some(Determinacy::Crisp),
        None,
        None,
        vec![
            discharged(CorrespondenceLaw::GetPut),
            discharged(CorrespondenceLaw::PutGet),
            discharged(CorrespondenceLaw::SectionLaw),
        ],
        None,
        None,
        None,
        None,
        None,
        None,
    )
    .map_err(|construct| IngestDiagnostic {
        failure_class: LangFailure::SilentIngestDrop,
        construct: format!("exact surface correspondence is not well-formed: {construct}"),
    })
}

impl Bridge for PlainTextBridge {
    fn lift(&self, bytes: &[u8]) -> Result<Lifted, IngestDiagnostic> {
        // Valid UTF-8 or HARD FAIL — never a lossy repair that would corrupt surface
        // material. A degenerate (empty / whitespace-only / control-char) string is still a
        // lawful raw surface and lifts; only non-UTF-8 bytes are refused.
        let text = std::str::from_utf8(bytes).map_err(|e| IngestDiagnostic {
            failure_class: LangFailure::NonUtf8Surface,
            construct: format!(
                "non-UTF-8 input: {} byte(s), first invalid byte at index {}",
                bytes.len(),
                e.valid_up_to()
            ),
        })?;
        let surface = SurfaceForm {
            text: text.to_owned(),
            script: UNDETERMINED_SCRIPT.to_owned(),
            encoding: "UTF-8".to_owned(),
            normalization: normalization_label(text).to_owned(),
            collation: "und".to_owned(),
        };
        let correspondence = exact_surface_correspondence(&surface)?;
        // One honest ledger row: the surface stratum round-trips exactly (nothing is
        // dropped). The unanalyzed status is recorded via the raw analysis level, never
        // charged as a projection loss.
        let mut loss = crate::registry::LossLedger::new();
        let ledger = vec![crate::registry::emit_ledger_row(
            &mut loss,
            format!(
                "lang-plain-text:{}",
                digest16("lang-plain-text", &surface.surface_key())
            ),
            String::new(),
            false,
            PreservationKind::Exact,
            "n/a".to_owned(),
            Vec::new(),
            Vec::new(),
        )];
        Ok(Lifted {
            forms: Vec::new(),
            surfaces: vec![surface],
            correspondence,
            ledger,
            loss,
        })
    }

    fn emit(&self, lifted: &Lifted) -> Vec<u8> {
        // The surface stratum round-trips exactly: re-emit the lifted surface's text bytes
        // verbatim. A plain-text lift yields exactly one surface; an absent surface emits
        // nothing (there was no text to carry).
        lifted
            .surfaces
            .first()
            .map(|s| s.text.clone().into_bytes())
            .unwrap_or_default()
    }
}

#[path = "plain_text.tests.rs"]
#[cfg(test)]
mod tests;
