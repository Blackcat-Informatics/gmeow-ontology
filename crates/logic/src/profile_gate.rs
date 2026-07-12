// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Profile gates for retired cut syntax and native procedural builtins.
//!
//! # Structural no-write firewall
//!
//! The query path accepts only `&dyn WorldFactSource`: there is no write path in
//! goal resolution. Cut (`!`) is recognized by the parser solely to produce a clear,
//! typed retirement diagnostic before evaluation.
//!
//! `logic:ProceduralExecution` remains the facet that licenses the closed set of
//! native arithmetic/comparison builtins. It no longer licenses search-control
//! semantics.

use crate::query_ir::{QBodyLit, QProgram};
use gmeow_logic_compile::ir::SemanticProfileId;

/// The canonical full IRI for the procedural Prolog profile (used in diagnostics).
pub const PROCEDURAL_PROLOG_PROFILE: &str =
    "https://blackcatinformatics.ca/logic/ProceduralPrologProfile";

/// Return `true` if any rule body in `program` contains a [`QBodyLit::Cut`].
pub fn has_cut(program: &QProgram) -> bool {
    program
        .rules
        .iter()
        .any(|rule| rule.body.iter().any(|lit| matches!(lit, QBodyLit::Cut)))
}

/// Reject retired cut syntax before any engine evaluates the program.
///
/// # Errors
///
/// Returns `Err` when the program contains cut. The parser retains the marker so
/// callers receive this stable diagnostic instead of a generic syntax failure.
pub fn reject_cut(program: &QProgram) -> gmeow_errors::Result<()> {
    if !has_cut(program) {
        return Ok(());
    }
    Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: "program contains retired cut syntax (`!`); rewrite the rule declaratively"
            .to_owned(),
    }))
}

// ── Arithmetic-builtin gate (G2a) ──────────────────────────────────────

/// Return `true` if any rule body in `program` contains an arithmetic/comparison
/// builtin ([`QBodyLit::Builtin`]).
pub fn has_builtin(program: &QProgram) -> bool {
    program.rules.iter().any(|rule| {
        rule.body
            .iter()
            .any(|lit| matches!(lit, QBodyLit::Builtin(_)))
    })
}

/// Assert that if `program` contains an arithmetic/comparison builtin, `profile`
/// resolves to a preset whose facet bundle licenses procedural execution.
///
/// Arithmetic builtins (`logic:builtinArithmetic`) are gated to
/// `ProceduralPrologProfile` (per `slices/grounding/logic/module.ttl`).
/// The decision is **facet-derived** (the same [`is_procedural_profile`] predicate
/// used for native procedural builtins): an unrecognized profile resolves to no preset and does not license
/// builtins. There is no fallback or silent stripping.
///
/// If the program contains no builtin this function always returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err` naming the offending profile when the program contains a
/// builtin and the profile resolves to no procedural-licensing preset.
pub fn check_builtin_profile(program: &QProgram, profile: &str) -> gmeow_errors::Result<()> {
    if !has_builtin(program) {
        return Ok(());
    }
    if is_procedural_profile(profile) {
        return Ok(());
    }
    Err(gmeow_errors::Diag::of_kind(crate::error::Reason {
        detail: format!(
            "program contains an arithmetic/comparison builtin but profile {profile:?} \
             does not denote ProceduralPrologProfile; builtins are only permitted under \
             {PROCEDURAL_PROLOG_PROFILE:?}"
        ),
    }))
}

