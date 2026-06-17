// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Epistemic-entrenchment ordering reader for deterministic AGM revision (#505).
//!
//! Stratum-C revision must be **deterministic**: admitting a counterfactual
//! antecedent `A` may force retracting conflicting base facts, and in a
//! tightly-linked graph there can be many minimal ways to do so. The design's own
//! clause — *closeness is declared data, not a fixed semantics* — resolves this:
//! the selection among minimal revisions is exactly AGM **epistemic entrenchment**
//! (Gärdenfors–Makinson), and a *total* order yields a unique maxichoice revision.
//!
//! This module reads an entrenchment ordering from the base world using the
//! vocabulary the logic engine already owns — **no new ontology terms**:
//!
//! - [`OVERRIDES`] (`gmeow:overrides`) — pairwise norm precedence: `N1 overrides
//!   N2` ⇒ `N1` is the more entrenched.
//! - [`STRONGER_THAN`] (`gmeow:strongerThan`) — orders authority levels
//!   (`absolute ≻ high ≻ medium ≻ conditional`); a norm inherits its level's rank
//!   via [`HAS_AUTHORITY_LEVEL`] (`gmeow:hasAuthorityLevel`).
//! - [`MORE_SEVERE_THAN`] (`gmeow:moreSevereThan`) — orders severity levels.
//! - [`SHARPENS`] (`gmeow:sharpens`) — standpoint refinement: the sharper (more
//!   specific) standpoint is the more entrenched.
//!
//! The ordering is a **strict partial order over IRIs**. Two IRIs that the order
//! leaves incomparable are a *genuine tie*; the revision engine ([`crate::counterfactual`])
//! turns a tie into `unknown` rather than branching. Per Principles 9 and 12 the
//! ordering is **local to one revision** (never a global privileging) and is
//! solver work read from recorded claims, never a reasoner entailment.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use crate::store::WorldStore;

// ── Reused vocabulary IRIs (no new ontology terms) ───────────────────────────

/// `gmeow:overrides` — pairwise norm precedence (subject prevails over object).
pub const OVERRIDES: &str = "https://blackcatinformatics.ca/gmeow/overrides";
/// `gmeow:strongerThan` — orders authority-level value individuals.
pub const STRONGER_THAN: &str = "https://blackcatinformatics.ca/gmeow/strongerThan";
/// `gmeow:moreSevereThan` — orders severity-level value individuals.
pub const MORE_SEVERE_THAN: &str = "https://blackcatinformatics.ca/gmeow/moreSevereThan";
/// `gmeow:sharpens` — standpoint refinement (sharper = more specific).
pub const SHARPENS: &str = "https://blackcatinformatics.ca/gmeow/sharpens";
/// `gmeow:hasAuthorityLevel` — links a norm to its authority-level individual.
pub const HAS_AUTHORITY_LEVEL: &str = "https://blackcatinformatics.ca/gmeow/hasAuthorityLevel";

/// The four predicates whose `subject ≻ object` reading contributes a direct
/// entrenchment edge. Kept as an ordered slice for deterministic ingestion.
const DIRECT_EDGE_PREDICATES: [&str; 4] = [OVERRIDES, STRONGER_THAN, MORE_SEVERE_THAN, SHARPENS];

// ── Ordering ─────────────────────────────────────────────────────────────────

/// A strict partial order over IRIs: *more entrenched* ≻ *less entrenched*.
///
/// `greater[x]` holds every IRI strictly **less** entrenched than `x` (i.e. every
/// `y` with `x ≻ y`), transitively closed. The relation is irreflexive; a cycle in
/// the source edges is reported as an error by [`Entrenchment::read_from_world`]
/// rather than silently collapsed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Entrenchment {
    /// `x → { y : x ≻ y }`, transitively closed.
    greater: BTreeMap<String, BTreeSet<String>>,
    /// Every IRI that participates in at least one ordering edge.
    entities: BTreeSet<String>,
}

