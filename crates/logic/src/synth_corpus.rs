// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Synthetic relational-core Datalog generators for the engine benchmark harness.
//!
//! Each generator emits a self-contained forward-Datalog workload for one classic
//! relational-core family, parameterized by a single scale integer `n`, as a triple
//! `(program, edb, expected_rows)`:
//!
//! - `program` — canonical typed [`gmeow_logic_compile::ir::LogicProgram`] input for
//!   [`crate::cost::run_native_forward`].
//! - `edb` — the world-scoped [`purrdf::RdfDataset`] EDB of `predicate(subject, object, world)`
//!   facts, built the SAME way `benches/graph.rs` and `benches/foundation.rs`
//!   construct their synthetic inputs.
//! - `expected_rows` — the **analytically-known** size of the derived relation(s),
//!   computed by a CLOSED-FORM FORMULA over `n`, never by running an engine. It is
//!   the total count of DERIVED (non-EDB) facts across every IDB predicate — the
//!   quantity [`crate::cost::CostVector::total_derivations`] projects — so a native
//!   run agreeing with it is a non-tautological check (formula vs. engine, not
//!   engine vs. itself).
//!
//! The module is `pub` on `gmeow-logic` so BOTH the in-crate benches
//! (`crates/logic/benches/*`) and the `gmeow-conformance` crate (which depends on
//! `gmeow-logic`) reach the identical generators — one source, no duplication.
//!
//! Every generator is deterministic: identical `n` ⇒ an identical typed program, an
//! identical EDB fact set (pushed in a fixed order and frozen), and an identical
//! `expected_rows`.

use gmeow_logic_compile::ir::{ContextualScope, LogicAxiom, LogicProgram, LogicRule};
use purrdf::{RdfDataset, RdfDatasetBuilder, RdfQuad, RdfTerm};

/// The single named-graph world every synthetic EDB fact lives in.
pub const WORLD: &str = "https://example.org/bench/synth/world";

/// The IRI namespace root for synthetic nodes, predicates, and rule names.
const BASE: &str = "https://example.org/bench/synth";

/// One synthetic relational-core workload: canonical rules, its world-scoped EDB,
/// and the analytically-known count of derived facts.
///
/// Not `Clone` — [`purrdf::RdfDataset`] owns its frozen tables and is intentionally move-only;
/// a consumer that needs the workload twice regenerates it (the generators are pure).
#[derive(Debug)]
pub struct SynthWorkload {
    /// Canonical typed logic program.
    pub program: LogicProgram,
    /// The world-scoped EDB of `predicate(subject, object, world)` facts.
    pub edb: RdfDataset,
    /// The closed-form count of DERIVED facts across every IDB predicate.
    pub expected_rows: u64,
}

fn axiom(subject: &str, predicate: &str, object: &str) -> LogicAxiom {
    LogicAxiom::new(
        subject,
        predicate,
        object,
        false,
        false,
        ContextualScope::default(),
    )
    .expect("synthetic benchmark axiom is valid")
}

fn rule(rule_iri: &str, head: (&str, &str, &str), body: Vec<(&str, &str, &str)>) -> LogicRule {
    let scope = ContextualScope {
        provenance: Some(rule_iri.to_owned()),
        ..ContextualScope::default()
    };
    LogicRule::new(
        axiom(head.0, head.1, head.2),
        body.into_iter()
            .map(|(subject, predicate, object)| axiom(subject, predicate, object))
            .collect(),
        Vec::new(),
        scope,
    )
}

fn program(rules: Vec<LogicRule>) -> LogicProgram {
    LogicProgram::new(Vec::new(), rules, Vec::new(), None)
}

/// A node IRI `…/n{i}` — the synthetic graph vertices.
fn node(i: usize) -> String {
    format!("{BASE}/n{i}")
}

