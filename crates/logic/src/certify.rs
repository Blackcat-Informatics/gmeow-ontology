// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! Static profile / decidability certifier — the Rust mirror of the Python
//! oracle in `src/gmeow_tools/logic_certify.py`.
//!
//! # Why a mirror exists
//!
//! Python remains the *conformance oracle* (slow, simple, correct); this crate
//! is the *engine* (fast path).  The `oracle ≡ engine` gate (issue #502, Task 6)
//! diffs the Python [`CertificationVerdict.to_json()`] against the Rust
//! [`CertificationVerdict::to_json`] for the SAME input.  For that diff to hold,
//! every field name, every violation string, and the SCC-cycle rendering must be
//! **byte-identical** to Python.  This module copies the Python check logic and
//! the diagnostic-string format verbatim (the strings cite
//! `LOGIC-SEMANTICS.md`, so a failure self-documents on both sides).
//!
//! # The predicate-naming parity normalization (the crux)
//!
//! The Python certifier runs over the typed IR
//! (`gmeow_tools.logic_ir.LogicProgram`), where each atom carries a *full IRI*
//! predicate string and an `rdf:type`-folding rule (`_predicate_key`):
//!
//! * a non-`rdf:type` atom's key is its bare predicate IRI;
//! * an `rdf:type` atom whose **object** is a non-variable (a named class) folds
//!   the class into the key as `"{rdf:type-iri} {class-iri}"`, so class-level
//!   recursion (`?x a C :- ?y a C`) shows up as a self-cycle.
//!
//! The Rust certifier instead consumes the **`.rls` text** that
//! `gmeow_tools.logic_projections.project_nemo` emits.  `project_nemo`:
//!
//! * writes predicates as bare angle-bracketed IRIs — `<http://…/p>(?S, ?O, ?W)`
//!   — and Nemo's parser resolves an `<iri>` tag to its bare content
//!   ([`nemo`'s `resolve_tag`] returns `iri.content()`), so
//!   `Atom::predicate().to_string()` yields **exactly** the bare IRI the Python
//!   IR stores. No prefix expansion, no re-bracketing.
//! * uses the **arity-3 ternary encoding** `pred(subject, object, world)`, so an
//!   atom's *object* is `terms()[1]`. This is the slot the `rdf:type` fold keys
//!   on — identical to the Python IR, where the class sits in `atom.obj`.
//! * does **not** special-case `rdf:type`: it emits the predicate IRI raw with
//!   the class in the object slot, exactly as the IR holds it. So replicating the
//!   Python `_predicate_key` fold over `terms()[1]` reproduces the Python key
//!   byte-for-byte.
//!
//! Therefore [`predicate_key`] re-implements Python's `_predicate_key`:
//! `predicate IRI` from `Atom::predicate()`, and — when the predicate is
//! `rdf:type` and the object term is **not** a variable — the bare object IRI
//! (Nemo displays a ground IRI as `<iri>`, which we de-bracket to the bare form
//! the Python IR carries). The rendered cycle string `[P -> Q -> P]` is thus
//! identical on both sides.
//!
//! Negation parity: Nemo's parser splits a rule body into
//! [`nemo::rule_model::components::rule::Rule::body_positive`] and
//! `body_negative`; the latter are the `~atom` negation-as-failure literals. We
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
//! # Division of labour with the budget governor (honesty invariant, #502)
//!
//! Rejecting a genuinely **non-terminating** rule set up front is *this static
//! certifier's* job, not the runtime budget governor's. The governor in
//! [`crate::py::materialize`] is **post-hoc**: Nemo's `reason()` runs to fixpoint
//! with no native budget hook, so the governor bounds answer/firing counts *after*
//! the chase reaches fixpoint and `time_ms` bounds only post-fixpoint work — it
//! cannot interrupt the chase mid-flight. This differs from the Python oracle,
//! which cuts mid-chase. The divergence is **named, not glossed**: on terminating
//! fixtures the verdict and budget strings match the oracle exactly; the
//! behavioural difference on non-terminating inputs is documented here, in
//! `py.rs`, and in `crates/logic/README.md`. Keeping `oracle ≡ engine` truthful
//! is the reason the certifier exists as the front-line termination guard.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::nemo_engine::NemoParsedRules;
use nemo::rule_model::components::atom::Atom;
use nemo::rule_model::programs::ProgramRead;