/// The outcome of selecting the least-entrenched element among candidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LeastEntrenched {
    /// A unique strict minimum — the deterministic retraction choice.
    Unique(String),
    /// Two or more candidates are mutually incomparable: a genuine tie. The
    /// revision must degrade to `unknown` rather than pick arbitrarily.
    Tie(Vec<String>),
    /// No candidates were supplied.
    Empty,
}

impl Entrenchment {
    /// Read the entrenchment ordering from the named graph `base_world` (a bare
    /// IRI, no angle brackets).
    ///
    /// Ingestion is deterministic (predicates and quads processed in sorted
    /// order). Direct edges come from [`DIRECT_EDGE_PREDICATES`]; authority-level
    /// inheritance adds an edge `n1 ≻ n2` whenever `n1`/`n2` carry authority
    /// levels `l1`/`l2` with `l1 ≻ l2` in the level order. The result is
    /// transitively closed.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` if the source edges contain a cycle (an entrenchment
    /// ordering must be a strict partial order; `x ≻ … ≻ x` is contradictory).
    pub fn read_from_world(store: &WorldStore, base_world: &str) -> Result<Self, String> {
        // (1) Collect direct edges (subject ≻ object), deterministically.
        let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
        for pred in DIRECT_EDGE_PREDICATES {
            for q in store.quads_for_pattern_in_world(base_world, None, Some(pred), None) {
                if let (Some(s), Some(o)) = (named_iri(&q.subject), term_iri(&q.object)) {
                    edges.insert((s, o));
                }
            }
        }

        // (2) Authority-level inheritance: norm → its level individual.
        //     n1 ≻ n2 when level(n1) ≻ level(n2) in the strongerThan order.
        //     A norm carries exactly one authority level; conflicting
        //     `hasAuthorityLevel` assertions are a contradiction in the source
        //     data and are rejected rather than silently collapsed (the surviving
        //     value would otherwise depend on ingestion order, breaking
        //     determinism).
        let mut norm_level: BTreeMap<String, String> = BTreeMap::new();
        for q in store.quads_for_pattern_in_world(base_world, None, Some(HAS_AUTHORITY_LEVEL), None)
        {
            if let (Some(s), Some(o)) = (named_iri(&q.subject), term_iri(&q.object)) {
                if let Some(prev) = norm_level.get(&s) {
                    if prev != &o {
                        return Err(format!(
                            "conflicting hasAuthorityLevel values for norm {s:?}: \
                             {prev:?} vs {o:?}"
                        ));
                    }
                } else {
                    norm_level.insert(s, o);
                }
            }
        }
        if !norm_level.is_empty() {
            // Level order must come only from `gmeow:strongerThan(level, level)`;
            // an `overrides`/`moreSevereThan`/`sharpens` edge that happens to touch
            // a level individual must NOT synthesize false authority precedence.
            let mut level_edges: BTreeSet<(String, String)> = BTreeSet::new();
            for q in store.quads_for_pattern_in_world(base_world, None, Some(STRONGER_THAN), None) {
                if let (Some(s), Some(o)) = (named_iri(&q.subject), term_iri(&q.object)) {
                    level_edges.insert((s, o));
                }
            }
            let level_order = closure(&level_edges);
            for (n1, l1) in &norm_level {
                for (n2, l2) in &norm_level {
                    if n1 == n2 {
                        continue;
                    }
                    if l1 != l2 && reaches(&level_order, l1, l2) {
                        edges.insert((n1.clone(), n2.clone()));
                    }
                }
            }
        }

        // (3) Transitively close, detecting cycles.
        let greater = closure(&edges);
        for (x, ys) in &greater {
            if ys.contains(x) {
                return Err(format!(
                    "entrenchment ordering has a cycle through {x:?}; \
                     a strict partial order cannot contain x ≻ … ≻ x"
                ));
            }
        }

        let mut entities: BTreeSet<String> = BTreeSet::new();
        for (a, b) in &edges {
            entities.insert(a.clone());
            entities.insert(b.clone());
        }

        Ok(Self { greater, entities })
    }

