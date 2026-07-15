// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The **lawful `put` leg, derived from the same node as `get`** (F4 — the up-lift that
//! replaces the SSSOM-reading heuristic).
//!
//! A `logic:Correspondence` carries `get` (down-projection) and, for a mnemomorphic cell,
//! a `put` (up-projection) obtained by **projecting along the retained witness** — *not*
//! by re-deriving a plausible source. This module is that derivation: given a
//! correspondence with a `get` leg and no authored `put`, [`derive_put`] mints the lawful
//! `put` leg (or honestly declares the cell `unsupported`), and [`CorrespondenceProgram::with_derived_puts`]
//! folds the result back so every up-lift leg is a projection of the *same* canonical
//! object as its `get`.
//!
//! # The two lawful sources of `put` (take1 §6.1) — and the honest floor
//!
//! Lawful `put` comes from exactly two sources, never from naive backward-execution (the
//! amnesic anti-pattern):
//!
//! 1. **a mnemomorphic witness** — `mnemomorphic = true` on an injective-enough rung
//!    (≤ [`crate::ir::MorphismClass::WellBehavedLens`]). The witness recovers
//!    `S`, so `put` is the projection along it: a `CompleteOver` up-lift carrying
//!    a (provisional) discharged [`CorrespondenceLaw::SectionLaw`] — `put ∘ get
//!    = id_S`.
//! 2. **a co-authored put-with-claim** — the author declares a law status (a non-empty
//!    `law_claims`) without a retained witness. The `put` is *minted-with-claim*: a
//!    candidate preimage, `ValidationOnly`, carrying an honest
//!    [`DischargeVerdict::ObligationUnknown`] [`CorrespondenceLaw::PutGet`].
//!
//! Neither a witness nor a co-authored claim ⇒ the up-lift is **`Unsupported`**: the
//! construct is carried and flagged in the loss ledger, never silently minted (the
//! legalization floor — LOGIC-IR.md § Lowering).
//!
//! # Decidability without an execution engine (F3 is off this path)
//!
//! The round-trip law `put ∘ get = id_S` is decided **structurally over the leg bodies**,
//! not by running a leg on data: the lawful `put` body is the structural inverse
//! [`crate::ir::LegPath::invert`] of the resolved `get` body, and the conformance Round-trip
//! gate verifies `put == get.invert()` over the normalized canonical path form (a graph-iso
//! over the canonical IR, `LOGIC-CONFORMANCE.md`). No leg is run on data, so the F3 executor
//! stays off this path — yet a `put` whose *body* is the wrong path genuinely fails (it is
//! NOT a string compare of mint IRIs; the leg IRI is only the leg's content-addressed name).

use sha2::{Digest, Sha256};

use gmeow_errors::Diag;

use crate::ir::{
    Correspondence, CorrespondenceLaw, DischargeCondition, DischargeVerdict, LawClaimIr,
    MorphismClass, PreservationKind, TransactionProgramIr,
};

use super::correspondence::CorrespondenceProgram;

/// A `put` leg derived from the same node as `get`: the minted leg IRI, whether it is a
/// lawful recovery (vs a minted-with-claim candidate preimage), the law claim the
/// derivation licenses, the up-lift preservation polarity, and the loss-ledger residue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedPut {
    /// The minted `put` leg IRI (`<get_leg>/put#<sha8>`), content-addressed to the
    /// get-side identity so the Round-trip gate recomputes it decidably.
    pub put_leg: String,
    /// `true` ⇒ a lawful recovery (projection along the witness); `false` ⇒ a
    /// minted-with-claim candidate preimage (co-authored put-with-claim).
    pub mnemomorphic_recovery: bool,
    /// The law claim this derivation licenses — a (provisional) discharged `SectionLaw`
    /// for a recovery, an honest `ObligationUnknown` `PutGet` for a minted-with-claim.
    pub section_claim: LawClaimIr,
    /// The up-lift preservation polarity for the loss ledger: `CompleteOver` for an
    /// invertible recovery, `ValidationOnly` for minted-with-claim.
    pub preservation: PreservationKind,
    /// The per-cell loss-ledger residue (empty for a recovery; the mint-with-claim
    /// disclosure otherwise). Never silent.
    pub residue: Vec<String>,
}

/// The outcome of deriving the `put` leg for one correspondence: either a lawful
/// [`DerivedPut`], or `Unsupported` (no witness and no co-authored claim — the up-lift is
/// carried and flagged in the loss ledger, never minted).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PutDerivation {
    /// A lawful derived `put` leg.
    Derived(DerivedPut),
    /// The up-lift is unsupported for this cell: `get` is non-injective and no witness or
    /// co-authored claim exists. The residue is the carried-and-flagged disclosure.
    Unsupported {
        /// The loss-ledger residue disclosing the unsupported up-lift (never empty).
        residue: Vec<String>,
    },
}

