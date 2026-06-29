// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Nemo reasoner bridge — native targets only.
//!
//! This module provides the surface that links the Nemo crate into
//! `gmeow-logic`.  Rule materialization is driven by [`run_chase`], which
//! owns a per-thread tokio `current_thread` runtime and calls the async Nemo
//! API (`load_string` → `reason` → `predicate_rows`) synchronously from the
//! perspective of the caller.
//!
//! # Provenance extraction
//!
//! After reasoning, [`run_chase`] calls `engine.trace()` for each derived fact
//! to extract:
//! - Whether the fact is EDB (asserted input) or IDB (derived by a rule).
//! - The rule name (if set via `#[name("...")]` in the `.rls` source) of the
//!   firing rule — used to recover the rule IRI.
//! - The immediate antecedent facts (direct children in the derivation tree).
//!
//! This information is bundled into [`ChaseRowWithProvenance`], which is what
//! `py.rs` uses to populate the full seam-contract metadata.
//!
//! # Platform note
//!
//! This crate is single-target native only.  Nemo's transitive dependencies
//! (`reqwest`, `tower-lsp`) require OS networking and the CPython ABI; no
//! alternative build path exists.
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

use gmeow_rdf::provenance::Attribution;
use nemo::api::{load_program, load_string, reason, validate};
use nemo::datavalues::AnyDataValue;
use nemo::execution::tracing::trace::{ExecutionTraceTree, TraceTreeRuleApplication};
use nemo::rule_model::components::atom::Atom;
use nemo::rule_model::components::fact::Fact;
use nemo::rule_model::components::tag::Tag;
use nemo::rule_model::programs::program::Program;
use nemo::rule_model::programs::ProgramRead;
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

// ── Chase result types ────────────────────────────────────────────────────────

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

/// Provenance metadata for a single derived fact, extracted via Nemo tracing.
///
/// - `is_edb`: `true` if the fact was part of the input (asserted), `false` if
///   it was derived by a rule application.
/// - `rule_name`: The name set on the firing rule via `#[name("...")]` in the
///   `.rls` source, or `None` for EDB facts and unnamed rules.
/// - `antecedent_rows`: The immediate antecedent facts (their display strings,
///   one `ChaseRow` per immediate premise) consumed by the firing rule.
/// - `attributions`: Structured slice attributions (§9 / S5). Records which
///   compilation units played which roles in this derivation. Empty for legacy
///   or unfilled contexts; populated at the validation boundary when slice
///   context is available.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaseProvenance {
    /// Whether this fact is an EDB (asserted input) fact.
    pub is_edb: bool,
    /// Name of the rule that derived this fact, as set via `#[name("...")]`.
    pub rule_name: Option<String>,
    /// Immediate antecedent facts (premises) that the rule consumed.
    pub antecedent_rows: Vec<ChaseRow>,
    /// Structured slice attributions (§9 / S5).
    pub attributions: Vec<Attribution>,
}

/// A single materialized row with its provenance metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaseRowWithProvenance {
    /// The derived fact.
    pub row: ChaseRow,
    /// Provenance metadata (EDB/IDB, rule name, antecedents).
    pub provenance: ChaseProvenance,
}

// ── Internal helper: reconstruct a parseable Nemo fact string ─────────────────

/// Reconstruct a Nemo-parseable fact atom string from a [`ChaseRow`].
///
/// Format: `<predicate_iri>(val0, val1, val2).`
///
/// This is used to pass derived facts back into `engine.trace()`.
fn chase_row_to_fact_string(row: &ChaseRow) -> String {
    // The predicate in ChaseRow is the raw IRI (Tag::to_string() strips angle
    // brackets that Nemo internally stores).  We need to detect if it looks like
    // an IRI (contains "://") and wrap in angle brackets accordingly.
    let pred = if row.predicate.contains("://") {
        format!("<{}>", row.predicate)
    } else {
        row.predicate.clone()
    };

    let args = row.values.join(", ");
    format!("{}({}).", pred, args)
}

// ── Trace extraction ──────────────────────────────────────────────────────────

/// Extract a flat [`ChaseRow`] from a Nemo [`ExecutionTraceTree::Fact`] leaf.
fn extract_row_from_fact_string(fact_str: &str) -> Option<ChaseRow> {
    // Nemo's GroundAtom::to_string() gives: `pred(val0, val1, val2)`
    // We need to parse out the predicate and values.
    // The predicate ends at the first `(`.
    let open = fact_str.find('(')?;
    let close = fact_str.rfind(')')?;
    if open >= close {
        return None;
    }

    let pred_raw = &fact_str[..open];
    // Strip angle brackets from IRI predicates
    let predicate = if pred_raw.starts_with('<') && pred_raw.ends_with('>') {
        pred_raw[1..pred_raw.len() - 1].to_owned()
    } else {
        pred_raw.to_owned()
    };

    let args_str = &fact_str[open + 1..close];
    // Simple split by ", " — this is safe for our use case since Nemo's display
    // form quotes strings and wraps IRIs in angle brackets, so the ", " separator
    // does not appear inside values.
    let values: Vec<String> = if args_str.is_empty() {
        vec![]
    } else {
        split_nemo_args(args_str)
    };

    Some(ChaseRow { predicate, values })
}

