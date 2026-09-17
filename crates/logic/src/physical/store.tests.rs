// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::physical::cursor::LendingIterator;
use crate::seam::{BudgetStatus, DerivationId, DerivedQuad, WorldFactPattern, WorldSourceIdentity};

fn term(iri: &str) -> TermValue {
    TermValue::iri(iri)
}

/// Drain a [`RowCursor`] into a `Vec` of id rows — a `#[cfg(test)]`-only helper for
/// asserting a selection's full sequence.  `select` now returns a lending cursor
/// (no eager `Vec` on the production hot path), so tests collect it here.
fn select_rows(s: &RelationStore, predicate: &str, bound: Bound) -> Vec<(TermId, TermId, RowId)> {
    let mut cursor = s.select(predicate, bound);
    let mut rows = Vec::new();
    while let Some(row) = cursor.next() {
        rows.push(row);
    }
    rows
}

/// The interned id for an IRI, asserting it is present.
fn id_of(s: &RelationStore, iri: &str) -> TermId {
    s.iri_id(iri)
        .unwrap_or_else(|| panic!("term {iri:?} must be interned"))
}

/// Resolve selected `(subject_id, object_id, row_id)` id rows back to their
/// `TermValue` surfaces via the store's interner — `select` returns ids only, never
/// cloned terms, so a caller resolves lazily exactly here (the `RowId` is the delta
/// probe key, not a surface, so it is dropped for the surface assertions).
fn resolved(s: &RelationStore, rows: &[(TermId, TermId, RowId)]) -> Vec<(TermValue, TermValue)> {
    rows.iter()
        .map(|&(si, oi, _row)| {
            (
                s.interner().resolve(si).clone(),
                s.interner().resolve(oi).clone(),
            )
        })
        .collect()
}

/// Build a store with a small `knows`/`likes` corpus.
///
/// `knows`: (a,b), (a,c), (b,c)  — `likes`: (a,c)
fn sample_store() -> RelationStore {
    let knows = "http://ex/knows";
    let likes = "http://ex/likes";
    let mut s = RelationStore::new();
    assert!(
        s.insert(knows, &term("http://ex/a"), &term("http://ex/b"))
            .is_some()
    );
    assert!(
        s.insert(knows, &term("http://ex/a"), &term("http://ex/c"))
            .is_some()
    );
    assert!(
        s.insert(knows, &term("http://ex/b"), &term("http://ex/c"))
            .is_some()
    );
    assert!(
        s.insert(likes, &term("http://ex/a"), &term("http://ex/c"))
            .is_some()
    );
    s
}

/// Resolve selected id rows to an ORDER-INDEPENDENT `(subject, object)` surface set.
///
/// The arrangement enumerates batch-then-tail (an internal storage order), NOT a
/// stable emission order — winner selection, a total order over provenance, fixes
/// output — so a store test asserts the row SET, never a sequence.
fn resolved_set(s: &RelationStore, rows: &[(TermId, TermId, RowId)]) -> BTreeSet<(String, String)> {
    resolved(s, rows)
        .into_iter()
        .map(|(a, b)| (format!("{a:?}"), format!("{b:?}")))
        .collect()
}

fn pair(a: &str, b: &str) -> (String, String) {
    (format!("{:?}", term(a)), format!("{:?}", term(b)))
}

#[test]
fn physical_select_subject_bound() {
    let s = sample_store();
    let a = id_of(&s, "http://ex/a");
    let got = select_rows(&s, "http://ex/knows", Bound::Subject(a));
    assert_eq!(
        resolved_set(&s, &got),
        [
            pair("http://ex/a", "http://ex/b"),
            pair("http://ex/a", "http://ex/c"),
        ]
        .into()
    );
}

#[test]
fn physical_select_object_bound() {
    let s = sample_store();
    let c = id_of(&s, "http://ex/c");
    let got = select_rows(&s, "http://ex/knows", Bound::Object(c));
    assert_eq!(
        resolved_set(&s, &got),
        [
            pair("http://ex/a", "http://ex/c"),
            pair("http://ex/b", "http://ex/c"),
        ]
        .into()
    );
}

#[test]
fn physical_select_both_bound() {
    let s = sample_store();
    let a = id_of(&s, "http://ex/a");
    let b = id_of(&s, "http://ex/b");
    let c = id_of(&s, "http://ex/c");
    let got = select_rows(&s, "http://ex/knows", Bound::Both(a, c));
    assert_eq!(
        resolved_set(&s, &got),
        [pair("http://ex/a", "http://ex/c")].into()
    );

    // A both-bound miss (b is interned but (b,b) is not a tuple) yields nothing.
    let none = select_rows(&s, "http://ex/knows", Bound::Both(b, b));
    assert!(none.is_empty());
}

