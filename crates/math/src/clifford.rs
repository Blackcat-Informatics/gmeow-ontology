// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Exact sparse Clifford algebras over diagonal orthonormal signatures.
//!
//! The kernel represents a basis blade by a `u64` generator mask and a
//! multivector by a deterministically ordered sparse map of exact [`Rational`]
//! coefficients. It therefore supports every `Cl(p,q)` with `p + q <= 64`
//! without floating-point arithmetic, dense `2^n` allocations, or iteration-order
//! nondeterminism. The public basis convention is positive-first: `e_1, …, e_p`
//! square to `+1`, while `e_(p+1), …, e_(p+q)` square to `-1`.
//!
//! A positive extension preserves that convention by inserting the new positive
//! generator after the old positive block and shifting the negative block one
//! place. The exact split is therefore
//! `Cl(p+1,q) = embed(Cl(p,q)) ⊕ e_(p+1) embed(Cl(p,q))` as a vector space (and
//! module), never as a direct sum of algebras.

use std::collections::BTreeMap;
use std::fmt;

use gmeow_errors::{Diag, Result};

use crate::Rational;
use crate::error::{CliffordBladeOutOfRange, CliffordGradeOutOfRange, InvalidCliffordSignature};

/// Maximum number of Clifford generators representable by a basis-blade mask.
pub const MAX_CLIFFORD_GENERATORS: u8 = 64;

/// A real diagonal orthonormal Clifford signature `(p,q)`.
///
/// The first `p` generator indices square to `+1`; the following `q` indices
/// square to `-1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CliffordSignature {
    positive: u8,
    negative: u8,
}

impl CliffordSignature {
    /// Construct `(p,q)`, rejecting signatures that need more than 64 generators.
    pub fn new(positive: u8, negative: u8) -> Result<Self> {
        let generators = positive.checked_add(negative).ok_or_else(|| {
            Diag::of_kind(InvalidCliffordSignature {
                detail: format!(
                    "Cl({positive},{negative}) overflows the signature generator count"
                ),
            })
        })?;
        if generators > MAX_CLIFFORD_GENERATORS {
            return Err(Diag::of_kind(InvalidCliffordSignature {
                detail: format!(
                    "Cl({positive},{negative}) has {generators} generators; the exact mask kernel supports at most {MAX_CLIFFORD_GENERATORS}"
                ),
            }));
        }
        Ok(Self { positive, negative })
    }

    /// Number `p` of generators whose square is `+1`.
    pub fn positive(self) -> u8 {
        self.positive
    }

    /// Number `q` of generators whose square is `-1`.
    pub fn negative(self) -> u8 {
        self.negative
    }

    /// Total number `n = p + q` of generators.
    pub fn generators(self) -> u8 {
        self.positive + self.negative
    }

    /// Vector-space dimension `2^(p+q)` of the algebra.
    pub fn algebra_dimension(self) -> u128 {
        1_u128 << self.generators()
    }

    /// Square of the generator at `index`, under the positive-first convention.
    pub fn generator_square(self, index: u8) -> Result<i8> {
        if index >= self.generators() {
            return Err(Diag::of_kind(CliffordBladeOutOfRange {
                detail: format!(
                    "generator e{} lies outside Cl({},{}) with {} generators",
                    u16::from(index) + 1,
                    self.positive,
                    self.negative,
                    self.generators()
                ),
            }));
        }
        Ok(if index < self.positive { 1 } else { -1 })
    }

    /// Add one positive generator, producing canonical positive-first `Cl(p+1,q)`.
    ///
    /// The exact embedding of the old algebra shifts its negative basis block;
    /// [`CliffordAlgebra::join_positive_extension`] and
    /// [`CliffordAlgebra::split_positive_extension`] implement that embedding.
    pub fn positive_extension(self) -> Result<Self> {
        let positive = self.positive.checked_add(1).ok_or_else(|| {
            Diag::of_kind(InvalidCliffordSignature {
                detail: "positive Clifford index overflow".to_string(),
            })
        })?;
        Self::new(positive, self.negative)
    }