// ── Constants (verbatim from logic_certify.py) ───────────────────────────────

/// The governing design document, cited in messages so failures self-document.
const DOC: &str = "LOGIC-SEMANTICS.md";
/// `§Semantic profiles` section heading (quoted verbatim, like Python).
const SEC_PROFILES: &str = "§Semantic profiles";
/// `§Decidability` section heading (quoted verbatim, like Python).
const SEC_DECIDABILITY: &str = "§Decidability";

/// The `rdf:type` predicate IRI string, exactly as the IR / Nemo stores it.
///
/// Mirrors Python `_RDF_TYPE = str(RDF.type)`.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

// ── Verdict ──────────────────────────────────────────────────────────────────

/// The static-certification verdict for a program against a declared profile.
///
/// Mirrors Python `CertificationVerdict`. [`to_json`](Self::to_json) produces a
/// key-for-key, value-for-value match of the Python `to_json()` dict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CertificationVerdict {
    /// The declared profile-id string, e.g. `"PositiveHornProfile"`.
    pub profile_id: String,
    /// The decidability class string, e.g. `"terminating/PTIME-data"`.
    pub decidability_class: String,
    /// True iff no violations were found (StableModel advisories count).
    pub certified: bool,
    /// The deterministic, sorted list of diagnostic strings.
    pub violations: Vec<String>,
}

