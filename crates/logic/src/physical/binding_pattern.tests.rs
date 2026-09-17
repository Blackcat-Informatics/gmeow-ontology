// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// The 8 arity-3 patterns (every subset of `{0,1,2}`), for exhaustive lattice-law
/// coverage.
fn all_arity3() -> Vec<BindingPattern> {
    (0..8u8)
        .map(|m| BindingPattern::from_bools((0..3).map(|i| (m >> i) & 1 == 1)))
        .collect()
}

#[test]
fn arity_and_is_bound_agree_with_constructors() {
    let p = BindingPattern::from_bools([true, false, true]);
    assert_eq!(p.arity(), 3);
    assert!(p.is_bound(0));
    assert!(!p.is_bound(1));
    assert!(p.is_bound(2));
    assert!(!p.is_bound(3), "out-of-range position is never bound");

    let q = BindingPattern::from_bound_positions(3, [0, 2]);
    assert_eq!(p, q, "from_bools and from_bound_positions agree");
    assert_eq!(
        q.bound_positions().collect::<Vec<_>>(),
        vec![0, 2],
        "bound_positions ascending"
    );
}

#[test]
fn is_all_free_only_when_nothing_bound() {
    assert!(BindingPattern::from_bools([false, false, false]).is_all_free());
    assert!(!BindingPattern::from_bools([false, true, false]).is_all_free());
}

#[test]
fn subsumes_is_reflexive() {
    for p in all_arity3() {
        assert!(p.subsumes(&p), "reflexive: {} ⊑ {}", p.code(), p.code());
    }
}

#[test]
fn subsumes_is_antisymmetric() {
    for a in all_arity3() {
        for b in all_arity3() {
            if a.subsumes(&b) && b.subsumes(&a) {
                assert_eq!(a, b, "antisymmetry: {} ⊑⊒ {} ⇒ equal", a.code(), b.code());
            }
        }
    }
}

#[test]
fn subsumes_is_transitive() {
    for a in all_arity3() {
        for b in all_arity3() {
            for c in all_arity3() {
                if a.subsumes(&b) && b.subsumes(&c) {
                    assert!(
                        a.subsumes(&c),
                        "transitivity: {} ⊑ {} ⊑ {}",
                        a.code(),
                        b.code(),
                        c.code()
                    );
                }
            }
        }
    }
}

#[test]
fn all_free_subsumes_everything_top_subsumes_nothing_else() {
    let bottom = BindingPattern::from_bound_positions(3, []);
    let top = BindingPattern::from_bound_positions(3, [0, 1, 2]);
    for p in all_arity3() {
        assert!(bottom.subsumes(&p), "⊥ (all-free) is the most general");
        assert!(p.subsumes(&top), "⊤ (all-bound) is the most specific");
    }
}

#[test]
fn meet_is_bound_set_intersection_and_a_lower_bound() {
    for a in all_arity3() {
        for b in all_arity3() {
            let m = a.meet(&b);
            assert_eq!(m.bound, a.bound & b.bound, "meet = bound-set intersection");
            // A lower bound under ⊑: m ⊑ a and m ⊑ b.
            assert!(m.subsumes(&a), "meet ⊑ a");
            assert!(m.subsumes(&b), "meet ⊑ b");
            // Greatest such: any common lower bound l ⊑ a, l ⊑ b has l ⊑ m.
            for l in all_arity3() {
                if l.subsumes(&a) && l.subsumes(&b) {
                    assert!(l.subsumes(&m), "meet is the GREATEST lower bound");
                }
            }
        }
    }
}

#[test]
fn join_is_bound_set_union_and_an_upper_bound() {
    for a in all_arity3() {
        for b in all_arity3() {
            let j = a.join(&b);
            assert_eq!(j.bound, a.bound | b.bound, "join = bound-set union");
            // An upper bound under ⊑: a ⊑ j and b ⊑ j.
            assert!(a.subsumes(&j), "a ⊑ join");
            assert!(b.subsumes(&j), "b ⊑ join");
            // Least such: any common upper bound u ⊒ a, u ⊒ b has m ⊑ u.
            for u in all_arity3() {
                if a.subsumes(&u) && b.subsumes(&u) {
                    assert!(j.subsumes(&u), "join is the LEAST upper bound");
                }
            }
        }
    }
}

#[test]
fn meet_and_join_are_commutative() {
    for a in all_arity3() {
        for b in all_arity3() {
            assert_eq!(a.meet(&b), b.meet(&a), "meet commutes");
            assert_eq!(a.join(&b), b.join(&a), "join commutes");
        }
    }
}

#[test]
fn absorption_laws() {
    for a in all_arity3() {
        for b in all_arity3() {
            assert_eq!(a.meet(&a.join(&b)), a, "a ∧ (a ∨ b) = a");
            assert_eq!(a.join(&a.meet(&b)), a, "a ∨ (a ∧ b) = a");
        }
    }
}

#[test]
#[should_panic(expected = "meet requires equal arity")]
fn meet_arity_mismatch_panics() {
    let a = BindingPattern::from_bools([true, false]);
    let b = BindingPattern::from_bools([true, false, true]);
    let _ = a.meet(&b);
}

#[test]
fn code_round_trips_arity2() {
    for code in ["bb", "bf", "fb", "ff"] {
        let p = BindingPattern::from_code(code);
        assert_eq!(p.arity(), 2);
        assert_eq!(p.code(), code, "arity-2 code round-trips");
    }
}

#[test]
fn code_round_trips_arity3() {
    for m in 0..8u8 {
        let p = BindingPattern::from_bools((0..3).map(|i| (m >> i) & 1 == 1));
        let round = BindingPattern::from_code(&p.code());
        assert_eq!(p, round, "arity-3 code round-trips");
    }
}

#[test]
fn arity2_codes_are_the_legacy_binary_adornments() {
    // The exact legacy Adorn::code mapping — critical for magic-IRI stability
    // (the binary regression tests depend on byte-identical magic predicate IRIs).
    assert_eq!(BindingPattern::from_bound_positions(2, [0, 1]).code(), "bb");
    assert_eq!(BindingPattern::from_bound_positions(2, [0]).code(), "bf");
    assert_eq!(BindingPattern::from_bound_positions(2, [1]).code(), "fb");
    assert_eq!(BindingPattern::from_bound_positions(2, []).code(), "ff");
}
