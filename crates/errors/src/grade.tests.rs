// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Exhaustively check the bounded-lattice laws over a finite carrier. This is
/// a proof over the whole domain, not a sampled property test.
fn assert_lattice_laws<L: BoundedLattice + std::fmt::Debug>(all: &[L]) {
    for &a in all {
        // Idempotence.
        assert_eq!(a.join(a), a, "join idempotence");
        assert_eq!(a.meet(a), a, "meet idempotence");
        // Identities with BOTTOM/TOP.
        assert_eq!(a.join(L::BOTTOM), a, "join bottom identity");
        assert_eq!(a.meet(L::TOP), a, "meet top identity");
        assert_eq!(a.join(L::TOP), L::TOP, "join top absorbs");
        assert_eq!(a.meet(L::BOTTOM), L::BOTTOM, "meet bottom absorbs");
        for &b in all {
            // Commutativity.
            assert_eq!(a.join(b), b.join(a), "join commutative");
            assert_eq!(a.meet(b), b.meet(a), "meet commutative");
            // Absorption.
            assert_eq!(a.join(a.meet(b)), a, "absorption join/meet");
            assert_eq!(a.meet(a.join(b)), a, "absorption meet/join");
            // leq consistency: a ⊑ b  ⇔  join == b  ⇔  meet == a.
            assert_eq!(a.leq(b), a.join(b) == b, "leq via join");
            assert_eq!(a.leq(b), a.meet(b) == a, "leq via meet");
            for &c in all {
                // Associativity.
                assert_eq!(a.join(b).join(c), a.join(b.join(c)), "join associative");
                assert_eq!(a.meet(b).meet(c), a.meet(b.meet(c)), "meet associative");
            }
        }
    }
}

#[test]
fn truth_axis_lattices_obey_the_laws() {
    assert_lattice_laws(&Severity::ALL);
    assert_lattice_laws(&Standpoint::ALL);
    assert_lattice_laws(&Blocking::ALL);
    assert_lattice_laws(&GateVerdict::ALL);
}

#[test]
fn knowledge_axis_belnap_obeys_the_laws() {
    assert_lattice_laws(&Belnap::ALL);
}

/// Every grade in the finite bilattice, for exhaustive gate/merge tests.
fn all_grades() -> Vec<Grade> {
    let mut out = Vec::new();
    for &s in &Severity::ALL {
        for &c in &FindingCategory::ALL {
            for &p in &Standpoint::ALL {
                out.push(Grade::new(s, c, p));
            }
        }
    }
    out
}

#[test]
fn gate_is_monotone_over_the_truth_order() {
    // g1 ⊑_t g2  ⇒  gate(g1) ⊑ gate(g2), over every ordered pair.
    for &g1 in &all_grades() {
        for &g2 in &all_grades() {
            if g1.leq_truth(g2) {
                assert!(
                    gate(g1).leq(gate(g2)),
                    "gate not monotone: {g1:?} ⊑_t {g2:?} but gate {:?} ⋢ {:?}",
                    gate(g1),
                    gate(g2)
                );
            }
        }
    }
}

#[test]
fn advisory_and_permitted_conflict_never_gate() {
    for &g in &all_grades() {
        if g.standpoint == Standpoint::Advisory {
            assert_eq!(
                gate(g),
                GateVerdict::Collected,
                "advisory must not gate: {g:?}"
            );
        }
        if g.category == FindingCategory::PermittedEpistemicConflict {
            assert_eq!(
                gate(g),
                GateVerdict::Collected,
                "permitted epistemic conflict must not gate: {g:?}"
            );
        }
    }
}

#[test]
fn gate_fatal_exactly_on_the_principal_up_set() {
    for &g in &all_grades() {
        let expected = g.severity == Severity::Error
            && g.category.blocking() == Blocking::Blocking
            && g.standpoint == Standpoint::Binding;
        assert_eq!(
            gate(g) == GateVerdict::Fatal,
            expected,
            "gate fatal region mismatch at {g:?}"
        );
    }
}

#[test]
fn transient_chatter_never_gates_and_takes_no_stance() {
    // The closed chatter kind is non-gating at EVERY severity and standpoint,
    // and carries no coherence stance — it is transient bookkeeping only.
    assert_eq!(FindingCategory::Transient.blocking(), Blocking::Coherent);
    assert_eq!(FindingCategory::Transient.polarity(), Belnap::Neither);
    for &sev in &Severity::ALL {
        for &standpoint in &Standpoint::ALL {
            let g = Grade::new(sev, FindingCategory::Transient, standpoint);
            assert_ne!(
                gate(g),
                GateVerdict::Fatal,
                "transient chatter must never gate: {g:?}"
            );
        }
    }
}

#[test]
fn merge_is_order_independent() {
    // merge(a,b).grade == merge(b,a).grade for every pair — hash-cons cannot
    // depend on which shard folded first. Knowledge join is commutative too.
    for &a in &all_grades() {
        for &b in &all_grades() {
            let ab = a.merge(b);
            let ba = b.merge(a);
            assert_eq!(ab.grade.severity, ba.grade.severity, "severity merge order");
            assert_eq!(
                ab.grade.standpoint, ba.grade.standpoint,
                "standpoint merge order"
            );
            assert_eq!(ab.grade.category, ba.grade.category, "category merge order");
            assert_eq!(ab.knowledge, ba.knowledge, "knowledge merge order");
            // Severity/standpoint are the joins of the inputs.
            assert_eq!(ab.grade.severity, a.severity.join(b.severity));
            assert_eq!(ab.grade.standpoint, a.standpoint.join(b.standpoint));
        }
    }
}

#[test]
fn contradictory_pair_merges_to_a_glut_not_to_either_side() {
    let witness = Grade::new(
        Severity::Error,
        FindingCategory::ContradictionWitness,
        Standpoint::Binding,
    );
    let permitted = Grade::new(
        Severity::Note,
        FindingCategory::PermittedEpistemicConflict,
        Standpoint::Perspectival,
    );
    let merged = witness.merge(permitted);
    assert!(
        merged.is_glut(),
        "disagreeing witnesses must produce a glut"
    );
    assert_eq!(merged.knowledge, Belnap::Both);
    // The glut is not either input's polarity taken alone.
    assert_ne!(merged.knowledge, witness.category.polarity());
    assert_ne!(merged.knowledge, permitted.category.polarity());
}

#[test]
fn agreeing_witnesses_do_not_glut() {
    let a = Grade::new(
        Severity::Error,
        FindingCategory::DataShapeViolation,
        Standpoint::Binding,
    );
    let b = Grade::new(
        Severity::Warning,
        FindingCategory::ContradictionWitness,
        Standpoint::Perspectival,
    );
    // Both assert a defect (Supported); their join stays Supported, no glut.
    assert!(!a.merge(b).is_glut());
    assert_eq!(a.merge(b).knowledge, Belnap::Supported);
}

#[test]
fn unknown_severity_token_is_a_hard_fail() {
    // F2: parsing must reject an unknown token, never silently default.
    assert!(Severity::parse("bogus").is_err());
    assert!(Severity::parse("").is_err());
    // Known aliases still resolve (behavior preserved).
    assert_eq!(Severity::parse("fatal").unwrap(), Severity::Error);
    assert_eq!(Severity::parse("warn").unwrap(), Severity::Warning);
}