/// Split a Nemo argument list string into individual argument strings.
///
/// Respects:
/// - `<...>` IRI wrapping (no comma inside angle brackets counts as separator)
/// - `"..."` string quoting (no comma inside quotes counts as separator)
/// - `^^<...>` typed literal suffix (handled by staying inside `<>` after `^^`)
fn split_nemo_args(s: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_iri = false;
    let mut in_str = false;
    let mut escaped = false;

    for c in s.chars() {
        if escaped {
            current.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if in_str => {
                current.push(c);
                escaped = true;
            }
            '<' if !in_str => {
                in_iri = true;
                current.push(c);
            }
            '>' if in_iri && !in_str => {
                in_iri = false;
                current.push(c);
            }
            '"' if !in_iri => {
                in_str = !in_str;
                current.push(c);
            }
            ',' if !in_iri && !in_str => {
                args.push(current.trim().to_owned());
                current = String::new();
            }
            other => {
                current.push(other);
            }
        }
    }
    if !current.trim().is_empty() {
        args.push(current.trim().to_owned());
    }
    args
}

/// Derive the fact string for the conclusion of a `Rule` node in the trace tree.
///
/// We apply the rule's substitution to the head atom at `head_index`, then
/// format the result as a string.  This replicates what the private
/// `TraceTreeRuleApplication::to_derived_atom()` does without depending on it.
fn rule_conclusion_string(rule_application: &TraceTreeRuleApplication) -> String {
    let head_atoms = rule_application.rule.head();
    let hi = rule_application.head_index;
    if hi < head_atoms.len() {
        let mut atom: Atom = head_atoms[hi].clone();
        rule_application.assignment.apply(&mut atom);
        atom.to_string()
    } else {
        // Fallback: should not happen for valid traces.
        String::new()
    }
}

/// Extract provenance from one level of an [`ExecutionTraceTree`] node.
///
/// For an EDB (`Fact`) leaf, returns a provenance record with `is_edb: true`
/// and no antecedents.  For an IDB (`Rule`) node, collects the immediate
/// children (the direct premises of the rule firing) as [`ChaseRow`]s.
///
/// # Errors
///
/// Returns `Err(String)` if any immediate antecedent fact string cannot be
/// decoded by [`extract_row_from_fact_string`] — this is a hard failure per
/// the no-optionality doctrine; silently dropping an undecodable antecedent
/// would fabricate provenance metadata.
fn extract_provenance_from_tree(tree: &ExecutionTraceTree) -> Result<ChaseProvenance, String> {
    match tree {
        ExecutionTraceTree::Fact(_ground_atom) => {
            // EDB (asserted) fact — no rule fired, no antecedents.
            Ok(ChaseProvenance {
                is_edb: true,
                rule_name: None,
                antecedent_rows: vec![],
                attributions: vec![],
            })
        }
        ExecutionTraceTree::Rule(rule_application, subtrees) => {
            // IDB (derived) fact.
            let rule_name = rule_application.rule.name();

            // Collect immediate antecedents (the top-level fact each subtree
            // represents — the GroundAtom or derived fact, one level down).
            let antecedent_rows: Vec<ChaseRow> = subtrees
                .iter()
                .map(|subtree| {
                    let fact_str = match subtree {
                        ExecutionTraceTree::Fact(ga) => ga.to_string(),
                        ExecutionTraceTree::Rule(app, _) => {
                            // The conclusion of this sub-derivation.
                            // to_derived_atom() is private; reconstruct it.
                            rule_conclusion_string(app)
                        }
                    };
                    extract_row_from_fact_string(&fact_str).ok_or_else(|| {
                        format!("nemo trace error: could not decode antecedent fact {fact_str:?}")
                    })
                })
                .collect::<Result<Vec<ChaseRow>, String>>()?;

            Ok(ChaseProvenance {
                is_edb: false,
                rule_name,
                antecedent_rows,
                attributions: vec![],
            })
        }
    }
}

// ── Chase driver ──────────────────────────────────────────────────────────────

