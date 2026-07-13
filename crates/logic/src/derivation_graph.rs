// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact incremental reasoning via a truth-maintenance derivation graph
//! (child S6b).
//!
//! # Why this exists
//!
//! The native chase's single-immediate-antecedent provenance and the
//! first-wins [`crate::foundation::FoundationQuad`] record exactly **one**
//! derivation per fact. That is sufficient to *render* a proof, but it cannot
//! answer the incremental question: *if a source assertion is removed, does this
//! derived fact survive?* A flattened "contributing slice set" loses the
//! disjunctive structure — a fact with two proofs `{A,B}` and `{C}` flattens to
//! `{A,B,C}`, from which removal-survival is undecidable (design doc Principle 7).
//!
//! This module records the full disjunctive justification structure:
//!
//! ```text
//! Fact = OR( Asserted{unit}, RuleApplication(AND premise₁, premise₂, …), … )
//! ```
//!
//! and supports the truth-maintenance queries S6b needs: deletion survival
//! (least-fixpoint over the surviving justifications) and incremental ==
//! clean-rebuild equivalence.
//!
//! # Persistent identity is independent of runtime IDs (golden-pinned)
//!
//! A [`FactKey`] is the **content-addressed reifier IRI** of a `(S, P, O)` triple,
//! and a [`RuleApplication`]'s [`derivation_id`](RuleApplication::derivation_id) is
//! `mint_derivation_id(rule_iri, sorted(premise reifier IRIs))`
//! ([`crate::provenance::mint_derivation_id`]). **Numeric interner IDs
//! (`UnitId`/`QuadHandle`/…) MUST NEVER enter these hashes** — only content
//! (IRIs). This is the load-bearing invariant: the same logical graph built with a
//! different interner-id assignment (e.g. inputs inserted in a different order)
//! produces byte-identical derivation IDs. The
//! `runtime_id_independence_graph_digest_golden` test pins this.
//!
//! # No-optionality / hard-fail
//!
//! - A [`RuleApplication`] whose premise list contains its own conclusion fact key
//!   is rejected at insertion ([`DerivationGraph::add_derivation`] returns `Err`):
//!   a fact may never justify itself directly (self-attestation guard). Cyclic
//!   *mutual* support across distinct facts is permitted in the structure but is
//!   correctly excluded from `survives` unless an independent base grounds the
//!   cycle (the least-fixpoint never bootstraps a cycle from nothing).
//! - There is no degraded fallback: every query returns an exact answer over the
//!   recorded justifications.

use std::collections::{BTreeMap, BTreeSet};

use crate::provenance::mint_derivation_id;

/// Wrap a provenance-derivation condition message as a typed diagnostic on the
/// shared substrate, preserving the authored text verbatim.
fn provenance_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Provenance { detail })
}

// ── Fact key ────────────────────────────────────────────────────────────────

/// The content-addressed identity of a fact: its reifier IRI.
///
/// This is **never** a numeric runtime id. Two structurally identical
/// `(S, P, O)` triples share one `FactKey` regardless of how/when they were
/// interned. Ordering is lexicographic over the IRI string (deterministic, never
/// hashmap-iteration or insertion order).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FactKey(pub String);

impl FactKey {
    /// Borrow the underlying reifier IRI.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for FactKey {
    fn from(s: &str) -> Self {
        FactKey(s.to_owned())
    }
}

impl From<String> for FactKey {
    fn from(s: String) -> Self {
        FactKey(s)
    }
}

// ── Source unit identity ────────────────────────────────────────────────────

/// The content-addressed identity of an asserting source unit (a slice IRI, the
/// root-ontology IRI, the runtime-input unit IRI, …).
///
/// This is the **public** unit IRI, never the graph-local numeric `UnitId`
/// (design doc Principle 9: "RDF links the public slice IRI, never the graph-local
/// numeric `SliceId`"). Keeping it a content string is what makes deletion-survival
/// and incremental==rebuild equivalence independent of interner-id assignment.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UnitKey(pub String);

impl UnitKey {
    /// Borrow the underlying unit IRI.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for UnitKey {
    fn from(s: &str) -> Self {
        UnitKey(s.to_owned())
    }
}

impl From<String> for UnitKey {
    fn from(s: String) -> Self {
        UnitKey(s)
    }
}

