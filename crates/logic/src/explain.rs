// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The explanation skeleton emitter (issue #497) — the sole authority; the Python
//! explanation oracle (`gmeow_tools.logic_explain`) was retired in #497.
//!
//! Given a materialization result (a list of derived quads each carrying full seam
//! provenance), this module reconstructs the derivation tree for a target quad and
//! emits its **cited-IRI skeleton** — the conformance surface the runner compares.
//! Prose rendering is NOT reproduced here: the conformance gate compares only the
//! `cited_iris` set and matches explanations by `target_quad_reifier`.
//!
//! # The authoritative recipe
//!
//! Every IRI and every ordering decision here is golden-pinned (conformance
//! goldens compare the cited-IRI skeleton):
//!
//! 1. **Reifier recipe** — `sha1("<{s}> <{p}> {obj_n3}")` under `{NAMESPACE}reifier/`,
//!    reusing [`crate::provenance::reifier_from_strings`] (the object N3 string is used
//!    verbatim; subject/predicate are IRIs wrapped in `<...>`).
//! 2. **World-scoped index** — keyed by `(graph, reifier)` so identical `(S, P, O)`
//!    triples in different worlds never collide and antecedents resolve from the
//!    correct world.
//! 3. **DFS reconstruction** — antecedents are `sorted(src for src in source_quad_ids
//!    if src != target_reifier)`, resolved within the same graph, with a cycle guard
//!    on the `(graph, reifier)` visited set. Steps are returned in DFS order: current
//!    step first, then each child subtree.
//! 4. **Sorted everywhere** — `term_iris`, `source_step_ids`, and `cited_iris` use the
//!    same lexicographic sort the Python `sorted()` produces. `cited_iris` is a
//!    [`BTreeSet`] (sorted by construction).
//!
//! # No-optionality
//!
//! An unresolved antecedent reifier (`source_quad_ids` references a quad not present in
//! the result for its world) is a hard error ([`ExplainError::UnresolvedReifier`]). A
//! repeated `(graph, reifier)` in the visited set is a hard error
//! ([`ExplainError::Cycle`]). There is no silent skip and no degraded fallback.

use std::collections::{BTreeSet, HashMap};

use crate::provenance::{reifier_from_strings, ASSERT_RULE_IRI};

// ── Input row ────────────────────────────────────────────────────────────────

/// One materialized quad row, the input unit of the explanation engine.
///
/// Mirrors the Python `DerivedQuad` fields the explanation engine reads:
/// `graph`, `subject`, `predicate`, `obj` (object in canonical N3 form),
/// `derivation_id`, `rule_iri`, and `source_quad_ids` (antecedent reifier IRIs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// World (named-graph) IRI this quad lives in.
    pub graph: String,
    /// Subject IRI.
    pub subject: String,
    /// Predicate IRI.
    pub predicate: String,
    /// Object term in canonical N3 form (`<iri>` for an IRI; `"lex"^^<dt>` for a literal).
    pub obj: String,
    /// Content-addressed derivation IRI for this quad's firing.
    pub derivation_id: String,
    /// The firing rule IRI (`logic:assert` for asserted facts).
    pub rule_iri: String,
    /// Reifier IRIs of the antecedent quads consumed by the firing.
    pub source_quad_ids: Vec<String>,
}

// ── Errors ───────────────────────────────────────────────────────────────────

/// Errors raised while reconstructing a derivation tree.  Mirror the Python
/// `ExplainError` conditions exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExplainError {
    /// A reifier referenced in `source_quad_ids` (or the target reifier) has no
    /// corresponding quad in the result for the given world.
    UnresolvedReifier {
        /// The unresolved reifier IRI.
        reifier: String,
        /// The world it was looked up within.
        graph: String,
    },
    /// A `(graph, reifier)` pair was visited twice — the proof trace must be a DAG.
    Cycle {
        /// The reifier IRI where the cycle was detected.
        reifier: String,
        /// The world the cycle was detected in.
        graph: String,
    },
    /// The requested target index is out of range for the supplied rows.
    TargetIndexOutOfRange {
        /// The requested index.
        index: usize,
        /// The number of rows supplied.
        len: usize,
    },
}

impl std::fmt::Display for ExplainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExplainError::UnresolvedReifier { reifier, graph } => write!(
                f,
                "Cannot resolve reifier IRI <{reifier}> in world <{graph}> to a quad. \
                 This IRI appears in source_quad_ids but has no corresponding quad in \
                 the materialization result."
            ),
            ExplainError::Cycle { reifier, graph } => write!(
                f,
                "Cycle detected in derivation graph at reifier <{reifier}> in world \
                 <{graph}>. The proof trace must be a DAG."
            ),
            ExplainError::TargetIndexOutOfRange { index, len } => {
                write!(f, "Target index {index} is out of range for {len} rows.")
            }
        }
    }
}

