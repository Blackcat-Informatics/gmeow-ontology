// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Static profile / decidability certifier — the Rust mirror of the Python
//! oracle in `src/gmeow_tools/logic_certify.py`.
//!
//! # Why a mirror exists
//!
//! Python remains the *conformance oracle* (slow, simple, correct); this crate
//! is the *engine* (fast path).  The `oracle ≡ engine` gate (Task 6)
//! diffs the Python `CertificationVerdict.to_json()` against the Rust
//! [`CertificationVerdict::to_json_pairs`] for the SAME input.  For that diff to hold,
//! every field name, every violation string, and the SCC-cycle rendering must be
//! **byte-identical** to Python.  This module copies the Python check logic and
//! the diagnostic-string format verbatim (the strings cite
//! `LOGIC-SEMANTICS.md`, so a failure self-documents on both sides).
//!
//! The verdict's `evolution_class` is a **Rust-native facet characterization that
//! goes BEYOND the (retiring) Python `_DECIDABILITY_CLASS` oracle**: it derives the
//! decidability of the contract's `logic:EvolutionMode` (static / state-transition /
//! transaction-path), an orthogonal facet the Python oracle never modelled.  It is
//! always present on the verdict (collapsing a missing facet to `StaticEvolution`),
//! so the Rust JSON carries five keys to the oracle's four.
//!
//! # The predicate-naming parity normalization (the crux)
//!
//! The legacy typed-IR certifier ran over a `LogicProgram` value (the `logic_ir`
//! typed IR was retired), where each atom carries a *full IRI* predicate
//! string and an `rdf:type`-folding rule (`_predicate_key`):
//!
//! * a non-`rdf:type` atom's key is its bare predicate IRI;
//! * an `rdf:type` atom whose **object** is a non-variable (a named class) folds
//!   the class into the key as `"{rdf:type-iri} {class-iri}"`, so class-level
//!   recursion (`?x a C :- ?y a C`) shows up as a self-cycle.
//!
//! The Rust certifier consumes typed evaluation rules lowered directly from the
//! canonical program:
//!
//! * writes predicates as bare angle-bracketed IRIs — `<http://…/p>(?S, ?O, ?W)`
//!   — so the predicate value is **exactly** the bare IRI the canonical
//!   IR stores. No prefix expansion, no re-bracketing.
//! * uses the **arity-3 ternary encoding** `pred(subject, object, world)`, so an
//!   atom's *object* is `terms()[1]`. This is the slot the `rdf:type` fold keys
//!   on — identical to the Python IR, where the class sits in `atom.obj`.
//! * does **not** special-case `rdf:type`: it emits the predicate IRI raw with
//!   the class in the object slot, exactly as the IR holds it. So replicating the
//!   Python `_predicate_key` fold over `terms()[1]` reproduces the Python key
//!   byte-for-byte.
//!
//! Therefore `eval_predicate_key` re-implements Python's `_predicate_key`:
//! `predicate IRI` from `Atom::predicate()`, and — when the predicate is
//! `rdf:type` and the object term is **not** a variable — the bare object IRI
//! in the canonical IR. The rendered cycle string `[P -> Q -> P]` is thus
//! identical on both sides.
//!
//! Negation parity: typed rules retain positive and negation-as-failure body atoms. We
//! label every `body_negative()` edge `"negative"`, matching the IR's
//! `LogicAxiom.negated` flag, so the StratifiedNAF SCC analysis sees the same
//! negative edges as Python.
//!
//! # Soundness and incompleteness
//!
//! Every check is a **sufficient** condition and is **necessarily incomplete**:
//! because termination is undecidable, a clean certification *proves* membership
//! in the declared decidable/terminating fragment, but a violation only proves
//! that the cheap structural sufficient condition does not hold — the program may
//! still terminate. This is the same accepted tradeoff the Python oracle and
//! `LOGIC-SEMANTICS.md §Decidability` state.
//!
//! # Division of labour with the budget governor (honesty invariant)
//!
//! Rejecting a genuinely **non-terminating** rule set up front is *this static
//! certifier's* job, not the runtime budget governor's. The governor in
//! `gmeow_logic.materialize` is **post-hoc for the count ceilings**: both the
//! native chase enforces deterministic derivation-step ceilings. Wall-clock budgets
//! are rejected at the public boundary rather than approximated. Asserted EDB input
//! is always kept in full.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

/// Wrap a certification condition message as a typed diagnostic on the shared
/// substrate, preserving the authored text verbatim.
fn certify_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Certify { detail })
}

// ── Constants (verbatim from logic_certify.py) ───────────────────────────────

/// The governing design document, cited in messages so failures self-document.
const DOC: &str = "LOGIC-SEMANTICS.md";
/// `§Semantic profiles` section heading (quoted verbatim, like Python).
const SEC_PROFILES: &str = "§Semantic profiles";
/// `§Decidability` section heading (quoted verbatim, like Python).
const SEC_DECIDABILITY: &str = "§Decidability";

/// The `rdf:type` predicate IRI string used by the canonical IR.
///
/// Mirrors Python `_RDF_TYPE = str(RDF.type)`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

// ── Verdict ──────────────────────────────────────────────────────────────────