    /// Compare two IRIs by entrenchment.
    ///
    /// - `Some(Greater)` — `a` is strictly more entrenched than `b` (`a ≻ b`).
    /// - `Some(Less)`    — `b ≻ a`.
    /// - `Some(Equal)`   — `a == b` (the same IRI).
    /// - `None`          — incomparable: a **genuine tie**.
    pub fn compare(&self, a: &str, b: &str) -> Option<Ordering> {
        if a == b {
            return Some(Ordering::Equal);
        }
        if reaches(&self.greater, a, b) {
            Some(Ordering::Greater)
        } else if reaches(&self.greater, b, a) {
            Some(Ordering::Less)
        } else {
            None
        }
    }

    /// Whether every pair drawn from `iris` is comparable — i.e. the order is
    /// **total** over that set. The deterministic-revision verdict hinges on this:
    /// total ⇒ a unique world; not total ⇒ a genuine tie ⇒ `unknown`.
    pub fn is_total_over<'a, I>(&self, iris: I) -> bool
    where
        I: IntoIterator<Item = &'a str> + Clone,
    {
        let items: Vec<&str> = iris.into_iter().collect();
        for (i, a) in items.iter().enumerate() {
            for b in &items[i + 1..] {
                if self.compare(a, b).is_none() {
                    return false;
                }
            }
        }
        true
    }

    /// Select the unique strictly-least-entrenched IRI among `candidates`.
    ///
    /// Returns [`LeastEntrenched::Unique`] when exactly one candidate is `≺` every
    /// other; [`LeastEntrenched::Tie`] when the strict minimum is not unique (some
    /// candidates are mutually incomparable); [`LeastEntrenched::Empty`] for no
    /// candidates. This is the retraction-choice primitive: AGM revision retracts
    /// the least entrenched first, and a tie is never broken arbitrarily.
    pub fn least_entrenched(&self, candidates: &[String]) -> LeastEntrenched {
        match candidates.len() {
            0 => return LeastEntrenched::Empty,
            1 => return LeastEntrenched::Unique(candidates[0].clone()),
            _ => {}
        }
        // A candidate is a strict minimum iff every other candidate is ≻ it.
        let minima: Vec<&String> = candidates
            .iter()
            .filter(|c| {
                candidates
                    .iter()
                    .all(|other| other == *c || self.compare(other, c) == Some(Ordering::Greater))
            })
            .collect();
        match minima.as_slice() {
            [only] => LeastEntrenched::Unique((*only).clone()),
            _ => {
                // No (or no unique) strict minimum: the candidates that are not
                // dominated by some other candidate form the incomparable tie set.
                let mut tie: Vec<String> = candidates
                    .iter()
                    .filter(|c| {
                        !candidates
                            .iter()
                            .any(|other| self.compare(other, c) == Some(Ordering::Less))
                    })
                    .cloned()
                    .collect();
                tie.sort();
                tie.dedup();
                LeastEntrenched::Tie(tie)
            }
        }
    }

    /// Select the unique strictly-**most**-entrenched IRI among `candidates`.
    ///
    /// The dual of [`Self::least_entrenched`]: returns [`LeastEntrenched::Unique`]
    /// when exactly one candidate is `≻` every other, and [`LeastEntrenched::Tie`]
    /// when the strict maximum is not unique. This is the arbitration primitive
    /// for an internally over-determined antecedent: when two `assume(p(s,·))`
    /// atoms claim different values for one functional slot, the **most entrenched**
    /// value wins; an incomparable maximum is a genuine tie ⇒ `unknown`.
    pub fn most_entrenched(&self, candidates: &[String]) -> LeastEntrenched {
        match candidates.len() {
            0 => return LeastEntrenched::Empty,
            1 => return LeastEntrenched::Unique(candidates[0].clone()),
            _ => {}
        }
        // A candidate is a strict maximum iff it is ≻ every other candidate.
        let maxima: Vec<&String> = candidates
            .iter()
            .filter(|c| {
                candidates
                    .iter()
                    .all(|other| other == *c || self.compare(c, other) == Some(Ordering::Greater))
            })
            .collect();
        match maxima.as_slice() {
            [only] => LeastEntrenched::Unique((*only).clone()),
            _ => {
                // No unique strict maximum: the candidates not dominated by any
                // other form the incomparable tie set.
                let mut tie: Vec<String> = candidates
                    .iter()
                    .filter(|c| {
                        !candidates
                            .iter()
                            .any(|other| self.compare(other, c) == Some(Ordering::Greater))
                    })
                    .cloned()
                    .collect();
                tie.sort();
                tie.dedup();
                LeastEntrenched::Tie(tie)
            }
        }
    }