impl std::error::Error for ExplainError {}

/// Raised when a cited IRI is not present in the full proof trace.  Mirrors the
/// Python `FaithfulnessError`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaithfulnessError {
    /// The IRI that was cited but is not in the proof trace.
    pub cited_iri: String,
    /// The `derivation_id` of the quad being explained.
    pub explanation_target: String,
}

impl std::fmt::Display for FaithfulnessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Faithfulness violation: cited IRI <{}> is not present in the proof trace \
             for derivation <{}>.",
            self.cited_iri, self.explanation_target
        )
    }
}

impl std::error::Error for FaithfulnessError {}

// ── Proof-tree types ─────────────────────────────────────────────────────────

/// One node in the derivation tree, rendered for the explanation skeleton.
///
/// Mirrors the Python `ExplanationStep` NamedTuple field-for-field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExplanationStep {
    /// Stable derivation IRI for this step.
    pub derivation_id: String,
    /// The firing rule IRI (or the assert sentinel for asserted facts).
    pub rule_iri: String,
    /// Reifier IRI for this step's `(S, P, O)`.
    pub quad_reifier: String,
    /// Subject IRI.
    pub subject_iri: String,
    /// Predicate IRI.
    pub predicate_iri: String,
    /// Object term in canonical N3 form.
    pub obj_n3: String,
    /// World (named-graph) IRI.
    pub graph_iri: String,
    /// Sorted, distinct term IRIs cited at this step (subject, predicate, and the
    /// object IRI if the object is an IRI).
    pub term_iris: Vec<String>,
    /// Sorted derivation IDs of the immediate antecedents' first step.
    pub source_step_ids: Vec<String>,
    /// `true` iff this quad was an input fact (`rule_iri == logic:assert`).
    pub is_asserted: bool,
    /// Depth in the derivation tree (0 = the target quad).
    pub depth: u32,
}

/// The full explanation for a single derived (or asserted) quad.
///
/// Mirrors the Python `Explanation` minus the prose machinery (the conformance gate
/// compares only `cited_iris` and matches by `target_quad_reifier`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Explanation {
    /// The `derivation_id` of the quad being explained.
    pub target_derivation_id: String,
    /// The reifier IRI of the target quad's `(S, P, O)`.
    pub target_quad_reifier: String,
    /// The named-graph (world) IRI the target quad lives in.
    pub world_iri: String,
    /// The derivation tree in DFS order (target first, then antecedent subtrees).
    pub step_skeleton: Vec<ExplanationStep>,
    /// The complete cited-IRI set (the conformance surface), sorted by construction.
    pub cited_iris: BTreeSet<String>,
}

// ── Reifier helper ───────────────────────────────────────────────────────────

/// Compute the reifier IRI for a row, using the byte-identical Python recipe.
///
/// Subject/predicate are IRIs (wrapped `<...>`); the object N3 string is used verbatim.
pub fn reifier_from_row(row: &Row) -> String {
    reifier_from_strings(&row.subject, &row.predicate, &row.obj)
}

// ── Term-IRI collection ──────────────────────────────────────────────────────

/// Collect the sorted, distinct term IRIs cited at one step: subject, predicate, and
/// the object IRI if the object is an IRI (N3 `<...>`).  Mirrors `_collect_term_iris`.
fn collect_term_iris(row: &Row) -> Vec<String> {
    let mut iris: BTreeSet<String> = BTreeSet::new();
    iris.insert(row.subject.clone());
    iris.insert(row.predicate.clone());
    if let Some(inner) = strip_iri_n3(&row.obj) {
        iris.insert(inner.to_owned());
    }
    iris.into_iter().collect()
}

/// If `n3` is an IRI N3 token `<iri>`, return the inner IRI; otherwise `None`.
fn strip_iri_n3(n3: &str) -> Option<&str> {
    n3.strip_prefix('<').and_then(|s| s.strip_suffix('>'))
}

// ── Index ────────────────────────────────────────────────────────────────────

/// Precompute one reifier IRI per row (same order as `rows`).
///
/// Reifiers are SHA-1-derived `String`s that cannot be borrowed from `Row`, so we
/// materialise them once into an owned `Vec` whose lifetime can then be tied to the
/// borrow-based index.
fn precompute_reifiers(rows: &[Row]) -> Vec<String> {
    rows.iter().map(reifier_from_row).collect()
}