/// The static-certification verdict for a program against a declared profile.
///
/// Mirrors Python `CertificationVerdict`. [`to_json_pairs`](Self::to_json_pairs)
/// yields fields for a key-for-key, value-for-value match of the Python
/// `to_json()` dict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationVerdict {
    /// The declared profile-id string, e.g. `"PositiveHornProfile"`.
    pub profile_id: String,
    /// The decidability class string, e.g. `"terminating/PTIME-data"`.
    pub decidability_class: String,
    /// The evolution-facet decidability class derived from the contract's
    /// `logic:EvolutionMode` (see [`evolution_decidability_class`]). ALWAYS
    /// present: a contract with no evolution facet selected collapses to
    /// `StaticEvolution` → `"static/single-state"` at this boundary, so the
    /// field is a required `String`, never an `Option`. Transaction Logic is the
    /// `transaction-path` value of this orthogonal facet, not a 7th profile.
    pub evolution_class: String,
    /// True iff no violations were found (StableModel advisories count).
    pub certified: bool,
    /// The deterministic, sorted list of diagnostic strings.
    pub violations: Vec<String>,
}

impl CertificationVerdict {
    /// Render the verdict as `(key, value)` pairs in sorted-key order:
    ///
    /// ```json
    /// {
    ///   "certified": <bool>,
    ///   "decidability_class": <string>,
    ///   "evolution_class": <string>,
    ///   "profile_id": <string>,
    ///   "violations": [<string>, …]   // sorted
    /// }
    /// ```
    ///
    /// Keys are sorted. `evolution_class` sorts AFTER `decidability_class` and
    /// BEFORE `profile_id` (the tuple order below reflects that). `violations` is
    /// sorted. `evolution_class` is a Rust-native facet characterization beyond
    /// the (retiring) Python oracle's four keys; the PyO3 surface in `py.rs` and
    /// the conformance serializer materialise this into the same five keys.
    pub fn to_json_pairs(&self) -> (bool, &str, &str, &str, Vec<String>) {
        let mut sorted = self.violations.clone();
        sorted.sort();
        (
            self.certified,
            self.decidability_class.as_str(),
            self.evolution_class.as_str(),
            self.profile_id.as_str(),
            sorted,
        )
    }
}

// ── Parsed-program view (head/body predicates + negation) ────────────────────

/// One logical S/O position of an atom: `(predicate_key, slot, term_string)`
/// where `slot` is `"S"` or `"O"`. `is_var` records whether the term is a
/// variable; `term` is the bare variable name (`?x`) when it is, else the
/// constant's display.
#[derive(Clone)]
struct PosTerm {
    key: String,
    slot: &'static str,
    is_var: bool,
    term: String,
}

/// A flattened, parser-derived view of one rule: head predicate keys, positive
/// body predicate keys, and negative (negation-as-failure) body predicate keys,
/// plus the S/O position lists the weak-acyclicity analysis consumes.
///
/// The keys are computed by [`eval_predicate_key`] so they match the Python IR's
/// `_predicate_key` byte-for-byte (see the module docs).
struct RuleView {
    head_keys: Vec<String>,
    positive_body_keys: Vec<String>,
    negative_body_keys: Vec<String>,
    /// Per-rule DL-safety witness: variables used anywhere, and the subset bound
    /// by a positive body atom. `head_key` is the head's predicate key (for the
    /// diagnostic). Mirrors Python `certify_dl_safe`.
    head_key_for_safety: String,
    used_vars: BTreeSet<String>,
    bound_vars: BTreeSet<String>,
    /// S/O positions of the head atom(s) (for weak acyclicity).
    head_positions: Vec<PosTerm>,
    /// S/O positions of the positive body atoms (frontier-edge sources).
    positive_positions: Vec<PosTerm>,
    /// S/O positions of ALL body atoms (positive ∪ negative) — the source set for
    /// special (existential) edges, mirroring Python `_positions(src_atom)` over
    /// `rule.body`.
    all_body_positions: Vec<PosTerm>,
}

// Typed canonical-IR rule views are the sole certification input.

fn eval_term_surface(term: &crate::rule_ir::EvalTerm) -> (bool, String) {
    match term {
        crate::rule_ir::EvalTerm::Var(name) => (true, name.clone()),
        crate::rule_ir::EvalTerm::ConstNamed(iri) => (false, iri.clone()),
        crate::rule_ir::EvalTerm::ConstLit(value) => {
            (false, crate::provenance::term_display(value))
        }
    }
}

fn eval_predicate_key(atom: &crate::rule_ir::EvalAtom) -> String {
    if atom.predicate == RDF_TYPE {
        let (is_var, object) = eval_term_surface(&atom.object);
        if !is_var {
            return format!("{} {object}", atom.predicate);
        }
    }
    atom.predicate.clone()
}

fn eval_atom_variables(atom: &crate::rule_ir::EvalAtom) -> BTreeSet<String> {
    [&atom.subject, &atom.object]
        .into_iter()
        .filter_map(|term| match term {
            crate::rule_ir::EvalTerm::Var(name) => Some(name.clone()),
            _ => None,
        })
        .collect()
}

fn eval_atom_positions(atom: &crate::rule_ir::EvalAtom) -> Vec<PosTerm> {
    let key = eval_predicate_key(atom);
    [(&atom.subject, "S"), (&atom.object, "O")]
        .into_iter()
        .map(|(term, slot)| {
            let (is_var, term) = eval_term_surface(term);
            PosTerm {
                key: key.clone(),
                slot,
                is_var,
                term,
            }
        })
        .collect()
}

