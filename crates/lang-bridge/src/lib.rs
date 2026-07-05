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

pub mod bcp47;
pub mod bridge;
pub mod conllu;
pub mod emit;
pub mod engine;
pub mod grammar;
pub mod lower;
pub mod nif;
pub mod ontolex;
pub mod plain_text;
pub mod rdf_scan;
pub mod registry;
pub mod semaf;
pub mod tei;

pub use bcp47::{derive_bcp47_tag, Bcp47Derivation, Bcp47Target};
pub use bridge::{
    exact_round_trip_holds, is_exact_correspondence, Bridge, IngestDiagnostic, LangFailure, Lifted,
};
pub use conllu::{
    analysis_level, conllu_correspondence, conllu_leg_pair, parse as parse_conllu, parse_feats,
    serialize as serialize_conllu, to_forms, ConlluBridge, ConlluDoc, ConlluSentence, ConlluToken,
    TokenId, UD_SIGN_SYSTEM,
};
pub use emit::{assert_no_digest_collision, digest16, ntriples_sorted};
pub use engine::{
    interpretation_act_to_ntriples, EngineError, EngineRegistry, FixtureEngine, NlpEngine, Reading,
};
pub use grammar::{
    canonicalize_expr, grammar_correspondence, grammar_leg_pair, grammar_to_ntriples,
    parse_grammar, serialize_grammar, AbnfBridge, EbnfBridge, Formalism, Grammar, GrammarRule,
    RuleExpr,
};
pub use lower::{
    flagship_svo_sentence, grammar_rule_to_derivation, grammar_to_derivation_rules, lower_svo,
    svo_grammar, DerivationRule, Lowering, LoweringError, LoweringStage, REQUIRED_STAGES,
};
pub use nif::NifBridge;
pub use ontolex::{ontolex_correspondence, OntoLexBridge, ONTOLEX_SIGN_SYSTEM};
pub use plain_text::{
    exact_surface_correspondence, normalization_label, PlainTextBridge, UNDETERMINED_SCRIPT,
};
pub use registry::{
    assert_registry_covers, registry, ConlluSource, EmittedArtifact, LangEmission,
    LangProjectionInput, LangProjectionTarget, NamedSource, EMISSION_WORTHY_CLASSES,
};
pub use semaf::SemafBridge;
pub use tei::TeiBridge;