/// The content-addressed `put` leg IRI (the leg's NAME) for a correspondence:
/// `<get_leg>/put#<sha8>` where the hash folds ONLY the get-side identity the derivation
/// reads — the IRI, relation, morphism class/kind, the `mnemomorphic` bit, and the `get_leg`.
/// This is a stable *name* for the minted leg; it is NOT what the round-trip gate checks.
/// The gate composes the leg BODIES (`put == get.invert()` over canonical path form), so a
/// matching mint IRI never substitutes for a matching body.
pub fn derived_put_iri(get_leg: &str, c: &Correspondence) -> String {
    let key = format!(
        "{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}\u{1f}{}",
        c.iri,
        c.relation.as_str(),
        c.morphism_class.as_str(),
        c.morphism_kind.as_str(),
        if c.mnemomorphic { "true" } else { "false" },
        get_leg,
    );
    let digest = Sha256::digest(key.as_bytes());
    let short: String = digest.iter().take(8).map(|b| format!("{b:02x}")).collect();
    format!("{get_leg}/put#{short}")
}

/// The three lawful up-lift classes a cell can fall into — the single authority for the
/// `put` polarity decision, keyed off exactly the three inputs the derivation reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PutClass {
    /// A lawful recovery: a mnemomorphic witness on an injective-enough rung projects the
    /// `put` — a `CompleteOver` up-lift.
    CompleteOver,
    /// A minted-with-claim candidate preimage: no witness, but a co-authored law status —
    /// a `ValidationOnly` up-lift.
    ValidationOnly,
    /// Neither a witness nor a co-authored claim: the honest legalization floor.
    Unsupported,
}

/// Classify one cell's `put` up-lift from the three inputs the derivation reads — the
/// SINGLE authority for the polarity decision. The `CompleteOver` arm is the
/// witness-retraction predicate: `mnemomorphic` on an injective-enough rung
/// (`Isomorphism` / `SectionRetraction` / `WellBehavedLens`), decided by the explicit
/// [`MorphismClass::is_injective_rung`] predicate rather than the fragile derived-`Ord`
/// `≤ WellBehavedLens` comparison (a spine reorder must not silently reclassify recovery).
/// A non-empty `law_claims` without a witness is `ValidationOnly`; otherwise `Unsupported`.
pub(crate) fn classify_put(
    mnemomorphic: bool,
    morphism_class: MorphismClass,
    law_claims: &[LawClaimIr],
) -> PutClass {
    if mnemomorphic && morphism_class.is_injective_rung() {
        PutClass::CompleteOver
    } else if !law_claims.is_empty() {
        PutClass::ValidationOnly
    } else {
        PutClass::Unsupported
    }
}

/// Derive the lawful `put` leg for one correspondence (which must carry a `get` leg and no
/// authored `put` leg).
///
/// # Errors
///
/// Hard-fails (no-optionality) if the cell carries no `get` leg (there is no view for the
/// witness to travel in) or already carries a `put` leg (the caller must not re-derive an
/// authored leg).
pub fn derive_put(c: &Correspondence) -> gmeow_errors::Result<PutDerivation> {
    let Some(get_leg) = c.get_leg.as_deref() else {
        return Err(Diag::of_kind(crate::error::Put {
            detail: format!(
                "derive_put on <{}>: no logic:getLeg — there is no view for the witness to \
                 travel in, so no lawful put can be derived",
                c.iri
            ),
        }));
    };
    if c.put_leg.is_some() {
        return Err(Diag::of_kind(crate::error::Put {
            detail: format!(
                "derive_put on <{}>: already carries a logic:putLeg; the derivation is only for \
                 cells whose put is to be derived from the witness, never to overwrite an \
                 authored leg",
                c.iri
            ),
        }));
    }

    let mint = derived_put_iri(get_leg, c);

    match classify_put(c.mnemomorphic, c.morphism_class, &c.law_claims) {
        // Source (1): the mnemomorphic witness. put is the projection along it — a lawful
        // recovery. The SectionLaw is discharged PROVISIONALLY (the conformance Round-trip
        // / Law gate confirms it, or degrades it to ObligationUnknown — debugify/Alive2:
        // validate the transform, don't trust it).
        PutClass::CompleteOver => Ok(PutDerivation::Derived(DerivedPut {
            put_leg: mint,
            mnemomorphic_recovery: true,
            section_claim: LawClaimIr {
                law: CorrespondenceLaw::SectionLaw,
                verdict: DischargeVerdict::ObligationDischarged,
                condition: Some(DischargeCondition::DischargeFiniteClosure),
            },
            preservation: PreservationKind::CompleteOver,
            residue: Vec::new(),
        })),

        // Source (2): a co-authored put-with-claim — signalled by a declared law status
        // (non-empty law_claims). The put is minted-with-claim: a candidate preimage,
        // ValidationOnly, ObligationUnknown.
        //
        // A `mnemomorphic` flag on a NON-injective rung is NOT a source: the witness cannot
        // honour the rung, so there is no lawful recovery to project (`classify_put` already
        // rejected it from `CompleteOver`). Minting a recovery put for it would have
        // `derive_put` disagree with the Mnemomorphism gate, which REDs that incoherent
        // declaration — two sources of truth for one coherence rule. It falls to
        // `ValidationOnly` only when a claim is authored; otherwise `Unsupported`.
        PutClass::ValidationOnly => Ok(PutDerivation::Derived(DerivedPut {
            put_leg: mint,
            mnemomorphic_recovery: false,
            section_claim: LawClaimIr {
                law: CorrespondenceLaw::PutGet,
                verdict: DischargeVerdict::ObligationUnknown,
                condition: None,
            },
            preservation: PreservationKind::ValidationOnly,
            residue: vec![
                "put minted-with-claim: a candidate preimage under a declared law status, \
                 NOT a lawful recovery — naive backward-execution is forbidden as the \
                 architecture (take1 §6.1)"
                    .to_owned(),
            ],
        })),

        // Neither a witness nor a co-authored claim: the honest legalization floor. The
        // up-lift is carried and flagged, never minted (LOGIC-IR.md § Lowering).
        PutClass::Unsupported => Ok(PutDerivation::Unsupported {
            residue: vec![
                "up-lift unsupported: get is non-injective and the cell carries neither a \
                 mnemomorphic witness nor a co-authored put-with-claim (take1 §11)"
                    .to_owned(),
            ],
        }),
    }
}