fn eval_rule_views(rules: &[crate::rule_ir::EvalRule]) -> Vec<RuleView> {
    rules
        .iter()
        .map(|rule| {
            let head_keys = vec![eval_predicate_key(&rule.head)];
            let positive: Vec<&crate::rule_ir::EvalAtom> =
                rule.body.iter().filter(|atom| !atom.negated).collect();
            let negative: Vec<&crate::rule_ir::EvalAtom> =
                rule.body.iter().filter(|atom| atom.negated).collect();
            let positive_body_keys = positive
                .iter()
                .map(|atom| eval_predicate_key(atom))
                .collect();
            let negative_body_keys = negative
                .iter()
                .map(|atom| eval_predicate_key(atom))
                .collect();
            let bound_vars = positive
                .iter()
                .flat_map(|atom| eval_atom_variables(atom))
                .collect();
            let mut used_vars = eval_atom_variables(&rule.head);
            for atom in &rule.body {
                used_vars.extend(eval_atom_variables(atom));
            }
            let head_positions = eval_atom_positions(&rule.head);
            let positive_positions = positive
                .iter()
                .flat_map(|atom| eval_atom_positions(atom))
                .collect();
            let all_body_positions = rule.body.iter().flat_map(eval_atom_positions).collect();
            RuleView {
                head_key_for_safety: head_keys[0].clone(),
                head_keys,
                positive_body_keys,
                negative_body_keys,
                used_vars,
                bound_vars,
                head_positions,
                positive_positions,
                all_body_positions,
            }
        })
        .collect()
}

// ── Predicate dependency graph// ── Predicate dependency graph + Tarjan SCC ──────────────────────────────────

/// The predicate dependency graph: nodes are predicate keys, edges are
/// `head ← body` with a `negative` flag for negation-as-failure body literals.
///
/// Mirrors Python `PredicateDepGraph`.
struct DepGraph {
    nodes: BTreeSet<String>,
    /// `(head, body, negative)` triples; `negative == true` ⇔ NAF body literal.
    edges: BTreeSet<(String, String, bool)>,
}

impl DepGraph {
    /// Build the graph from parsed rule views (one `head ← body` edge per
    /// (rule, body-atom) pair). Mirrors `PredicateDepGraph.from_program`.
    fn from_views(views: &[RuleView]) -> DepGraph {
        let mut nodes: BTreeSet<String> = BTreeSet::new();
        let mut edges: BTreeSet<(String, String, bool)> = BTreeSet::new();
        for v in views {
            for head in &v.head_keys {
                nodes.insert(head.clone());
                for body in &v.positive_body_keys {
                    nodes.insert(body.clone());
                    edges.insert((head.clone(), body.clone(), false));
                }
                for body in &v.negative_body_keys {
                    nodes.insert(body.clone());
                    edges.insert((head.clone(), body.clone(), true));
                }
            }
        }
        DepGraph { nodes, edges }
    }

    /// Sorted adjacency `head -> [body, …]` (edge direction head ← body), so the
    /// Tarjan numbering is deterministic. Mirrors `PredicateDepGraph.successors`.
    fn successors(&self) -> BTreeMap<String, Vec<String>> {
        let mut adj: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for n in &self.nodes {
            adj.entry(n.clone()).or_default();
        }
        for (head, body, _neg) in &self.edges {
            adj.entry(head.clone()).or_default().push(body.clone());
            adj.entry(body.clone()).or_default();
        }
        for v in adj.values_mut() {
            v.sort();
            v.dedup();
        }
        adj
    }

    /// The strongly-connected components, via [`tarjan_scc`].
    fn sccs(&self) -> Vec<BTreeSet<String>> {
        tarjan_scc(&self.successors())
    }

    /// The `(head, body)` pairs of all negative (NAF) edges.
    fn negative_edges(&self) -> BTreeSet<(String, String)> {
        self.edges
            .iter()
            .filter(|(_, _, neg)| *neg)
            .map(|(h, b, _)| (h.clone(), b.clone()))
            .collect()
    }
}

