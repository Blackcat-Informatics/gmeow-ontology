// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::code::register_code;
use crate::diag::{Diag, Focus, Slot};
use crate::grade::{FindingCategory, GateVerdict, Grade, Severity, Standpoint, gate};
use crate::model::Location;

fn diag_at(code: &'static str, category: FindingCategory, path: &str, message: &str) -> Diag {
    let c = register_code(code);
    let mut d = Diag::new(
        c,
        Grade::new(Severity::Error, category, Standpoint::Binding),
        message,
    );
    d = d.with_location(Location {
        path: Some(path.to_owned()),
        ..Location::default()
    });
    d
}

#[test]
fn recorded_rejection_preserves_causal_closure_and_live_source_at_the_boundary() {
    let mut ledger = DiagLedger::new();
    let cause = ledger.attach(
        diag_at(
            "test.record.cause",
            FindingCategory::DataShapeViolation,
            "cause.ttl",
            "missing premise",
        )
        .with_derived_from_quads(["urn:source:quad".to_owned()]),
        StageId::new("source"),
    );
    ledger.attach(
        diag_at(
            "test.record.unrelated",
            FindingCategory::DataShapeViolation,
            "unrelated.ttl",
            "unrelated",
        ),
        StageId::new("other"),
    );
    let live = Diag::from(std::io::Error::other("failed native read"))
        .with_context("selected source")
        .with_focus("urn:source:focus")
        .with_antecedents([cause]);
    assert!(live.is::<std::io::Error>());
    let recorded = ledger.record(live, StageId::new("observation"));
    assert_eq!(recorded.causes.len(), 1);
    assert_eq!(recorded.causes[0].derived_from_quads, ["urn:source:quad"]);
    assert_eq!(
        recorded.root.antecedents.as_ref(),
        &[recorded.causes[0].fingerprint]
    );
    assert_eq!(recorded.root.frames.len(), 2);
    assert_eq!(recorded.root.frames[0].message, "selected source");
    assert_eq!(recorded.root.frames[1].message, "failed native read");
    assert_eq!(recorded.root.grade.standpoint, Standpoint::Binding);
    let bytes = serde_json::to_vec(&recorded).unwrap();
    let restored: RecordedDiag = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored, recorded);
    let mut replay = DiagLedger::new();
    replay.replay(restored.causes);
    replay.replay([*restored.root]);
    assert_eq!(replay.len(), 2);
    assert_eq!(replay.verdict(), ledger.verdict());
}

#[test]
fn hash_cons_identity_dedups_and_message_is_not_in_the_fingerprint() {
    let mut ledger = DiagLedger::new();
    let a = diag_at(
        "test.ledger.identity",
        FindingCategory::DataShapeViolation,
        "a.ttl",
        "first message",
    );
    let b = diag_at(
        "test.ledger.identity",
        FindingCategory::DataShapeViolation,
        "a.ttl",
        "DIFFERENT message",
    );
    let ra = ledger.attach(a, StageId::new("stage-1"));
    let rb = ledger.attach(b, StageId::new("stage-1"));
    // Same (code, category, anchor) => one node, same handle, despite different message.
    assert_eq!(ra, rb);
    assert_eq!(ledger.len(), 1);
}

#[test]
fn distinct_observed_slots_are_not_silently_collapsed() {
    // R1: two findings share an anchor but observed different values — both
    // observations survive as a multiset.
    let mut ledger = DiagLedger::new();
    let mut a = diag_at(
        "test.ledger.r1",
        FindingCategory::DataShapeViolation,
        "x.ttl",
        "cardinality",
    );
    a = a.with_observed(Slot::new("3"));
    let mut b = diag_at(
        "test.ledger.r1",
        FindingCategory::DataShapeViolation,
        "x.ttl",
        "cardinality",
    );
    b = b.with_observed(Slot::new("7"));
    ledger.attach(a, StageId::new("s"));
    ledger.attach(b, StageId::new("s"));
    let node = ledger.emit_sorted()[0];
    assert_eq!(
        node.observations.len(),
        2,
        "distinct observations must both survive"
    );
}