    /// Signature obtained by removing the last positive generator.
    pub fn without_last_positive_generator(self) -> Result<Self> {
        if self.positive > 0 {
            Self::new(self.positive - 1, self.negative)
        } else {
            Err(Diag::of_kind(InvalidCliffordSignature {
                detail: format!(
                    "Cl({},{}) has no positive generator to remove",
                    self.positive, self.negative
                ),
            }))
        }
    }
}

/// A canonical basis blade encoded as the set of generators in its product.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BasisBlade {
    mask: u64,
}

impl BasisBlade {
    /// Construct a blade and verify that its mask uses only declared generators.
    pub fn new(mask: u64, generator_count: u8) -> Result<Self> {
        if generator_count > MAX_CLIFFORD_GENERATORS {
            return Err(Diag::of_kind(InvalidCliffordSignature {
                detail: format!(
                    "basis-blade validation requested {generator_count} generators; maximum is {MAX_CLIFFORD_GENERATORS}"
                ),
            }));
        }
        if generator_count < MAX_CLIFFORD_GENERATORS && mask >= (1_u64 << generator_count) {
            return Err(Diag::of_kind(CliffordBladeOutOfRange {
                detail: format!(
                    "basis-blade mask 0x{mask:016x} uses a generator outside a {generator_count}-generator algebra"
                ),
            }));
        }
        Ok(Self { mask })
    }

    /// The scalar blade `1`.
    pub const fn scalar() -> Self {
        Self { mask: 0 }
    }

    /// Construct the one-generator blade `e_(index+1)`.
    pub fn generator(index: u8, generator_count: u8) -> Result<Self> {
        if index >= generator_count || generator_count > MAX_CLIFFORD_GENERATORS {
            return Err(Diag::of_kind(CliffordBladeOutOfRange {
                detail: format!(
                    "generator index {index} lies outside a {generator_count}-generator algebra"
                ),
            }));
        }
        Self::new(1_u64 << index, generator_count)
    }

    /// Raw generator mask.
    pub const fn mask(self) -> u64 {
        self.mask
    }

    /// Grade (number of generators) of this blade.
    pub const fn grade(self) -> u32 {
        self.mask.count_ones()
    }
}

/// A basis blade with an exact orientation/metric sign (`+1` or `-1`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedBlade {
    sign: i8,
    blade: BasisBlade,
}

impl SignedBlade {
    fn new(sign: i8, blade: BasisBlade) -> Self {
        debug_assert!(matches!(sign, -1 | 1));
        Self { sign, blade }
    }

    /// Product sign (`+1` or `-1`).
    pub const fn sign(self) -> i8 {
        self.sign
    }

    /// Canonical blade after product reordering and metric cancellation.
    pub const fn blade(self) -> BasisBlade {
        self.blade
    }
}

/// A sparse exact multivector in canonical blade order.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Multivector {
    terms: BTreeMap<BasisBlade, Rational>,
}

impl Multivector {
    /// The additive zero multivector.
    pub fn zero() -> Self {
        Self::default()
    }

    /// A scalar multivector.
    pub fn scalar(coefficient: Rational) -> Self {
        Self::from_term(BasisBlade::scalar(), coefficient)
    }

    /// A one-term multivector. An exact zero coefficient is canonicalized away.
    pub fn from_term(blade: BasisBlade, coefficient: Rational) -> Self {
        let mut terms = BTreeMap::new();
        if !coefficient.is_zero() {
            terms.insert(blade, coefficient);
        }
        Self { terms }
    }

    /// Construct from an iterable of terms, combining duplicate blades exactly.
    pub fn from_terms(terms: impl IntoIterator<Item = (BasisBlade, Rational)>) -> Result<Self> {
        let mut out = Self::zero();
        for (blade, coefficient) in terms {
            out.accumulate(blade, coefficient)?;
        }
        Ok(out)
    }

    /// Number of non-zero basis-blade coefficients.
    pub fn term_count(&self) -> usize {
        self.terms.len()
    }

    /// Whether this multivector is exactly zero.
    pub fn is_zero(&self) -> bool {
        self.terms.is_empty()
    }