#[test]
fn physical_select_any_yields_every_row() {
    let s = sample_store();
    let got = select_rows(&s, "http://ex/knows", Bound::Any);
    assert_eq!(
        resolved_set(&s, &got),
        [
            pair("http://ex/a", "http://ex/b"),
            pair("http://ex/a", "http://ex/c"),
            pair("http://ex/b", "http://ex/c"),
        ]
        .into()
    );
}

#[test]
fn physical_dedup_returns_false_and_stores_one_row() {
    let knows = "http://ex/knows";
    let mut s = RelationStore::new();
    assert!(
        s.insert(knows, &term("http://ex/a"), &term("http://ex/b"))
            .is_some()
    );
    // Re-inserting the same (s,p,o) is a no-op that reports None (no new row id).
    assert!(
        s.insert(knows, &term("http://ex/a"), &term("http://ex/b"))
            .is_none()
    );
    assert_eq!(s.len_for("http://ex/knows"), 1);
    assert_eq!(
        resolved_set(&s, &select_rows(&s, "http://ex/knows", Bound::Any)),
        [pair("http://ex/a", "http://ex/b")].into(),
    );
}

/// `insert` stamps each newly-inserted row with a dense [`RowId`] in store-wide
/// insertion order — `0, 1, 2, …` ACROSS relations, not per-relation — and `select`
/// hands each selected row that same id.  A dedup returns `None` (no id consumed), so
/// the id space stays gap-free and `row_count` counts exactly the live rows.  The row
/// ids are asserted as SETS (the arrangement stores rows value-sorted, so a selection
/// enumerates them in storage order, not insertion order).
#[test]
fn physical_insert_assigns_dense_cross_relation_row_ids() {
    let knows = "http://ex/knows";
    let likes = "http://ex/likes";
    let mut s = RelationStore::new();
    // Interleave predicates so a per-relation index would NOT match the global RowId.
    let r0 = s
        .insert(knows, &term("http://ex/a"), &term("http://ex/b"))
        .map(|(_, _, r)| r);
    let r1 = s
        .insert(likes, &term("http://ex/a"), &term("http://ex/c"))
        .map(|(_, _, r)| r);
    let r2 = s
        .insert(knows, &term("http://ex/a"), &term("http://ex/c"))
        .map(|(_, _, r)| r);
    assert_eq!(r0, Some(RowId::from_index(0)));
    assert_eq!(r1, Some(RowId::from_index(1)));
    assert_eq!(
        r2,
        Some(RowId::from_index(2)),
        "RowIds span relations in insertion order"
    );
    // A dedup consumes no RowId — the space stays dense and `row_count` is exact.
    assert_eq!(
        s.insert(knows, &term("http://ex/a"), &term("http://ex/b")),
        None
    );
    assert_eq!(s.row_count(), 3, "three distinct rows ⇒ RowIds 0..3");
    // `select` hands each row its store-global RowId (never a per-relation index):
    // knows carries ids {0, 2}, likes carries {1} — the interleaved likes took id 1.
    let knows_ids: BTreeSet<RowId> = select_rows(&s, knows, Bound::Any)
        .iter()
        .map(|&(_, _, r)| r)
        .collect();
    assert_eq!(
        knows_ids,
        [RowId::from_index(0), RowId::from_index(2)].into(),
        "selected rows carry their store-global RowId, not a per-relation index",
    );
    let likes_ids: BTreeSet<RowId> = select_rows(&s, likes, Bound::Any)
        .iter()
        .map(|&(_, _, r)| r)
        .collect();
    assert_eq!(likes_ids, [RowId::from_index(1)].into());
}