/// Run the Nemo chase on a complete `.rls` program string and return all
/// materialized facts with their provenance as a flat list of
/// [`ChaseRowWithProvenance`].
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
/// [`ChaseRowWithProvenance`].  Provenance (EDB/IDB, rule name, antecedents)
/// is extracted via Nemo's `trace()` API.  The order of rows and predicates
/// is not specified.
///
/// # Errors
///
/// Returns a `String` describing the first error encountered (parse, validation,
/// execution, or trace).
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
pub fn run_chase(rls: String) -> Result<Vec<ChaseRowWithProvenance>, String> {
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

            // ── 4. Trace each fact for provenance ────────────────────────────
            // Build Nemo Fact objects from our ChaseRows for the trace call.
            // Fact::parse failure is a hard error — emitting false (fabricated
            // EDB) provenance for an unparsable derived fact violates AC-c
            // faithfulness and the no-optionality doctrine.
            let mut parseable_facts: Vec<(usize, Fact)> = Vec::new();
            for (idx, row) in rows.iter().enumerate() {
                let fact_str = chase_row_to_fact_string(row);
                let fact = Fact::parse(&fact_str).map_err(|e| {
                    format!(
                        "nemo provenance error: failed to re-parse derived fact \
                         at index {idx} ({fact_str:?}): {e:?}"
                    )
                })?;
                parseable_facts.push((idx, fact));
            }

            // Call engine.trace() on all parseable facts at once.
            // Returns (ExecutionTrace, Vec<TraceFactHandle>) with one handle per fact.
            let trace_facts: Vec<Fact> = parseable_facts.iter().map(|(_, f)| f.clone()).collect();

            // Build a map: row_index → ChaseProvenance
            let mut provenance_map: Vec<ChaseProvenance> = rows
                .iter()
                .map(|_| ChaseProvenance {
                    is_edb: true,
                    rule_name: None,
                    antecedent_rows: vec![],
                    attributions: vec![],
                })
                .collect();

            if !trace_facts.is_empty() {
                // engine.trace() failure is a hard error — degrading to false EDB
                // provenance would fabricate derivation metadata, violating AC-c.
                let (trace, handles) = engine
                    .trace(trace_facts)
                    .await
                    .map_err(|e| format!("nemo trace error: {e:?}"))?;

                for ((row_idx, _), handle) in parseable_facts.iter().zip(handles.iter()) {
                    if let Some(tree) = trace.tree(*handle) {
                        provenance_map[*row_idx] = extract_provenance_from_tree(&tree)?;
                    }
                    // tree == None means Nemo could not find the fact in the
                    // trace; that is also a faithfulness failure — propagate it.
                    else {
                        return Err(format!(
                            "nemo trace error: no trace tree for derived fact at index {row_idx}"
                        ));
                    }
                }
            }

            // ── 5. Build ChaseRowWithProvenance ──────────────────────────────
            let result: Vec<ChaseRowWithProvenance> = rows
                .into_iter()
                .zip(provenance_map)
                .map(|(row, provenance)| ChaseRowWithProvenance { row, provenance })
                .collect();

            Ok(result)
        })
    })
}