#[test]
fn length_prefixed_fingerprint_resists_delimiter_injection() {
    // R2: ("ab","c") and ("a","bc") must not collide across field boundaries.
    let ctx = SourceContext::default();
    let f1 = DiagFingerprint::compute("ab", FindingCategory::DataShapeViolation, &ctx);
    let f2 = DiagFingerprint::compute("a", FindingCategory::DataShapeViolation, &ctx);
    assert_ne!(f1, f2);
    // A focus that concatenates to the same bytes as a different split.
    let c1 = SourceContext {
        focus: Some(Focus("xy".to_owned())),
        ..SourceContext::default()
    };
    let c2 = SourceContext {
        focus: Some(Focus("x".to_owned())),
        location: Location {
            path: Some("y".to_owned()),
            ..Location::default()
        },
        ..SourceContext::default()
    };
    assert_ne!(
        DiagFingerprint::compute("k", FindingCategory::DataShapeViolation, &c1),
        DiagFingerprint::compute("k", FindingCategory::DataShapeViolation, &c2)
    );
}

#[test]
fn fresh_and_replayed_ledger_are_byte_identical_and_carry_no_arena_ref() {
    let mut fresh = DiagLedger::new();
    fresh.attach(
        diag_at(
            "test.ledger.replay",
            FindingCategory::DataShapeViolation,
            "r.ttl",
            "boom",
        ),
        StageId::new("stage-x"),
    );
    // Serialize the lowered nodes (the cache surface).
    let nodes: Vec<DiagNode> = fresh.emit_sorted().into_iter().cloned().collect();
    let bytes = serde_json::to_vec(&nodes).unwrap();
    // The serialized form must not encode an arena DiagRef (no NonZeroU32 handle).
    // DiagRef is not Serialize, so this is structural; assert the JSON has the
    // content-address edge shape, and round-trips identically.
    let replayed_nodes: Vec<DiagNode> = serde_json::from_slice(&bytes).unwrap();
    let mut replayed = DiagLedger::new();
    replayed.replay(replayed_nodes);
    let replay_bytes = serde_json::to_vec(
        &replayed
            .emit_sorted()
            .into_iter()
            .cloned()
            .collect::<Vec<_>>(),
    )
    .unwrap();
    assert_eq!(
        bytes, replay_bytes,
        "fresh and replayed must be byte-identical"
    );
}

#[test]
#[should_panic(expected = "contradicts its pinned digest")]
fn node_contradicting_its_pinned_digest_is_a_hard_fail() {
    // F1: hand-build a node whose stored fingerprint does not match its content.
    let mut ledger = DiagLedger::new();
    let good = DiagFingerprint::compute(
        "test.ledger.f1",
        FindingCategory::DataShapeViolation,
        &SourceContext::default(),
    );
    // Flip a byte so the stored fingerprint contradicts the identity fields.
    let mut wrong = good;
    wrong.0[0] ^= 0xff;
    let node = DiagNode {
        fingerprint: wrong,
        stage: StageId::new("s"),
        grade: Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
        code: "test.ledger.f1".to_owned(),
        observations: vec![Observation {
            message: "x".to_owned(),
            observed: None,
            expected: None,
        }],
        frames: Vec::new(),
        antecedents: Box::new([]),
        source_ctx: SourceContext::default(),
        attributions: Vec::new(),
        advice: Vec::new(),
        remediation: Vec::new(),
        guidance: Vec::new(),
        derived_from_quads: Vec::new(),
        labels: Vec::new(),
        tags: Vec::new(),
        documented_terms: Vec::new(),
        failure_class: None,
        knowledge: Belnap::Supported,
        emitted_at: SerLocation {
            file: "x".to_owned(),
            line: 1,
            column: 1,
        },
        locus_stage: None,
    };
    ledger.replay([node]);
}

