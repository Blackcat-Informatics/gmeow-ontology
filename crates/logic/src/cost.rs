// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic engine-benchmark seams + the decomposable cost vector.
//!
//! This module is the public boundary a benchmark harness drives each reasoning
//! engine through, and the carrier of the deterministic cost signal the
//! `LOGIC-PERFORMANCE.md §Measurement doctrine` mandates: *"Cost is an algebra,
//! not a scalar … carried as a decomposable cost vector keyed by (rule, predicate,
//! stratum), reusing the stratification the certifier already computes; the
//! committed-derivation count, allocation bytes/count, and peak-live bytes are its
//! scalar projections."*
//!
//! - [`CostVector`] is the scalar-projection carrier of that cost semiring: a
//!   sorted [`std::collections::BTreeMap`] from [`CostKey`] `(rule, predicate,
//!   stratum)` to the committed-derived-row count, plus the allocation-scalar slots
//!   a later measurement pass fills. Its serialization is integer-valued and
//!   byte-deterministic (identical inputs ⇒ identical bytes), so it is the integer
//!   baseline the drift gate compares.
//! - [`run_native_forward`] drives the native stratified core and returns the full
//!   decomposable [`NativeForwardRun`] (rows + `consumed_steps` + cost vector +
//!   engine identity).
//! - [`run_nemo_forward`] drives the demoted Nemo bootstrap oracle and returns only
//!   its deterministically-ordered [`ForwardRows`] — Nemo exposes no governor step
//!   counts, so no cost vector is fabricated for it (the no-optionality doctrine:
//!   an absent measurement is absent, never a zeroed lie).
//!
//! Both seams build the typed EDB through the SAME
//! [`crate::reason::build_edb_facts`] the production reasoning path uses, so every
//! engine sees a byte-identical fact set and any measured difference is the engine's
//! alone.

use std::collections::BTreeMap;

use purrdf::{RdfDataset, TermValue};

use crate::oracle::{ForwardBudget, ForwardOracle, TypedChaseResult, TypedRow};
use crate::result::EngineId;

/// Wrap a cost-seam condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn cost_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Engine { detail })
}

/// One coordinate of the decomposable cost vector: `(rule, predicate, stratum)`.
///
/// `rule` is the firing rule IRI (from the row's provenance — never the EDB-echo
/// assert sentinel, since only derived rows are keyed), `predicate` is the bare
/// predicate IRI of the derived row, and `stratum` is that predicate's stratum
/// index from the certifier ([`crate::certify::predicate_strata`]). Ordered
/// `(rule, predicate, stratum)` so the [`BTreeMap`] emission is deterministic.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CostKey {
    /// The firing rule IRI (`#[name(...)]`) that committed the derivation.
    pub rule: String,
    /// The bare predicate IRI of the derived row.
    pub predicate: String,
    /// The predicate's stratum index (certifier stratification).
    pub stratum: u32,
}

/// The decomposable cost vector — the scalar-projection carrier of the cost
/// semiring (`LOGIC-PERFORMANCE.md §Measurement doctrine`).
///
/// The primary axis is the committed-derived-row count per [`CostKey`], held in a
/// sorted [`BTreeMap`] so every projection and serialization is deterministic. The
/// allocation scalars ([`Self::alloc_bytes`], [`Self::alloc_count`],
/// [`Self::peak_live_bytes`]) are the remaining scalar projections; they default to
/// `0` here and are populated by the deterministic allocation-measurement pass — no
/// allocation number is invented at this seam.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CostVector {
    /// Committed-derived-row count per `(rule, predicate, stratum)`, sorted.
    counts: BTreeMap<CostKey, u64>,
    /// Total bytes allocated during the run (a scalar projection; `0` until measured).
    alloc_bytes: u64,
    /// Total allocation count during the run (a scalar projection; `0` until measured).
    alloc_count: u64,
    /// Peak simultaneously-live bytes during the run (a scalar projection; `0` until measured).
    peak_live_bytes: u64,
}

