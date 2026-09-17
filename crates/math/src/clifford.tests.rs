// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn r(value: i128) -> Rational {
    Rational::from_i128(value).expect("integer rational")
}

fn term(blade: BasisBlade, coefficient: i128) -> Multivector {
    Multivector::from_term(blade, r(coefficient))
}

#[test]
fn signatures_pin_cl12_and_positive_cl13_dimensions() {
    for (p, q) in [(12, 0), (6, 6)] {
        let base = CliffordAlgebra::new(p, q).expect("Cl12 signature");
        let extension = base.positive_extension().expect("positive extension");
        assert_eq!(base.dimension(), 4096);
        assert_eq!(extension.dimension(), 8192);
        assert_eq!(extension.signature().positive(), p + 1);
        assert_eq!(extension.signature().negative(), q);
    }
    assert!(CliffordAlgebra::new(65, 0).is_err());
}

#[test]
fn generator_squares_and_anticommutation_follow_the_signature() {
    let algebra = CliffordAlgebra::new(2, 1).expect("Cl(2,1)");
    let positive = algebra.generator(0).expect("positive generator");
    let other_positive = algebra.generator(1).expect("positive generator");
    let negative = algebra.generator(2).expect("negative generator");

    assert_eq!(
        algebra
            .geometric_product_blades(negative, negative)
            .expect("negative square"),
        SignedBlade::new(-1, BasisBlade::scalar())
    );
    assert_eq!(
        algebra
            .geometric_product_blades(positive, positive)
            .expect("positive square"),
        SignedBlade::new(1, BasisBlade::scalar())
    );
    let forward = algebra
        .geometric_product_blades(positive, other_positive)
        .expect("forward product");
    let reverse = algebra
        .geometric_product_blades(other_positive, positive)
        .expect("reverse product");
    assert_eq!(forward.blade(), reverse.blade());
    assert_eq!(forward.sign(), -reverse.sign());
}

#[test]
fn exterior_product_and_left_contraction_have_exact_blade_rules() {
    let algebra = CliffordAlgebra::new(3, 0).expect("Cl(3,0)");
    let e1 = algebra.generator(0).expect("e1");
    let e2 = algebra.generator(1).expect("e2");
    let e12 = algebra.blade(e1.mask() | e2.mask()).expect("e12");

    assert!(
        algebra
            .exterior_product_blades(e1, e1)
            .expect("wedge")
            .is_none()
    );
    assert_eq!(
        algebra
            .exterior_product_blades(e1, e2)
            .expect("wedge")
            .expect("nonzero")
            .blade(),
        e12
    );
    assert_eq!(
        algebra
            .left_contraction_blades(e1, e12)
            .expect("contraction")
            .expect("nonzero")
            .blade(),
        e2
    );
    assert!(
        algebra
            .left_contraction_blades(e12, e1)
            .expect("contraction")
            .is_none()
    );
}

#[test]
fn sparse_geometric_product_is_distributive() {
    let algebra = CliffordAlgebra::new(3, 0).expect("Cl(3,0)");
    let e1 = term(algebra.generator(0).expect("e1"), 1);
    let e2 = term(algebra.generator(1).expect("e2"), 1);
    let e3 = term(algebra.generator(2).expect("e3"), 1);
    let sum = e2.checked_add(&e3).expect("sum");

    let left = algebra.geometric_product(&e1, &sum).expect("a(b+c)");
    let right = algebra
        .geometric_product(&e1, &e2)
        .expect("ab")
        .checked_add(&algebra.geometric_product(&e1, &e3).expect("ac"))
        .expect("ab+ac");
    assert_eq!(left, right);
}

#[test]
fn grade_projection_and_involutions_are_exact() {
    let algebra = CliffordAlgebra::new(3, 0).expect("Cl(3,0)");
    let scalar = BasisBlade::scalar();
    let e1 = algebra.generator(0).expect("e1");
    let e12 = algebra.blade(0b011).expect("e12");
    let e123 = algebra.blade(0b111).expect("e123");
    let value = Multivector::from_terms([(scalar, r(1)), (e1, r(2)), (e12, r(3)), (e123, r(4))])
        .expect("multivector");

    assert_eq!(
        algebra.grade_projection(&value, 2).expect("grade 2"),
        term(e12, 3)
    );
    let reverse = value.reversion().expect("reversion");
    assert_eq!(reverse.coefficient(e1), r(2));
    assert_eq!(reverse.coefficient(e12), r(-3));
    assert_eq!(reverse.coefficient(e123), r(-4));
    let grade = value.grade_involution().expect("grade involution");
    assert_eq!(grade.coefficient(e1), r(-2));
    assert_eq!(grade.coefficient(e12), r(3));
    assert_eq!(grade.coefficient(e123), r(-4));
    let conjugate = value.clifford_conjugation().expect("conjugation");
    assert_eq!(conjugate.coefficient(e1), r(-2));
    assert_eq!(conjugate.coefficient(e12), r(-3));
    assert_eq!(conjugate.coefficient(e123), r(4));
}

