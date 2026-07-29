// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`Bridge`] abstraction: lift external bytes into `lang:` forms + surfaces under a
//! carried `logic:Correspondence`, and emit them back. This module is code organization
//! only — every identity and law it references is defined in [`gmeow_logic_compile::ir`].

use gmeow_lang_form::{Form, SurfaceForm};
use gmeow_logic_compile::ir::{Correspondence, DischargeVerdict, LegPath};
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;

/// The `lang:` namespace base, kept byte-identical to the translation producer so every
/// `lang:` local name resolves to the same IRI across producers.
use gmeow_ns::LANG_NS;

/// A bridge lifts an external byte stream into `lang:` forms/surfaces (fully or
/// hard-fails) and can emit that product back out.
///
/// The trait deliberately has **no** `preservation()` method and **no** bespoke
/// round-trip harness method: the preservation and round-trip judgments belong to the
/// [`Correspondence`] a lift carries in its [`Lifted`] product, decided by the shared
/// helpers ([`exact_round_trip_holds`], [`is_exact_correspondence`]) over the landed
/// lens-law spine — never a per-bridge law shadow.
pub trait Bridge {
    /// Lift external bytes into a [`Lifted`] product, or hard-fail with a typed
    /// [`IngestDiagnostic`]. A lift is total-or-nothing: it either accounts for its input
    /// completely or reports the exact construct it could not account for; it never
    /// silently drops material (the `lang:SilentIngestDrop` failure class).
    fn lift(&self, bytes: &[u8]) -> Result<Lifted, IngestDiagnostic>;

    /// Emit a previously lifted product back to a byte stream.
    fn emit(&self, lifted: &Lifted) -> Vec<u8>;
}

/// The product of a successful [`Bridge::lift`]: the lifted forms and surfaces, the
/// `logic:Correspondence` the bridge carries for the lift, and the per-item loss-ledger
/// rows the lift accumulated (the honest preservation record).
#[derive(Debug, Clone)]
pub struct Lifted {
    /// The lifted `lang:` forms (structural, surface-free identities).
    pub forms: Vec<Form>,
    /// The surface realizations of those forms (surface material lives here, never on the
    /// form or the correspondence).
    pub surfaces: Vec<SurfaceForm>,
    /// The `logic:Correspondence` this lift carries — the single law spine the round-trip
    /// and exactness judgments are decided over.
    pub correspondence: Correspondence,
    /// The loss-ledger rows accumulated by the lift, each declaring its preservation kind.
    /// The rows carry only identity/judgment; their drops live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The loss store the lift interned every row's drops into (keyed by target focus). Read
    /// each row's residue back through `loss.projection_drops_for(&row.target)`.
    pub loss: LossLedger,
}

/// The typed "lift fully or hard-fail" carrier: a bridge that cannot account for a
/// construct raises this rather than dropping it. `construct` names the offending input
/// fragment so the failure is diagnosable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestDiagnostic {
    /// The `lang:` failure class this diagnostic instantiates.
    pub failure_class: LangFailure,
    /// The concrete construct the bridge could not account for.
    pub construct: String,
}

/// A `lang:` ingestion failure class — the typed reasons a lift may hard-fail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LangFailure {
    /// `lang:SilentIngestDrop` — a construct would be dropped without being accounted for;
    /// the floor a bridge must never cross silently.
    SilentIngestDrop,
    /// `lang:NonUtf8Surface` — the input bytes are not valid UTF-8, so no `lang:SurfaceForm`
    /// can carry them as text. A hard fail, never a silent lossy repair (which would corrupt
    /// the surface material a stable hash depends on).
    NonUtf8Surface,
}

impl LangFailure {
    /// The `lang:` local name exactly as it appears in the module vocabulary.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SilentIngestDrop => "SilentIngestDrop",
            Self::NonUtf8Surface => "NonUtf8Surface",
        }
    }

    /// The full `lang:` IRI (`LANG_NS + local_name`).
    pub fn iri(&self) -> String {
        format!("{LANG_NS}{}", self.as_str())
    }
}

/// Whether the exact-round-trip law holds for a get/put leg pair: `put == get.invert()`
/// over the normalized canonical [`LegPath`] form. This is the DECIDABLE round-trip check
/// the correspondence gates reuse — a structural graph-iso identity over the landed leg
/// bodies, not a data-execution round-trip.
pub fn exact_round_trip_holds(get: &LegPath, put: &LegPath) -> bool {
    put.normalize() == get.invert().normalize()
}

/// Whether a carried correspondence is *exact*: it sits on an injective rung (iso,
/// section/retraction, or well-behaved lens) AND at least one of its law claims is
/// conclusively discharged. Both are required — an injective rung whose laws are all
/// merely `ObligationUnknown` is a claim, not a discharge, so it is not exact.
pub fn is_exact_correspondence(c: &Correspondence) -> bool {
    c.morphism_class.is_injective_rung()
        && c.law_claims
            .iter()
            .any(|lc| lc.verdict == DischargeVerdict::ObligationDischarged)
}