/// Extract the local name from a profile reference (full IRI, `prefix:Local`, or a
/// bare local name) — the substring after the last `/` or `:`.
fn profile_local_name(profile: &str) -> &str {
    profile
        .strip_prefix('<')
        .and_then(|value| value.strip_suffix('>'))
        .unwrap_or(profile)
        .rsplit(['/', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(profile)
}

/// Resolve every accepted spelling of a semantic preset to one full-IRI identity.
/// Unknown values remain distinct so an unrecognized contract cannot alias a known
/// preset or another caller's opaque profile.
pub(crate) fn canonical_profile_identity(profile: &str) -> String {
    if let Some(preset) = SemanticProfileId::from_local(profile_local_name(profile)) {
        return preset.iri();
    }
    match profile_local_name(profile) {
        LEWIS_SKEPTICAL_SHORT => LEWIS_SKEPTICAL_PROFILE.to_owned(),
        LEWIS_CREDULOUS_SHORT => LEWIS_CREDULOUS_PROFILE.to_owned(),
        _ => profile.to_owned(),
    }
}

/// Return `true` if `profile` denotes a reasoning contract whose facet bundle
/// licenses native procedural builtins.
///
/// The decision is facet-derived from `logic:ProceduralExecution`, not from a
/// raw profile-name match.
fn is_procedural_profile(profile: &str) -> bool {
    SemanticProfileId::from_local(profile_local_name(profile))
        .is_some_and(SemanticProfileId::permits_procedural_execution)
}

// ── Probabilistic profile recognition ─────────────────────────────────

/// The canonical full IRI for the probabilistic profile.
pub const PROBABILISTIC_PROFILE: &str = "https://blackcatinformatics.ca/logic/ProbabilisticProfile";

/// The bare short name accepted as an alias for [`PROBABILISTIC_PROFILE`].
const PROBABILISTIC_SHORT_NAME: &str = "ProbabilisticProfile";

/// Return `true` if `profile` denotes the probabilistic profile.
///
/// Matching mirrors [`is_procedural_profile`]: full IRI, bare short name, or any
/// prefixed form ending in the short name (`logic:ProbabilisticProfile`, etc.).
/// Probabilistic inference is available ONLY under this profile.
pub fn is_probabilistic_profile(profile: &str) -> bool {
    profile == PROBABILISTIC_PROFILE
        || profile == PROBABILISTIC_SHORT_NAME
        || profile.ends_with(PROBABILISTIC_SHORT_NAME)
}

// ── Lewis multi-world profile recognition ─────────────────────────────

/// The opt-in, budget-capped Lewis multi-world profiles. Non-default: Stratum-C
/// resolves under the deterministic-revision profile unless one of these is named.
pub const LEWIS_SKEPTICAL_PROFILE: &str =
    "https://blackcatinformatics.ca/logic/LewisSkepticalProfile";
/// Credulous counterpart of [`LEWIS_SKEPTICAL_PROFILE`].
pub const LEWIS_CREDULOUS_PROFILE: &str =
    "https://blackcatinformatics.ca/logic/LewisCredulousProfile";

const LEWIS_SKEPTICAL_SHORT: &str = "LewisSkepticalProfile";
const LEWIS_CREDULOUS_SHORT: &str = "LewisCredulousProfile";

/// Lewis quantifier over the closest counterfactual worlds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LewisMode {
    /// φ holds iff it holds in **every** closest world (intersection of answers).
    Skeptical,
    /// φ holds iff it holds in **some** closest world (union of answers).
    Credulous,
}

/// Return the [`LewisMode`] a profile denotes, or `None` for the default
/// deterministic-revision profile. Matching mirrors [`is_procedural_profile`]:
/// full IRI, bare short name, or any prefixed form ending in the short name.
pub fn lewis_mode(profile: &str) -> Option<LewisMode> {
    if profile == LEWIS_SKEPTICAL_PROFILE
        || profile == LEWIS_SKEPTICAL_SHORT
        || profile.ends_with(LEWIS_SKEPTICAL_SHORT)
    {
        Some(LewisMode::Skeptical)
    } else if profile == LEWIS_CREDULOUS_PROFILE
        || profile == LEWIS_CREDULOUS_SHORT
        || profile.ends_with(LEWIS_CREDULOUS_SHORT)
    {
        Some(LewisMode::Credulous)
    } else {
        None
    }
}

// ── Evolution-facet recognition (logic:EvolutionMode) ────────────────────────

/// The canonical full IRIs for the three `logic:EvolutionMode` facet values.
pub const STATIC_EVOLUTION: &str = "https://blackcatinformatics.ca/logic/StaticEvolution";
/// State-transition counterpart of [`STATIC_EVOLUTION`].
pub const STATE_TRANSITION_EVOLUTION: &str =
    "https://blackcatinformatics.ca/logic/StateTransitionEvolution";
/// Transaction-path counterpart of [`STATIC_EVOLUTION`].
pub const TRANSACTION_PATH_EVOLUTION: &str =
    "https://blackcatinformatics.ca/logic/TransactionPathEvolution";

const STATIC_EVOLUTION_SHORT: &str = "StaticEvolution";
const STATE_TRANSITION_EVOLUTION_SHORT: &str = "StateTransitionEvolution";
const TRANSACTION_PATH_EVOLUTION_SHORT: &str = "TransactionPathEvolution";

/// Normalize a `logic:EvolutionMode` reference (full IRI,
/// `prefix:Local`, or a bare local name) to its bare local name.
///
/// Recognition mirrors [`is_procedural_profile`]: a reference is normalized by its
/// trailing local name ([`profile_local_name`]), so `StaticEvolution`,
/// `logic:StaticEvolution`, and the full IRI all collapse to the same local name.
///
/// Transaction Logic is the `transaction-path` value of this orthogonal Evolution
/// facet, NOT a parallel profile: a contract selects an evolution mode the same way
/// it selects any other single-valued facet.
///
/// # Returns
///
/// `Some("StaticEvolution" | "StateTransitionEvolution" | "TransactionPathEvolution")`
/// for a recognized value; `None` for an empty string or any non-empty value that
/// denotes none of the three modes — the caller turns that `None` into a hard fail
/// (there is no silent fallback).
pub fn evolution_mode_local(evolution: &str) -> Option<&'static str> {
    match profile_local_name(evolution) {
        STATIC_EVOLUTION_SHORT => Some(STATIC_EVOLUTION_SHORT),
        STATE_TRANSITION_EVOLUTION_SHORT => Some(STATE_TRANSITION_EVOLUTION_SHORT),
        TRANSACTION_PATH_EVOLUTION_SHORT => Some(TRANSACTION_PATH_EVOLUTION_SHORT),
        _ => None,
    }
}

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::dispatch_query;
    use crate::query_ir::parse_query_program;
    use crate::seam::WorldFactSnapshot;
    use crate::store::WorldStore;

    const BASE: &str = "https://example.org/";
    const HORN_PROFILE: &str = "https://blackcatinformatics.ca/logic/PositiveHornProfile";
    const WORLD: &str = "http://logic.test/world/gate";

    fn cut_program() -> crate::query_ir::QProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:edge(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
        );
        parse_query_program(&src).unwrap()
    }

    fn no_cut_program() -> crate::query_ir::QProgram {
        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:reach(X, Y) :- ex:edge(X, Y).\n\
             ?- ex:reach(ex:a, Y).\n"
        );
        parse_query_program(&src).unwrap()
    }

    // ── has_cut ────────────────────────────────────────────────────────────────

    #[test]
    fn has_cut_detects_cut_in_body() {
        assert!(
            has_cut(&cut_program()),
            "cut program must report has_cut=true"
        );
    }

    #[test]
    fn has_cut_false_when_no_cut() {
        assert!(
            !has_cut(&no_cut_program()),
            "non-cut program must report has_cut=false"
        );
    }

    // ── retired cut syntax ────────────────────────────────────────────────────

    #[test]
    fn cut_is_rejected_under_every_profile() {
        let prog = cut_program();
        let error = reject_cut(&prog).expect_err("cut must be retired unconditionally");
        assert!(error.message().contains("retired cut syntax"));
    }

    #[test]
    fn no_cut_any_profile_is_ok() {
        let prog = no_cut_program();
        assert!(reject_cut(&prog).is_ok());
    }

    // ── Lewis profile recognition ──────────────────────────────────────

    #[test]
    fn lewis_mode_recognizes_full_iri_and_short_and_prefixed() {
        assert_eq!(
            lewis_mode(LEWIS_SKEPTICAL_PROFILE),
            Some(LewisMode::Skeptical)
        );
        assert_eq!(
            lewis_mode("LewisSkepticalProfile"),
            Some(LewisMode::Skeptical)
        );
        assert_eq!(
            lewis_mode("logic:LewisCredulousProfile"),
            Some(LewisMode::Credulous)
        );
        assert_eq!(
            lewis_mode(LEWIS_CREDULOUS_PROFILE),
            Some(LewisMode::Credulous)
        );
    }

    #[test]
    fn lewis_mode_default_profiles_are_none() {
        assert_eq!(lewis_mode(HORN_PROFILE), None);
        assert_eq!(lewis_mode("PositiveHornProfile"), None);
        assert_eq!(lewis_mode(""), None);
    }

    // ── Evolution-facet recognition (logic:EvolutionMode) ─────────────────────

    #[test]
    fn evolution_mode_recognizes_full_iri_short_and_prefixed() {
        assert_eq!(
            evolution_mode_local(STATIC_EVOLUTION),
            Some("StaticEvolution")
        );
        assert_eq!(
            evolution_mode_local("StaticEvolution"),
            Some("StaticEvolution")
        );
        assert_eq!(
            evolution_mode_local("logic:StaticEvolution"),
            Some("StaticEvolution")
        );
        assert_eq!(
            evolution_mode_local(STATE_TRANSITION_EVOLUTION),
            Some("StateTransitionEvolution")
        );
        assert_eq!(
            evolution_mode_local("logic:StateTransitionEvolution"),
            Some("StateTransitionEvolution")
        );
        assert_eq!(
            evolution_mode_local(TRANSACTION_PATH_EVOLUTION),
            Some("TransactionPathEvolution")
        );
        assert_eq!(
            evolution_mode_local("TransactionPathEvolution"),
            Some("TransactionPathEvolution")
        );
    }

    #[test]
    fn evolution_mode_unknown_and_empty_are_none() {
        assert_eq!(evolution_mode_local(""), None);
        assert_eq!(evolution_mode_local("NotAnEvolutionMode"), None);
        assert_eq!(evolution_mode_local("logic:PositiveHornProfile"), None);
    }

    // ── No-write firewall ──────────────────────────────────────────────────────
    //
    // Reject a cut program through production dispatch and verify the store remains
    // unchanged.

    #[test]
    fn rejected_cut_program_leaves_store_unchanged() {
        let store = WorldStore::new();
        store.insert_quad(
            WORLD,
            &format!("{BASE}a"),
            &format!("{BASE}edge"),
            &format!("{BASE}b"),
        );
        store.insert_quad(
            WORLD,
            &format!("{BASE}a"),
            &format!("{BASE}edge"),
            &format!("{BASE}c"),
        );

        let before = store.quads_in_world(WORLD).len();

        let foreign = WorldFactSnapshot::from_world(&store, WORLD, PROCEDURAL_PROLOG_PROFILE)
            .expect("from_world must succeed");

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:edge(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        let error = dispatch_query(
            &foreign,
            WORLD,
            &prog,
            PROCEDURAL_PROLOG_PROFILE,
            &crate::query_ir::Budget::default(),
        )
        .expect_err("cut must be rejected even under the procedural builtin profile");
        assert!(error.message().contains("retired cut syntax"));

        let after = store.quads_in_world(WORLD).len();
        assert_eq!(
            before, after,
            "store quad count must be unchanged after the rejected query"
        );
    }
}