/// Tarjan's strongly-connected-components algorithm (hand-rolled, no deps — no
/// petgraph). Iterative DFS for deep graphs; node iteration is sorted so the
/// result is fully deterministic. Mirrors Python `tarjan_scc`.
pub fn tarjan_scc(graph: &BTreeMap<String, Vec<String>>) -> Vec<BTreeSet<String>> {
    use crate::dense::DenseInterner;

    // Lowered to dense `u32` ids: all per-node Tarjan state lives in dense
    // arrays keyed by id, and successor lists are precomputed `Vec<u32>`.
    //
    // DETERMINISM CONTRACT: the result order is driven by the DFS-root iteration
    // order, which MUST stay sorted-IRI order (the old `graph.keys()` order). A
    // first-seen interner would assign ids in discovery order, so we pre-intern
    // every vertex in sorted-key order up front; that fixes id == sorted-key rank
    // for declared vertices and makes root iteration over `0..n_keys` equivalent
    // to iterating `graph.keys()`. Successor IRIs that are not graph keys (sinks)
    // are interned lazily and never enter the root loop. Successor ORDER is
    // preserved exactly (no sort): the `Vec<String>` order maps straight through.
    let mut interner = DenseInterner::new();
    // (1) Intern declared vertices in sorted-key order → ids 0..n_keys match the
    //     old sorted root-iteration order exactly.
    let n_keys = graph.len();
    for key in graph.keys() {
        interner.intern(key);
    }
    // (2) Build dense successor lists, interning any sink IRIs (ids ≥ n_keys).
    //     `succ[id]` is `None` for sink-only ids (no outgoing edges declared).
    let mut succ: Vec<Vec<u32>> = vec![Vec::new(); n_keys];
    for (key, outs) in graph {
        let id = interner.get(key).expect("declared key interned") as usize;
        succ[id] = outs.iter().map(|o| interner.intern(o)).collect();
    }
    let n = interner.len();
    // Sink ids (≥ n_keys) have no successor list; treat as empty.
    let successors_of = |id: usize| -> &[u32] { if id < n_keys { &succ[id] } else { &[] } };

    const UNVISITED: usize = usize::MAX;
    let mut index_counter: usize = 0;
    let mut indices: Vec<usize> = vec![UNVISITED; n];
    let mut low: Vec<usize> = vec![0; n];
    let mut on_stack: Vec<bool> = vec![false; n];
    let mut stack: Vec<u32> = Vec::new();
    let mut result: Vec<BTreeSet<String>> = Vec::new();

    // Roots iterated in sorted-IRI order (== id order for declared keys 0..n_keys).
    for start in 0..n_keys {
        if indices[start] != UNVISITED {
            continue;
        }
        // Iterative DFS: each frame is (node id, next-child-index).
        let mut work: Vec<(u32, usize)> = vec![(start as u32, 0)];
        while let Some(&(node, child_idx)) = work.last() {
            let node = node as usize;
            if child_idx == 0 {
                indices[node] = index_counter;
                low[node] = index_counter;
                index_counter += 1;
                stack.push(node as u32);
                on_stack[node] = true;
            }
            let outs = successors_of(node);
            let mut recursed = false;
            let mut i = child_idx;
            while i < outs.len() {
                let s = outs[i] as usize;
                if indices[s] == UNVISITED {
                    // Advance this frame past `s`, then descend into it.
                    let len = work.len();
                    work[len - 1] = (node as u32, i + 1);
                    work.push((s as u32, 0));
                    recursed = true;
                    break;
                }
                if on_stack[s] {
                    low[node] = low[node].min(indices[s]);
                }
                i += 1;
            }
            if recursed {
                continue;
            }
            if low[node] == indices[node] {
                let mut component: BTreeSet<String> = BTreeSet::new();
                loop {
                    let w = stack.pop().expect("stack non-empty while closing SCC") as usize;
                    on_stack[w] = false;
                    let done = w == node;
                    component.insert(interner.resolve(w as u32).to_owned());
                    if done {
                        break;
                    }
                }
                result.push(component);
            }
            work.pop();
            if let Some(&(parent, _)) = work.last() {
                let node_low = low[node];
                low[parent as usize] = low[parent as usize].min(node_low);
            }
        }
    }
    result
}

/// The offending predicate cycle that closes through a negated dependency.
///
/// The adjacency runs `head → body`. A negated dependency is the edge `dst ← src`
/// (rule head `dst` with a negation-as-failure body atom on `src`); inside an SCC
/// `src` reaches `dst` again, so the rendered cycle closes back to the head:
/// `(dst, src, …, dst)` — i.e. `head → body → … → head`. A self-loop renders as
/// `(dst, dst)` (e.g. `[p -> p]`). The result is always non-empty and always
/// starts and ends with `dst`, so `render_cycle` can never produce a blank cycle.
/// BFS over sorted successors. Mirrors Python `_shortest_cycle`.
fn shortest_cycle(
    adj: &BTreeMap<String, Vec<String>>,
    src: &str,
    dst: &str,
) -> Option<Vec<String>> {
    if src == dst {
        return Some(vec![dst.to_owned(), dst.to_owned()]);
    }
    let mut queue: std::collections::VecDeque<String> = std::collections::VecDeque::new();
    queue.push_back(src.to_owned());
    let mut prev: HashMap<String, String> = HashMap::new();
    prev.insert(src.to_owned(), src.to_owned());
    while let Some(node) = queue.pop_front() {
        if let Some(succs) = adj.get(&node) {
            for succ in succs {
                if !prev.contains_key(succ) {
                    prev.insert(succ.clone(), node.clone());
                    if succ == dst {
                        // Reconstruct the return path src → … → dst.
                        let mut path = vec![dst.to_owned()];
                        let mut cur = dst.to_owned();
                        while cur != src {
                            cur = prev[&cur].clone();
                            path.push(cur.clone());
                        }
                        path.reverse(); // now src → … → dst
                        // Close the cycle through the negated head
                        // edge dst ← src: head(dst) → body(src) → … → dst.
                        // Python: (dst, *path).
                        let mut cycle: Vec<String> = Vec::with_capacity(path.len() + 1);
                        cycle.push(dst.to_owned());
                        cycle.extend(path.iter().cloned());
                        return Some(cycle);
                    }
                    queue.push_back(succ.clone());
                }
            }
        }
    }
    None
}

/// Render a predicate cycle as `[P -> Q -> P]`. Mirrors Python `_render_cycle`.
fn render_cycle(cycle: &[String]) -> String {
    format!("[{}]", cycle.join(" -> "))
}

/// Whether the dependency graph is stratifiable, plus the offending cycle if it
/// is not. Mirrors Python `stratify` (only the parts the checks consume).
fn offending_cycle(graph: &DepGraph) -> Option<Vec<String>> {
    let sccs = graph.sccs();
    let mut node_to_scc: HashMap<String, usize> = HashMap::new();
    for (idx, comp) in sccs.iter().enumerate() {
        for node in comp {
            node_to_scc.insert(node.clone(), idx);
        }
    }
    let adj = graph.successors();
    // Iterate negative edges in sorted order (Python sorts `neg`), so the first
    // offending cycle found is deterministic and matches Python.
    for (head, body) in graph.negative_edges() {
        if node_to_scc.get(&head) == node_to_scc.get(&body)
            && let Some(cycle) = shortest_cycle(&adj, &body, &head)
        {
            return Some(cycle);
        }
    }
    None
}