impl CertificationVerdict {
    /// Render the verdict as `(key, value)` pairs in the same shape as Python's
    /// `CertificationVerdict.to_json()`:
    ///
    /// ```json
    /// {
    ///   "certified": <bool>,
    ///   "decidability_class": <string>,
    ///   "profile_id": <string>,
    ///   "violations": [<string>, …]   // sorted
    /// }
    /// ```
    ///
    /// Keys are sorted (the Python `to_json` literal is already in sorted-key
    /// order). `violations` is sorted, identically to Python. The PyO3 surface
    /// in `py.rs` materialises this into a `PyDict` with the same four keys.
    pub fn to_json_pairs(&self) -> (bool, &str, &str, Vec<String>) {
        let mut sorted = self.violations.clone();
        sorted.sort();
        (
            self.certified,
            self.decidability_class.as_str(),
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
/// The keys are computed by [`predicate_key`] so they match the Python IR's
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

/// Compute the dependency-graph node key for a Nemo [`Atom`], mirroring Python
/// `_predicate_key`.
///
/// For an `rdf:type` atom whose object (the arity-3 `terms()[1]` slot) is a
/// **non-variable**, the key folds in the class:
/// `"{rdf:type-iri} {class-iri}"`. Otherwise the key is the bare predicate IRI.
fn predicate_key(atom: &Atom) -> String {
    let predicate = atom.predicate().to_string();
    if predicate == RDF_TYPE {
        // Arity-3 encoding `pred(subject, object, world)`: object is terms()[1].
        if let Some(obj) = atom.terms().nth(1) {
            if !obj.is_variable() {
                return format!("{predicate} {}", debracket(&obj.to_string()));
            }
        }
    }
    predicate
}

/// Strip the outer `<…>` (Nemo's ground-IRI display) or `"…"` (literal display)
/// so the folded class string matches the bare IRI / value the Python IR stores
/// in `atom.obj`.
fn debracket(s: &str) -> String {
    if let Some(inner) = s.strip_prefix('<').and_then(|t| t.strip_suffix('>')) {
        return inner.to_owned();
    }
    if let Some(inner) = s.strip_prefix('"').and_then(|t| t.strip_suffix('"')) {
        return inner.to_owned();
    }
    s.to_owned()
}

/// The logical (subject, object) terms of an atom in the arity-3 `.rls`
/// encoding `pred(subject, object, world)`.
///
/// **World-slot normalization (parity-critical).** The Python certifier runs
/// over the IR, whose atom is `(subject, predicate, obj)` with NO world slot —
/// `project_nemo` injects the world variable into a *third* term that the IR
/// never carries. To keep the variable/position analysis byte-identical to
/// Python we expose only `terms()[0]` (subject) and `terms()[1]` (object) as the
/// logical S/O positions, and **drop `terms()[2]` (the world context)**. The
/// predicate IRI is a constant on both sides (the IR's `atom.predicate` is never
/// a variable), so it is excluded from the variable set, exactly as Python's
/// `_atom_variables` excludes it in practice.
fn logical_terms(atom: &Atom) -> Vec<&nemo::rule_model::components::term::Term> {
    atom.terms().take(2).collect()
}

/// The bare variable names occurring in an atom's logical S/O terms (Nemo renders
/// a universal variable as `?Name`; we record `?Name` to match Python `_is_var`).
/// The world slot is excluded (see [`logical_terms`]).
fn atom_variables(atom: &Atom) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for term in logical_terms(atom) {
        if term.is_variable() {
            // Nemo's Display for a variable is `?Name` (matches the IR's `?x`).
            out.insert(term.to_string());
        }
    }
    out
}

/// The S ("S") and O ("O") positions of an atom as [`PosTerm`]s. Slot[0] is the
/// subject, slot[1] the object; the world slot (slot[2]) is excluded. Mirrors
/// Python `_positions` over the IR's subject/object (predicate is constant).
fn atom_so_positions(atom: &Atom) -> Vec<PosTerm> {
    let key = predicate_key(atom);
    let mut out = Vec::new();
    for (idx, slot) in [(0usize, "S"), (1usize, "O")] {
        if let Some(term) = atom.terms().nth(idx) {
            out.push(PosTerm {
                key: key.clone(),
                slot,
                is_var: term.is_variable(),
                term: term.to_string(),
            });
        }
    }
    out
}

/// Parse the `.rls` text via Nemo's own parser and project each rule into a
/// [`RuleView`]. Reusing Nemo's parser (no second IR) is mandated by the task:
/// it guarantees the predicate surface is identical to the engine's.
///
/// # Errors
///
/// Returns the Nemo parse-error string when the `.rls` text does not parse.
fn parse_rule_views(rules: &str) -> Result<Vec<RuleView>, String> {
    // Parse WITHOUT Nemo's semantic validation: the validator rejects the very
    // rule shapes the certifier must flag (unsafe head vars, negation-only
    // bodies). Syntax errors still fail loudly. See `parse_unvalidated`.
    let parsed = NemoParsedRules::parse_unvalidated(rules)?;
    let program = parsed.into_program();

    let mut views: Vec<RuleView> = Vec::new();
    for rule in program.rules() {
        let head_keys: Vec<String> = rule.head().iter().map(predicate_key).collect();
        let positive_body_keys: Vec<String> = rule.body_positive().map(predicate_key).collect();
        let negative_body_keys: Vec<String> = rule.body_negative().map(predicate_key).collect();

        // DL-safety bookkeeping (mirrors Python `certify_dl_safe`): a variable is
        // "bound" iff it appears in a positive body atom; "used" = head ∪ body.
        let mut bound_vars: BTreeSet<String> = BTreeSet::new();
        for atom in rule.body_positive() {
            bound_vars.extend(atom_variables(atom));
        }
        let mut used_vars: BTreeSet<String> = BTreeSet::new();
        for atom in rule.head() {
            used_vars.extend(atom_variables(atom));
        }
        for atom in rule.body_atoms() {
            used_vars.extend(atom_variables(atom));
        }

        // The head key used in DL-safety diagnostics is the first head atom's key
        // (rules have exactly one head atom in the gmeow-logic fragment).
        let head_key_for_safety = head_keys.first().cloned().unwrap_or_default();

        // S/O position lists for weak acyclicity.
        let mut head_positions: Vec<PosTerm> = Vec::new();
        for atom in rule.head() {
            head_positions.extend(atom_so_positions(atom));
        }
        let mut positive_positions: Vec<PosTerm> = Vec::new();
        for atom in rule.body_positive() {
            positive_positions.extend(atom_so_positions(atom));
        }
        let mut all_body_positions: Vec<PosTerm> = Vec::new();
        for atom in rule.body_atoms() {
            all_body_positions.extend(atom_so_positions(atom));
        }

        views.push(RuleView {
            head_keys,
            positive_body_keys,
            negative_body_keys,
            head_key_for_safety,
            used_vars,
            bound_vars,
            head_positions,
            positive_positions,
            all_body_positions,
        });
    }
    Ok(views)
}

// ── Predicate dependency graph + Tarjan SCC ──────────────────────────────────

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
    let mut index_counter: usize = 0;
    let mut stack: Vec<String> = Vec::new();
    let mut on_stack: HashSet<String> = HashSet::new();
    let mut indices: HashMap<String, usize> = HashMap::new();
    let mut low: HashMap<String, usize> = HashMap::new();
    let mut result: Vec<BTreeSet<String>> = Vec::new();

    // Sorted node iteration → deterministic SCC numbering (matches Python).
    let vertices: Vec<String> = graph.keys().cloned().collect();

    for start in &vertices {
        if indices.contains_key(start) {
            continue;
        }
        // Iterative DFS: each frame is (node, next-child-index).
        let mut work: Vec<(String, usize)> = vec![(start.clone(), 0)];
        while let Some((node, child_idx)) = work.last().cloned() {
            if child_idx == 0 {
                indices.insert(node.clone(), index_counter);
                low.insert(node.clone(), index_counter);
                index_counter += 1;
                stack.push(node.clone());
                on_stack.insert(node.clone());
            }
            let successors = graph.get(&node).cloned().unwrap_or_default();
            let mut recursed = false;
            let mut i = child_idx;
            while i < successors.len() {
                let succ = &successors[i];
                if !indices.contains_key(succ) {
                    // Advance this frame past `succ`, then descend into it.
                    let len = work.len();
                    work[len - 1] = (node.clone(), i + 1);
                    work.push((succ.clone(), 0));
                    recursed = true;
                    break;
                }
                if on_stack.contains(succ) {
                    let succ_index = indices[succ];
                    let entry = low.get_mut(&node).expect("low set on push");
                    *entry = (*entry).min(succ_index);
                }
                i += 1;
            }
            if recursed {
                continue;
            }
            if low[&node] == indices[&node] {
                let mut component: BTreeSet<String> = BTreeSet::new();
                loop {
                    let w = stack.pop().expect("stack non-empty while closing SCC");
                    on_stack.remove(&w);
                    let done = w == node;
                    component.insert(w);
                    if done {
                        break;
                    }
                }
                result.push(component);
            }
            work.pop();
            if let Some((parent, _)) = work.last().cloned() {
                let node_low = low[&node];
                let entry = low.get_mut(&parent).expect("parent low set on push");
                *entry = (*entry).min(node_low);
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
        if node_to_scc.get(&head) == node_to_scc.get(&body) {
            if let Some(cycle) = shortest_cycle(&adj, &body, &head) {
                return Some(cycle);
            }
        }
    }
    None
}

/// True iff the program is stratifiable (no negative edge inside an SCC).
fn is_stratified(graph: &DepGraph) -> bool {
    offending_cycle(graph).is_none()
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
/// "existential" head variable). For real `project_nemo` output every head
/// variable is a frontier variable (bound positively, carrying the world var),
/// so the graph has no special edges and this is vacuously satisfied — but a
/// pathological input (e.g. a head var bound only under negation) reproduces
/// Python's special-edge diagnostics byte-for-byte.
///
/// Positions are `(predicate_key, slot)` with slot ∈ {`"S"`,`"O"`} — the logical
/// subject/object slots (the world slot and the constant predicate are excluded,
/// see [`logical_terms`]); this matches Python, where the predicate term is never
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
fn decidability_class(profile: &str) -> &'static str {
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

/// Statically certify `rules` (`.rls` text) against its `declared_profile`.
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
/// # Errors
///
/// Returns the Nemo parse-error string if `rules` does not parse as `.rls`.
pub fn certify(rules: &str, profile: &str) -> Result<CertificationVerdict, String> {
    let views = parse_rule_views(rules)?;
    let graph = DepGraph::from_views(&views);

    let mut violations: Vec<String> = Vec::new();
    match profile {
        "PositiveHornProfile" => {
            violations.extend(certify_positive_horn(&views));
            violations.extend(certify_dl_safe(&views));
            violations.extend(certify_weak_acyclicity(&views));
            violations.extend(certify_joint_acyclicity(&views));
            violations.extend(certify_guarded(&views));
            violations.extend(certify_sticky(&views));
        }
        "StratifiedNAFProfile" => {
            violations.extend(certify_stratified_naf(&graph));
            violations.extend(certify_dl_safe(&views));
            violations.extend(certify_weak_acyclicity(&views));
            violations.extend(certify_joint_acyclicity(&views));
            violations.extend(certify_guarded(&views));
            violations.extend(certify_sticky(&views));
        }
        "WellFoundedProfile" => {
            violations.extend(certify_well_founded(&views));
            violations.extend(certify_dl_safe(&views));
            violations.extend(certify_weak_acyclicity(&views));
        }
        "StableModelProfile" => {
            violations.extend(certify_stable_model(&graph));
            violations.extend(certify_dl_safe(&views));
            violations.extend(certify_weak_acyclicity(&views));
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
        certified,
        violations,
    })
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // The arity-3 ternary `.rls` encoding `project_nemo` emits.
    const P: &str = "http://example.org/p";
    const Q: &str = "http://example.org/q";
    const R: &str = "http://example.org/r";
    const S: &str = "http://example.org/s";

    // ── Tarjan SCC ────────────────────────────────────────────────────────────

    #[test]
    fn tarjan_finds_simple_cycle() {
        let mut g: BTreeMap<String, Vec<String>> = BTreeMap::new();
        g.insert("a".into(), vec!["b".into()]);
        g.insert("b".into(), vec!["a".into()]);
        g.insert("c".into(), vec!["a".into()]);
        let sccs = tarjan_scc(&g);
        let big = sccs.iter().find(|s| s.len() > 1).expect("cycle SCC");
        assert_eq!(
            big,
            &["a".to_owned(), "b".to_owned()]
                .into_iter()
                .collect::<BTreeSet<_>>()
        );
        assert!(sccs.contains(&["c".to_owned()].into_iter().collect()));
    }

    #[test]
    fn tarjan_is_deterministic() {
        let mut g: BTreeMap<String, Vec<String>> = BTreeMap::new();
        g.insert("a".into(), vec!["b".into(), "c".into()]);
        g.insert("b".into(), vec!["c".into()]);
        g.insert("c".into(), vec!["a".into()]);
        g.insert("d".into(), vec![]);
        assert_eq!(tarjan_scc(&g), tarjan_scc(&g));
    }

    // ── Stratification ─────────────────────────────────────────────────────────

    /// `r(x,y) :- p(x,y)` and `s(x,x) :- p(x,y), ~q(x,y)` — no negative cycle.
    fn stratified_rls() -> String {
        format!(
            "<{R}>(?x, ?y, ?W) :- <{P}>(?x, ?y, ?W) .\n\
             <{S}>(?x, ?x, ?W) :- <{P}>(?x, ?y, ?W), ~<{Q}>(?x, ?y, ?W) .\n"
        )
    }

    /// `p(x,y) :- ~q(x,y)` and `q(x,y) :- p(x,y)` — cycle through negation.
    fn non_stratified_rls() -> String {
        format!(
            "<{P}>(?x, ?y, ?W) :- ~<{Q}>(?x, ?y, ?W) .\n\
             <{Q}>(?x, ?y, ?W) :- <{P}>(?x, ?y, ?W) .\n"
        )
    }

    #[test]
    fn stratified_set_certifies() {
        let verdict = certify(&stratified_rls(), "StratifiedNAFProfile").expect("parse");
        assert!(verdict.certified, "violations: {:?}", verdict.violations);
        assert!(verdict.violations.is_empty());
        assert_eq!(verdict.decidability_class, "terminating/PTIME-data");
    }

    #[test]
    fn non_stratified_set_is_flagged_with_cycle_message() {
        let verdict = certify(&non_stratified_rls(), "StratifiedNAFProfile").expect("parse");
        assert!(!verdict.certified);
        // The pathological fixture (a negation-only body, no positive atom to bind
        // the world var) legitimately also trips DL-safety / weak-acyclicity — the
        // SAME violations the Python `certify_program` emits for this program (see
        // the parity-check note in the module docs). We assert the stratification
        // cycle is present, with the deterministic rendering Python produces.
        let cycle = verdict
            .violations
            .iter()
            .find(|v| v.contains("StratifiedNAFProfile violation"))
            .expect("cycle violation present");
        assert!(cycle.contains("crosses a negated body atom"));
        assert!(cycle.contains("not stratifiable"));
        assert!(cycle.contains("LOGIC-SEMANTICS.md §Semantic profiles"));
        // The exact rendered cycle: negative edge (p ← q); shortest_cycle BFS from
        // body `q` to head `p` yields the closing form `[p -> q -> p]` — the cycle
        // closes back to the negated head, identical to the Python oracle
        // (verified against logic_certify.py).
        assert!(
            cycle.contains(&format!("[{P} -> {Q} -> {P}]")),
            "unexpected cycle text: {cycle}"
        );
    }

    #[test]
    fn stratified_naf_check_is_deterministic() {
        let a = certify(&non_stratified_rls(), "StratifiedNAFProfile").expect("parse");
        let b = certify(&non_stratified_rls(), "StratifiedNAFProfile").expect("parse");
        assert_eq!(a, b);
    }

    // ── PositiveHorn ────────────────────────────────────────────────────────────

    #[test]
    fn positive_horn_rejects_negation() {
        let rls = format!("<{P}>(?x, ?y, ?W) :- ~<{Q}>(?x, ?y, ?W) .\n");
        let verdict = certify(&rls, "PositiveHornProfile").expect("parse");
        assert!(!verdict.certified);
        assert!(
            verdict
                .violations
                .iter()
                .any(|v| v.contains("PositiveHornProfile violation")
                    && v.contains("no negation-as-failure")),
            "{:?}",
            verdict.violations
        );
    }

    #[test]
    fn positive_program_certifies_under_positive_horn() {
        let rls = format!("<{R}>(?x, ?y, ?W) :- <{P}>(?x, ?y, ?W), <{Q}>(?x, ?y, ?W) .\n");
        let verdict = certify(&rls, "PositiveHornProfile").expect("parse");
        assert!(verdict.certified, "violations: {:?}", verdict.violations);
    }

    // ── DL-safety ───────────────────────────────────────────────────────────────

    #[test]
    fn dl_safety_violation_detected() {
        // ?z appears only in the head — unbound by any positive body atom.
        let rls = format!("<{R}>(?x, ?z, ?W) :- <{P}>(?x, ?y, ?W) .\n");
        let verdict = certify(&rls, "StratifiedNAFProfile").expect("parse");
        assert!(!verdict.certified);
        assert!(
            verdict
                .violations
                .iter()
                .any(|v| v.contains("DL-safety violation")
                    && v.contains("?z")
                    && v.contains("not DL-safe")),
            "{:?}",
            verdict.violations
        );
    }

    // ── StableModel advisory ─────────────────────────────────────────────────────

    #[test]
    fn stable_model_advisory_present_when_not_stratified() {
        let verdict = certify(&non_stratified_rls(), "StableModelProfile").expect("parse");
        assert!(!verdict.certified);
        assert_eq!(verdict.decidability_class, "NP-hard");
        assert!(
            verdict
                .violations
                .iter()
                .any(|v| v.contains("StableModelProfile is NP-hard")
                    && v.contains("LOGIC-SEMANTICS.md §Decidability")),
            "{:?}",
            verdict.violations
        );
    }

    #[test]
    fn stable_model_advisory_absent_when_stratified() {
        // A stratified set is also stable=well-founded ⇒ no NP-hard advisory.
        let verdict = certify(&stratified_rls(), "StableModelProfile").expect("parse");
        assert!(
            !verdict.violations.iter().any(|v| v.contains("NP-hard")),
            "{:?}",
            verdict.violations
        );
    }

    // ── Vacuous decidable-fragment checks pass on function-free input ────────────

    #[test]
    fn function_free_program_certifies_acyclicity() {
        // The stratified, function-free program certifies under StratifiedNAF
        // (weak/joint acyclicity, guard, sticky all vacuously pass).
        let verdict = certify(&stratified_rls(), "StratifiedNAFProfile").expect("parse");
        assert!(verdict.certified, "violations: {:?}", verdict.violations);
    }

    // ── rdf:type class-level fold (parity normalization) ─────────────────────────

    #[test]
    fn rdf_type_class_level_self_cycle_through_negation_flagged() {
        // `?x a C :- ~(?x a C)` — a class-level negative self-loop. The fold keys
        // on the class IRI in the object slot, so the cycle is visible.
        let cls = "http://example.org/C";
        let rls = format!("<{RDF_TYPE}>(?x, <{cls}>, ?W) :- ~<{RDF_TYPE}>(?x, <{cls}>, ?W) .\n");
        let verdict = certify(&rls, "StratifiedNAFProfile").expect("parse");
        let cycle = verdict
            .violations
            .iter()
            .find(|v| v.contains("StratifiedNAFProfile violation"))
            .expect("cycle violation present");
        assert!(cycle.contains("crosses a negated body atom"), "{cycle}");
        // The folded key is `{rdf:type} {class}`, so the rendered cycle names it —
        // proving the rdf:type fold over the object slot matches Python.
        assert!(cycle.contains(&format!("{RDF_TYPE} {cls}")), "{cycle}");
    }

    // ── Unknown/operational profiles ─────────────────────────────────────────────

    #[test]
    fn procedural_prolog_has_no_static_certification() {
        let rls = format!("<{R}>(?x, ?y, ?W) :- <{P}>(?x, ?y, ?W) .\n");
        let verdict = certify(&rls, "ProceduralPrologProfile").expect("parse");
        assert!(!verdict.certified);
        assert!(
            verdict
                .violations
                .iter()
                .any(|v| v.contains("no static decidability certification")),
            "{:?}",
            verdict.violations
        );
        assert_eq!(verdict.decidability_class, "operational/Turing-complete");
    }

    // ── to_json shape parity ─────────────────────────────────────────────────────

    #[test]
    fn to_json_pairs_sorts_violations() {
        let verdict = CertificationVerdict {
            profile_id: "StratifiedNAFProfile".into(),
            decidability_class: "terminating/PTIME-data".into(),
            certified: false,
            violations: vec!["zeta".into(), "alpha".into(), "mu".into()],
        };
        let (certified, dc, pid, violations) = verdict.to_json_pairs();
        assert!(!certified);
        assert_eq!(dc, "terminating/PTIME-data");
        assert_eq!(pid, "StratifiedNAFProfile");
        assert_eq!(violations, vec!["alpha", "mu", "zeta"]);
    }
}