// ── Rule application ────────────────────────────────────────────────────────

/// A single derived justification: a rule firing that proves a conclusion fact
/// from an AND of premise facts.
///
/// `premises` are the content keys (reifier IRIs) of the consumed antecedent
/// facts — **not** numeric ids. The set is stored sorted so two firings with the
/// same premises in a different order are equal and content-addressed identically.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleApplication {
    /// The IRI of the fired rule (or a sentinel pass IRI). Content, not a runtime id.
    pub rule_iri: String,
    /// The content keys (reifier IRIs) of the AND-ed premise facts, sorted.
    pub premises: Box<[FactKey]>,
}

impl RuleApplication {
    /// Construct a rule application, sorting and de-duplicating the premises so the
    /// stored form is canonical (content-addressed identity is order-independent).
    #[must_use]
    pub fn new(rule_iri: impl Into<String>, premises: impl IntoIterator<Item = FactKey>) -> Self {
        let mut prem: Vec<FactKey> = premises.into_iter().collect();
        prem.sort();
        prem.dedup();
        RuleApplication {
            rule_iri: rule_iri.into(),
            premises: prem.into_boxed_slice(),
        }
    }

    /// The content-addressed derivation IRI for this firing.
    ///
    /// `mint_derivation_id(rule_iri, sorted(premise reifier IRIs))` — byte-identical
    /// to [`crate::provenance::mint_derivation_id`], which is golden-pinned to the
    /// Python oracle. The premises are already sorted by [`RuleApplication::new`];
    /// `mint_derivation_id` re-sorts defensively, so the id never depends on the
    /// insertion order.
    #[must_use]
    pub fn derivation_id(&self) -> String {
        let refs: Vec<&str> = self.premises.iter().map(FactKey::as_str).collect();
        mint_derivation_id(&self.rule_iri, &refs)
    }
}

// ── Justification ───────────────────────────────────────────────────────────

/// One element of a fact's OR-of-justifications.
///
/// A fact is derivable iff **at least one** of its justifications holds. An
/// `Asserted` justification holds iff its unit is still present; a `Derived`
/// justification holds iff **every** premise is (recursively) derivable.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Justification {
    /// The fact was directly asserted by a source unit.
    Asserted {
        /// The asserting unit's content key (public IRI, never a numeric id).
        unit: UnitKey,
    },
    /// The fact was derived by a rule firing from an AND of premises.
    Derived(RuleApplication),
}

// ── Derivation graph ────────────────────────────────────────────────────────

/// The truth-maintenance derivation graph: each fact maps to its OR-set of
/// justifications.
///
/// The map is a [`BTreeMap`] and each justification set a [`BTreeSet`], so all
/// iteration is in deterministic content order (never hashmap-iteration order).
/// This is what lets the incremental result equal a clean rebuild byte-for-byte.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DerivationGraph {
    justifications: BTreeMap<FactKey, BTreeSet<Justification>>,
}

impl DerivationGraph {
    /// An empty graph.
    #[must_use]
    pub fn new() -> Self {
        DerivationGraph {
            justifications: BTreeMap::new(),
        }
    }

    /// Record that `fact` is directly asserted by `unit`.
    pub fn add_assertion(&mut self, fact: FactKey, unit: UnitKey) {
        self.justifications
            .entry(fact)
            .or_default()
            .insert(Justification::Asserted { unit });
    }

    /// Record that `fact` is derivable via `app` (a rule firing).
    ///
    /// # Errors
    ///
    /// Returns `Err` if `app.premises` lists `fact` itself — a fact may never be a
    /// premise of its own derivation (the self-attestation guard: a generated
    /// analysis fact must never become an input to its own computation). Mutual
    /// support across *distinct* facts is allowed (and correctly handled by
    /// [`DerivationGraph::survives`]).
    pub fn add_derivation(
        &mut self,
        fact: FactKey,
        app: RuleApplication,
    ) -> gmeow_errors::Result<()> {
        // `premises` is sorted (RuleApplication::new), so a binary search is the
        // correct membership test for the self-attestation guard.
        if app.premises.binary_search(&fact).is_ok() {
            return Err(provenance_err(format!(
                "self-attestation: fact <{}> cannot be a premise of its own \
                 derivation (rule <{}>)",
                fact.as_str(),
                app.rule_iri
            )));
        }
        self.justifications
            .entry(fact)
            .or_default()
            .insert(Justification::Derived(app));
        Ok(())
    }

