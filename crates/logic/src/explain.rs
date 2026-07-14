// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The explanation skeleton emitter — the sole authority; the Python
//! explanation oracle (`gmeow_tools.logic_explain`) was retired.
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

use std::collections::BTreeSet;
use std::hash::{BuildHasher, Hash, Hasher};

use foldhash::fast::FixedState;
use gmeow_errors::dag::{DagError, DagNode, walk};
use hashbrown::HashTable;

use crate::provenance::{ASSERT_RULE_IRI, mint_derivation_id, reifier_from_strings};

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

fn reifier_key_hash(graph: &str, reifier: &str) -> u64 {
    let mut hasher = FixedState::default().build_hasher();
    graph.hash(&mut hasher);
    reifier.hash(&mut hasher);
    hasher.finish()
}

/// Build a borrowed-key `(graph, reifier) → row-index` lookup.
///
/// The table stores dense row indexes only; key equality reads `rows` and `reifiers`,
/// so the reusable index does not clone every graph/reifier pair or become
/// self-referential. On a duplicate key the LAST row wins, matching Python `dict`.
fn build_reifier_index(rows: &[Row], reifiers: &[String]) -> HashTable<usize> {
    let mut index: HashTable<usize> = HashTable::new();
    for (i, (row, reifier)) in rows.iter().zip(reifiers.iter()).enumerate() {
        let hash = reifier_key_hash(&row.graph, reifier);
        if let Some(slot) = index.find_mut(hash, |&candidate| {
            rows[candidate].graph == row.graph && reifiers[candidate] == *reifier
        }) {
            *slot = i;
        } else {
            index.insert_unique(hash, i, |&candidate| {
                reifier_key_hash(&rows[candidate].graph, &reifiers[candidate])
            });
        }
    }
    index
}

// ── Reconstruction ───────────────────────────────────────────────────────────

/// Reconstruct the derivation tree for the quad at `(graph, target_reifier)`,
/// returning steps in DFS order: the current step first, then each antecedent
/// subtree.
///
/// The cycle-guarded, backtracking depth-first traversal is delegated to the ONE
/// workspace DAG engine, [`gmeow_errors::dag::walk`] — there is no second copy of
/// the traversal here. This function supplies the domain closures (world-scoped
/// resolution and the sorted/deduped antecedent ordering) and assembles the
/// golden-pinned [`ExplanationStep`] skeleton from the returned tree.
///
/// `graph_iri` is invariant through the whole recursion, so the walk keys on the
/// reifier `&str` alone (the world is captured by the closures) — preserving the
/// original zero-allocation visited set.
fn reconstruct_tree<'a>(
    target_reifier: &'a str,
    graph_iri: &'a str,
    rows: &'a [Row],
    reifiers: &'a [String],
    index: &HashTable<usize>,
) -> Result<Vec<ExplanationStep>, ExplainError> {
    let tree = walk(
        target_reifier,
        // Resolve a reifier to its row index within this world.
        |reifier: &&'a str| {
            let hash = reifier_key_hash(graph_iri, reifier);
            index
                .find(hash, |&candidate| {
                    rows[candidate].graph == graph_iri && reifiers[candidate] == *reifier
                })
                .copied()
        },
        // Antecedent reifiers: sorted and deduped, excluding the self-reference
        // asserted facts carry (a dual-witness listed twice must not double-cite).
        |reifier: &&'a str, row_idx: &usize| {
            let mut antecedents: Vec<&'a str> = rows[*row_idx]
                .source_quad_ids
                .iter()
                .filter(|src| src.as_str() != *reifier)
                .map(String::as_str)
                .collect();
            antecedents.sort();
            antecedents.dedup();
            antecedents
        },
    )
    .map_err(|err| match err {
        DagError::Unresolved(reifier) => ExplainError::UnresolvedReifier {
            reifier: reifier.to_owned(),
            graph: graph_iri.to_owned(),
        },
        DagError::Cycle(reifier) => ExplainError::Cycle {
            reifier: reifier.to_owned(),
            graph: graph_iri.to_owned(),
        },
    })?;

    let mut steps = Vec::new();
    assemble_steps(&tree, rows, &mut steps);
    Ok(steps)
}