/// Run the chase and return bare [`ChaseRow`]s (without provenance).
///
/// Convenience wrapper used by unit tests that only care about derived facts,
/// not their provenance.  Equivalent to `run_chase(rls).map(|rows| rows.into_iter().map(|r| r.row).collect())`.
pub fn run_chase_rows(rls: String) -> Result<Vec<ChaseRow>, String> {
    run_chase(rls).map(|rows| rows.into_iter().map(|r| r.row).collect())
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

    /// Parse a Nemo rule program **without** the semantic-validation pass.
    ///
    /// [`Self::parse`] (via [`nemo::api::load_program`]) runs Nemo's validator,
    /// which *rejects* the very rule shapes the static certifier exists to flag —
    /// e.g. a head variable not bound by a positive body atom (Nemo error 202,
    /// "unsafe variable used in rule head") or a rule with no positive literals
    /// (error 994, "rule without positive literals are currently unsupported").
    /// The certifier must *see* those rules to diagnose them (DL-safety,
    /// PositiveHorn-negation, StratifiedNAF cycles), so it parses with the
    /// translation-only path `ProgramHandle::from_file` + `materialize`, which
    /// builds the `Program` from the AST translation without the safety
    /// validation that `load_program` applies afterwards.
    ///
    /// Genuine *syntax* errors still fail here (they originate in the parser, not
    /// the validator), so malformed `.rls` is still rejected loudly.
    ///
    /// # Errors
    ///
    /// Returns a string error if Nemo cannot lex/parse the program text.
    pub fn parse_unvalidated(rules: &str) -> Result<Self, String> {
        use nemo::rule_file::RuleFile;
        use nemo::rule_model::programs::handle::ProgramHandle;
        use nemo::rule_model::programs::ProgramWrite;

        let file = RuleFile::new(rules.to_owned(), "<gmeow-logic-certify>".to_owned());
        let warned = ProgramHandle::from_file(&file)
            .map_err(|report| format!("nemo parse error: {report:?}"))?;
        let handle = warned.into_object();

        // Materialize a `Program` from the translated statements (no validation).
        // `handle.materialize()` is private to crate visibility for some Nemo
        // revisions, so rebuild via the public `ProgramRead`/`ProgramWrite` API.
        let mut program = Program::default();
        for statement in handle.statements() {
            program.add_statement(statement.clone());
        }
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
    fn assert_row_present(rows: &[ChaseRowWithProvenance], predicate: &str, values: &[&str]) {
        let target_values: Vec<String> = values.iter().map(|s| s.to_string()).collect();
        let found = rows
            .iter()
            .any(|r| r.row.predicate == predicate && r.row.values == target_values);
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
        let tc_rows: Vec<&ChaseRowWithProvenance> =
            rows.iter().filter(|r| r.row.predicate == "tc").collect();
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
        let tc1: Vec<&ChaseRowWithProvenance> =
            rows1.iter().filter(|r| r.row.predicate == "tc").collect();
        assert!(!tc1.is_empty(), "first call: expected tc facts");

        // Second call — a different, independent program
        let simple_program = "parent(alice, bob). parent(bob, carol).";
        let rows2 = run_chase(simple_program.to_owned()).expect("second chase call should succeed");
        let parent_rows: Vec<&ChaseRowWithProvenance> = rows2
            .iter()
            .filter(|r| r.row.predicate == "parent")
            .collect();
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

    /// EDB facts must be marked `is_edb = true`, IDB (derived) facts `false`.
    #[test]
    fn test_edb_facts_are_marked_edb() {
        let rows = run_chase(TC_PROGRAM.to_owned()).expect("chase should succeed");
        // `e` facts are EDB
        let e_rows: Vec<&ChaseRowWithProvenance> =
            rows.iter().filter(|r| r.row.predicate == "e").collect();
        for r in &e_rows {
            assert!(
                r.provenance.is_edb,
                "e-fact {r:?} should be EDB but is_edb=false"
            );
        }
    }

    /// `tc(a,c)` — the closure fact requiring two rule firings — must be IDB
    /// (derived), not EDB.
    #[test]
    fn test_derived_tc_a_c_is_idb() {
        let rows = run_chase(TC_PROGRAM.to_owned()).expect("chase should succeed");
        let tc_ac: Vec<&ChaseRowWithProvenance> = rows
            .iter()
            .filter(|r| r.row.predicate == "tc" && r.row.values == ["a", "c"])
            .collect();
        assert_eq!(tc_ac.len(), 1, "expected exactly one tc(a,c) fact");
        let prov = &tc_ac[0].provenance;
        assert!(
            !prov.is_edb,
            "tc(a,c) must be IDB (derived), but is_edb=true"
        );
        assert!(
            !prov.antecedent_rows.is_empty(),
            "tc(a,c) must have antecedent rows but antecedent_rows is empty"
        );
    }

    /// Named rules: the rule name must be extractable via `rule_name`.
    #[test]
    fn test_named_rule_provenance() {
        let named_program = r#"
#[name("my-transitivity")]
tc(?x,?z) :- tc(?x,?y), e(?y,?z) .
tc(?x,?y) :- e(?x,?y) .
e(a,b).
e(b,c).
"#;
        let rows = run_chase(named_program.to_owned()).expect("chase should succeed");
        // tc(a,c) requires the named transitivity rule
        let tc_ac: Vec<&ChaseRowWithProvenance> = rows
            .iter()
            .filter(|r| r.row.predicate == "tc" && r.row.values == ["a", "c"])
            .collect();
        assert_eq!(tc_ac.len(), 1, "expected exactly one tc(a,c) fact");
        let prov = &tc_ac[0].provenance;
        assert_eq!(
            prov.rule_name.as_deref(),
            Some("my-transitivity"),
            "expected rule_name='my-transitivity', got {:?}",
            prov.rule_name
        );
    }

    /// `split_nemo_args` must correctly split IRI and string arguments.
    #[test]
    fn test_split_nemo_args_iri() {
        let s = "<http://a.com>, <http://b.com>, \"world\"";
        let parts = split_nemo_args(s);
        assert_eq!(parts, vec!["<http://a.com>", "<http://b.com>", "\"world\""]);
    }

    #[test]
    fn test_split_nemo_args_with_lang_literal() {
        let s = "<http://s>, \"hello\"@en, \"http://w\"";
        let parts = split_nemo_args(s);
        assert_eq!(parts, vec!["<http://s>", "\"hello\"@en", "\"http://w\""]);
    }
}