    /// Incrementally **modify** a fact's derived justifications: drop every existing
    /// `Derived` justification for `fact` and insert `apps` in their place. Any
    /// `Asserted` justifications for the fact are preserved untouched.
    ///
    /// This is the incremental engine's "the rule layer recomputed this fact's
    /// firings" primitive. Applying it to a stale graph reproduces the same final
    /// state a clean rebuild would (the incremental==rebuild equivalence).
    ///
    /// # Errors
    ///
    /// Returns `Err` (and leaves the graph unchanged) if any application in `apps`
    /// lists `fact` itself as a premise (self-attestation guard).
    pub fn replace_derivations(
        &mut self,
        fact: &FactKey,
        apps: impl IntoIterator<Item = RuleApplication>,
    ) -> gmeow_errors::Result<()> {
        let apps: Vec<RuleApplication> = apps.into_iter().collect();
        for app in &apps {
            if app.premises.binary_search(fact).is_ok() {
                return Err(provenance_err(format!(
                    "self-attestation: fact <{}> cannot be a premise of its own \
                     derivation (rule <{}>)",
                    fact.as_str(),
                    app.rule_iri
                )));
            }
        }
        let set = self.justifications.entry(fact.clone()).or_default();
        // Keep only the Asserted justifications; drop all Derived ones.
        set.retain(|j| matches!(j, Justification::Asserted { .. }));
        for app in apps {
            set.insert(Justification::Derived(app));
        }
        // A fact left with no justifications at all is removed entirely so the
        // graph stays equal to a clean rebuild (which never records an empty fact).
        if self
            .justifications
            .get(fact)
            .is_some_and(BTreeSet::is_empty)
        {
            self.justifications.remove(fact);
        }
        Ok(())
    }

    /// Incrementally **delete** an asserting unit's contribution to a fact: drop the
    /// `Asserted{unit}` justification for `fact` if present. Derived justifications
    /// are untouched. A fact left with no justifications is removed from the graph.
    pub fn remove_assertion(&mut self, fact: &FactKey, unit: &UnitKey) {
        if let Some(set) = self.justifications.get_mut(fact) {
            set.remove(&Justification::Asserted { unit: unit.clone() });
            if set.is_empty() {
                self.justifications.remove(fact);
            }
        }
    }

    /// Every fact that has at least one recorded justification.
    pub fn facts(&self) -> impl Iterator<Item = &FactKey> {
        self.justifications.keys()
    }

    /// The (sorted) justification set for a fact, or `None` if the fact is unknown.
    #[must_use]
    pub fn justifications_of(&self, fact: &FactKey) -> Option<&BTreeSet<Justification>> {
        self.justifications.get(fact)
    }

    /// The number of facts in the graph.
    #[must_use]
    pub fn len(&self) -> usize {
        self.justifications.len()
    }

