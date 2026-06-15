// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! Nemo reasoner bridge — native targets only.
//!
//! This module provides the surface that links the Nemo crate into
//! `gmeow-logic`.  Rule materialization is driven by [`run_chase`], which
//! owns a per-thread tokio `current_thread` runtime and calls the async Nemo
//! API (`load_string` → `reason` → `predicate_rows`) synchronously from the
//! perspective of the caller.
//!
//! # Platform note
//!
//! Nemo's transitive dependencies (`reqwest`, `tower-lsp`) use OS networking
//! unavailable on `wasm32-unknown-unknown`.  The `#[cfg(not(target_arch =
//! "wasm32"))]` guard in `lib.rs` is platform-correct, not an optionality
//! toggle: there are zero degraded fallbacks and zero feature flags controlling
//! this.  The wasm surface is provided by `wasm.rs` via wasm-bindgen.
//!
//! # Runtime flavour
//!
//! Nemo's own CLI uses `#[tokio::main(flavor = "current_thread")]`.  We
//! replicate that: the thread-local runtime is `current_thread`, started once
//! per OS thread and reused for every subsequent [`run_chase`] call on that
//! thread.  `block_on` may not be called from *inside* an existing tokio
//! runtime — callers that live inside `#[tokio::main]` (e.g. py.rs via PyO3)
//! MUST release the GIL **and** call this function from a non-async context
//! or a `spawn_blocking` task.

use nemo::api::{load_program, validate};
use nemo::api::{load_string, reason};
use nemo::datavalues::AnyDataValue;
use nemo::rule_model::programs::program::Program;
use nemo::rule_model::{components::tag::Tag, programs::ProgramRead};
use tokio::runtime::Runtime;

use std::cell::RefCell;
use std::fmt;
use std::sync::{LazyLock, Mutex};

// ── Process-global chase lock ─────────────────────────────────────────────────

/// Serialises all calls to the Nemo chase (`load_string` → `reason` →
/// `predicate_rows`).  Required because Nemo maintains a process-global
/// `Mutex<TimedCode>` timing singleton whose `start()`/`stop()` methods carry
/// a `debug_assert!` that fires if two `reason()` invocations overlap — even
/// on different OS threads.  A single mutex here prevents concurrent callers
/// from racing that global state, both in tests (default parallel `cargo test`)
/// and in production (Python threads calling materialise via PyO3).
static CHASE_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

// ── Thread-local tokio runtime ────────────────────────────────────────────────

thread_local! {
    /// A single `current_thread` tokio runtime per OS thread, created on first
    /// use and reused thereafter.  Matches the runtime flavour used by nemo-cli
    /// (`#[tokio::main(flavor = "current_thread")]`).
    static NEMO_RUNTIME: RefCell<Runtime> = RefCell::new(
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build per-thread tokio runtime for Nemo chase")
    );
}

// ── Chase result type ─────────────────────────────────────────────────────────

/// A single materialized row: `(predicate_name, values)`.
///
/// `values` are the string representations of each term in the row, using
/// [`AnyDataValue`]'s [`fmt::Display`] implementation (the canonical Nemo
/// surface string).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaseRow {
    /// The predicate name (e.g. `"tc"` for a rule `tc(?x,?y) :- …`).
    pub predicate: String,
    /// One string per column in the row.
    pub values: Vec<String>,
}

impl fmt::Display for ChaseRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.predicate, self.values.join(", "))
    }
}

// ── Chase driver ──────────────────────────────────────────────────────────────

/// Run the Nemo chase on a complete `.rls` program string and return all
/// materialized facts as a flat list of [`ChaseRow`]s.
///
/// # Arguments
///
/// * `rls` — A complete Nemo rule-language string.  May include inline ground
///   facts (e.g. `e(a,b).`) as well as rules.  This is exactly the shape that
///   `project_nemo` emits.
///
/// # Return value
///
/// On success, every derived fact for every derived predicate is returned as a
/// [`ChaseRow`].  The order of rows and predicates is not specified.
///
/// # Errors
///
/// Returns a `String` describing the first error encountered (parse, validation,
/// or execution).
///
/// # Thread safety
///
/// This function is safe to call from multiple threads simultaneously.  Each
/// thread owns its own tokio runtime via `thread_local!`, but calls are
/// serialised by [`CHASE_LOCK`] to prevent concurrent access to Nemo's
/// process-global `TimedCode` timing singleton.
///
/// # Footgun: no nesting
///
/// `block_on` panics if called from *inside* an existing tokio runtime.  PyO3
/// callers **must** wrap invocations with `py.allow_threads(|| run_chase(…))`
/// so the GIL is released and the call runs outside the interpreter's async
/// context.  Failing to do so will panic at runtime with "cannot start a
/// runtime within a runtime" (or equivalent).
pub fn run_chase(rls: String) -> Result<Vec<ChaseRow>, String> {
    // Serialise access to Nemo's process-global TimedCode singleton.
    // A poisoned lock means a previous chase panicked; recover the guard so
    // subsequent calls are not permanently wedged.
    let _guard = CHASE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    NEMO_RUNTIME.with(|cell| {
        let rt = cell.borrow();

        rt.block_on(async {
            // ── 1. Parse and initialise the engine ───────────────────────────
            let mut engine = load_string(rls)
                .await
                .map_err(|e| format!("nemo load error: {e:?}"))?;

            // ── 2. Run the chase ─────────────────────────────────────────────
            reason(&mut engine)
                .await
                .map_err(|e| format!("nemo reason error: {e:?}"))?;

            // ── 3. Collect all derived facts ─────────────────────────────────
            // `engine.program()` is the logical `Program` (implements
            // `ProgramRead`) so we can call `derived_predicates()` to get every
            // predicate head that exists after the chase — including EDB facts.
            let predicates: Vec<Tag> = engine.program().derived_predicates().into_iter().collect();

            let mut rows: Vec<ChaseRow> = Vec::new();
            for tag in predicates {
                if let Some(iter) = engine
                    .predicate_rows(&tag)
                    .await
                    .map_err(|e| format!("nemo predicate_rows error: {e:?}"))?
                {
                    for row_vals in iter {
                        let values: Vec<String> = row_vals
                            .iter()
                            .map(|v: &AnyDataValue| v.to_string())
                            .collect();
                        rows.push(ChaseRow {
                            predicate: tag.to_string(),
                            values,
                        });
                    }
                }
            }

            Ok(rows)
        })
    })
}