/// Assemble the golden-pinned step skeleton from a reconstructed DAG node, in the
/// original DFS order (current step, then each child subtree). Each step's
/// `source_step_ids` are the sorted derivation IDs of its immediate children — the
/// root step of each child subtree, exactly as before.
fn assemble_steps(node: &DagNode<&str, usize>, rows: &[Row], out: &mut Vec<ExplanationStep>) {
    let row = &rows[node.payload];

    let mut source_step_ids: Vec<String> = node
        .children
        .iter()
        .map(|child| rows[child.payload].derivation_id.clone())
        .collect();
    source_step_ids.sort();

    out.push(ExplanationStep {
        derivation_id: row.derivation_id.clone(),
        rule_iri: row.rule_iri.clone(),
        quad_reifier: node.key.to_owned(),
        subject_iri: row.subject.clone(),
        predicate_iri: row.predicate.clone(),
        obj_n3: row.obj.clone(),
        graph_iri: row.graph.clone(),
        term_iris: collect_term_iris(row),
        source_step_ids,
        is_asserted: row.rule_iri == ASSERT_RULE_IRI,
        depth: node.depth,
    });

    for child in &node.children {
        assemble_steps(child, rows, out);
    }
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

/// Reusable lazy explanation index over one materialized result.
///
/// Construction indexes row identity only; it does not construct any proof tree.
/// [`Self::explain_one`] descends the selected antecedents of exactly one queried row,
/// so an unrelated malformed/cyclic provenance component is neither traversed nor
/// allowed to poison the requested witness. This is the bounded backing seam for an
/// `explain(witness)` consumer.
pub struct LazyExplanationIndex<'a> {
    rows: &'a [Row],
    reifiers: Vec<String>,
    index: HashTable<usize>,
}

impl<'a> LazyExplanationIndex<'a> {
    /// Index a materialized result without constructing proof trees.
    #[must_use]
    pub fn new(rows: &'a [Row]) -> Self {
        let reifiers = precompute_reifiers(rows);
        let index = build_reifier_index(rows, &reifiers);
        Self {
            rows,
            reifiers,
            index,
        }
    }

    /// Lazily reconstruct one queried proof tree.
    ///
    /// # Errors
    ///
    /// Returns [`ExplainError::TargetIndexOutOfRange`] if the index is out of range,
    /// [`ExplainError::UnresolvedReifier`] if the queried subtree cites a missing row,
    /// or [`ExplainError::Cycle`] if that subtree contains a cycle.
    pub fn explain_one(&self, target_index: usize) -> Result<Explanation, ExplainError> {
        if target_index >= self.rows.len() {
            return Err(ExplainError::TargetIndexOutOfRange {
                index: target_index,
                len: self.rows.len(),
            });
        }
        explain_with_index(self.rows, &self.reifiers, target_index, &self.index)
    }

    /// Reconstruct every proof in input order while sharing the identity index.
    ///
    /// # Errors
    ///
    /// Propagates the first queried subtree's [`ExplainError`].
    pub fn explain_all(&self) -> Result<Vec<Explanation>, ExplainError> {
        let mut out = Vec::with_capacity(self.rows.len());
        for index in 0..self.rows.len() {
            out.push(self.explain_one(index)?);
        }
        Ok(out)
    }
}

/// Build the explanation for the row at `target_index`.
///
/// # Errors
///
/// Returns [`ExplainError::TargetIndexOutOfRange`] if the index is out of range,
/// [`ExplainError::UnresolvedReifier`] if an antecedent (or the target) cannot be
/// resolved within its world, or [`ExplainError::Cycle`] if the derivation graph
/// contains a cycle.
pub fn explain_one(rows: &[Row], target_index: usize) -> Result<Explanation, ExplainError> {
    LazyExplanationIndex::new(rows).explain_one(target_index)
}

/// Build an explanation for every row, in input order.  Mirrors the Python
/// `_run_explanations`, which iterates `result.quads` in order.
///
/// # Errors
///
/// Propagates any [`ExplainError`] from reconstructing an individual quad's tree.
pub fn explain_all(rows: &[Row]) -> Result<Vec<Explanation>, ExplainError> {
    LazyExplanationIndex::new(rows).explain_all()
}

/// Shared core: build the explanation for `target_index` against a prebuilt index.
///
/// `reifiers[i]` must be the pre-computed reifier IRI for `rows[i]`.
fn explain_with_index(
    rows: &[Row],
    reifiers: &[String],
    target_index: usize,
    index: &HashTable<usize>,
) -> Result<Explanation, ExplainError> {
    let target = &rows[target_index];
    let target_reifier = &reifiers[target_index];

    let steps = reconstruct_tree(target_reifier, &target.graph, rows, reifiers, index)?;
    let cited_iris = build_cited_iris(&steps, &target.graph);

    Ok(Explanation {
        target_derivation_id: target.derivation_id.clone(),
        target_quad_reifier: target_reifier.clone(),
        world_iri: target.graph.clone(),
        step_skeleton: steps,
        cited_iris,
    })
}

// ── Reasoning-result bridge ────────────────────────────────────────────────────