    /// Exact coefficient of `blade`, or zero when the term is absent.
    pub fn coefficient(&self, blade: BasisBlade) -> Rational {
        self.terms
            .get(&blade)
            .copied()
            .unwrap_or_else(Rational::zero)
    }

    /// Iterate non-zero terms in canonical mask order.
    pub fn terms(&self) -> impl Iterator<Item = (BasisBlade, Rational)> + '_ {
        self.terms
            .iter()
            .map(|(blade, coefficient)| (*blade, *coefficient))
    }

    fn accumulate(&mut self, blade: BasisBlade, coefficient: Rational) -> Result<()> {
        if coefficient.is_zero() {
            return Ok(());
        }
        let sum = self
            .terms
            .get(&blade)
            .copied()
            .unwrap_or_else(Rational::zero)
            .checked_add(coefficient)?;
        if sum.is_zero() {
            self.terms.remove(&blade);
        } else {
            self.terms.insert(blade, sum);
        }
        Ok(())
    }

    /// Exact checked multivector addition.
    pub fn checked_add(&self, other: &Self) -> Result<Self> {
        let mut out = self.clone();
        for (blade, coefficient) in other.terms() {
            out.accumulate(blade, coefficient)?;
        }
        Ok(out)
    }

    /// Exact checked multivector subtraction.
    pub fn checked_sub(&self, other: &Self) -> Result<Self> {
        let mut out = self.clone();
        let minus_one = Rational::from_i128(-1)?;
        for (blade, coefficient) in other.terms() {
            out.accumulate(blade, coefficient.checked_mul(minus_one)?)?;
        }
        Ok(out)
    }

    /// Exact checked scalar multiplication.
    pub fn checked_scale(&self, factor: Rational) -> Result<Self> {
        let mut out = Self::zero();
        for (blade, coefficient) in self.terms() {
            out.accumulate(blade, coefficient.checked_mul(factor)?)?;
        }
        Ok(out)
    }

    fn project_grade_unchecked(&self, grade: u32) -> Self {
        Self {
            terms: self
                .terms
                .iter()
                .filter(|(blade, _)| blade.grade() == grade)
                .map(|(blade, coefficient)| (*blade, *coefficient))
                .collect(),
        }
    }

    fn signed_by_grade(&self, exponent: impl Fn(u32) -> u32) -> Result<Self> {
        let minus_one = Rational::from_i128(-1)?;
        let mut out = Self::zero();
        for (blade, coefficient) in self.terms() {
            let coefficient = if exponent(blade.grade()).is_multiple_of(2) {
                coefficient
            } else {
                coefficient.checked_mul(minus_one)?
            };
            out.accumulate(blade, coefficient)?;
        }
        Ok(out)
    }

    /// Reversion: reverse the order of generators in every blade.
    pub fn reversion(&self) -> Result<Self> {
        self.signed_by_grade(|grade| grade * grade.saturating_sub(1) / 2)
    }

    /// Grade involution: negate every odd-grade component.
    pub fn grade_involution(&self) -> Result<Self> {
        self.signed_by_grade(|grade| grade)
    }

    /// Clifford conjugation (grade involution followed by reversion).
    pub fn clifford_conjugation(&self) -> Result<Self> {
        self.signed_by_grade(|grade| grade * (grade + 1) / 2)
    }
}

impl fmt::Display for Multivector {
    /// Render the sparse multivector in deterministic ascending blade-mask order.
    ///
    /// The format is intentionally plain and lossless: every coefficient is
    /// printed as a normalized integer or `numerator/denominator`, followed by
    /// `*e1^e2…` for a non-scalar blade. Zero is rendered as `0`.
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return formatter.write_str("0");
        }
        for (term_index, (blade, coefficient)) in self.terms().enumerate() {
            if term_index > 0 {
                formatter.write_str(" + ")?;
            }
            if coefficient.denominator() == 1 {
                write!(formatter, "{}", coefficient.numerator())?;
            } else {
                write!(
                    formatter,
                    "{}/{}",
                    coefficient.numerator(),
                    coefficient.denominator()
                )?;
            }
            if blade.mask() != 0 {
                formatter.write_str("*")?;
                let mut first = true;
                for index in 0..MAX_CLIFFORD_GENERATORS {
                    if blade.mask() & (1_u64 << index) == 0 {
                        continue;
                    }
                    if !first {
                        formatter.write_str("^")?;
                    }
                    write!(formatter, "e{}", u16::from(index) + 1)?;
                    first = false;
                }
            }
        }
        Ok(())
    }
}