/// The arrangement seals its tail into sorted batches past the threshold and still
/// returns the exact row SET (with the exact store-global RowIds) — the galloping
/// batch path, not just the tail leg.  A heavily-interleaved build (RowIds NOT
/// contiguous within a relation) confirms every selected row carries its dense global
/// id and `row_count` stays exact across relations.
#[test]
fn physical_sealed_batches_preserve_row_set_and_dense_ids() {
    let (p, q) = ("http://ex/p", "http://ex/q");
    let mut s = RelationStore::new();
    // Interleave p and q for > 2*threshold rows so BOTH relations seal batches and
    // neither relation's RowIds are the contiguous 0,1,2,….
    let n = super::TAIL_SEAL_THRESHOLD * 3;
    for i in 0..n {
        let pred = if i % 2 == 0 { p } else { q };
        assert!(
            s.insert(
                pred,
                &term("http://ex/s"),
                &term(&format!("http://ex/o{i:04}"))
            )
            .is_some()
        );
    }
    assert_eq!(s.row_count(), n, "every distinct row is counted, gap-free");
    // Each relation returns exactly its half of the rows, each with the global RowId
    // it was stamped with at insert (the even indices went to p, odd to q).
    let p_ids: BTreeSet<RowId> = select_rows(&s, p, Bound::Any)
        .iter()
        .map(|&(_, _, r)| r)
        .collect();
    let expect_p: BTreeSet<RowId> = (0..n).step_by(2).map(RowId::from_index).collect();
    assert_eq!(p_ids, expect_p, "p carries exactly the even-index RowIds");
    // A subject-bound gallop over the sealed batches finds every one of s's edges.
    let subj = id_of(&s, "http://ex/s");
    assert_eq!(
        select_rows(&s, p, Bound::Subject(subj)).len(),
        n / 2,
        "subject gallop over sealed batches finds all rows"
    );
    // Dedup still holds across sealed batches: re-inserting a sealed row is a no-op.
    assert!(
        s.insert(p, &term("http://ex/s"), &term("http://ex/o0000"))
            .is_none(),
        "a row already sealed into a batch is deduped by the galloping probe"
    );
}

#[test]
fn physical_contains_on_display_surfaces() {
    let s = sample_store();
    assert!(s.contains(
        "http://ex/knows",
        &TermValue::iri("http://ex/a"),
        &TermValue::iri("http://ex/b")
    ));
    // A never-seen term surface fails the lookup, so containment is false.
    assert!(!s.contains(
        "http://ex/knows",
        &TermValue::iri("http://ex/a"),
        &TermValue::iri("http://ex/z")
    ));
    // Unknown predicate is a clean miss, not a panic.
    assert!(!s.contains(
        "http://ex/nope",
        &TermValue::iri("http://ex/a"),
        &TermValue::iri("http://ex/b")
    ));
}

#[test]
fn physical_term_id_lookup_never_inserts() {
    let s = sample_store();
    // Interned terms resolve; a never-seen surface is None (⇒ empty selection).
    assert!(s.iri_id("http://ex/a").is_some());
    assert_eq!(s.iri_id("http://ex/never-seen"), None);
    // The miss did not insert: a second lookup still misses.
    assert_eq!(s.iri_id("http://ex/never-seen"), None);
}

#[test]
fn physical_interner_is_shared_across_relations() {
    // The same term inserted under two predicates mints ONE id (store-level
    // interner), and a Bound built from that id probes either relation.
    let s = sample_store();
    let a = id_of(&s, "http://ex/a");
    assert_eq!(
        resolved_set(&s, &select_rows(&s, "http://ex/likes", Bound::Subject(a))),
        [pair("http://ex/a", "http://ex/c")].into(),
    );
}

/// Emission-order guard: the `relations` table is a `PredId`-indexed `Vec`, so its
/// slot order is mint order.  The ONLY consumer-facing enumeration —
/// [`RelationStore::predicates`] — MUST still be lexical, sorted through the
/// `BTreeSet` sweep, NEVER leaking mint order.  Insert predicates in deliberately
/// anti-lexical order and assert the output is lexical regardless.
#[test]
fn physical_predicates_never_leak_hasher_order() {
    let mut s = RelationStore::new();
    // Insert in reverse-lexical order; a raw hash-map sweep would not be sorted.
    for pred in ["http://ex/zeta", "http://ex/mu", "http://ex/alpha"] {
        assert!(
            s.insert(pred, &term("http://ex/x"), &term("http://ex/y"))
                .is_some()
        );
    }
    let preds: Vec<&str> = s.predicates().collect();
    assert_eq!(
        preds,
        vec!["http://ex/alpha", "http://ex/mu", "http://ex/zeta"],
        "predicates() must be lexical — the PredId mint order must never leak"
    );
}

#[test]
fn physical_predicates_are_sorted_and_deterministic() {
    let s = sample_store();
    let preds: Vec<&str> = s.predicates().collect();
    assert_eq!(preds, vec!["http://ex/knows", "http://ex/likes"]);

    // Repeated builds give identical select output (determinism).
    let s2 = sample_store();
    assert_eq!(
        select_rows(&s, "http://ex/knows", Bound::Any),
        select_rows(&s2, "http://ex/knows", Bound::Any),
    );
    let p2: Vec<&str> = s2.predicates().collect();
    assert_eq!(preds, p2);
}