    /// Canonical byte serialization of the ordering: every transitively-closed
    /// `x ≻ y` edge, sorted, newline-framed. Used to derive the
    /// `entrenchment_hash` cache-key component.
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        for (x, ys) in &self.greater {
            for y in ys {
                out.push_str(x);
                out.push('\t');
                out.push_str(y);
                out.push('\n');
            }
        }
        out.into_bytes()
    }

    /// BLAKE3 digest of [`Self::canonical_bytes`] — the `entrenchment_hash`
    /// component of [`crate::versioning::CounterfactualKeyInputs`].
    pub fn hash(&self) -> [u8; 32] {
        *blake3::hash(&self.canonical_bytes()).as_bytes()
    }

    /// The set of IRIs participating in the ordering (test/inspection aid).
    pub fn entities(&self) -> &BTreeSet<String> {
        &self.entities
    }
}

// ── Graph helpers ────────────────────────────────────────────────────────────

/// Transitively close a set of `(from, to)` edges into a `from → {reachable}` map.
fn closure(edges: &BTreeSet<(String, String)>) -> BTreeMap<String, BTreeSet<String>> {
    let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (a, b) in edges {
        adj.entry(a.clone()).or_default().insert(b.clone());
    }
    let mut closed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let nodes: BTreeSet<&String> = edges.iter().flat_map(|(a, b)| [a, b]).collect();
    for start in nodes {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        let mut stack: Vec<String> = adj.get(start).into_iter().flatten().cloned().collect();
        while let Some(cur) = stack.pop() {
            if !seen.insert(cur.clone()) {
                continue;
            }
            if let Some(next) = adj.get(&cur) {
                for n in next {
                    stack.push(n.clone());
                }
            }
        }
        if !seen.is_empty() {
            closed.insert(start.clone(), seen);
        }
    }
    closed
}

/// Whether `a` reaches `b` in the transitively-closed map `m`.
fn reaches(m: &BTreeMap<String, BTreeSet<String>>, a: &str, b: &str) -> bool {
    m.get(a).map(|s| s.contains(b)).unwrap_or(false)
}

/// Extract the IRI string from an oxigraph subject (`NamedOrBlankNode`).
fn named_iri(s: &oxigraph::model::NamedOrBlankNode) -> Option<String> {
    match s {
        oxigraph::model::NamedOrBlankNode::NamedNode(n) => Some(n.as_str().to_owned()),
        oxigraph::model::NamedOrBlankNode::BlankNode(_) => None,
    }
}