/// True iff the program is stratifiable (no negative edge inside an SCC).
fn is_stratified(graph: &DepGraph) -> bool {
    offending_cycle(graph).is_none()
}

/// Whether the typed program is stratifiable (no negation-as-failure cycle).
///
/// Thin wrapper over the existing dependency-graph analysis, exposed so the later
/// `py.rs` materialize routing phase can dispatch stratifiable
/// programs to the stratified chase and non-stratifiable ones to the native
/// well-founded / stable-model evaluators.  Additive; reuses
/// [`eval_rule_views`], [`DepGraph`], and [`is_stratified`] verbatim.
///
/// # Errors
///
/// The input is already typed and therefore requires no text-rule parse step.
// Phase-A scaffolding for the `py.rs` materialize router that dispatches on
// stratifiability (stratified chase vs well-founded / stable-model evaluators) is
// Phase B; this helper is landed now so the routing change is additive.
#[allow(dead_code)]
pub(crate) fn is_stratifiable(rules: &[crate::rule_ir::EvalRule]) -> bool {
    let views = eval_rule_views(rules);
    let graph = DepGraph::from_views(&views);
    is_stratified(&graph)
}

/// The stratum index of every predicate in `rules`, keyed by its bare predicate
/// IRI — the stratification the certifier already computes, projected into the
/// `(rule, predicate, stratum)` coordinate the deterministic cost vector
/// ([`crate::cost::CostVector`]) buckets committed derivations by.
///
/// Reuses the existing SCC/stratification machinery verbatim: [`eval_rule_views`]
/// → [`DepGraph`] → [`DepGraph::sccs`] (the same Tarjan condensation the
/// stratifiability check runs). A predicate's stratum is the length of the longest
/// chain of cross-SCC dependency edges (head → body) from it down to a predicate it
/// no longer depends on — so a base/EDB predicate (a body that heads nothing) is
/// stratum `0`, a predicate deriving only from EDB is stratum `1`, and so on.
/// Mutually-recursive predicates (one SCC) share a stratum. The condensation is a
/// DAG, so the longest-chain depth is well-defined and terminating even for a
/// non-stratifiable program (a negative cycle collapses into one SCC).
///
/// **rdf:type fold collapse.** [`eval_predicate_key`] folds a ground `rdf:type` object
/// into the SCC node key (`"{rdf:type} {class}"`) for finer recursion analysis; the
/// cost vector keys on the BARE predicate of a materialized row, so several
/// class-folded keys collapse onto the bare `rdf:type` predicate here, taking the
/// MAX (deepest) stratum among them. For every non-`rdf:type` predicate the key IS
/// the bare IRI, so the projection is exact.
///
/// Deterministic: `eval_rule_views`, `DepGraph` (`BTreeSet`/`BTreeMap`), and
/// `sccs()` are all order-stable, and the result is a sorted [`BTreeMap`].
///
/// # Errors
///
/// The input is already typed and therefore requires no text-rule parse step.
pub(crate) fn predicate_strata(rules: &[crate::rule_ir::EvalRule]) -> BTreeMap<String, u32> {
    let views = eval_rule_views(rules);
    let graph = DepGraph::from_views(&views);
    let sccs = graph.sccs();

    // Every graph node's SCC index (each node lands in exactly one component).
    let mut scc_of: HashMap<String, usize> = HashMap::new();
    for (idx, comp) in sccs.iter().enumerate() {
        for node in comp {
            scc_of.insert(node.clone(), idx);
        }
    }

    // Dependency adjacency `head -> [body, …]` (a predicate depends on its bodies).
    let adj = graph.successors();

    // Longest cross-SCC dependency chain per SCC, memoized over the condensation DAG.
    let mut depth_cache: HashMap<usize, u32> = HashMap::new();
    for idx in 0..sccs.len() {
        scc_depth(idx, &sccs, &scc_of, &adj, &mut depth_cache);
    }

    // Project each node's SCC depth onto its bare predicate IRI, keeping the max.
    let mut out: BTreeMap<String, u32> = BTreeMap::new();
    for node in &graph.nodes {
        let depth = depth_cache[&scc_of[node]];
        // The `rdf:type` fold is `"{predicate} {class}"`; the bare predicate is the
        // token before the first space (IRIs never contain a space).
        let bare = node.split(' ').next().unwrap_or(node).to_owned();
        let slot = out.entry(bare).or_insert(0);
        *slot = (*slot).max(depth);
    }
    out
}

/// The longest chain of cross-SCC dependency edges from `scc_idx` down to a sink SCC
/// (memoized). The condensation is acyclic, so this recursion always terminates.
fn scc_depth(
    scc_idx: usize,
    sccs: &[BTreeSet<String>],
    scc_of: &HashMap<String, usize>,
    adj: &BTreeMap<String, Vec<String>>,
    cache: &mut HashMap<usize, u32>,
) -> u32 {
    if let Some(&d) = cache.get(&scc_idx) {
        return d;
    }
    // Seed the cache before recursing so a (structurally impossible) cross-SCC cycle
    // could not spin forever; the final write overwrites this seed.
    cache.insert(scc_idx, 0);
    let mut depth = 0u32;
    for node in &sccs[scc_idx] {
        if let Some(bodies) = adj.get(node) {
            for body in bodies {
                let body_scc = scc_of[body];
                if body_scc != scc_idx {
                    let below = scc_depth(body_scc, sccs, scc_of, adj, cache);
                    depth = depth.max(below + 1);
                }
            }
        }
    }
    cache.insert(scc_idx, depth);
    depth
}