/// Build the faithful cited-IRI derivation skeletons for every derived (and
/// asserted) quad in a reasoning result.
///
/// Each [`InferredAxiom`](crate::reason::InferredAxiom) is mapped to one explain
/// [`Row`]: the world becomes the row `graph`; the axiom's `(subject, predicate,
/// object)` its `(S, P, O)` (the `object` is already the `term_display`/N3 surface
/// the reasoner carries, so it is used verbatim — never re-encoded); and the firing
/// rule its `rule_iri` (an asserted EDB row carries
/// [`crate::provenance::ASSERT_RULE_IRI`]). An asserted row lists its OWN reifier as
/// its single source — the self-reference [`reconstruct_tree`] filters out, so it
/// stays a leaf; a derived row lists the reifiers of its immediate premises. The
/// per-row `derivation_id` is the content-addressed
/// [`crate::provenance::mint_derivation_id`] over that rule and those sources.
///
/// # Premise object surface
///
/// The two production reasoning paths record a premise's object in DIFFERENT
/// surfaces: the EL/chase path carries the `term_display` N3 form (`<iri>`), while
/// the finite DL post-pass carries a BARE resource IRI. A row's own `object` is
/// always the N3 form, so a premise reifier only joins its antecedent row once the
/// premise object is normalized to that N3 surface — [`premise_object_n3`] wraps a
/// bare IRI in `<...>` and leaves an already-N3 IRI, a literal, or a blank untouched.
/// Without it a DL-clash premise (a bare `owl:Nothing`, disjoint class, …) would
/// reify to a different IRI than its antecedent quad and fail to resolve.
///
/// # Premise closure
///
/// A result's `inferred()` closure is NOT self-contained: the default-world DL path
/// echoes only DERIVED axioms, not the asserted EDB facts a derived axiom's premises
/// cite. The row set is therefore CLOSED under premises — every premise not already
/// present as a row (in its axiom's world) is materialized as an asserted leaf row (an
/// input fact, which it always is: the omitted rows are exactly the EDB). Without this
/// closure the derivation tree would cite an antecedent reifier with no backing row —
/// a spurious [`ExplainError::UnresolvedReifier`] on a perfectly sound verdict.
///
/// Returns OWNED [`Explanation`]s (the [`Row`] buffer is built and consumed
/// internally) so a caller need not hold the borrow — sidestepping the
/// [`LazyExplanationIndex`] borrow-lifetime problem entirely.
///
/// # Errors
///
/// Propagates any [`ExplainError`] from reconstructing a quad's derivation tree —
/// an unresolved antecedent reifier ([`ExplainError::UnresolvedReifier`]) or a cycle
/// in the proof trace ([`ExplainError::Cycle`]). When the reasoner has already
/// produced a real verdict, either is an INTERNAL INVARIANT VIOLATION, and the
/// verdict-folding callers HARD-FAIL on it rather than degrading to an advisory.
pub fn explanations_for_result(
    result: &crate::result::ReasoningResult,
) -> Result<Vec<Explanation>, ExplainError> {
    let assert_rule = ASSERT_RULE_IRI.to_owned();
    let mut rows: Vec<Row> = Vec::with_capacity(result.inferred().len());
    // The `(world, reifier)` identities already carried by a row, so a premise is
    // synthesized into a leaf row at most once and never shadows a genuine one.
    let mut present: BTreeSet<(String, String)> = BTreeSet::new();

    // 1. One row per inferred axiom (derived or asserted-EDB echo).
    for axiom in result.inferred() {
        let self_reifier = reifier_from_strings(&axiom.subject, &axiom.predicate, &axiom.object);
        let rule_iri = axiom
            .rule_name
            .clone()
            .unwrap_or_else(|| assert_rule.clone());
        // Asserted facts are leaves: they carry their own reifier as the sole source
        // (filtered out during reconstruction). A derived quad cites the reifiers of
        // its immediate premises (object normalized to the row N3 surface).
        let source_quad_ids: Vec<String> = if axiom.is_edb {
            vec![self_reifier.clone()]
        } else {
            axiom
                .premises
                .iter()
                .map(|(s, p, o)| reifier_from_strings(s, p, &premise_object_n3(o)))
                .collect()
        };
        let source_refs: Vec<&str> = source_quad_ids.iter().map(String::as_str).collect();
        let derivation_id = mint_derivation_id(&rule_iri, &source_refs);
        present.insert((axiom.world.clone(), self_reifier));
        rows.push(Row {
            graph: axiom.world.clone(),
            subject: axiom.subject.clone(),
            predicate: axiom.predicate.clone(),
            obj: axiom.object.clone(),
            derivation_id,
            rule_iri,
            source_quad_ids,
        });
    }

    // 2. Close the row set under premises: any premise NOT already present as a row
    //    in its axiom's world is an omitted asserted (EDB) fact — materialize it as an
    //    asserted leaf so the derivation tree resolves. Collected first (immutable
    //    borrow of `rows`), then appended.
    let mut synthesized: Vec<Row> = Vec::new();
    for axiom in result.inferred() {
        if axiom.is_edb {
            continue;
        }
        for (s, p, o) in &axiom.premises {
            let obj = premise_object_n3(o).into_owned();
            let reifier = reifier_from_strings(s, p, &obj);
            let key = (axiom.world.clone(), reifier.clone());
            if !present.insert(key) {
                continue;
            }
            let source_quad_ids = vec![reifier];
            let source_refs: Vec<&str> = source_quad_ids.iter().map(String::as_str).collect();
            let derivation_id = mint_derivation_id(&assert_rule, &source_refs);
            synthesized.push(Row {
                graph: axiom.world.clone(),
                subject: s.clone(),
                predicate: p.clone(),
                obj,
                derivation_id,
                rule_iri: assert_rule.clone(),
                source_quad_ids,
            });
        }
    }
    rows.extend(synthesized);

    LazyExplanationIndex::new(&rows).explain_all()
}