#[test]
#[should_panic(expected = "cycle in diagnostic DAG")]
fn self_referential_antecedent_is_a_hard_fail() {
    // R7: a node whose antecedent edge is its own fingerprint.
    let mut ledger = DiagLedger::new();
    let ctx = SourceContext::default();
    let fp = DiagFingerprint::compute("test.ledger.r7", FindingCategory::DataShapeViolation, &ctx);
    let node = DiagNode {
        fingerprint: fp,
        stage: StageId::new("s"),
        grade: Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ),
        code: "test.ledger.r7".to_owned(),
        observations: vec![Observation {
            message: "x".to_owned(),
            observed: None,
            expected: None,
        }],
        frames: Vec::new(),
        antecedents: Box::new([fp]), // points at itself
        source_ctx: ctx,
        attributions: Vec::new(),
        advice: Vec::new(),
        remediation: Vec::new(),
        guidance: Vec::new(),
        derived_from_quads: Vec::new(),
        labels: Vec::new(),
        tags: Vec::new(),
        documented_terms: Vec::new(),
        failure_class: None,
        knowledge: Belnap::Supported,
        emitted_at: SerLocation {
            file: "x".to_owned(),
            line: 1,
            column: 1,
        },
        locus_stage: None,
    };
    // The attach/checking path (not the trusted replay path) runs the
    // acyclicity walk, so drive `insert` directly.
    ledger.insert(node);
}

#[test]
fn hash_cons_merge_of_grades_is_order_independent() {
    // The real determinism fix: two witnesses at one anchor (same code, category
    // and location => same fingerprint) merge their severity/standpoint by the
    // ⊑_t lattice join, so the surviving grade does not depend on attach order.
    // (Cross-node contradiction between DIFFERENT-category findings at one
    // location is a reasoner meta-finding, not a hash-cons merge — the ledger
    // never merges different fingerprints.)
    let build = |loud_first: bool| {
        let mut l = DiagLedger::new();
        let loud = diag_at(
            "test.ledger.join",
            FindingCategory::DataShapeViolation,
            "j.ttl",
            "loud",
        )
        .with_grade(Grade::new(
            Severity::Error,
            FindingCategory::DataShapeViolation,
            Standpoint::Binding,
        ));
        let quiet = diag_at(
            "test.ledger.join",
            FindingCategory::DataShapeViolation,
            "j.ttl",
            "quiet",
        )
        .with_grade(Grade::new(
            Severity::Warning,
            FindingCategory::DataShapeViolation,
            Standpoint::Advisory,
        ));
        if loud_first {
            l.attach(loud, StageId::new("s"));
            l.attach(quiet, StageId::new("s"));
        } else {
            l.attach(quiet, StageId::new("s"));
            l.attach(loud, StageId::new("s"));
        }
        l.emit_sorted()[0].grade
    };
    let a = build(true);
    let b = build(false);
    assert_eq!(a, b, "merged grade must be attach-order independent");
    // severity join = Error (max), standpoint join = Binding (max) => still gates.
    assert_eq!(a.severity, Severity::Error);
    assert_eq!(a.standpoint, Standpoint::Binding);
    assert_eq!(gate(a), GateVerdict::Fatal);
}