/// Extract the IRI string from an oxigraph object [`Term`] (IRIs only).
fn term_iri(o: &oxigraph::model::Term) -> Option<String> {
    match o {
        oxigraph::model::Term::NamedNode(n) => Some(n.as_str().to_owned()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: &str = "http://world/base";
    const G: &str = "https://blackcatinformatics.ca/gmeow/";

    fn ex(local: &str) -> String {
        format!("https://example.org/{local}")
    }
    fn gm(local: &str) -> String {
        format!("{G}{local}")
    }

    // ── overrides axis ─────────────────────────────────────────────────────────

    #[test]
    fn overrides_axis_orders_norms() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("constitution"), OVERRIDES, &ex("bylaw"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        assert_eq!(
            e.compare(&ex("constitution"), &ex("bylaw")),
            Some(Ordering::Greater),
            "the overriding norm is more entrenched"
        );
        assert_eq!(
            e.compare(&ex("bylaw"), &ex("constitution")),
            Some(Ordering::Less)
        );
    }

    // ── strongerThan axis (levels) ─────────────────────────────────────────────

    #[test]
    fn stronger_than_axis_orders_levels() {
        let store = WorldStore::new();
        store.insert_quad(
            W,
            &gm("authorityAbsolute"),
            STRONGER_THAN,
            &gm("authorityHigh"),
        );
        store.insert_quad(
            W,
            &gm("authorityHigh"),
            STRONGER_THAN,
            &gm("authorityMedium"),
        );
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        // Transitive: absolute ≻ medium even though only the chain is asserted.
        assert_eq!(
            e.compare(&gm("authorityAbsolute"), &gm("authorityMedium")),
            Some(Ordering::Greater)
        );
    }

    // ── authority-level inheritance ────────────────────────────────────────────

    #[test]
    fn authority_level_inheritance_orders_norms_by_level() {
        let store = WorldStore::new();
        store.insert_quad(
            W,
            &gm("authorityAbsolute"),
            STRONGER_THAN,
            &gm("authorityHigh"),
        );
        store.insert_quad(
            W,
            &ex("treaty"),
            HAS_AUTHORITY_LEVEL,
            &gm("authorityAbsolute"),
        );
        store.insert_quad(W, &ex("policy"), HAS_AUTHORITY_LEVEL, &gm("authorityHigh"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        assert_eq!(
            e.compare(&ex("treaty"), &ex("policy")),
            Some(Ordering::Greater),
            "treaty (absolute) outranks policy (high)"
        );
    }

    #[test]
    fn authority_inheritance_uses_strongerthan_only_not_other_edges() {
        // A non-strongerThan edge between two level individuals (here `overrides`)
        // must NOT be read as authority-level precedence: norms carrying those
        // levels stay incomparable unless a `strongerThan` chain links the levels.
        let store = WorldStore::new();
        // overrides edge directly between the *level* individuals (not strongerThan).
        store.insert_quad(W, &gm("authorityHigh"), OVERRIDES, &gm("authorityMedium"));
        store.insert_quad(W, &ex("policy"), HAS_AUTHORITY_LEVEL, &gm("authorityHigh"));
        store.insert_quad(
            W,
            &ex("guideline"),
            HAS_AUTHORITY_LEVEL,
            &gm("authorityMedium"),
        );
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        // The levels themselves are ordered by the direct `overrides` edge …
        assert_eq!(
            e.compare(&gm("authorityHigh"), &gm("authorityMedium")),
            Some(Ordering::Greater)
        );
        // … but that must not synthesize precedence between the *norms*: only a
        // `strongerThan` level order may be inherited, and there is none here.
        assert_eq!(
            e.compare(&ex("policy"), &ex("guideline")),
            None,
            "overrides between levels must not leak into authority-level inheritance"
        );
    }

    #[test]
    fn conflicting_authority_level_is_rejected() {
        let store = WorldStore::new();
        store.insert_quad(
            W,
            &ex("treaty"),
            HAS_AUTHORITY_LEVEL,
            &gm("authorityAbsolute"),
        );
        store.insert_quad(W, &ex("treaty"), HAS_AUTHORITY_LEVEL, &gm("authorityHigh"));
        let err = Entrenchment::read_from_world(&store, W).unwrap_err();
        assert!(err.contains("conflicting hasAuthorityLevel"), "got: {err}");
    }

    // ── moreSevereThan axis ────────────────────────────────────────────────────

    #[test]
    fn more_severe_than_axis_orders_levels() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("catastrophic"), MORE_SEVERE_THAN, &ex("minor"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        assert_eq!(
            e.compare(&ex("catastrophic"), &ex("minor")),
            Some(Ordering::Greater)
        );
    }

    // ── sharpens axis ──────────────────────────────────────────────────────────

    #[test]
    fn sharpens_axis_orders_standpoints() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("cityCouncilView"), SHARPENS, &ex("regionalView"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        assert_eq!(
            e.compare(&ex("cityCouncilView"), &ex("regionalView")),
            Some(Ordering::Greater),
            "the sharper standpoint is more entrenched"
        );
    }

    // ── tie / incomparability ──────────────────────────────────────────────────

    #[test]
    fn incomparable_iris_are_a_tie() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        store.insert_quad(W, &ex("c"), OVERRIDES, &ex("d"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        // a and c are in disjoint chains — incomparable.
        assert_eq!(e.compare(&ex("a"), &ex("c")), None);
        assert!(!e.is_total_over([ex("a").as_str(), ex("c").as_str()]));
        assert!(e.is_total_over([ex("a").as_str(), ex("b").as_str()]));
    }

    #[test]
    fn least_entrenched_unique_on_total_chain() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        store.insert_quad(W, &ex("b"), OVERRIDES, &ex("c"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        // Among {a, c}, c is strictly least entrenched (a ≻ b ≻ c).
        assert_eq!(
            e.least_entrenched(&[ex("a"), ex("c")]),
            LeastEntrenched::Unique(ex("c"))
        );
    }

    #[test]
    fn least_entrenched_tie_on_incomparable() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        store.insert_quad(W, &ex("c"), OVERRIDES, &ex("d"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        // b and d are both minimal but incomparable → tie.
        match e.least_entrenched(&[ex("b"), ex("d")]) {
            LeastEntrenched::Tie(t) => {
                assert_eq!(t, vec![ex("b"), ex("d")]);
            }
            other => panic!("expected Tie, got {other:?}"),
        }
    }

    #[test]
    fn most_entrenched_unique_and_tie() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        // a ≻ b: the most entrenched is a (unique).
        assert_eq!(
            e.most_entrenched(&[ex("a"), ex("b")]),
            LeastEntrenched::Unique(ex("a"))
        );

        // Two incomparable values → tie for most entrenched.
        let store2 = WorldStore::new();
        store2.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        store2.insert_quad(W, &ex("c"), OVERRIDES, &ex("d"));
        let e2 = Entrenchment::read_from_world(&store2, W).unwrap();
        match e2.most_entrenched(&[ex("a"), ex("c")]) {
            LeastEntrenched::Tie(t) => assert_eq!(t, vec![ex("a"), ex("c")]),
            other => panic!("expected Tie, got {other:?}"),
        }
    }

    #[test]
    fn least_entrenched_empty_and_singleton() {
        let e = Entrenchment::default();
        assert_eq!(e.least_entrenched(&[]), LeastEntrenched::Empty);
        assert_eq!(
            e.least_entrenched(&[ex("solo")]),
            LeastEntrenched::Unique(ex("solo"))
        );
    }

    // ── cycle detection ────────────────────────────────────────────────────────

    #[test]
    fn cycle_in_edges_is_rejected() {
        let store = WorldStore::new();
        store.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        store.insert_quad(W, &ex("b"), OVERRIDES, &ex("a"));
        let err = Entrenchment::read_from_world(&store, W).unwrap_err();
        assert!(err.contains("cycle"), "got: {err}");
    }

    // ── hash determinism ───────────────────────────────────────────────────────

    #[test]
    fn hash_is_deterministic_and_order_sensitive() {
        let store1 = WorldStore::new();
        store1.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        let e1 = Entrenchment::read_from_world(&store1, W).unwrap();

        // Same content, inserted again — identical hash.
        let store2 = WorldStore::new();
        store2.insert_quad(W, &ex("a"), OVERRIDES, &ex("b"));
        let e2 = Entrenchment::read_from_world(&store2, W).unwrap();
        assert_eq!(e1.hash(), e2.hash());

        // Different ordering — different hash.
        let store3 = WorldStore::new();
        store3.insert_quad(W, &ex("b"), OVERRIDES, &ex("a"));
        let e3 = Entrenchment::read_from_world(&store3, W).unwrap();
        assert_ne!(e1.hash(), e3.hash());
    }

    #[test]
    fn empty_world_yields_empty_ordering() {
        let store = WorldStore::new();
        let e = Entrenchment::read_from_world(&store, W).unwrap();
        assert!(e.entities().is_empty());
        assert_eq!(e.compare(&ex("a"), &ex("b")), None);
    }
}