#[test]
fn cl12_cl13_positive_extension_split_is_exact_for_both_signatures() {
    for (p, q) in [(12, 0), (6, 6)] {
        let base = CliffordAlgebra::new(p, q).expect("base");
        let extension = base.positive_extension().expect("extension");
        let a = Multivector::from_terms([
            (BasisBlade::scalar(), r(3)),
            (base.generator(0).expect("base e1"), r(2)),
        ])
        .expect("a");
        let b = Multivector::from_terms([
            (BasisBlade::scalar(), r(-5)),
            (base.generator(1).expect("base e2"), r(7)),
        ])
        .expect("b");

        let joined = extension
            .join_positive_extension(&a, &b)
            .expect("embed(a) + e_(p+1) embed(b)");
        let (split_a, split_b) = extension.split_positive_extension(&joined).expect("split");
        assert_eq!(split_a, a);
        assert_eq!(split_b, b);
        assert_eq!(
            extension
                .join_positive_extension(&split_a, &split_b)
                .expect("rejoin"),
            joined
        );
    }
}

#[test]
fn pseudoscalar_squares_are_calculated_for_both_cl12_and_cl13_families() {
    for (p, q) in [(12, 0), (6, 6), (13, 0), (7, 6)] {
        let algebra = CliffordAlgebra::new(p, q).expect("signature");
        assert_eq!(algebra.pseudoscalar_square().expect("I^2"), 1);
    }
}

#[test]
fn rank_zero_and_rank_sixty_four_have_exact_dimensions_without_shift_overflow() {
    let scalars = CliffordAlgebra::new(0, 0).expect("Cl(0,0)");
    assert_eq!(scalars.dimension(), 1);
    assert_eq!(
        scalars.pseudoscalar().expect("scalar pseudoscalar"),
        BasisBlade::scalar()
    );
    assert!(
        scalars
            .signature()
            .without_last_positive_generator()
            .is_err()
    );

    let rank_64 = CliffordAlgebra::new(32, 32).expect("Cl(32,32)");
    assert_eq!(rank_64.dimension(), 1_u128 << 64);
    assert_eq!(
        rank_64.pseudoscalar().expect("rank-64 pseudoscalar").mask(),
        u64::MAX
    );
    assert_eq!(rank_64.signature().generator_square(31).expect("e32"), 1);
    assert_eq!(rank_64.signature().generator_square(32).expect("e33"), -1);
}

#[test]
fn positive_extension_embeds_and_shifts_the_negative_block() {
    let base = CliffordAlgebra::new(2, 2).expect("Cl(2,2)");
    let extension = base.positive_extension().expect("Cl(3,2)");
    let old_last_positive = term(base.generator(1).expect("old positive"), 2);
    let old_first_negative = term(base.generator(2).expect("old negative"), 3);
    let value = old_last_positive
        .checked_add(&old_first_negative)
        .expect("base value");
    let embedded = extension
        .embed_positive_extension(&value)
        .expect("embedded value");

    assert_eq!(
        embedded.coefficient(extension.generator(1).expect("same positive")),
        r(2)
    );
    assert_eq!(
        embedded.coefficient(extension.generator(3).expect("shifted negative")),
        r(3)
    );
    assert_eq!(
        extension
            .signature()
            .generator_square(2)
            .expect("new positive"),
        1
    );
    assert_eq!(
        extension
            .signature()
            .generator_square(3)
            .expect("first negative"),
        -1
    );
}