#[test]
fn cross_stage_merge_keeps_emit_sorted_attach_order_independent() {
    // The stated Hard Invariant: the same witness (same code/category/anchor =>
    // same fingerprint) attached at two DIFFERENT stages must yield a
    // byte-identical `(stage, fingerprint)` order regardless of which stage
    // attached first — stage is merged to the lexicographic minimum, not
    // pinned by the first writer.
    let build = |b_first: bool| {
        let mut l = DiagLedger::new();
        let first = diag_at(
            "test.ledger.stage-merge",
            FindingCategory::DataShapeViolation,
            "s.ttl",
            "first",
        );
        let second = diag_at(
            "test.ledger.stage-merge",
            FindingCategory::DataShapeViolation,
            "s.ttl",
            "second",
        );
        if b_first {
            l.attach(first, StageId::new("stage-b"));
            l.attach(second, StageId::new("stage-a"));
        } else {
            l.attach(first, StageId::new("stage-a"));
            l.attach(second, StageId::new("stage-b"));
        }
        l
    };
    let ledger_b_first = build(true);
    let ledger_a_first = build(false);
    // One node either way (content address is identity).
    assert_eq!(ledger_b_first.len(), 1);
    assert_eq!(ledger_a_first.len(), 1);
    // The emitted `(stage, fingerprint)` sequence is byte-identical.
    let key = |l: &DiagLedger| -> Vec<(String, DiagFingerprint)> {
        l.emit_sorted()
            .into_iter()
            .map(|n| (n.stage.as_str().to_owned(), n.fingerprint))
            .collect()
    };
    assert_eq!(
        key(&ledger_b_first),
        key(&ledger_a_first),
        "emit_sorted must be attach-order independent"
    );
    // And the surviving stage is the lexicographic minimum in both.
    assert_eq!(ledger_b_first.emit_sorted()[0].stage.as_str(), "stage-a");
    assert_eq!(ledger_a_first.emit_sorted()[0].stage.as_str(), "stage-a");
}

#[test]
fn annotate_by_fingerprint_is_idempotent_and_preserves_identity() {
    // D1: a later pass hangs a remediation on an already-interned witness by
    // fingerprint — in place, no merge, no new node — and a second call (or a
    // cache replay) does NOT grow the remediation vec. The content address is
    // unchanged (remediation is not in the fingerprint), so F1 stays valid.
    use crate::diag::Remediation;
    use crate::grade::Standpoint;
    let mut ledger = DiagLedger::new();
    let d = diag_at(
        "test.ledger.annotate",
        FindingCategory::DataShapeViolation,
        "a.ttl",
        "boom",
    );
    ledger.attach(d, StageId::new("s"));
    let fp = ledger.emit_sorted()[0].fingerprint;
    let iri_before = fingerprint_iri(&fp);
    let arena_len_before = ledger.len();

    let rem = Remediation::new("introduce the mediating relator", Standpoint::Advisory);
    // First annotation lands.
    let r1 = ledger.annotate(&fp, rem.clone()).expect("finding present");
    assert_eq!(
        ledger.node_by_fingerprint(&fp).unwrap().remediation.len(),
        1
    );
    // Second identical annotation (or a replay) is a no-op — the vec does not grow.
    let r2 = ledger.annotate(&fp, rem.clone()).expect("still present");
    assert_eq!(r1, r2);
    assert_eq!(
        ledger.node_by_fingerprint(&fp).unwrap().remediation.len(),
        1,
        "idempotent annotate must not grow the vec on replay"
    );
    // Identity is untouched: same fingerprint IRI, same arena size (no new node).
    assert_eq!(fingerprint_iri(&fp), iri_before);
    assert_eq!(ledger.len(), arena_len_before);
    // Annotating an absent finding is None, never a silent create.
    let absent = DiagFingerprint::compute(
        "test.ledger.absent",
        FindingCategory::DataShapeViolation,
        &SourceContext::default(),
    );
    assert!(ledger.annotate(&absent, rem).is_none());
    assert_eq!(ledger.len(), arena_len_before);
}

#[test]
fn anchor_is_code_blind_and_trivial_when_locationless() {
    // D3: the anchor fingerprint drops code+category, so two DIFFERENT-code
    // findings at ONE source position share it (the cross-code join key the
    // same-fingerprint merge cannot make); and a locationless context is a
    // TRIVIAL anchor the cross-node-glut guard excludes.
    use crate::diag::Focus;
    use crate::model::Location;
    let anchored = SourceContext {
        location: Location {
            path: Some("x.ttl".to_owned()),
            ..Location::default()
        },
        focus: Some(Focus("https://ex/f".to_owned())),
        ..SourceContext::default()
    };
    // Different code strings, one anchor.
    let a = DiagFingerprint::compute("code.one", FindingCategory::ContradictionWitness, &anchored);
    let b = DiagFingerprint::compute(
        "code.two",
        FindingCategory::PermittedEpistemicConflict,
        &anchored,
    );
    assert_ne!(
        a, b,
        "different-code fingerprints differ (they key on the code)"
    );
    assert_eq!(
        DiagFingerprint::anchor(&anchored),
        DiagFingerprint::anchor(&anchored),
    );
    // The anchor is code-blind: recomputed from a context differing ONLY in
    // (irrelevant) code — anchor ignores code entirely — it is stable.
    assert!(anchored.is_non_trivial(), "path+focus is a real position");
    assert!(
        anchor_iri(&DiagFingerprint::anchor(&anchored))
            .starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/anchor/")
    );

    // A locationless / focusless context is a TRIVIAL anchor.
    let trivial = SourceContext::default();
    assert!(!trivial.is_non_trivial());
}