impl CostVector {
    /// Aggregate a typed forward result into the cost vector, keying every DERIVED
    /// (non-EDB) row by `(firing rule, predicate, stratum)` and counting it.
    ///
    /// `strata` is the certifier's per-predicate stratum map
    /// ([`crate::certify::predicate_strata`]) over the SAME rule text that produced
    /// `chase`. EDB-echo rows are skipped (they carry no firing rule). A derived row
    /// whose provenance carries no firing rule, or whose predicate the certifier
    /// never stratified, is a genuine engine/certifier inconsistency and a hard
    /// error — never a silently-dropped or defaulted derivation.
    ///
    /// # Errors
    ///
    /// Returns `Err` if a derived row lacks a firing rule name, or if a derived
    /// row's predicate is absent from `strata`.
    pub(crate) fn from_chase(
        chase: &TypedChaseResult,
        strata: &BTreeMap<String, u32>,
    ) -> gmeow_errors::Result<Self> {
        let mut counts: BTreeMap<CostKey, u64> = BTreeMap::new();
        for (row, prov) in &chase.rows {
            if prov.is_edb {
                continue;
            }
            let Some(rule) = prov.rule_name.clone() else {
                return Err(cost_err(format!(
                    "derived row for predicate {:?} carries no firing rule — a derived \
                     fact must cite the rule that committed it, never an empty attribution",
                    row.predicate
                )));
            };
            let predicate = row.predicate.clone();
            let Some(&stratum) = strata.get(&predicate) else {
                return Err(cost_err(format!(
                    "derived predicate {predicate:?} has no stratum in the certifier's \
                     stratification — a rule-head predicate must be stratified; this is an \
                     engine/certifier inconsistency, not a defaultable cost"
                )));
            };
            *counts
                .entry(CostKey {
                    rule,
                    predicate,
                    stratum,
                })
                .or_insert(0) += 1;
        }
        Ok(CostVector {
            counts,
            alloc_bytes: 0,
            alloc_count: 0,
            peak_live_bytes: 0,
        })
    }

    /// The total committed-derivation count — the sum over every coordinate (the
    /// scalar projection onto the counting semiring's `1`).
    #[must_use]
    pub fn total_derivations(&self) -> u64 {
        self.counts.values().sum()
    }

    /// The number of distinct `(rule, predicate, stratum)` coordinates carrying a count.
    #[must_use]
    pub fn attributed_coordinates(&self) -> usize {
        self.counts.len()
    }

    /// The sorted `(rule, predicate, stratum, count)` tuples — the integer-valued,
    /// byte-deterministic serialization the committed baseline compares.
    ///
    /// [`BTreeMap`] iteration is in `CostKey` order, so the tuple sequence is a pure
    /// function of `(engine version, corpus)` — identical inputs ⇒ identical bytes.
    #[must_use]
    pub fn to_sorted_tuples(&self) -> Vec<(String, String, u32, u64)> {
        self.counts
            .iter()
            .map(|(key, &count)| (key.rule.clone(), key.predicate.clone(), key.stratum, count))
            .collect()
    }

    /// Total bytes allocated during the run (a scalar projection; `0` until measured).
    #[must_use]
    pub fn alloc_bytes(&self) -> u64 {
        self.alloc_bytes
    }

    /// Total allocation count during the run (a scalar projection; `0` until measured).
    #[must_use]
    pub fn alloc_count(&self) -> u64 {
        self.alloc_count
    }

    /// Peak simultaneously-live bytes during the run (a scalar projection; `0` until measured).
    #[must_use]
    pub fn peak_live_bytes(&self) -> u64 {
        self.peak_live_bytes
    }

    /// Attach the deterministically-measured allocation scalars (the remaining
    /// scalar projections of the cost semiring). The later allocation-measurement
    /// pass calls this once; the derivation counts are never touched.
    pub fn set_allocation(&mut self, alloc_bytes: u64, alloc_count: u64, peak_live_bytes: u64) {
        self.alloc_bytes = alloc_bytes;
        self.alloc_count = alloc_count;
        self.peak_live_bytes = peak_live_bytes;
    }
}

/// A single materialized row exposed across the public benchmark seam: the bare
/// relation name plus its decoded native-term arguments (the neutral shape both the
/// native and Nemo engines emit — a public projection of the crate-internal
/// [`TypedRow`]).
#[derive(Debug, Clone, PartialEq)]
pub struct ForwardRow {
    /// The relation name (a full predicate IRI, un-bracketed, or a bare program-local symbol).
    pub predicate: String,
    /// One decoded native term per column in the row.
    pub args: Vec<TermValue>,
}

/// A deterministically-ordered materialized row set — the engine-neutral forward
/// output both seams return.
///
/// The rows are sorted by `(predicate, term-display of each argument)`, a total
/// order independent of the engine's internal emission order, so two engines (or two
/// runs of one engine) over the same EDB yield byte-comparable row sets.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ForwardRows {
    /// The materialized rows in canonical `(predicate, args)` order.
    pub rows: Vec<ForwardRow>,
}

impl ForwardRows {
    /// Project a typed chase result's rows into the sorted public row set.
    fn from_chase(chase: &TypedChaseResult) -> Self {
        Self::from_typed_rows(chase.rows.iter().map(|(row, _prov)| row))
    }