/// An exact real Clifford algebra `Cl(p,q)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CliffordAlgebra {
    signature: CliffordSignature,
}

impl CliffordAlgebra {
    /// Construct `Cl(p,q)`.
    pub fn new(positive: u8, negative: u8) -> Result<Self> {
        Ok(Self {
            signature: CliffordSignature::new(positive, negative)?,
        })
    }

    /// Construct an algebra from a validated signature.
    pub const fn from_signature(signature: CliffordSignature) -> Self {
        Self { signature }
    }

    /// Algebra signature.
    pub const fn signature(self) -> CliffordSignature {
        self.signature
    }

    /// Vector-space dimension `2^n`.
    pub fn dimension(self) -> u128 {
        self.signature.algebra_dimension()
    }

    /// Validate and construct a blade in this algebra.
    pub fn blade(self, mask: u64) -> Result<BasisBlade> {
        BasisBlade::new(mask, self.signature.generators())
    }

    /// Construct generator `e_(index+1)` in this algebra.
    pub fn generator(self, index: u8) -> Result<BasisBlade> {
        BasisBlade::generator(index, self.signature.generators())
    }

    /// The top-grade pseudoscalar blade.
    pub fn pseudoscalar(self) -> Result<BasisBlade> {
        let generators = self.signature.generators();
        let mask = if generators == MAX_CLIFFORD_GENERATORS {
            u64::MAX
        } else {
            (1_u64 << generators) - 1
        };
        self.blade(mask)
    }

    fn validate_blade(self, blade: BasisBlade) -> Result<()> {
        BasisBlade::new(blade.mask, self.signature.generators()).map(|_| ())
    }

    fn validate_multivector(self, multivector: &Multivector) -> Result<()> {
        for (blade, _) in multivector.terms() {
            self.validate_blade(blade)?;
        }
        Ok(())
    }

    /// Keep exactly the terms of `grade`, rejecting a grade outside this algebra.
    pub fn grade_projection(self, value: &Multivector, grade: u8) -> Result<Multivector> {
        self.validate_multivector(value)?;
        if grade > self.signature.generators() {
            return Err(Diag::of_kind(CliffordGradeOutOfRange {
                detail: format!(
                    "grade {grade} lies outside Cl({},{}) whose maximum grade is {}",
                    self.signature.positive(),
                    self.signature.negative(),
                    self.signature.generators()
                ),
            }));
        }
        Ok(value.project_grade_unchecked(u32::from(grade)))
    }

    fn reordering_sign(left: u64, right: u64) -> i8 {
        let mut parity = 0_u32;
        let mut remaining = left;
        while remaining != 0 {
            let index = remaining.trailing_zeros();
            let lower = if index == 0 { 0 } else { (1_u64 << index) - 1 };
            parity ^= (right & lower).count_ones() & 1;
            remaining &= remaining - 1;
        }
        if parity == 0 { 1 } else { -1 }
    }

    /// Exact geometric product of two basis blades.
    pub fn geometric_product_blades(
        self,
        left: BasisBlade,
        right: BasisBlade,
    ) -> Result<SignedBlade> {
        self.validate_blade(left)?;
        self.validate_blade(right)?;

        let mut sign = Self::reordering_sign(left.mask, right.mask);
        let mut overlap = left.mask & right.mask;
        while overlap != 0 {
            let index = overlap.trailing_zeros() as u8;
            sign *= self.signature.generator_square(index)?;
            overlap &= overlap - 1;
        }
        Ok(SignedBlade::new(
            sign,
            BasisBlade {
                mask: left.mask ^ right.mask,
            },
        ))
    }

