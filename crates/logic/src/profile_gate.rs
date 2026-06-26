// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Profile gate — cut-confinement guard (AC-2).
//!
//! # Structural no-write firewall
//!
//! The entire query and resolution path (`run_scryer`, `resolve`, `fast_path`) accepts only
//! `&dyn ScryerForeign` and `&WorldStore` — **shared, read-only references**. There is no
//! `&mut` write path anywhere in the resolution stack. A `cut` (`!`) in a rule body can
//! influence *which* answers Scryer Prolog returns (first-answer commitment), but it can
//! **never produce a stored quad**: the engine drives a fresh Scryer machine per query,
//! collects virtual answer bindings into a `Vec<Binding>`, and returns. Zero quads are
//! inserted into `WorldStore.inner` during or after resolution. Cut is virtual-only by
//! construction.
//!
//! # Cut-permission is facet-derived (#767)
//!
//! Cut-confinement is decided in FACET terms, not by a raw profile-name match: a
//! profile reference (full IRI, `prefix:Local`, or bare local name) is resolved to its
//! preset, and cut is licensed iff that preset's facet bundle carries the
//! procedural-execution facet (`logic:ProceduralExecution`, see
//! [`SemanticProfileId::permits_cut`]).  An unrecognized reference resolves to no preset
//! and does not license cut — the AC-2 seal is preserved.

use crate::compile::ir::SemanticProfileId;
use crate::query_ir::{QBodyLit, QProgram};

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

/// Assert that if `program` contains cut, `profile` resolves to a preset whose
/// facet bundle licenses cut.
///
/// Cut may appear ONLY under a profile whose facet bundle licenses it. Hard-fail
/// otherwise — there is no fallback or silent stripping of cut.
///
/// The decision is **facet-derived** (#767): `profile` is resolved to its preset
/// via its local name ([`SemanticProfileId::from_local`]) and cut is licensed iff
/// that preset's facet bundle carries the procedural-execution facet
/// (`logic:ProceduralExecution`, exposed by [`SemanticProfileId::permits_cut`] and
/// pinned to the facet by the `procedural_preset_carries_procedural_execution_facet`
/// test) — see [`is_procedural_profile`]. An unrecognized reference resolves to no
/// preset and does not license cut. This is NOT a raw name match against
/// `ProceduralPrologProfile`; that preset only licenses cut because its facet bundle
/// carries the procedural-execution facet.
///
/// If the program contains no cut this function always returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err(String)` with a message naming the offending profile when the program
/// contains cut and the profile resolves to no cut-licensing preset.
pub fn check_cut_profile(program: &QProgram, profile: &str) -> Result<(), String> {
    if !has_cut(program) {
        return Ok(());
    }
    if is_procedural_profile(profile) {
        return Ok(());
    }
    Err(format!(
        "program contains cut (`!`) but profile {profile:?} does not denote \
         ProceduralPrologProfile; cut is only permitted under \
         {PROCEDURAL_PROLOG_PROFILE:?}"
    ))
}

// ── Arithmetic-builtin gate (#1009 G2a) ──────────────────────────────────────

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
/// `ProceduralPrologProfile` (per `slices/core/logic/module.ttl`), exactly as cut is.
/// The decision is **facet-derived** (the same [`is_procedural_profile`] predicate
/// used for cut): an unrecognized profile resolves to no preset and does not license
/// builtins. There is no fallback or silent stripping.
///
/// If the program contains no builtin this function always returns `Ok(())`.
///
/// # Errors
///
/// Returns `Err(String)` naming the offending profile when the program contains a
/// builtin and the profile resolves to no procedural-licensing preset.
pub fn check_builtin_profile(program: &QProgram, profile: &str) -> Result<(), String> {
    if !has_builtin(program) {
        return Ok(());
    }
    if is_procedural_profile(profile) {
        return Ok(());
    }
    Err(format!(
        "program contains an arithmetic/comparison builtin but profile {profile:?} \
         does not denote ProceduralPrologProfile; builtins are only permitted under \
         {PROCEDURAL_PROLOG_PROFILE:?}"
    ))
}

/// Extract the local name from a profile reference (full IRI, `prefix:Local`, or a
/// bare local name) — the substring after the last `/` or `:`.
fn profile_local_name(profile: &str) -> &str {
    profile
        .rsplit(['/', ':'])
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(profile)
}

/// Return `true` if `profile` denotes a reasoning contract whose facet bundle
/// licenses cut.
///
/// The decision is **facet-derived**: the profile string is resolved to its preset and
/// the cut decision flows from that preset's procedural-execution facet
/// ([`SemanticProfileId::permits_cut`] ⇔ the `logic:ProceduralExecution` facet in its
/// `logic:expandsToFacet` bundle), NOT from a raw `ProceduralPrologProfile` name match.
/// An unrecognized profile reference resolves to no preset and therefore does not
/// license cut (hard-fail), preserving the AC-2 seal.
fn is_procedural_profile(profile: &str) -> bool {
    SemanticProfileId::from_local(profile_local_name(profile))
        .is_some_and(SemanticProfileId::permits_cut)
}