// ── Profile-family checks (each → Vec<String>) ───────────────────────────────

/// PositiveHorn forbids negation-as-failure. Mirrors `certify_positive_horn`.
fn certify_positive_horn(views: &[RuleView]) -> Vec<String> {
    let mut problems: Vec<String> = Vec::new();
    for v in views {
        for head in &v.head_keys {
            for neg in &v.negative_body_keys {
                problems.push(format!(
                    "PositiveHornProfile violation: rule head {head} has a negated body atom \
                     {neg} — PositiveHorn admits monotonic Horn rules only, no \
                     negation-as-failure ({DOC} {SEC_PROFILES})"
                ));
            }
        }
    }
    problems
}

/// StratifiedNAF: certified iff no predicate SCC crosses a negative edge.
/// Mirrors `certify_stratified_naf`.
fn certify_stratified_naf(graph: &DepGraph) -> Vec<String> {
    match offending_cycle(graph) {
        None => vec![],
        Some(cycle) => vec![format!(
            "StratifiedNAFProfile violation: predicate cycle {} crosses a negated body atom \
             — not stratifiable ({DOC} {SEC_PROFILES})",
            render_cycle(&cycle)
        )],
    }
}

/// WellFounded requires a normal program; vacuous for today's single-head,
/// function-free IR. Mirrors `certify_well_founded`.
fn certify_well_founded(_views: &[RuleView]) -> Vec<String> {
    // No non-normal rule is expressible in the projected fragment (single-atom
    // heads, function-free); per-rule normality checks land here once the IR can
    // express disjunctive heads / function terms.
    vec![]
}

/// StableModel is NP-hard in general — advisory unless also stratified.
/// Mirrors `certify_stable_model`.
fn certify_stable_model(graph: &DepGraph) -> Vec<String> {
    if is_stratified(graph) {
        return vec![];
    }
    vec![format!(
        "StableModelProfile is NP-hard in general; this rule set is not constrained to a \
         tractable subfragment — entailment under budget may return unknown \
         ({DOC} {SEC_DECIDABILITY})"
    )]
}

// ── Decidable-fragment checks (sufficient conditions only) ────────────────────

/// DL-safety: every rule variable must be bound by a positive body atom.
/// Mirrors `certify_dl_safe`.
fn certify_dl_safe(views: &[RuleView]) -> Vec<String> {
    let mut problems: Vec<String> = Vec::new();
    for v in views {
        // unsafe = sorted(used - bound). BTreeSet difference is already sorted.
        for var in v.used_vars.difference(&v.bound_vars) {
            problems.push(format!(
                "DL-safety violation: variable {var} in rule {} is not bound by any positive \
                 body atom — unsafe rule, not DL-safe ({DOC} {SEC_DECIDABILITY})",
                v.head_key_for_safety
            ));
        }
    }
    problems
}