// ── extract_edb round-trip via a minimal WorldFactSource test double ───────────

/// A hand-rolled `WorldFactSource` yielding a fixed list of `DerivedQuad`s in
/// `world`. Only `in_world` is exercised by `extract_edb`; the other legs are
/// vacuous (and unused) for this test.
struct FakeForeign {
    world: String,
    quads: Vec<DerivedQuad>,
    identity: WorldSourceIdentity,
}

impl FakeForeign {
    fn new(world: &str, tuples: &[(&str, &str, &str)]) -> Self {
        let world_iri = world.to_owned();
        let quads = tuples
            .iter()
            .map(|(s, p, o)| DerivedQuad {
                graph: world_iri.clone(),
                subject: term(s),
                predicate: (*p).to_owned(),
                object: term(o),
                graph_component: world_iri.clone(),
                derivation_id: DerivationId("http://ex/d".to_owned()),
                rule_iri: "http://ex/r".to_owned(),
                source_quad_ids: vec![],
                profile: "http://ex/profile".to_owned(),
                budget_status: BudgetStatus::Ok,
            })
            .collect();
        Self {
            world: world_iri,
            quads,
            identity: WorldSourceIdentity::new("test-generation", "test-contract"),
        }
    }
}

impl WorldFactSource for FakeForeign {
    fn identity(&self) -> &WorldSourceIdentity {
        &self.identity
    }

    fn visit_world(
        &self,
        world: &str,
        pattern: &WorldFactPattern,
        visitor: &mut dyn FnMut(&DerivedQuad) -> gmeow_errors::Result<()>,
    ) -> gmeow_errors::Result<()> {
        for quad in &self.quads {
            if quad.graph == world
                && pattern
                    .subject
                    .as_ref()
                    .is_none_or(|subject| &quad.subject == subject)
                && pattern
                    .predicate
                    .as_ref()
                    .is_none_or(|predicate| &quad.predicate == predicate)
                && pattern
                    .object
                    .as_ref()
                    .is_none_or(|object| &quad.object == object)
            {
                visitor(quad)?;
            }
        }
        Ok(())
    }

    fn derived_by(
        &self,
        quad_id: Option<&DerivationId>,
        rule: Option<&str>,
        sources: Option<&[String]>,
    ) -> gmeow_errors::Result<Vec<crate::seam::DerivationRecord>> {
        Ok(self
            .quads
            .iter()
            .filter(|quad| {
                quad_id.is_none_or(|candidate| candidate == &quad.derivation_id)
                    && rule.is_none_or(|candidate| candidate == quad.rule_iri)
                    && sources.is_none_or(|candidate| candidate == quad.source_quad_ids)
            })
            .map(|quad| {
                (
                    quad.derivation_id.clone(),
                    quad.rule_iri.clone(),
                    quad.source_quad_ids.clone(),
                )
            })
            .collect())
    }
}

#[test]
fn physical_extract_edb_round_trips() {
    let foreign = FakeForeign::new(
        "http://ex/world",
        &[
            ("http://ex/a", "http://ex/knows", "http://ex/b"),
            ("http://ex/a", "http://ex/knows", "http://ex/c"),
            ("http://ex/a", "http://ex/likes", "http://ex/c"),
            // A duplicate quad must collapse to one row.
            ("http://ex/a", "http://ex/knows", "http://ex/b"),
        ],
    );
    let edb = extract_edb(&foreign, &foreign.world).expect("extract test EDB");

    let preds: Vec<&str> = edb.predicates().collect();
    assert_eq!(preds, vec!["http://ex/knows", "http://ex/likes"]);
    assert_eq!(edb.len_for("http://ex/knows"), 2);
    assert_eq!(edb.len_for("http://ex/likes"), 1);
    assert_eq!(
        resolved_set(&edb, &select_rows(&edb, "http://ex/knows", Bound::Any)),
        [
            pair("http://ex/a", "http://ex/b"),
            pair("http://ex/a", "http://ex/c"),
        ]
        .into()
    );
    assert!(edb.contains(
        "http://ex/likes",
        &TermValue::iri("http://ex/a"),
        &TermValue::iri("http://ex/c")
    ));
}