    /// Whether the graph holds no facts.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.justifications.is_empty()
    }

    /// Compute the set of facts that remain derivable after removing some source
    /// assertions and/or rules.
    ///
    /// A fact remains derivable iff it has, under the *surviving* justifications,
    /// **either**:
    ///
    /// 1. an `Asserted{unit}` justification whose `unit` is **not** in
    ///    `removed_units`, **or**
    /// 2. a `Derived(app)` justification whose `rule_iri` is **not** in
    ///    `removed_rules` **and** all of whose premises are themselves still
    ///    derivable.
    ///
    /// This is the least fixpoint of the monotone "derivable" operator, started
    /// from the surviving asserted base and grown by surviving rule firings until
    /// closure. Cycles are handled safely: a fact participating only in a mutual
    /// support cycle with no independent base is **never** added (the least
    /// fixpoint does not bootstrap), so a fact cannot pull itself into existence.
    ///
    /// The returned set is sorted (it is a [`BTreeSet`]), so two equal-content
    /// graphs return byte-identical surviving sets.
    #[must_use]
    pub fn survives(
        &self,
        removed_units: &BTreeSet<UnitKey>,
        removed_rules: &BTreeSet<String>,
    ) -> BTreeSet<FactKey> {
        let mut derivable: BTreeSet<FactKey> = BTreeSet::new();

        // Least fixpoint: repeatedly add any fact with a now-satisfied
        // justification until no more facts can be added. Iteration over the
        // BTreeMap is content-ordered, so the *result* is order-independent; only
        // the number of passes (never the answer) depends on traversal order.
        loop {
            let mut grew = false;
            for (fact, proofs) in &self.justifications {
                if derivable.contains(fact) {
                    continue;
                }
                let now = proofs.iter().any(|j| match j {
                    Justification::Asserted { unit } => !removed_units.contains(unit),
                    Justification::Derived(app) => {
                        !removed_rules.contains(&app.rule_iri)
                            && app.premises.iter().all(|p| derivable.contains(p))
                    }
                });
                if now {
                    derivable.insert(fact.clone());
                    grew = true;
                }
            }
            if !grew {
                break;
            }
        }

        derivable
    }

    /// The set of all facts derivable with nothing removed — the full closure.
    #[must_use]
    pub fn all_derivable(&self) -> BTreeSet<FactKey> {
        self.survives(&BTreeSet::new(), &BTreeSet::new())
    }

    /// A content-addressed digest of the entire graph (every fact + its sorted
    /// justifications), independent of insertion order and runtime ids.
    ///
    /// Two graphs built from the same logical content — even with different
    /// interner-id assignments or insertion orders — produce the same digest.
    /// Used by the incremental==rebuild equivalence test.
    #[must_use]
    pub fn content_digest(&self) -> String {
        let mut hasher = blake3::Hasher::new();
        for (fact, proofs) in &self.justifications {
            hasher.update(b"F\n");
            hasher.update(fact.as_str().as_bytes());
            hasher.update(b"\n");
            for j in proofs {
                match j {
                    Justification::Asserted { unit } => {
                        hasher.update(b"A\n");
                        hasher.update(unit.as_str().as_bytes());
                        hasher.update(b"\n");
                    }
                    Justification::Derived(app) => {
                        hasher.update(b"D\n");
                        // Use the content-addressed derivation id, which already
                        // folds rule_iri + sorted premise reifier IRIs.
                        hasher.update(app.derivation_id().as_bytes());
                        hasher.update(b"\n");
                    }
                }
            }
        }
        hasher.finalize().to_hex().to_string()
    }
}

// ── Construction from a foundation chase ────────────────────────────────────

/// Build a derivation graph from a foundation materialization
/// ([`crate::foundation::evaluate`] output).
///
/// Each [`crate::foundation::FoundationQuad`] becomes one justification for its
/// fact key (the quad's reifier IRI):
///
/// - an asserted quad (`rule_iri == logic:assert`) → an `Asserted` justification
///   whose unit is the quad's **world IRI** (the asserting unit in the v1 oracle —
///   each world is its own assertion unit);
/// - a derived quad → a `Derived(RuleApplication)` whose premises are the quad's
///   `source_quad_ids` (already reifier IRIs).
///
/// The self-attestation guard fires if any derived quad lists its own reifier as a
/// source (it never should — that would be a malformed chase).
///
/// # Errors
///
/// Returns `Err` if a derived quad is self-referential (self-attestation), or if a
/// quad's reifier cannot be recomputed.
pub fn from_foundation_quads(
    quads: &[crate::foundation::FoundationQuad],
) -> gmeow_errors::Result<DerivationGraph> {
    use crate::foundation::ASSERT_RULE_IRI;

    let mut graph = DerivationGraph::new();
    for q in quads {
        let fact = FactKey(crate::foundation::quad_reifier(q)?);
        if q.rule_iri == ASSERT_RULE_IRI {
            // Asserted: the world IRI is the assertion unit in the v1 oracle.
            graph.add_assertion(fact, UnitKey(q.graph.clone()));
        } else {
            let premises = q
                .source_quad_ids
                .iter()
                .map(|s| FactKey(s.clone()))
                .collect::<Vec<_>>();
            let app = RuleApplication::new(q.rule_iri.clone(), premises);
            graph.add_derivation(fact, app)?;
        }
    }
    Ok(graph)
}

#[cfg(test)]
mod tests;