// ── Probabilistic profile recognition (#506) ─────────────────────────────────

/// The canonical full IRI for the probabilistic profile.
pub const PROBABILISTIC_PROFILE: &str = "https://blackcatinformatics.ca/logic/ProbabilisticProfile";

/// The bare short name accepted as an alias for [`PROBABILISTIC_PROFILE`].
const PROBABILISTIC_SHORT_NAME: &str = "ProbabilisticProfile";

/// Return `true` if `profile` denotes the probabilistic profile.
///
/// Matching mirrors [`is_procedural_profile`]: full IRI, bare short name, or any
/// prefixed form ending in the short name (`logic:ProbabilisticProfile`, etc.).
/// Probabilistic inference (#506) is available ONLY under this profile.
pub fn is_probabilistic_profile(profile: &str) -> bool {
    profile == PROBABILISTIC_PROFILE
        || profile == PROBABILISTIC_SHORT_NAME
        || profile.ends_with(PROBABILISTIC_SHORT_NAME)
}

// ── Lewis multi-world profile recognition (#505) ─────────────────────────────

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

// ── Unit tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::query_ir::parse_query_program;
    use crate::seam::WorldStoreForeign;
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

    // ── check_cut_profile — Ok cases ──────────────────────────────────────────

    #[test]
    fn cut_with_full_procedural_iri_is_ok() {
        let prog = cut_program();
        assert!(
            check_cut_profile(&prog, PROCEDURAL_PROLOG_PROFILE).is_ok(),
            "cut + full IRI must be Ok"
        );
    }

    #[test]
    fn cut_with_bare_short_name_is_ok() {
        let prog = cut_program();
        assert!(
            check_cut_profile(&prog, "ProceduralPrologProfile").is_ok(),
            "cut + bare short name must be Ok"
        );
    }

    #[test]
    fn cut_with_prefixed_name_ending_in_short_name_is_ok() {
        let prog = cut_program();
        // A profile.json-style prefixed form that ends_with the short name.
        assert!(
            check_cut_profile(&prog, "logic:ProceduralPrologProfile").is_ok(),
            "cut + prefixed name ending in short name must be Ok"
        );
    }

    // ── check_cut_profile — Err cases ─────────────────────────────────────────

    #[test]
    fn cut_with_positive_horn_profile_returns_err() {
        let prog = cut_program();
        let result = check_cut_profile(&prog, HORN_PROFILE);
        assert!(result.is_err(), "cut + PositiveHornProfile must be Err");
        let msg = result.unwrap_err();
        assert!(
            msg.contains(HORN_PROFILE),
            "error message must name the offending profile: {msg:?}"
        );
    }

    #[test]
    fn cut_with_empty_profile_returns_err() {
        let prog = cut_program();
        let result = check_cut_profile(&prog, "");
        assert!(result.is_err(), "cut + empty profile must be Err");
    }

    // ── check_cut_profile — no-cut programs always pass ───────────────────────

    #[test]
    fn no_cut_any_profile_is_ok() {
        let prog = no_cut_program();
        assert!(check_cut_profile(&prog, HORN_PROFILE).is_ok());
        assert!(check_cut_profile(&prog, "").is_ok());
        assert!(check_cut_profile(&prog, "SomeRandomProfile").is_ok());
    }

    // ── Lewis profile recognition (#505) ──────────────────────────────────────

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

    // ── No-write firewall ──────────────────────────────────────────────────────
    //
    // Run a terminating cut program through `run_scryer` under the procedural
    // profile and verify the store quad count is unchanged — cut is virtual-only,
    // no quads are inserted.

    #[test]
    fn no_write_firewall_cut_program_leaves_store_unchanged() {
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

        let world_nn = oxigraph::model::NamedNode::new(WORLD).unwrap();
        let foreign = WorldStoreForeign::from_world(&store, WORLD, PROCEDURAL_PROLOG_PROFILE)
            .expect("from_world must succeed");

        let src = format!(
            ":- prefix(ex, '{BASE}').\n\
             ex:first(X, Y) :- ex:edge(X, Y), !.\n\
             ?- ex:first(ex:a, Y).\n"
        );
        let prog = parse_query_program(&src).unwrap();

        // Gate passes; engine runs; answers are virtual only.
        check_cut_profile(&prog, PROCEDURAL_PROLOG_PROFILE)
            .expect("gate must pass under ProceduralPrologProfile");

        let ans = crate::scryer_engine::run_scryer(
            &foreign,
            &world_nn,
            &prog,
            &[], // no tabling — cut program is procedural
            &crate::query_ir::Budget::default(),
        )
        .expect("run_scryer must succeed on a terminating cut program");

        // Cut commits to the first answer; store is still unchanged.
        assert_eq!(
            ans.bindings.len(),
            1,
            "cut must commit to first answer: {ans:?}"
        );
        let after = store.quads_in_world(WORLD).len();
        assert_eq!(
            before, after,
            "store quad count must be UNCHANGED after run_scryer (no-write firewall)"
        );
    }
}