/// Normalize a reasoning premise's object to the canonical N3 surface a [`Row`]'s
/// `obj` (and hence [`reifier_from_row`]) uses, so a premise reifier joins its
/// antecedent quad regardless of which production path recorded the premise.
///
/// A bare resource IRI (the finite DL post-pass surface) is wrapped in `<...>`; an
/// already-N3 IRI (`<...>`, the EL/chase surface), a literal (`"..."`), or a blank
/// node (`_:...`) is used verbatim.
fn premise_object_n3(object: &str) -> std::borrow::Cow<'_, str> {
    if object.starts_with('<') || object.starts_with('"') || object.starts_with("_:") {
        std::borrow::Cow::Borrowed(object)
    } else {
        std::borrow::Cow::Owned(format!("<{object}>"))
    }
}

/// Render a full Markdown explanation file for `expl`.
///
/// Produces the exact byte format used by the `conformance/logic/cases/*/expected/explanation/`
/// goldens: a cited-IRI HTML comment skeleton, a step-skeleton HTML comment, a prose header,
/// and an indented DFS-ordered proof tree. No trailing blank line — the file ends with the
/// final triple line's trailing `\n`.
pub fn render_markdown(expl: &Explanation) -> String {
    let mut out = String::new();

    // ── cited-iri-skeleton comment ────────────────────────────────────────────
    out.push_str("<!-- cited-iri-skeleton\n");
    for iri in &expl.cited_iris {
        out.push_str("  ");
        out.push_str(iri);
        out.push('\n');
    }
    out.push_str("-->\n");
    out.push('\n');

    // ── step-skeleton comment ─────────────────────────────────────────────────
    out.push_str("<!-- step-skeleton\n");
    for step in &expl.step_skeleton {
        out.push_str("  step derivation=");
        out.push_str(&step.derivation_id);
        out.push('\n');
        out.push_str("    rule=");
        out.push_str(&step.rule_iri);
        out.push('\n');
        for term in &step.term_iris {
            out.push_str("    term=");
            out.push_str(term);
            out.push('\n');
        }
    }
    out.push_str("-->\n");
    out.push('\n');

    // ── prose header ──────────────────────────────────────────────────────────
    out.push_str("# Explanation for `<");
    out.push_str(&expl.target_quad_reifier);
    out.push_str(">`\n");
    out.push('\n');
    out.push_str("**World:** `<");
    out.push_str(&expl.world_iri);
    out.push_str(">`\n");
    out.push_str("**Target derivation:** `<");
    out.push_str(&expl.target_derivation_id);
    out.push_str(">`\n");
    out.push('\n');

    // ── prose tree (DFS order) ────────────────────────────────────────────────
    for step in &expl.step_skeleton {
        let ind = "  ".repeat(step.depth as usize);
        let tind = "  ".repeat(step.depth as usize + 1);

        if step.is_asserted {
            out.push_str(&ind);
            out.push_str("**Asserted fact** (input \u{2014} `<");
            out.push_str(&step.quad_reifier);
            out.push_str(">`):\n");
        } else {
            out.push_str(&ind);
            out.push_str("**Derived** by rule `<");
            out.push_str(&step.rule_iri);
            out.push_str(">`:\n");
        }

        out.push_str(&tind);
        out.push('`');
        out.push('<');
        out.push_str(&step.subject_iri);
        out.push('>');
        out.push('`');
        out.push(' ');
        out.push('`');
        out.push('<');
        out.push_str(&step.predicate_iri);
        out.push('>');
        out.push('`');
        out.push(' ');
        out.push('`');
        out.push_str(&step.obj_n3);
        out.push('`');
        out.push_str(" *(in `<");
        out.push_str(&step.graph_iri);
        out.push_str(">`)*\n");
    }

    out
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