// ── The Z-set seam: signed-weight consolidation (compiled + exercised) ───────

/// Build a single-row `Batch<i64>` with an explicit signed weight — the seam a
/// signed delta rides.  (Set-semantics `insert` never mints a non-unit weight, so
/// the seam is exercised here by constructing weighted batches directly.)
fn weighted_row(s: TermId, o: TermId, r: RowId, w: i64) -> Batch<i64> {
    Batch {
        subj: vec![s],
        obj: vec![o],
        row_id: vec![r],
        weight: vec![w],
        object_index: OnceLock::new(),
    }
}

/// The consolidation merge is generic over the [`Weight`] monoid and compiles for
/// `W = i64` (a Z-set): a `+1` and a `-1` on the SAME key combine to `0`, which
/// annihilates and DROPS the row — retraction falls out of the same merge, no
/// special deletion pass.  This proves "the representation admits signed weights" is
/// a compiled, exercised fact, not a promise; production stays at the ZST `W = ()`.
#[test]
fn batch_merge_is_a_z_set_over_signed_weights() {
    let s = TermId::from_index(0);
    let o = TermId::from_index(1);

    // (+1) + (-1) = 0 ⇒ the row annihilates and is dropped (retraction).
    let plus = weighted_row(s, o, RowId::from_index(5), 1);
    let minus = weighted_row(s, o, RowId::from_index(2), -1);
    let retracted = merge_batches(&plus, &minus).expect("signed retraction combines");
    assert_eq!(
        retracted.len(),
        0,
        "(+1)+(-1)=0 annihilates the shared-key row"
    );

    // (+1) + (+2) = 3 ⇒ one surviving row, weights summed, LOWER RowId kept (R4).
    let two = weighted_row(s, o, RowId::from_index(2), 2);
    let summed = merge_batches(&plus, &two).expect("signed addition combines");
    assert_eq!(summed.len(), 1, "a non-annihilating combine keeps one row");
    assert_eq!(summed.weight[0], 3, "weights sum: 1 + 2 = 3");
    assert_eq!(
        summed.row_id[0],
        RowId::from_index(2),
        "the lower RowId deterministically survives a key collision"
    );

    // Disjoint keys interleave with NO combine — the set-semantics `W = ()` shape.
    let o2 = TermId::from_index(2);
    let a = weighted_row(s, o, RowId::from_index(0), 1);
    let b = weighted_row(s, o2, RowId::from_index(1), 1);
    let disjoint = merge_batches(&a, &b).expect("disjoint signed batches interleave");
    assert_eq!(
        disjoint.len(),
        2,
        "disjoint keys interleave, no combine fires"
    );
}

/// Saturation is not a ring operation (and is not associative across mixed-sign
/// updates), so overflow must hard-fail instead of silently changing the Z-set.
#[test]
fn signed_weight_overflow_never_saturates() {
    let err = i64::MAX
        .combine(1)
        .expect_err("signed overflow must be a structured failure");
    assert!(err.message().contains("overflow"), "{err}");
    assert!(err.message().contains("addition"), "{err}");
}

// ── Chase-invented Skolem-term nulls ─────────────────────────────────────────

fn witness(ordinal: usize, frontier: Vec<TermValue>) -> SkolemTerm {
    SkolemTerm {
        scope: WitnessContract::native(crate::native_semantics::SemanticVocabulary::Exact)
            .scope("urn:test-world", [1; 32]),
        rule_iri: "http://ex/rule".to_owned(),
        ordinal,
        frontier,
    }
}

#[test]
fn skolem_mint_is_deterministic_and_idempotent() {
    let mut reg = SkolemRegistry::new();
    let a = reg.mint(witness(0, vec![term("http://ex/a")]));
    // Re-firing on the SAME frontier recovers the SAME witness (restricted-chase
    // blocking) and does not grow the registry — the fixpoint's teeth.
    let b = reg.mint(witness(0, vec![term("http://ex/a")]));
    assert_eq!(a, b);
    assert_eq!(reg.len(), 1);
    assert!(reg.is_invented(&a));
}

#[test]
fn skolem_distinct_frontiers_give_distinct_witnesses() {
    // The standard restricted chase mints one fresh witness per frontier binding
    // Distinct frontier values imply distinct nulls.
    let mut reg = SkolemRegistry::new();
    let wa = reg.mint(witness(0, vec![term("http://ex/a")]));
    let wb = reg.mint(witness(0, vec![term("http://ex/b")]));
    assert_ne!(wa, wb);
    assert_eq!(reg.len(), 2);
}

