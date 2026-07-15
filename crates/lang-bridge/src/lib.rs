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
pub mod error;
pub mod gmn1_codec;
pub mod gmn_symbology;
pub mod grammar;
pub mod lower;
pub mod nif;
pub mod ontolex;
pub mod plain_text;
pub mod rdf_scan;
pub mod registry;
pub mod semaf;
pub mod tei;

pub use bcp47::{Bcp47Derivation, Bcp47Target, derive_bcp47_tag};
pub use bridge::{
    Bridge, IngestDiagnostic, LangFailure, Lifted, exact_round_trip_holds, is_exact_correspondence,
};
pub use conllu::{
    ConlluBridge, ConlluDoc, ConlluSentence, ConlluToken, TokenId, UD_SIGN_SYSTEM, analysis_level,
    conllu_correspondence, conllu_leg_pair, parse as parse_conllu, parse_feats,
    serialize as serialize_conllu, to_forms,
};
pub use emit::{assert_no_digest_collision, digest16, ntriples_sorted};
pub use engine::{
    EngineError, EngineRegistry, FixtureEngine, NlpEngine, Reading, interpretation_act_to_ntriples,
};
pub use gmn_symbology::{GMN_LANG_AST_COLUMNS, gmn_glyph_token_cost};
pub use gmn1_codec::{
    ConstructCoverageTally, CoverageReport, Gmn0Model, Gmn1ConstructCategory, Gmn1Document,
    Gmn1Error, GmnDictionary, GmnGlyphRegistry, QuadCoverage, classify_model,
    gmn0_canonically_equal, gmn1_read, gmn1_write, gmn1_write_tabular, measure_coverage,
    round_trip_check,
};
pub use grammar::{
    AbnfBridge, EbnfBridge, Formalism, Grammar, GrammarRule, RuleExpr, canonicalize_expr,
    grammar_correspondence, grammar_leg_pair, grammar_to_ntriples, parse_grammar,
    serialize_grammar,
};
pub use lower::{
    DerivationRule, Lowering, LoweringError, LoweringStage, REQUIRED_STAGES, flagship_svo_sentence,
    grammar_rule_to_derivation, grammar_to_derivation_rules, lower_svo, svo_grammar,
};
pub use nif::NifBridge;
pub use ontolex::{ONTOLEX_SIGN_SYSTEM, OntoLexBridge, ontolex_correspondence};
pub use plain_text::{
    PlainTextBridge, UNDETERMINED_SCRIPT, exact_surface_correspondence, normalization_label,
};
pub use registry::{
    EMISSION_WORTHY_CLASSES, EmittedArtifact, LangEmission, LangProjectionInput,
    LangProjectionTarget, NamedSource, assert_registry_covers, registry,
};
pub use semaf::SemafBridge;
pub use tei::TeiBridge;