// ── Legacy synchronous parse/validate surface ─────────────────────────────────

/// A parsed, validated Nemo rule program ready to be handed to a tokio runtime
/// for execution via [`nemo::api::reason`].
///
/// `NemoParsedRules` is the synchronous half of the pipeline.  The async
/// chase is now driven by [`run_chase`], which manages its own per-thread
/// runtime.  This type is retained for callers that need only parse/validate
/// without running the full chase.
#[derive(Debug)]
pub struct NemoParsedRules {
    program: Program,
}

impl NemoParsedRules {
    /// Parse and validate a Nemo rule program from a source string.
    ///
    /// Uses [`nemo::api::load_program`], which is fully synchronous (no tokio
    /// required).  Actual reasoning ([`nemo::api::reason`]) is async and is
    /// driven by [`run_chase`] via a thread-local tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns a string error if Nemo cannot parse or validate the program.
    pub fn parse(rules: &str) -> Result<Self, String> {
        let program = load_program(rules.to_owned(), "<gmeow-logic>".to_owned())
            .map_err(|report| format!("nemo parse error: {report:?}"))?;
        Ok(Self { program })
    }

    /// Validate a Nemo rule string and return any diagnostics as a string.
    ///
    /// This is a pure syntax/semantic check; no engine is instantiated.
    pub fn lint(rules: &str) -> String {
        let report = validate(rules.to_owned(), "<gmeow-logic>".to_owned());
        format!("{report:?}")
    }

    /// Return the inner [`Program`] for use by the async chase driver.
    pub fn into_program(self) -> Program {
        self.program
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal transitive-closure program with inline EDB facts.
    ///
    /// Rules:
    ///   `tc(?x,?y) :- e(?x,?y) .`
    ///   `tc(?x,?z) :- tc(?x,?y), e(?y,?z) .`
    ///
    /// EDB:
    ///   `e(a,b). e(b,c).`
    ///
    /// Expected derived facts for `tc`:
    ///   `tc(a,b)`, `tc(b,c)`, `tc(a,c)` — the closure fact `tc(a,c)` is the
    ///   key witness that the chase actually ran (it requires two rule firings).
    const TC_PROGRAM: &str = r#"
tc(?x,?y) :- e(?x,?y) .
tc(?x,?z) :- tc(?x,?y), e(?y,?z) .
e(a,b).
e(b,c).
"#;

    /// Helper: assert that a specific `(predicate, values)` tuple is present
    /// in the result set.
    fn assert_row_present(rows: &[ChaseRow], predicate: &str, values: &[&str]) {
        let target_values: Vec<String> = values.iter().map(|s| s.to_string()).collect();
        let found = rows
            .iter()
            .any(|r| r.predicate == predicate && r.values == target_values);
        assert!(
            found,
            "expected row {predicate}({}) not found in:\n{rows:#?}",
            values.join(", ")
        );
    }

    /// Run the transitive-closure chase and assert the derived closure fact.
    #[test]
    fn test_transitive_closure_chase() {
        let rows =
            run_chase(TC_PROGRAM.to_owned()).expect("chase should succeed on a valid TC program");

        // At minimum we expect the three tc facts: base copies + the closure
        let tc_rows: Vec<&ChaseRow> = rows.iter().filter(|r| r.predicate == "tc").collect();
        assert!(
            tc_rows.len() >= 3,
            "expected at least 3 tc facts (tc(a,b), tc(b,c), tc(a,c)), got {}: {tc_rows:#?}",
            tc_rows.len()
        );

        // The critical witness: tc(a,c) requires two rule firings
        assert_row_present(&rows, "tc", &["a", "c"]);

        // Sanity: base facts must also be present
        assert_row_present(&rows, "tc", &["a", "b"]);
        assert_row_present(&rows, "tc", &["b", "c"]);
    }

    /// Call the driver twice in a row on the same thread to prove the
    /// thread-local runtime is reused cleanly without "cannot start a runtime
    /// within a runtime" panics.
    #[test]
    fn test_same_thread_double_call() {
        // First call — TC program
        let rows1 = run_chase(TC_PROGRAM.to_owned()).expect("first chase call should succeed");
        let tc1: Vec<&ChaseRow> = rows1.iter().filter(|r| r.predicate == "tc").collect();
        assert!(!tc1.is_empty(), "first call: expected tc facts");

        // Second call — a different, independent program
        let simple_program = "parent(alice, bob). parent(bob, carol).";
        let rows2 = run_chase(simple_program.to_owned()).expect("second chase call should succeed");
        let parent_rows: Vec<&ChaseRow> =
            rows2.iter().filter(|r| r.predicate == "parent").collect();
        assert_eq!(
            parent_rows.len(),
            2,
            "second call: expected 2 parent facts, got {}: {parent_rows:#?}",
            parent_rows.len()
        );

        // Runtime is still alive and healthy — a third call works too
        let rows3 = run_chase(TC_PROGRAM.to_owned())
            .expect("third chase call (same thread) should succeed");
        assert_row_present(&rows3, "tc", &["a", "c"]);
    }
}