#[test]
fn sparse_terms_combine_delete_zero_and_render_deterministically() {
    let algebra = CliffordAlgebra::new(2, 0).expect("Cl(2,0)");
    let e1 = algebra.generator(0).expect("e1");
    let e2 = algebra.generator(1).expect("e2");
    let value = Multivector::from_terms([
        (e2, Rational::new(-3, 4).expect("-3/4")),
        (e1, r(2)),
        (e1, r(-2)),
        (BasisBlade::scalar(), r(5)),
    ])
    .expect("canonical sparse value");
    assert_eq!(value.term_count(), 2);
    assert_eq!(
        value
            .terms()
            .map(|(blade, _)| blade.mask())
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(value.to_string(), "5 + -3/4*e2");
    assert_eq!(value.to_string(), value.to_string());
}

#[test]
fn invalid_grade_and_checked_coefficient_overflow_hard_fail() {
    let algebra = CliffordAlgebra::new(2, 1).expect("Cl(2,1)");
    let value = Multivector::scalar(Rational::one());
    assert!(algebra.grade_projection(&value, 4).is_err());

    let scalar = BasisBlade::scalar();
    assert!(
        Multivector::from_terms([
            (scalar, Rational::from_i128(i128::MAX).expect("max")),
            (scalar, Rational::one()),
        ])
        .is_err()
    );
}

#[test]
fn geometric_product_is_associative_and_bilinear_on_exact_multivectors() {
    let algebra = CliffordAlgebra::new(2, 1).expect("Cl(2,1)");
    let one = Multivector::scalar(Rational::one());
    let e1 = term(algebra.generator(0).expect("e1"), 2);
    let e2 = term(algebra.generator(1).expect("e2"), -3);
    let e3 = term(algebra.generator(2).expect("e3"), 5);
    let a = one.checked_add(&e1).expect("a");
    let b = e2.checked_add(&e3).expect("b");
    let c = one.checked_sub(&e3).expect("c");

    let ab_c = algebra
        .geometric_product(&algebra.geometric_product(&a, &b).expect("ab"), &c)
        .expect("(ab)c");
    let a_bc = algebra
        .geometric_product(&a, &algebra.geometric_product(&b, &c).expect("bc"))
        .expect("a(bc)");
    assert_eq!(ab_c, a_bc);

    let left = algebra
        .geometric_product(&a.checked_add(&b).expect("a+b"), &c)
        .expect("(a+b)c");
    let left_expanded = algebra
        .geometric_product(&a, &c)
        .expect("ac")
        .checked_add(&algebra.geometric_product(&b, &c).expect("bc"))
        .expect("ac+bc");
    assert_eq!(left, left_expanded);

    let right = algebra
        .geometric_product(&a, &b.checked_add(&c).expect("b+c"))
        .expect("a(b+c)");
    let right_expanded = algebra
        .geometric_product(&a, &b)
        .expect("ab")
        .checked_add(&algebra.geometric_product(&a, &c).expect("ac"))
        .expect("ab+ac");
    assert_eq!(right, right_expanded);
}

#[test]
fn involutions_obey_their_automorphism_and_anti_automorphism_laws() {
    let algebra = CliffordAlgebra::new(2, 1).expect("Cl(2,1)");
    let a = Multivector::from_terms([
        (BasisBlade::scalar(), r(2)),
        (algebra.generator(0).expect("e1"), r(3)),
        (algebra.blade(0b110).expect("e2e3"), r(-1)),
    ])
    .expect("a");
    let b = Multivector::from_terms([
        (algebra.generator(1).expect("e2"), r(5)),
        (algebra.blade(0b111).expect("e123"), r(2)),
    ])
    .expect("b");
    let ab = algebra.geometric_product(&a, &b).expect("ab");

    let reverse_ab = ab.reversion().expect("reverse(ab)");
    let reverse_ba = algebra
        .geometric_product(
            &b.reversion().expect("reverse(b)"),
            &a.reversion().expect("reverse(a)"),
        )
        .expect("reverse(b)reverse(a)");
    assert_eq!(reverse_ab, reverse_ba);

    let grade_ab = ab.grade_involution().expect("grade(ab)");
    let grade_product = algebra
        .geometric_product(
            &a.grade_involution().expect("grade(a)"),
            &b.grade_involution().expect("grade(b)"),
        )
        .expect("grade(a)grade(b)");
    assert_eq!(grade_ab, grade_product);

    let conjugate_ab = ab.clifford_conjugation().expect("conjugate(ab)");
    let conjugate_ba = algebra
        .geometric_product(
            &b.clifford_conjugation().expect("conjugate(b)"),
            &a.clifford_conjugation().expect("conjugate(a)"),
        )
        .expect("conjugate(b)conjugate(a)");
    assert_eq!(conjugate_ab, conjugate_ba);
}

fn reference_product(signature: CliffordSignature, left: u64, right: u64) -> (i8, u64) {
    let mut sign = 1_i8;
    let mut blade = left;
    for index in 0..signature.generators() {
        let bit = 1_u64 << index;
        if right & bit == 0 {
            continue;
        }
        if (blade >> (index + 1)).count_ones() % 2 == 1 {
            sign = -sign;
        }
        if blade & bit == 0 {
            blade |= bit;
        } else {
            blade ^= bit;
            sign *= signature.generator_square(index).expect("reference metric");
        }
    }
    (sign, blade)
}

#[test]
fn blade_kernel_matches_an_exhaustive_small_algebra_reference() {
    for (p, q) in [(0, 0), (1, 0), (0, 1), (2, 1), (2, 2)] {
        let algebra = CliffordAlgebra::new(p, q).expect("small algebra");
        let blade_count = 1_u64 << algebra.signature().generators();
        for left in 0..blade_count {
            for right in 0..blade_count {
                let actual = algebra
                    .geometric_product_blades(
                        algebra.blade(left).expect("left blade"),
                        algebra.blade(right).expect("right blade"),
                    )
                    .expect("blade product");
                let expected = reference_product(algebra.signature(), left, right);
                assert_eq!((actual.sign(), actual.blade().mask()), expected);
            }
        }
    }
}

#[test]
fn blades_outside_the_signature_hard_fail() {
    let algebra = CliffordAlgebra::new(3, 0).expect("Cl(3,0)");
    assert!(algebra.blade(0b1000).is_err());
    assert!(algebra.generator(3).is_err());
    assert!(BasisBlade::new(u64::MAX, 63).is_err());

    let foreign = BasisBlade::new(0b1000, 4).expect("Cl4 blade");
    let invalid = Multivector::from_term(foreign, Rational::one());
    assert!(
        algebra
            .geometric_product(&invalid, &Multivector::scalar(Rational::one()))
            .is_err()
    );
}