/// Freeze a set of `(subject, predicate, object)` triples into a world-scoped
/// [`purrdf::RdfDataset`] EDB. The triples are pushed in the given order and the frozen
/// dataset's internal order is deterministic, so the EDB is a pure function of the
/// triple sequence.
fn edb_from_triples(triples: &[(String, &str, String)]) -> RdfDataset {
    let mut builder = RdfDatasetBuilder::new();
    for (s, p, o) in triples {
        builder.push_owned_quad(
            &RdfQuad::new(RdfTerm::iri(s.clone()), *p, RdfTerm::iri(o.clone()))
                .in_graph(RdfTerm::iri(WORLD)),
        );
    }
    let frozen = builder.freeze().expect("synthetic EDB is a valid dataset");
    // The builder is the sole owner immediately after `freeze`, so unwrapping the
    // Arc yields the owned dataset without a clone (RdfDataset is not Clone).
    std::sync::Arc::try_unwrap(frozen).expect("frozen synthetic EDB is uniquely owned")
}

/// The `edge` EDB predicate IRI.
fn edge_p() -> String {
    format!("{BASE}/edge")
}
/// The `path` IDB predicate IRI (transitive closure of `edge`).
fn path_p() -> String {
    format!("{BASE}/path")
}

/// **Transitive closure** — a length-`n` linear edge chain
/// `v0 → v1 → … → vn`; `path` is the transitive closure of `edge`.
///
/// A chain of `n` edges over `n + 1` nodes has closure size `C(n+1, 2) =
/// n·(n+1)/2` — every ordered pair `(vi, vj)` with `i < j`. That is the only IDB
/// predicate, so `expected_rows = n·(n+1)/2`.
///
/// # Panics
/// Panics if `n == 0` (an empty program derives nothing to benchmark).
#[must_use]
pub fn transitive_closure(n: usize) -> SynthWorkload {
    assert!(n >= 1, "transitive_closure needs n >= 1");
    let edge = edge_p();
    let path = path_p();

    let mut triples = Vec::with_capacity(n);
    for i in 0..n {
        triples.push((node(i), edge.as_str(), node(i + 1)));
    }
    let edb = edb_from_triples(&triples);

    let program = program(vec![
        rule(
            &format!("{BASE}/rules/tc-base"),
            ("?s", &path, "?o"),
            vec![("?s", &edge, "?o")],
        ),
        rule(
            &format!("{BASE}/rules/tc-step"),
            ("?s", &path, "?o"),
            vec![("?s", &edge, "?m"), ("?m", &path, "?o")],
        ),
    ]);

    let expected_rows = (n as u64) * (n as u64 + 1) / 2;
    SynthWorkload {
        program,
        edb,
        expected_rows,
    }
}

/// **Strongly connected** — a single directed `n`-cycle
/// `v0 → v1 → … → v(n-1) → v0` (the `build_scc_graph` shape: a chain closed into a
/// cycle by a back-edge). `path` is the transitive closure of `edge` and
/// `same_component` pairs mutually-reachable nodes.
///
/// In an `n`-cycle every node reaches every node (including itself, around the
/// loop), so `path` has `n²` ordered pairs and `same_component` (mutual
/// reachability) is complete at `n²` too. Both are IDB predicates, so
/// `expected_rows = 2·n²`.
///
/// # Panics
/// Panics if `n == 0`.
#[must_use]
pub fn strongly_connected(n: usize) -> SynthWorkload {
    assert!(n >= 1, "strongly_connected needs n >= 1");
    let edge = edge_p();
    let path = path_p();
    let same = format!("{BASE}/same_component");

    let mut triples = Vec::with_capacity(n);
    for i in 0..n {
        triples.push((node(i), edge.as_str(), node((i + 1) % n)));
    }
    let edb = edb_from_triples(&triples);

    let program = program(vec![
        rule(
            &format!("{BASE}/rules/scc-path-base"),
            ("?s", &path, "?o"),
            vec![("?s", &edge, "?o")],
        ),
        rule(
            &format!("{BASE}/rules/scc-path-step"),
            ("?s", &path, "?o"),
            vec![("?s", &edge, "?m"), ("?m", &path, "?o")],
        ),
        rule(
            &format!("{BASE}/rules/scc-same-component"),
            ("?s", &same, "?o"),
            vec![("?s", &path, "?o"), ("?o", &path, "?s")],
        ),
    ]);

    let n2 = (n as u64) * (n as u64);
    let expected_rows = 2 * n2;
    SynthWorkload {
        program,
        edb,
        expected_rows,
    }
}