    /// Exact exterior product of two blades, or `None` when a repeated generator
    /// makes the alternating product zero.
    pub fn exterior_product_blades(
        self,
        left: BasisBlade,
        right: BasisBlade,
    ) -> Result<Option<SignedBlade>> {
        self.validate_blade(left)?;
        self.validate_blade(right)?;
        if left.mask & right.mask != 0 {
            return Ok(None);
        }
        Ok(Some(SignedBlade::new(
            Self::reordering_sign(left.mask, right.mask),
            BasisBlade {
                mask: left.mask | right.mask,
            },
        )))
    }

    /// Left contraction of two basis blades, using the grade-difference part of
    /// their geometric product.
    pub fn left_contraction_blades(
        self,
        left: BasisBlade,
        right: BasisBlade,
    ) -> Result<Option<SignedBlade>> {
        let product = self.geometric_product_blades(left, right)?;
        if left.grade() <= right.grade() && product.blade.grade() == right.grade() - left.grade() {
            Ok(Some(product))
        } else {
            Ok(None)
        }
    }

    fn multiply_terms(
        self,
        left: &Multivector,
        right: &Multivector,
        product: impl Fn(Self, BasisBlade, BasisBlade) -> Result<Option<SignedBlade>>,
    ) -> Result<Multivector> {
        self.validate_multivector(left)?;
        self.validate_multivector(right)?;
        let minus_one = Rational::from_i128(-1)?;
        let mut out = Multivector::zero();
        for (left_blade, left_coefficient) in left.terms() {
            for (right_blade, right_coefficient) in right.terms() {
                let Some(signed) = product(self, left_blade, right_blade)? else {
                    continue;
                };
                let mut coefficient = left_coefficient.checked_mul(right_coefficient)?;
                if signed.sign < 0 {
                    coefficient = coefficient.checked_mul(minus_one)?;
                }
                out.accumulate(signed.blade, coefficient)?;
            }
        }
        Ok(out)
    }

    /// Exact checked geometric product of sparse multivectors.
    pub fn geometric_product(self, left: &Multivector, right: &Multivector) -> Result<Multivector> {
        self.multiply_terms(left, right, |algebra, a, b| {
            algebra.geometric_product_blades(a, b).map(Some)
        })
    }

    /// Exact checked exterior product of sparse multivectors.
    pub fn exterior_product(self, left: &Multivector, right: &Multivector) -> Result<Multivector> {
        self.multiply_terms(left, right, Self::exterior_product_blades)
    }

    /// Exact checked left contraction of sparse multivectors.
    pub fn left_contraction(self, left: &Multivector, right: &Multivector) -> Result<Multivector> {
        self.multiply_terms(left, right, Self::left_contraction_blades)
    }

    /// Exact square of the pseudoscalar (`+1` or `-1`).
    pub fn pseudoscalar_square(self) -> Result<i8> {
        Ok(self
            .geometric_product_blades(self.pseudoscalar()?, self.pseudoscalar()?)?
            .sign)
    }

    /// Add a positive generator, producing canonical positive-first `Cl(p+1,q)`.
    pub fn positive_extension(self) -> Result<Self> {
        Ok(Self::from_signature(self.signature.positive_extension()?))
    }

    fn positive_extension_base(self) -> Result<Self> {
        Ok(Self::from_signature(
            self.signature.without_last_positive_generator()?,
        ))
    }

    fn embed_positive_extension_blade(self, base_blade: BasisBlade) -> Result<BasisBlade> {
        let base = self.positive_extension_base()?;
        base.validate_blade(base_blade)?;
        let insertion = u32::from(base.signature.positive());
        let lower_mask = if insertion == 0 {
            0
        } else {
            (1_u64 << insertion) - 1
        };
        let low = base_blade.mask() & lower_mask;
        let high = base_blade.mask() & !lower_mask;
        self.blade(low | (high << 1))
    }

