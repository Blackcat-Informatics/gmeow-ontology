// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only
//! Executed lens-law discharge for a `logic:Correspondence`.
//!
//! A correspondence is an asymmetric lens: a forward `get` leg (down-projection to an
//! external vocabulary) and an inverse `put` leg (the ingest up-lift). The prior
//! round-trip gate compared the two legs' `LegPath` *bodies* syntactically — a purely
//! textual inversion audit that a re-authored cell carrying an unrecoverable guard atom
//! could slip past (the `mapSiocTopic` failure mode). This module discharges the laws by
//! EXECUTION instead: it RUNS both SPARQL `CONSTRUCT` legs through the single native
//! authority in [`gmeow_logic::correspondence_exec`] and compares the resulting atom sets.
//! A verdict records bounded behavioural evidence on the declared seeds. It is not
//! authority for an unrestricted optimizer rewrite.
//!
//! The two laws execute over independently selected input domains:
//!
//! * [`CorrespondenceLaw::SectionLaw`](gmeow_logic_compile::ir::CorrespondenceLaw::SectionLaw) — `put ∘ get = id_S`. For each source seed `s`:
//!   run `get` over `s` → the forward image `v`; run `put` over `v` → the recovered source
//!   `s'`; the law holds on that seed iff `s' == s` (no spurious atom fabricated, no source
//!   atom dropped). Discharged iff every seed round-trips exactly; otherwise Violated with a
//!   [`Countermodel`] naming the failing seed and its spurious/missing atoms.
//! * [`CorrespondenceLaw::PutGet`](gmeow_logic_compile::ir::CorrespondenceLaw::PutGet) — `get ∘ put = id_V` on independently selected view seeds:
//!   `get(put(v)) == v`. View seeds come from the put input pattern, independently
//!   of the get output. Both laws share the prepared native legs, not their domains.
//!   This CONSTRUCT fragment replaces the source; these checks do not establish
//!   the stateful GetPut or PutPut laws.
//!
//! ## Branch coverage and evidence scope
//!
//! A single happy-path seed is a *test*, not a *proof*: a `put` atom that fabricates only on
//! inputs the seed never exercises would round-trip cleanly and pass. So [`derive_seeds`]
//! synthesises one seed per `UNION` branch of the selected leg's `WHERE` clause — instantiating
//! every positive triple pattern of that branch with fresh, deterministic per-variable IRIs
//! (`http://seed.example/vN`) — PLUS one combined seed unioning all branches. Every guard
//! atom and every variable position of `get` is therefore exercised at least once. A `put`
//! branch that fabricates an atom keyed to a specific `get` branch's data is forced to fire
//! under that branch's dedicated seed, where its fabricated atom is not among the seed's
//! source atoms — so the round-trip inequality surfaces it. The combined seed additionally
//! exercises cross-branch interference. Nothing here reads the clock or a random source; the
//! seed IRIs vary only by a deterministic index, so a verdict (and its countermodel bytes)
//! are reproducible.

pub use gmeow_logic::correspondence_exec::{
    Atom, Countermodel, DischargeOutcome, SeedGraph, derive_seeds, discharge_algebra_laws,
    discharge_laws, discharge_put_get_law, discharge_section_law,
};

#[cfg(test)]
use gmeow_logic_compile::ir::{CorrespondenceLaw, DischargeVerdict, MorphismClass};
#[cfg(test)]
use std::collections::{BTreeMap, BTreeSet};
#[cfg(test)]
use std::sync::OnceLock;

#[path = "correspondence_law.tests.rs"]
#[cfg(test)]
mod tests;

#[cfg(test)]
#[path = "correspondence_law_test_support.rs"]
mod test_support;
#[cfg(test)]
use test_support::term_str;