#[test]
fn skolem_distinct_ordinals_give_distinct_witnesses() {
    // The n distinct existential vars of `≥n p.D` (same frontier) are distinct.
    let mut reg = SkolemRegistry::new();
    let w0 = reg.mint(witness(0, vec![term("http://ex/a")]));
    let w1 = reg.mint(witness(1, vec![term("http://ex/a")]));
    assert_ne!(w0, w1);
    assert_eq!(reg.len(), 2);
}

#[test]
fn skolem_addresses_on_values_not_variable_names() {
    // The Skolem function keys on the bound frontier VALUES.  Two firings of
    // alpha-variant rules (?x vs ?y) that bind the SAME data mint the byte-identical
    // null — `content_key` alpha-normalized identity (no lexical name in the key).
    let mut reg = SkolemRegistry::new();
    let frontier = vec![term("http://ex/a"), term("http://ex/b")];
    let a = reg.mint(witness(0, frontier.clone()));
    let b = reg.mint(witness(0, frontier));
    assert_eq!(a, b);
    assert_eq!(reg.len(), 1);
}

#[test]
fn skolem_recipe_round_trips_and_nests() {
    // The IRI → recipe lookup recovers the structured recipe (decomposability),
    // and a frontier slot may itself be a prior invented null (nested Skolem term)
    // that is still decomposable through the registry.
    let mut reg = SkolemRegistry::new();
    let inner = reg.mint(witness(0, vec![term("http://ex/a")]));
    let inner_iri = match &inner {
        TermValue::Iri(s) => s.clone(),
        _ => unreachable!("mint returns an IRI"),
    };
    let outer = reg.mint(witness(0, vec![inner.clone()]));
    let outer_iri = match &outer {
        TermValue::Iri(s) => s.clone(),
        _ => unreachable!(),
    };

    // The outer recipe decomposes to reveal the inner null in its frontier…
    let outer_recipe = reg.recipe(&outer_iri).expect("outer recipe retained");
    assert_eq!(outer_recipe.frontier, vec![inner]);
    // …and the inner null is itself decomposable (its own frontier is `a`).
    let inner_recipe = reg.recipe(&inner_iri).expect("inner recipe retained");
    assert_eq!(inner_recipe.frontier, vec![term("http://ex/a")]);
    // A term this registry never minted is not recognized as invented.
    assert!(!reg.is_invented(&term("http://ex/a")));
}

#[test]
fn skolem_content_key_is_injective_across_frontier_shapes() {
    // A frontier term whose `term_display` surface itself contains the field
    // separator MUST NOT be able to forge a boundary.  `term("a>\u{1f}<b")`
    // renders as `<a>\u{1f}<b>` — byte-identical to the two-term frontier
    // `[term("a"), term("b")]` rendered as `<a>` `\u{1f}` `<b>` joined.  Under a
    // naive separator-joined key these two DISTINCT recipes collide to one
    // witness; the length-prefixed encoding keeps them distinct.
    let one = witness(0, vec![term("a>\u{1f}<b")]);
    let two = witness(0, vec![term("a"), term("b")]);
    assert_ne!(
        one.witness_iri(),
        two.witness_iri(),
        "distinct frontier recipes must mint distinct witnesses"
    );

    // The same collision, driven through the registry: two mints, two witnesses.
    let mut reg = SkolemRegistry::new();
    let w_one = reg.mint(one);
    let w_two = reg.mint(two);
    assert_ne!(w_one, w_two);
    assert_eq!(reg.len(), 2);
}
#[test]
fn native_membership_and_witnesses_preserve_distinct_datatype_fields() {
    let plain = TermValue::simple_literal("a");
    let langless = TermValue::Literal {
        lexical_form: "a".to_owned(),
        datatype: gmeow_term_arena::engine::RDF_LANG_STRING.to_owned(),
        language: None,
        direction: None,
    };
    let subject = term("urn:identity:s");
    let mut store = RelationStore::new();
    store.insert("urn:identity:p", &subject, &plain);
    assert!(store.contains("urn:identity:p", &subject, &plain));
    assert!(!store.contains("urn:identity:p", &subject, &langless));
    assert!(
        !store
            .select_pattern(
                "urn:identity:p",
                Some(&subject),
                Some(&langless),
                false,
                true
            )
            .any_remaining()
    );
    assert_ne!(
        witness(0, vec![plain]).witness_iri(),
        witness(0, vec![langless]).witness_iri()
    );
}
