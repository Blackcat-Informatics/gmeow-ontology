// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! `gmeow-lang-bridge` — the shared `lang:` ingester/emitter skeleton.
//!
//! Every `lang:` bridge (plain text, CoNLL-U, EBNF/ABNF, OntoLex-Lemon, …) lifts an
//! external byte stream into [`gmeow_lang_form`] forms and surfaces, and does so under a
//! single transformational rule: **a bridge CARRIES a `logic:Correspondence`** — the
//! landed lens-law spine in [`gmeow_logic_compile::ir`] — rather than minting a parallel
//! `lang:` law-spine or a bespoke round-trip harness. This mirrors how the translation
//! producer already carries a `logic:Correspondence` for each crossing: the round-trip,
//! exactness, and preservation judgments a bridge makes are decided over the *same*
//! [`Correspondence`](gmeow_logic_compile::ir::Correspondence) machinery, so there is one
//! law spine in the system, not one per surface.
//!
//! This crate is CODE ORGANIZATION ONLY: it declares the [`Bridge`] trait, the [`Lifted`]
//! product a lift yields, the typed [`IngestDiagnostic`] a hard-failing lift raises, and
//! two thin helpers ([`exact_round_trip_holds`], [`is_exact_correspondence`]) that read
//! decidable facts off the landed correspondence IR. The identity and the laws themselves
//! live in `logic-compile`, never here.
//!
//! [`emit`] extracts the deterministic N-Triples / content-address emitter shared by the
//! translation corpus, the form corpus, and future projection corpora, so there is one
//! digest and one line-canonicalization implementation across every `lang:` producer.

pub mod bridge;
pub mod emit;
pub mod plain_text;

pub use bridge::{
    exact_round_trip_holds, is_exact_correspondence, Bridge, IngestDiagnostic, LangFailure, Lifted,
};
pub use emit::{assert_no_digest_collision, digest16, ntriples_sorted};
pub use plain_text::{
    exact_surface_correspondence, normalization_label, PlainTextBridge, UNDETERMINED_SCRIPT,
};
