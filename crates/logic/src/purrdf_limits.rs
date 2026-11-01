// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The evaluation ceilings gmeow states for every purrdf Datalog evaluation it requests.
//!
//! purrdf 4.0 removed its target-sized defaults: an unstated stored-fact, join-step or
//! term-arena setting now imposes no logical ceiling. gmeow keeps the ceilings it ran
//! under before that change, stated here once, so a rule set that grows past them is
//! refused with purrdf's limit report instead of consuming the host. The selected
//! settings are part of purrdf's execution contract identity, so they change only by a
//! deliberate edit here.

use purrdf::datalog::seminaive::EvalOptions;

/// Stored facts per native evaluation: purrdf's former native default, 2^22.
pub(crate) const NATIVE_MAX_STORED_FACTS: u64 = 4_194_304;
/// Stored facts per evaluation inside one wasm32 linear memory: purrdf's former browser
/// default, 2^17.
pub(crate) const WASM32_MAX_STORED_FACTS: u64 = 131_072;
/// The stored-fact ceiling for the target this build runs on.
pub(crate) const MAX_STORED_FACTS: u64 = if cfg!(target_arch = "wasm32") {
    WASM32_MAX_STORED_FACTS
} else {
    NATIVE_MAX_STORED_FACTS
};
/// Join steps per evaluation: purrdf's former default on every target, 2^20.
pub(crate) const MAX_JOIN_STEPS: u64 = 1_048_576;
/// Interned term-arena bytes per evaluation: purrdf's former default on every target,
/// 2^24.
pub(crate) const MAX_TERM_ARENA_BYTES: u64 = 16_777_216;

/// The stated options for a purrdf materialization, certain-answer or explanation run.
pub(crate) fn eval_options() -> EvalOptions {
    EvalOptions::default()
        .with_max_stored_facts(MAX_STORED_FACTS)
        .with_max_join_steps(MAX_JOIN_STEPS)
        .with_max_term_arena_bytes(MAX_TERM_ARENA_BYTES)
}