    /// Project any iterator of crate-internal [`TypedRow`]s into the sorted public set.
    fn from_typed_rows<'a>(rows: impl Iterator<Item = &'a TypedRow>) -> Self {
        let mut out: Vec<ForwardRow> = rows
            .map(|row| ForwardRow {
                predicate: row.predicate.clone(),
                args: row.args.clone(),
            })
            .collect();
        // Canonical, engine-independent order: predicate first, then the term-display
        // surface of each argument (deterministic and total — the display is a pure
        // function of the term).
        out.sort_by(|a, b| {
            let ka = row_sort_key(a);
            let kb = row_sort_key(b);
            ka.cmp(&kb)
        });
        ForwardRows { rows: out }
    }

    /// Whether the row set is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// The number of rows.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }
}

/// The canonical sort key of a row: `(predicate, [term-display, …])` — a pure
/// function of the row, so the ordering is fully deterministic.
fn row_sort_key(row: &ForwardRow) -> (String, Vec<String>) {
    (
        row.predicate.clone(),
        row.args
            .iter()
            .map(crate::provenance::term_display)
            .collect(),
    )
}

/// The full decomposable result of a native forward run: the deterministically
/// ordered row set, the governor's committed-derivation count, the cost vector, and
/// the engine-version identity the run was produced under.
#[derive(Debug, Clone)]
pub struct NativeForwardRun {
    /// The materialized rows (asserted EDB ∪ derived), in canonical order.
    pub rows: ForwardRows,
    /// The governor's committed-derivation count (`0` on the ungoverned arity-generic path).
    pub consumed_steps: u64,
    /// The decomposable cost vector keyed by `(rule, predicate, stratum)`.
    pub cost: CostVector,
    /// The engine-version identity this run was produced under.
    pub engine: EngineId,
}

/// Drive the NATIVE stratified forward core over `edb` under `rules`, returning the
/// decomposable [`NativeForwardRun`].
///
/// The typed EDB is built through [`crate::reason::build_edb_facts`] (the exact
/// production construction), then the native evaluation runs through
/// [`crate::oracle::native_forward_with_frontier`] — the same body the production
/// [`crate::oracle::NativeForwardOracle`] runs, additionally surfacing the
/// governor's completion frontier. `consumed_steps` is read from that frontier; the
/// [`CostVector`] is aggregated from the result's derived rows + provenance and the
/// certifier's per-predicate stratification ([`crate::certify::predicate_strata`]).
///
/// # Errors
///
/// Returns `Err` if the EDB cannot be built, if the native chase declines the rule
/// set (a declared gap), if the certifier cannot stratify the rules, or if a derived
/// row cannot be attributed a `(rule, predicate, stratum)` coordinate.
pub fn run_native_forward(edb: &RdfDataset, rules: &str) -> gmeow_errors::Result<NativeForwardRun> {
    let facts = crate::reason::build_edb_facts(edb)?;
    let (chase, frontier) = crate::oracle::native_forward_with_frontier(&facts, rules)?;
    let strata = crate::certify::predicate_strata(rules)?;
    let cost = CostVector::from_chase(&chase, &strata)?;
    Ok(NativeForwardRun {
        rows: ForwardRows::from_chase(&chase),
        consumed_steps: frontier.consumed_steps,
        cost,
        engine: EngineId::native(),
    })
}