/// Weak acyclicity: no position-graph cycle crosses an existential edge.
///
/// Faithful mirror of Python `certify_weak_acyclicity` (NOT a stub): it builds
/// the full position dependency graph and fires the special-edge branch whenever
/// a head variable is *not* bound by a positive body atom (a value-inventing
/// "existential" head variable). For compiler-lowered rules every head
/// variable is a frontier variable (bound positively, carrying the world var),
/// so the graph has no special edges and this is vacuously satisfied — but a
/// pathological input (e.g. a head var bound only under negation) reproduces
/// Python's special-edge diagnostics byte-for-byte.
///
/// Positions are `(predicate_key, slot)` with slot ∈ {`"S"`,`"O"`} — the logical
/// subject/object slots (the world slot and the constant predicate are excluded,
/// see `logical terms`); this matches Python, where the predicate term is never
/// a variable so no `"P"` position is ever emitted.
fn certify_weak_acyclicity(views: &[RuleView]) -> Vec<String> {
    type Pos = (String, &'static str);
    let mut normal_edges: BTreeSet<(Pos, Pos)> = BTreeSet::new();
    let mut special_edges: BTreeSet<(Pos, Pos)> = BTreeSet::new();

    for v in views {
        // Positive-body variable occurrences and the set of positive-body vars.
        let mut body_var_positions: BTreeMap<String, Vec<Pos>> = BTreeMap::new();
        let mut body_vars: BTreeSet<String> = BTreeSet::new();
        for p in &v.positive_positions {
            if p.is_var {
                body_var_positions
                    .entry(p.term.clone())
                    .or_default()
                    .push((p.key.clone(), p.slot));
                body_vars.insert(p.term.clone());
            }
        }
        // Every body position (positive AND negative) — the special-edge source
        // set when a head variable is value-inventing (mirrors Python `_positions`
        // over `rule.body`).
        let all_body_positions: Vec<Pos> = v
            .all_body_positions
            .iter()
            .filter(|p| p.is_var)
            .map(|p| (p.key.clone(), p.slot))
            .collect();

        for head in &v.head_positions {
            if !head.is_var {
                continue;
            }
            let head_pos: Pos = (head.key.clone(), head.slot);
            if body_vars.contains(&head.term) {
                if let Some(srcs) = body_var_positions.get(&head.term) {
                    for src in srcs {
                        normal_edges.insert((src.clone(), head_pos.clone()));
                    }
                }
            } else {
                for src in &all_body_positions {
                    special_edges.insert((src.clone(), head_pos.clone()));
                }
            }
        }
    }

    // Adjacency over all edges; a special edge is dangerous iff dst can reach src.
    let mut adj: BTreeMap<Pos, Vec<Pos>> = BTreeMap::new();
    for (src, dst) in normal_edges.iter().chain(special_edges.iter()) {
        adj.entry(src.clone()).or_default().push(dst.clone());
        adj.entry(dst.clone()).or_default();
    }
    for v in adj.values_mut() {
        v.sort();
        v.dedup();
    }

    fn reaches(
        adj: &BTreeMap<(String, &'static str), Vec<(String, &'static str)>>,
        start: &(String, &'static str),
        target: &(String, &'static str),
    ) -> bool {
        let mut seen: HashSet<(String, &'static str)> = HashSet::new();
        let mut queue: std::collections::VecDeque<(String, &'static str)> =
            std::collections::VecDeque::new();
        queue.push_back(start.clone());
        while let Some(node) = queue.pop_front() {
            if let Some(succs) = adj.get(&node) {
                for succ in succs {
                    if succ == target {
                        return true;
                    }
                    if seen.insert(succ.clone()) {
                        queue.push_back(succ.clone());
                    }
                }
            }
        }
        false
    }

    let mut problems: Vec<String> = Vec::new();
    for (src, dst) in &special_edges {
        if reaches(&adj, dst, src) {
            problems.push(format!(
                "Weak-acyclicity violation: position {}[{}] -> {}[{}] is an existential \
                 (special) edge inside a cycle — the chase may not terminate \
                 ({DOC} {SEC_DECIDABILITY})",
                src.0, src.1, dst.0, dst.1
            ));
        }
    }
    problems
}

/// Joint acyclicity: vacuous for the existential-free fragment.
/// Mirrors `certify_joint_acyclicity`.
fn certify_joint_acyclicity(_views: &[RuleView]) -> Vec<String> {
    vec![]
}

/// Guardedness: vacuous for the function-/existential-free fragment.
/// Mirrors `certify_guarded`.
fn certify_guarded(_views: &[RuleView]) -> Vec<String> {
    vec![]
}

/// Stickiness: vacuous for the existential-free fragment.
/// Mirrors `certify_sticky`.
fn certify_sticky(_views: &[RuleView]) -> Vec<String> {
    vec![]
}

// ── Dispatch ─────────────────────────────────────────────────────────────────

/// The decidability-class string emitted per declared profile.
/// Mirrors Python `_DECIDABILITY_CLASS`.
///
/// NOTE — knowingly-untouched asymmetry: this profile dispatch still has a soft
/// `_ => "unknown"` fallback arm, whereas [`evolution_decidability_class`] (the
/// orthogonal Evolution-facet characterization) hard-fails on an unrecognized
/// value. That asymmetry is out of the Evolution-facet work's scope and is left
/// deliberately untouched here so it can be flagged rather than silently changed.
pub(crate) fn decidability_class(profile: &str) -> &'static str {
    match profile {
        "PositiveHornProfile" => "terminating/PTIME-data",
        "StratifiedNAFProfile" => "terminating/PTIME-data",
        "WellFoundedProfile" => "three-valued/PTIME",
        "StableModelProfile" => "NP-hard",
        "ProceduralPrologProfile" => "operational/Turing-complete",
        "ProbabilisticProfile" => "probabilistic/#P-hard",
        _ => "unknown",
    }
}

/// The evolution-facet decidability class for a `logic:EvolutionMode` reference.
///
/// Transaction Logic is the `transaction-path` value of the orthogonal Evolution
/// facet, NOT a 7th profile. The reference is first normalized to its bare local
/// name via [`crate::profile_gate::evolution_mode_local`] (full IRI,
/// `logic:Local`, or bare local name all accepted), then mapped to the locked
/// decidability-class strings grounded in `LOGIC-SEMANTICS.md
/// §Turing-Completeness, Decidability, and Termination` and `LOGIC-TRANSACTION.md`
/// (conflict-serializability is classified — not searched — in polynomial time,
/// bounded by the same step governor the sequential core uses).
///
/// Unlike the profile [`decidability_class`], there is NO soft `"unknown"` arm: a
/// non-empty value that denotes none of the three modes is a HARD FAIL.
///
/// # Errors
///
/// Returns `Err` naming the offending value when `evolution` denotes none of the
/// three `logic:EvolutionMode` values.
pub fn evolution_decidability_class(evolution: &str) -> gmeow_errors::Result<&'static str> {
    match crate::profile_gate::evolution_mode_local(evolution) {
        Some("StaticEvolution") => Ok("static/single-state"),
        Some("StateTransitionEvolution") => Ok("state-transition/PTIME-per-step"),
        Some("TransactionPathEvolution") => Ok(
            "transaction-path/PTIME-classification (conflict-serializability; \
             classification not search; step-governor bounded)",
        ),
        // `evolution_mode_local` only ever returns those three (or None); the
        // catch-all keeps the match total without inventing a fourth class.
        Some(other) => Err(certify_err(format!(
            "unrecognized logic:EvolutionMode {other:?} — expected one of \
             StaticEvolution / StateTransitionEvolution / TransactionPathEvolution \
             ({DOC} {SEC_DECIDABILITY})"
        ))),
        None => Err(certify_err(format!(
            "unrecognized logic:EvolutionMode {evolution:?} — expected one of \
             StaticEvolution / StateTransitionEvolution / TransactionPathEvolution \
             ({DOC} {SEC_DECIDABILITY})"
        ))),
    }
}

/// Statically certify typed rule views against the declared profile.
///
/// **Sufficient-condition / necessarily-incomplete.** Because termination is
/// undecidable (Church/Turing), a clean verdict proves membership in the declared
/// decidable/terminating fragment, but a violation only proves that the cheap
/// structural sufficient condition does not hold — the program may still
/// terminate (`LOGIC-SEMANTICS.md §Decidability`). This is the Rust mirror of
/// Python `certify_program`; the verdict, the violation strings, and the SCC
/// cycle rendering are byte-identical (see module docs for the predicate-naming
/// normalization that makes this hold).
///
/// `profile` matches the Python profile-id strings: `"PositiveHornProfile"`,
/// `"StratifiedNAFProfile"`, `"WellFoundedProfile"`, `"StableModelProfile"`,
/// `"ProceduralPrologProfile"`, `"ProbabilisticProfile"`.
///
/// `evolution` is the contract's `logic:EvolutionMode` reference (full IRI,
/// `logic:Local`, or bare local name). `None` means no evolution facet was
/// selected and collapses to `StaticEvolution` at this boundary — the resulting
/// [`CertificationVerdict::evolution_class`] is ALWAYS a required `String`, never
/// an `Option`. An unrecognized non-empty value is a HARD FAIL (see
/// [`evolution_decidability_class`]).
///
/// # Errors
///
/// Returns an error if `evolution` denotes an unrecognized mode.
fn certify_views(
    views: &[RuleView],
    profile: &str,
    evolution: Option<&str>,
) -> gmeow_errors::Result<CertificationVerdict> {
    let evolution_class = evolution_decidability_class(evolution.unwrap_or("StaticEvolution"))?;
    let graph = DepGraph::from_views(views);

    let mut violations: Vec<String> = Vec::new();
    match profile {
        "PositiveHornProfile" => {
            violations.extend(certify_positive_horn(views));
            violations.extend(certify_dl_safe(views));
            violations.extend(certify_weak_acyclicity(views));
            violations.extend(certify_joint_acyclicity(views));
            violations.extend(certify_guarded(views));
            violations.extend(certify_sticky(views));
        }
        "StratifiedNAFProfile" => {
            violations.extend(certify_stratified_naf(&graph));
            violations.extend(certify_dl_safe(views));
            violations.extend(certify_weak_acyclicity(views));
            violations.extend(certify_joint_acyclicity(views));
            violations.extend(certify_guarded(views));
            violations.extend(certify_sticky(views));
        }
        "WellFoundedProfile" => {
            violations.extend(certify_well_founded(views));
            violations.extend(certify_dl_safe(views));
            violations.extend(certify_weak_acyclicity(views));
        }
        "StableModelProfile" => {
            violations.extend(certify_stable_model(&graph));
            violations.extend(certify_dl_safe(views));
            violations.extend(certify_weak_acyclicity(views));
        }
        other => {
            // ProceduralProlog / Probabilistic / unknown: no static guarantee.
            violations.push(format!(
                "{other} carries no static decidability certification — it is operational / \
                 probabilistic, outside the certifiable sufficient-condition fragments \
                 ({DOC} {SEC_DECIDABILITY})"
            ));
        }
    }

    // Deterministic: sort (Python `tuple(sorted(violations))`).
    violations.sort();
    let certified = violations.is_empty();
    Ok(CertificationVerdict {
        profile_id: profile.to_owned(),
        decidability_class: decidability_class(profile).to_owned(),
        evolution_class: evolution_class.to_owned(),
        certified,
        violations,
    })
}

/// Certify a program straight from the **canonical source AST**.
///
/// This is the canonical-AST front door to the certifier: the program is lowered
/// directly to typed evaluation rules and certified — no hand-authored rule text in the loop, so the
/// canonical IR is the single source feeding decidability certification too.
pub fn certify_program(
    program: &gmeow_logic_compile::ir::LogicProgram,
    profile: &str,
) -> gmeow_errors::Result<CertificationVerdict> {
    let rules = crate::lower::lower_eval_rules(program)?;
    let views = eval_rule_views(&rules);
    let evolution = program_evolution(program)?;
    certify_views(&views, profile, evolution.as_deref())
}

/// The single governing `logic:EvolutionMode` of a program's reasoning contracts,
/// or `None` when no contract selects one (⇒ `StaticEvolution` at the certify
/// boundary).
///
/// Zero or one contract carries an evolution facet in practice; if two contracts
/// disagree (distinct non-`None` evolution values) that is genuine ambiguity and a
/// HARD FAIL rather than silently picking one. Contracts that agree on the same
/// value collapse to that one value.
///
/// # Errors
///
/// Returns `Err` when two contracts select different `logic:EvolutionMode` values.
pub fn program_evolution(
    program: &gmeow_logic_compile::ir::LogicProgram,
) -> gmeow_errors::Result<Option<String>> {
    let mut chosen: Option<&str> = None;
    for contract in &program.contracts {
        if let Some(ev) = contract.evolution.as_deref() {
            match chosen {
                None => chosen = Some(ev),
                Some(prev) if prev == ev => {}
                Some(prev) => {
                    return Err(certify_err(format!(
                        "conflicting logic:EvolutionMode across reasoning contracts: \
                         {prev:?} vs {ev:?} — the governing evolution facet is ambiguous \
                         ({DOC} {SEC_DECIDABILITY})"
                    )));
                }
            }
        }
    }
    Ok(chosen.map(str::to_owned))
}

// ── Unit tests ────────────────────────────────────────────────────────────────