    fn project_positive_extension_blade(self, embedded_blade: BasisBlade) -> Result<BasisBlade> {
        self.validate_blade(embedded_blade)?;
        let base = self.positive_extension_base()?;
        let insertion = u32::from(base.signature.positive());
        let insertion_mask = 1_u64 << insertion;
        if embedded_blade.mask() & insertion_mask != 0 {
            return Err(Diag::of_kind(CliffordBladeOutOfRange {
                detail: format!(
                    "blade 0x{:016x} contains positive-extension generator e{}",
                    embedded_blade.mask(),
                    insertion + 1
                ),
            }));
        }
        let lower_mask = insertion_mask - 1;
        let low = embedded_blade.mask() & lower_mask;
        let high_mask = if insertion == 63 {
            0
        } else {
            !((1_u64 << (insertion + 1)) - 1)
        };
        let high = embedded_blade.mask() & high_mask;
        base.blade(low | (high >> 1))
    }

    /// Embed a value from `Cl(p,q)` into this canonical positive-first
    /// `Cl(p+1,q)`, preserving positive indices and shifting negative indices by
    /// one to make room for the newly introduced `e_(p+1)`.
    pub fn embed_positive_extension(self, value: &Multivector) -> Result<Multivector> {
        let base = self.positive_extension_base()?;
        base.validate_multivector(value)?;
        Multivector::from_terms(
            value
                .terms()
                .map(|(blade, coefficient)| {
                    self.embed_positive_extension_blade(blade)
                        .map(|embedded| (embedded, coefficient))
                })
                .collect::<Result<Vec<_>>>()?,
        )
    }

    /// Split `x` exactly as
    /// `x = embed(a) + e_(p+1) embed(b)` for `a,b ∈ Cl(p,q)`.
    ///
    /// This is an exact vector-space/module decomposition. It is not an algebra
    /// direct sum. `self` must have at least one positive generator; the removed
    /// generator is the last positive generator, which is the one introduced by
    /// [`Self::positive_extension`].
    pub fn split_positive_extension(
        self,
        value: &Multivector,
    ) -> Result<(Multivector, Multivector)> {
        self.validate_multivector(value)?;
        let base = self.positive_extension_base()?;
        let inserted_index = base.signature.positive();
        let inserted_mask = 1_u64 << inserted_index;
        let minus_one = Rational::from_i128(-1)?;
        let mut first = Multivector::zero();
        let mut last = Multivector::zero();

        for (blade, coefficient) in value.terms() {
            if blade.mask & inserted_mask == 0 {
                first.accumulate(self.project_positive_extension_blade(blade)?, coefficient)?;
            } else {
                let embedded = self.blade(blade.mask ^ inserted_mask)?;
                let base_blade = self.project_positive_extension_blade(embedded)?;
                // e_(p+1) embed(b) crosses exactly the old positive generators
                // present in b; the shifted negative block follows e_(p+1).
                let positive_mask = inserted_mask - 1;
                let lower_positive_grade = (embedded.mask & positive_mask).count_ones();
                let coefficient = if lower_positive_grade.is_multiple_of(2) {
                    coefficient
                } else {
                    coefficient.checked_mul(minus_one)?
                };
                last.accumulate(base_blade, coefficient)?;
            }
        }
        Ok((first, last))
    }

    /// Join `a,b ∈ Cl(p,q)` exactly as
    /// `embed(a) + e_(p+1) embed(b)` in this canonical `Cl(p+1,q)`.
    pub fn join_positive_extension(
        self,
        first: &Multivector,
        last: &Multivector,
    ) -> Result<Multivector> {
        let base = self.positive_extension_base()?;
        base.validate_multivector(first)?;
        base.validate_multivector(last)?;

        let embedded_first = self.embed_positive_extension(first)?;
        let embedded_last = self.embed_positive_extension(last)?;
        let generator =
            Multivector::from_term(self.generator(base.signature.positive())?, Rational::one());
        let tail = self.geometric_product(&generator, &embedded_last)?;
        embedded_first.checked_add(&tail)
    }
}

#[cfg(test)]
mod tests {
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
        let value =
            Multivector::from_terms([(scalar, r(1)), (e1, r(2)), (e12, r(3)), (e123, r(4))])
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
}