/// Build the `(graph, reifier)` → row-index lookup from pre-computed reifiers.
///
/// Borrows graph IRIs from `rows` and reifier strings from `reifiers`; both slices
/// must outlive the returned map.  On a duplicate `(graph, reifier)` the LAST row
/// wins, matching the Python `dict` overwrite semantics
/// (`index[(dq.graph, reifier)] = dq`).
fn build_reifier_index<'a>(
    rows: &'a [Row],
    reifiers: &'a [String],
) -> HashMap<(&'a str, &'a str), usize> {
    let mut index: HashMap<(&'a str, &'a str), usize> = HashMap::with_capacity(rows.len());
    for (i, (row, reifier)) in rows.iter().zip(reifiers.iter()).enumerate() {
        index.insert((row.graph.as_str(), reifier.as_str()), i);
    }
    index
}

// ── Reconstruction ───────────────────────────────────────────────────────────

/// Recursively reconstruct the derivation tree for the quad at `(graph, target_reifier)`.
///
/// Returns steps in DFS order: the current step first, then each antecedent subtree.
/// Mirrors `_reconstruct_derivation_tree` exactly (sorted antecedents, world-scoped
/// resolution, cycle guard on the `(graph, reifier)` visited set).
///
/// # Backtracking visited set
///
/// `visited` is a single `Vec` threaded through the entire recursion.  Before
/// descending into children we push the current `(graph_iri, target_reifier)` key
/// and pop it again after all children return, so the allocation is O(D) rather
/// than O(D²).  The borrowed `(&str, &str)` elements avoid per-step `String`
/// allocation entirely.
fn reconstruct_tree<'a>(
    target_reifier: &'a str,
    graph_iri: &'a str,
    rows: &'a [Row],
    index: &HashMap<(&str, &str), usize>,
    depth: u32,
    visited: &mut Vec<(&'a str, &'a str)>,
) -> Result<Vec<ExplanationStep>, ExplainError> {
    let lookup_key = (graph_iri, target_reifier);

    let row_idx = match index.get(&lookup_key) {
        Some(idx) => *idx,
        None => {
            return Err(ExplainError::UnresolvedReifier {
                reifier: target_reifier.to_owned(),
                graph: graph_iri.to_owned(),
            });
        }
    };

    if visited.contains(&lookup_key) {
        return Err(ExplainError::Cycle {
            reifier: target_reifier.to_owned(),
            graph: graph_iri.to_owned(),
        });
    }

    let row = &rows[row_idx];
    let is_asserted = row.rule_iri == ASSERT_RULE_IRI;

    // Antecedent reifiers: sorted, excluding the self-reference asserted facts carry.
    let mut antecedent_reifiers: Vec<&str> = row
        .source_quad_ids
        .iter()
        .filter(|src| src.as_str() != target_reifier)
        .map(String::as_str)
        .collect();
    antecedent_reifiers.sort();

    // Push current node before descending; pop after all children return (backtracking).
    visited.push(lookup_key);

    let mut child_steps: Vec<ExplanationStep> = Vec::new();
    let mut source_step_ids: Vec<String> = Vec::new();
    for src_reifier in antecedent_reifiers {
        let sub_steps = reconstruct_tree(src_reifier, graph_iri, rows, index, depth + 1, visited)?;
        if let Some(first) = sub_steps.first() {
            source_step_ids.push(first.derivation_id.clone());
        }
        child_steps.extend(sub_steps);
    }

    // Pop current node after all children have been processed.
    visited.pop();

    source_step_ids.sort();

    let step = ExplanationStep {
        derivation_id: row.derivation_id.clone(),
        rule_iri: row.rule_iri.clone(),
        quad_reifier: target_reifier.to_owned(),
        subject_iri: row.subject.clone(),
        predicate_iri: row.predicate.clone(),
        obj_n3: row.obj.clone(),
        graph_iri: row.graph.clone(),
        term_iris: collect_term_iris(row),
        source_step_ids,
        is_asserted,
        depth,
    };

    let mut steps = Vec::with_capacity(child_steps.len() + 1);
    steps.push(step);
    steps.extend(child_steps);
    Ok(steps)
}