/// The per-correspondence outcome of [`CorrespondenceProgram::with_derived_puts`]: the
/// correspondence IRI and the derivation result, for the gate report and the loss ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivedPutOutcome {
    /// The correspondence IRI the derivation is for.
    pub correspondence: String,
    /// The derivation outcome (a lawful derived put, or unsupported).
    pub derivation: PutDerivation,
}

impl CorrespondenceProgram {
    /// Derive the `put` leg for every member correspondence that carries a `get` leg and
    /// no authored `put` leg, returning the rebuilt program (with the minted legs and
    /// section claims folded in) plus the per-cell derivation outcomes for the gates and
    /// the loss ledger.
    ///
    /// A cell with an authored `put` (e.g. the §14 affine triangle) is left untouched; an
    /// `Unsupported` cell keeps its `put`-less form (the residue is carried in the
    /// returned outcome). Hard-fails only on a genuinely malformed cell (a derivation
    /// error or a rebuild that violates the constructor invariants).
    pub fn with_derived_puts(self) -> gmeow_errors::Result<(Self, Vec<DerivedPutOutcome>)> {
        let CorrespondenceProgram {
            correspondences,
            caveats,
            preservation,
            leg_programs,
        } = self;

        let mut rebuilt = Vec::with_capacity(correspondences.len());
        let mut outcomes = Vec::new();
        // The leg registry grows as we mint puts: a derived put leg is registered with its
        // BODY — the structural inverse of the resolved get body — so the round-trip gate
        // can later compose `put == get.invert()` over real path bodies, not IRI strings.
        let mut legs = leg_programs;

        for c in correspondences {
            if c.put_leg.is_some() || c.get_leg.is_none() {
                rebuilt.push(c);
                continue;
            }
            let iri = c.iri.clone();
            let derivation = derive_put(&c)?;
            match &derivation {
                PutDerivation::Derived(dp) => {
                    let mut law_claims = c.law_claims.clone();
                    law_claims.push(dp.section_claim);
                    // Register the derived put leg's body (the inverse of the get body), when
                    // the get leg resolves to a body. A bodyless get leg mints the put IRI but
                    // no body, so the round-trip gate REDs an unverifiable claim rather than
                    // passing it vacuously.
                    if let Some(get_iri) = c.get_leg.as_deref()
                        && let Some(get_body) = legs
                            .iter()
                            .find(|p| p.iri == get_iri)
                            .map(|p| p.body.clone())
                    {
                        legs.push(TransactionProgramIr {
                            iri: dp.put_leg.clone(),
                            body: get_body.invert(),
                        });
                    }
                    let mut rebuilt_correspondence = Correspondence::new(
                        c.iri.clone(),
                        c.relation,
                        c.morphism_class,
                        c.morphism_kind,
                        c.mnemomorphic,
                        c.determinacy,
                        c.get_leg.clone(),
                        Some(dp.put_leg.clone()),
                        law_claims,
                        c.confidence,
                        c.evidence_strength,
                        c.weight,
                        c.probability,
                        c.according_to.clone(),
                        // Preserve the authored per-correspondence preservation judgment so
                        // the derived program the gates run over still sees the rung.
                        c.preservation,
                    )?;
                    if let (Some(source), Some(target)) = (&c.source_endpoint, &c.target_endpoint) {
                        rebuilt_correspondence = rebuilt_correspondence
                            .with_endpoints(source.clone(), target.clone())?;
                    }
                    if c.grounding {
                        rebuilt_correspondence = rebuilt_correspondence.as_grounding();
                    }
                    rebuilt.push(rebuilt_correspondence);
                }
                PutDerivation::Unsupported { .. } => {
                    // No lawful put: keep the put-less cell; the residue rides the outcome.
                    rebuilt.push(c);
                }
            }
            outcomes.push(DerivedPutOutcome {
                correspondence: iri,
                derivation,
            });
        }

        Ok((
            CorrespondenceProgram::new(rebuilt, caveats, preservation).with_leg_programs(legs),
            outcomes,
        ))
    }
}

#[cfg(test)]
mod tests;