/// Drive the DEMOTED Nemo bootstrap oracle over `edb` under `rules`, returning ONLY
/// its deterministically-ordered [`ForwardRows`].
///
/// Nemo is off the primary reasoning path and exposes no governor step counts, so
/// this seam fabricates no cost vector for it (the no-optionality doctrine). It
/// reuses the SAME [`crate::reason::build_edb_facts`] EDB construction as
/// [`run_native_forward`], so both engines see a byte-identical fact set — the
/// precondition for a faithful native↔Nemo comparison.
///
/// # Errors
///
/// Returns `Err` if the EDB cannot be built or if the Nemo chase fails to
/// parse/validate/evaluate/decode.
pub fn run_nemo_forward(edb: &RdfDataset, rules: &str) -> gmeow_errors::Result<ForwardRows> {
    let facts = crate::reason::build_edb_facts(edb)?;
    let oracle = crate::oracle::nemo_forward_oracle();
    let chase = oracle.materialize(&facts, rules, &ForwardBudget::UNBOUNDED)?;
    Ok(ForwardRows::from_chase(&chase))
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::{RdfDatasetBuilder, RdfQuad, RdfTerm};

    const EDGE: &str = "http://example.org/edge";
    const PATH: &str = "http://example.org/path";
    const REACH: &str = "http://example.org/reach";
    const W: &str = "http://example.org/w";
    const A: &str = "http://example.org/a";
    const B: &str = "http://example.org/b";
    const C: &str = "http://example.org/c";

    /// A tiny 2-stratum pure-ternary Datalog program (binary-eligible, so the
    /// governed semi-naive core runs and `consumed_steps` is populated):
    ///
    /// * stratum 1 — `path` is the transitive closure of `edge`;
    /// * stratum 2 — `reach` echoes every `path` edge.
    ///
    /// Named rules (`#[name(...)]`) so derived rows carry a real firing-rule IRI.
    fn two_stratum_rules() -> String {
        format!(
            "#[name(\"http://example.org/rules/edge-is-path\")]\n\
             <{PATH}>(?s, ?o, ?w) :- <{EDGE}>(?s, ?o, ?w) .\n\
             #[name(\"http://example.org/rules/path-trans\")]\n\
             <{PATH}>(?s, ?o, ?w) :- <{PATH}>(?s, ?m, ?w), <{EDGE}>(?m, ?o, ?w) .\n\
             #[name(\"http://example.org/rules/path-is-reach\")]\n\
             <{REACH}>(?s, ?o, ?w) :- <{PATH}>(?s, ?o, ?w) .\n"
        )
    }

    /// EDB: `edge(a, b)`, `edge(b, c)` in world `W`.
    fn two_edge_edb() -> std::sync::Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for (s, o) in [(A, B), (B, C)] {
            builder.push_owned_quad(
                &RdfQuad::new(RdfTerm::iri(s), EDGE, RdfTerm::iri(o)).in_graph(RdfTerm::iri(W)),
            );
        }
        builder.freeze().expect("valid test dataset")
    }

    /// The native seam is deterministic (byte-identical cost vector across two runs)
    /// AND its attribution is non-vacuous (≥1 real-rule coordinate with a nonzero
    /// count), and the Nemo seam feasibly returns a non-empty row set over the SAME
    /// program — the seams a benchmark harness drives each engine through.
    #[test]
    fn native_forward_cost_vector_is_deterministic_and_attributed() {
        let edb = two_edge_edb();
        let rules = two_stratum_rules();

        let run_a = run_native_forward(edb.as_ref(), &rules).expect("native forward run a");
        let run_b = run_native_forward(edb.as_ref(), &rules).expect("native forward run b");

        // Determinism: the two cost vectors are byte-identical, and so is their
        // integer serialization.
        assert_eq!(
            run_a.cost, run_b.cost,
            "the native cost vector must be deterministic across identical runs"
        );
        assert_eq!(run_a.cost.to_sorted_tuples(), run_b.cost.to_sorted_tuples());
        assert_eq!(
            run_a.rows, run_b.rows,
            "the native row set must be deterministic"
        );
        assert_eq!(run_a.consumed_steps, run_b.consumed_steps);

        // Non-vacuous attribution: ≥1 coordinate keyed to a REAL rule IRI with a
        // nonzero count.
        let tuples = run_a.cost.to_sorted_tuples();
        assert!(
            tuples.iter().any(|(rule, _pred, _stratum, count)| {
                rule.starts_with("http://example.org/rules/") && *count > 0
            }),
            "the cost vector must attribute ≥1 derivation to a real rule: {tuples:?}"
        );
        assert!(
            run_a.cost.total_derivations() > 0,
            "the program derives facts, so total_derivations must be nonzero"
        );

        // The governed semi-naive core committed derivations, so the step probe is nonzero.
        assert!(
            run_a.consumed_steps > 0,
            "the binary-eligible program runs the governed core; consumed_steps must be nonzero"
        );

        // Engine identity is stamped.
        assert_eq!(run_a.engine, EngineId::native());

        // Two distinct strata are attributed: `path` (stratum 1) and `reach`
        // (stratum 2) — proving the stratum coordinate is threaded, not flattened.
        let strata: std::collections::BTreeSet<u32> = tuples
            .iter()
            .map(|(_r, _p, stratum, _c)| *stratum)
            .collect();
        assert!(
            strata.contains(&1) && strata.contains(&2),
            "both stratum 1 (path) and stratum 2 (reach) must be attributed: {tuples:?}"
        );

        // Feasibility: the Nemo seam returns a non-empty row set over the same program.
        let nemo = run_nemo_forward(edb.as_ref(), &rules).expect("nemo forward run");
        assert!(
            !nemo.is_empty(),
            "the Nemo seam must materialize a non-empty row set for the same program"
        );
    }
}