/// **Same generation** — the classic same-generation program over a small two-level
/// tree: a root `r` with `n` parent nodes, each parent with `n` children. `parent`
/// is the EDB relation and `same_gen` pairs nodes at the same tree depth.
///
/// The `n` parents are all children of `r`, giving `n²` sibling pairs at the parent
/// level. The `n²` grandchildren are ALL pairwise same-generation (their parents are
/// pairwise same-generation), giving `(n²)² = n⁴` pairs. The two levels are disjoint,
/// so `expected_rows = n² + n⁴`.
///
/// # Panics
/// Panics if `n == 0`.
#[must_use]
pub fn same_generation(n: usize) -> SynthWorkload {
    assert!(n >= 1, "same_generation needs n >= 1");
    let parent = format!("{BASE}/parent");
    let sg = format!("{BASE}/same_gen");
    let root = format!("{BASE}/root");

    // parent(p_i, r) for each of the n parents; parent(c_{i,j}, p_i) for each child.
    let mut triples: Vec<(String, &str, String)> = Vec::new();
    for i in 0..n {
        let p_i = format!("{BASE}/p{i}");
        triples.push((p_i.clone(), parent.as_str(), root.clone()));
        for j in 0..n {
            let c_ij = format!("{BASE}/c{i}_{j}");
            triples.push((c_ij, parent.as_str(), p_i.clone()));
        }
    }
    let edb = edb_from_triples(&triples);

    let program = program(vec![
        rule(
            &format!("{BASE}/rules/sg-base"),
            ("?x", &sg, "?y"),
            vec![("?x", &parent, "?p"), ("?y", &parent, "?p")],
        ),
        rule(
            &format!("{BASE}/rules/sg-step"),
            ("?x", &sg, "?y"),
            vec![
                ("?x", &parent, "?a"),
                ("?a", &sg, "?b"),
                ("?y", &parent, "?b"),
            ],
        ),
    ]);

    let n2 = (n as u64) * (n as u64);
    let expected_rows = n2 + n2 * n2;
    SynthWorkload {
        program,
        edb,
        expected_rows,
    }
}

/// **Reachability** — single-source reachability from `v0` along a length-`n` edge
/// chain `v0 → v1 → … → vn`. `reach(v0, x)` holds for every node `x` reachable from
/// the fixed source `v0`.
///
/// The source reaches the `n` downstream nodes `v1 … vn`, so `reach` has exactly `n`
/// facts and `expected_rows = n`.
///
/// # Panics
/// Panics if `n == 0`.
#[must_use]
pub fn reachability(n: usize) -> SynthWorkload {
    assert!(n >= 1, "reachability needs n >= 1");
    let edge = edge_p();
    let reach = format!("{BASE}/reach");
    let source = node(0);

    let mut triples = Vec::with_capacity(n);
    for i in 0..n {
        triples.push((node(i), edge.as_str(), node(i + 1)));
    }
    let edb = edb_from_triples(&triples);

    // The source `v0` is a constant in both the base and step rules, so `reach` is
    // seeded and extended only from that single source.
    let program = program(vec![
        rule(
            &format!("{BASE}/rules/reach-base"),
            (&source, &reach, "?o"),
            vec![(&source, &edge, "?o")],
        ),
        rule(
            &format!("{BASE}/rules/reach-step"),
            (&source, &reach, "?o"),
            vec![(&source, &reach, "?m"), ("?m", &edge, "?o")],
        ),
    ]);

    let expected_rows = n as u64;
    SynthWorkload {
        program,
        edb,
        expected_rows,
    }
}