/// Build the cited-IRI set (the conformance surface) from the steps + world IRI.
/// Mirrors `_build_proof_trace_iris`.
fn build_cited_iris(steps: &[ExplanationStep], world_iri: &str) -> BTreeSet<String> {
    let mut iris: BTreeSet<String> = BTreeSet::new();
    iris.insert(world_iri.to_owned());
    for step in steps {
        iris.insert(step.derivation_id.clone());
        iris.insert(step.rule_iri.clone());
        iris.insert(step.quad_reifier.clone());
        iris.insert(step.subject_iri.clone());
        iris.insert(step.predicate_iri.clone());
        iris.insert(step.graph_iri.clone());
        if let Some(inner) = strip_iri_n3(&step.obj_n3) {
            iris.insert(inner.to_owned());
        }
        for t in &step.term_iris {
            iris.insert(t.clone());
        }
        for s in &step.source_step_ids {
            iris.insert(s.clone());
        }
    }
    iris
}

// ── Public API ───────────────────────────────────────────────────────────────

/// Build the explanation for the row at `target_index`.
///
/// # Errors
///
/// Returns [`ExplainError::TargetIndexOutOfRange`] if the index is out of range,
/// [`ExplainError::UnresolvedReifier`] if an antecedent (or the target) cannot be
/// resolved within its world, or [`ExplainError::Cycle`] if the derivation graph
/// contains a cycle.
pub fn explain_one(rows: &[Row], target_index: usize) -> Result<Explanation, ExplainError> {
    if target_index >= rows.len() {
        return Err(ExplainError::TargetIndexOutOfRange {
            index: target_index,
            len: rows.len(),
        });
    }
    let reifiers = precompute_reifiers(rows);
    let index = build_reifier_index(rows, &reifiers);
    explain_with_index(rows, &reifiers, target_index, &index)
}

/// Build an explanation for every row, in input order.  Mirrors the Python
/// `_run_explanations`, which iterates `result.quads` in order.
///
/// # Errors
///
/// Propagates any [`ExplainError`] from reconstructing an individual quad's tree.
pub fn explain_all(rows: &[Row]) -> Result<Vec<Explanation>, ExplainError> {
    let reifiers = precompute_reifiers(rows);
    let index = build_reifier_index(rows, &reifiers);
    let mut out: Vec<Explanation> = Vec::with_capacity(rows.len());
    for i in 0..rows.len() {
        out.push(explain_with_index(rows, &reifiers, i, &index)?);
    }
    Ok(out)
}

/// Shared core: build the explanation for `target_index` against a prebuilt index.
///
/// `reifiers[i]` must be the pre-computed reifier IRI for `rows[i]`.
fn explain_with_index(
    rows: &[Row],
    reifiers: &[String],
    target_index: usize,
    index: &HashMap<(&str, &str), usize>,
) -> Result<Explanation, ExplainError> {
    let target = &rows[target_index];
    let target_reifier = &reifiers[target_index];

    let mut visited: Vec<(&str, &str)> = Vec::new();
    let steps = reconstruct_tree(target_reifier, &target.graph, rows, index, 0, &mut visited)?;
    let cited_iris = build_cited_iris(&steps, &target.graph);

    Ok(Explanation {
        target_derivation_id: target.derivation_id.clone(),
        target_quad_reifier: target_reifier.clone(),
        world_iri: target.graph.clone(),
        step_skeleton: steps,
        cited_iris,
    })
}

/// Assert that every cited IRI in `explanation` is present in the full proof trace
/// built from ALL `rows`.  Mirrors `assert_explanation_faithful`.
///
/// For a valid explanation (`cited_iris` is a subset of the trace by construction)
/// this always succeeds; it exists so a fabricated cited IRI is rejected.
///
/// # Errors
///
/// Returns [`FaithfulnessError`] for the first cited IRI (in sorted order) that is
/// not present in the full proof trace.
pub fn assert_faithful(explanation: &Explanation, rows: &[Row]) -> Result<(), FaithfulnessError> {
    let mut trace: BTreeSet<String> = BTreeSet::new();
    for row in rows {
        trace.insert(row.graph.clone());
        trace.insert(row.derivation_id.clone());
        trace.insert(row.rule_iri.clone());
        trace.insert(reifier_from_row(row));
        trace.insert(row.subject.clone());
        trace.insert(row.predicate.clone());
        if let Some(inner) = strip_iri_n3(&row.obj) {
            trace.insert(inner.to_owned());
        }
        for src in &row.source_quad_ids {
            trace.insert(src.clone());
        }
    }

    // cited_iris is a BTreeSet (already sorted); iterate in order so the first
    // offending IRI matches the Python `sorted(...)` iteration.
    for cited in &explanation.cited_iris {
        if !trace.contains(cited) {
            return Err(FaithfulnessError {
                cited_iri: cited.clone(),
                explanation_target: explanation.target_derivation_id.clone(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