#[test]
fn fingerprint_iri_is_stable_and_addressable() {
    let ctx = SourceContext::default();
    let fp = DiagFingerprint::compute("test.ledger.iri", FindingCategory::DataShapeViolation, &ctx);
    let iri = fingerprint_iri(&fp);
    assert!(iri.starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/finding/"));
    assert!(iri.ends_with(&fp.hex()));
}

// --- verdict() fold + CRDT union laws (T2 / T4) ---------------------------

/// A small pool of witness specs. Anchors 0 and 4 SHARE a fingerprint (same
/// code/category/path) so a union across ledgers exercises the hash-cons merge,
/// not just set-union of disjoint nodes. Grades are chosen so the pool contains
/// both Fatal (`gate == Fatal`) and Collected witnesses, so the verdict fold and
/// its homomorphism are non-vacuous.
fn witness_pool() -> Vec<Diag> {
    let g = |sev, cat, sp| Grade::new(sev, cat, sp);
    let specs: [(&'static str, FindingCategory, &str, Grade); 5] = [
        // Fatal: Error + Blocking + Binding.
        (
            "test.ledger.crdt.a",
            FindingCategory::DataShapeViolation,
            "a.ttl",
            g(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
        ),
        // Collected: advisory standpoint never gates.
        (
            "test.ledger.crdt.b",
            FindingCategory::PolicyWarning,
            "b.ttl",
            g(
                Severity::Error,
                FindingCategory::PolicyWarning,
                Standpoint::Advisory,
            ),
        ),
        // Collected: coherent category never gates.
        (
            "test.ledger.crdt.c",
            FindingCategory::PermittedEpistemicConflict,
            "c.ttl",
            g(
                Severity::Error,
                FindingCategory::PermittedEpistemicConflict,
                Standpoint::Binding,
            ),
        ),
        // Collected: non-error severity.
        (
            "test.ledger.crdt.d",
            FindingCategory::ModelingDisciplineViolation,
            "d.ttl",
            g(
                Severity::Warning,
                FindingCategory::ModelingDisciplineViolation,
                Standpoint::Binding,
            ),
        ),
        // Shares the anchor of spec 0 (same code/category/path) — hash-cons merge.
        (
            "test.ledger.crdt.a",
            FindingCategory::DataShapeViolation,
            "a.ttl",
            g(
                Severity::Warning,
                FindingCategory::DataShapeViolation,
                Standpoint::Advisory,
            ),
        ),
    ];
    specs
        .into_iter()
        .map(|(code, cat, path, grade)| diag_at(code, cat, path, "msg").with_grade(grade))
        .collect()
}

/// Build a ledger holding exactly the witnesses at `indices` from the pool.
fn ledger_from(indices: &[usize]) -> DiagLedger {
    let pool = witness_pool();
    let mut l = DiagLedger::new();
    for &i in indices {
        // Clone the spec by rebuilding from the pool each time (Diag is not Clone).
        let d = pool_diag(&pool, i);
        l.attach(d, StageId::new("s"));
    }
    l
}

/// Rebuild the pool witness at `i` (Diag has no Clone; the pool is cheap).
fn pool_diag(_pool: &[Diag], i: usize) -> Diag {
    witness_pool().into_iter().nth(i).expect("pool index")
}

/// Byte-serialize a ledger's deterministic node sequence — two ledgers are
/// "equal as state" iff these bytes match.
fn state_bytes(l: &DiagLedger) -> Vec<u8> {
    serde_json::to_vec(&l.emit_sorted().into_iter().cloned().collect::<Vec<_>>()).unwrap()
}

#[test]
fn verdict_is_the_join_fold_of_gate_over_every_witness() {
    // Over every subset of the pool, verdict() equals the manual gate-join fold.
    for mask in 0u32..(1 << 5) {
        let idx: Vec<usize> = (0..5).filter(|b| mask & (1 << b) != 0).collect();
        let l = ledger_from(&idx);
        let expected = l
            .emit_sorted()
            .iter()
            .map(|n| gate(n.grade))
            .fold(GateVerdict::Collected, GateVerdict::join);
        assert_eq!(l.verdict(), expected, "verdict fold mismatch for {idx:?}");
    }
    // Non-vacuity: the empty ledger is Collected, and a ledger with the Fatal
    // witness (spec 0) is Fatal.
    assert_eq!(DiagLedger::new().verdict(), GateVerdict::Collected);
    assert_eq!(ledger_from(&[0]).verdict(), GateVerdict::Fatal);
    assert_eq!(ledger_from(&[1, 2, 3]).verdict(), GateVerdict::Collected);
}

#[test]
fn union_is_commutative_associative_and_idempotent() {
    // Exhaustive over all pairs/triples of subsets of a 3-element index space —
    // small, but a genuine proof over the chosen carrier, not a sample.
    let subsets: Vec<Vec<usize>> = (0u32..(1 << 3))
        .map(|m| (0..3).filter(|b| m & (1 << b) != 0).collect())
        .collect();
    for a in &subsets {
        let la = ledger_from(a);
        // Idempotence: a ∪ a == a.
        let mut aa = ledger_from(a);
        aa.union(&la);
        assert_eq!(
            state_bytes(&aa),
            state_bytes(&la),
            "union idempotence {a:?}"
        );
        for b in &subsets {
            let lb = ledger_from(b);
            // Commutativity: a ∪ b == b ∪ a (byte-identical state).
            let mut ab = ledger_from(a);
            ab.union(&lb);
            let mut ba = ledger_from(b);
            ba.union(&la);
            assert_eq!(
                state_bytes(&ab),
                state_bytes(&ba),
                "union not commutative for {a:?} / {b:?}"
            );
            for c in &subsets {
                let lc = ledger_from(c);
                // Associativity: (a ∪ b) ∪ c == a ∪ (b ∪ c).
                let mut left = ledger_from(a);
                left.union(&lb);
                left.union(&lc);
                let mut bc = ledger_from(b);
                bc.union(&lc);
                let mut right = ledger_from(a);
                right.union(&bc);
                assert_eq!(
                    state_bytes(&left),
                    state_bytes(&right),
                    "union not associative for {a:?} / {b:?} / {c:?}"
                );
            }
        }
    }
}

#[test]
fn verdict_is_a_semilattice_homomorphism_over_union() {
    // The load-bearing theorem: verdict(a ∪ b) == verdict(a) ⊔ verdict(b) for
    // every pair of ledgers — so folding stage sub-ledgers in parallel and
    // joining their verdicts equals the verdict of the whole. Exhaustive over
    // all subset pairs of the full 5-element pool.
    let subsets: Vec<Vec<usize>> = (0u32..(1 << 5))
        .map(|m| (0..5).filter(|b| m & (1 << b) != 0).collect())
        .collect();
    for a in &subsets {
        for b in &subsets {
            let mut ab = ledger_from(a);
            ab.union(&ledger_from(b));
            let joined = ledger_from(a).verdict().join(ledger_from(b).verdict());
            assert_eq!(
                ab.verdict(),
                joined,
                "verdict homomorphism broken for {a:?} / {b:?}"
            );
        }
    }
}
