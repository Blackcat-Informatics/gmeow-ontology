// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Content-addressed provenance IRI helpers.
//!
//! Every function here **must** produce byte-identical output to the canonical
//! native statements recipe.  The goldens in
//! `tests/fixtures/logic/determinism-goldens.json` are normative; any deviation
//! from them is a hard test failure.  (The retired `logic_materialize.py` was
//! the prior Python authority; it was superseded by this crate.)
//!
//! # N3 serialization rules (mirror of rdflib `.n3()`)
//!
//! rdflib's `.n3()` produces:
//! - IRI: `<iri>`
//! - Language-tagged literal: `"lex"@lang`  (rdflib lower-cases the lang subtag)
//! - `xsd:string` literal: `"lex"` (datatype **elided**)
//! - `rdf:langString` literal: `"lex"@lang` (datatype **elided**, lang kept)
//! - Any other typed literal: `"lex"^^<datatype_iri>`
//!
//! Lexical-form escaping:
//! - `\` → `\\`
//! - `"` → `\"`
//! - `\n` (newline) → `\n`
//! - `\r` (CR) → `\r`
//! - `\t` (tab) → `\t`
//!
//! No numeric normalization — the lexical form is preserved verbatim.
//!
//! # Reifier recipe
//!
//! `sha1(s.n3() + " " + p.n3() + " " + o.n3()).hexdigest()`
//! under `{NAMESPACE}reifier/`.
//!
//! # Derivation-ID recipe
//!
//! `sha1(raw_rule_identity + "\n" + "\n".join(sorted(source_reifier_iris))).hexdigest()`
//! under `{NAMESPACE}derivation/`.
//! The firing identity is folded byte-for-byte (a full IRI, a native rule label, or
//! the assertion sentinel); public artifact rendering must not rewrite it before
//! hashing. Sources are sorted for order-independence.

use purrdf::TermValue;
use sha1::{Digest, Sha1};
use std::num::NonZeroU32;

/// The canonical `TermValue` surface renderer. ONE definition, and it lives with the
/// arena whose atom dictionary keys on it ([`gmeow_term_arena`]) — the provenance recipes
/// here fold the SAME bytes, so the reifier/derivation identities and the interner's dedup
/// key can never drift apart.
pub use gmeow_term_arena::engine::term_display;
use gmeow_term_arena::engine::term_n3_unchecked;

/// Wrap a provenance-derivation condition message as a typed diagnostic on the
/// shared substrate, preserving the authored text verbatim.
fn provenance_err(detail: String) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Provenance { detail })
}

// ── Namespace constants ────────────────────────────────────────────────────────

/// Vocabulary namespace — term IRIs are `NAMESPACE + local`.
/// Matches `gmeow_tools.config.NAMESPACE` exactly.
pub const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";

/// Logic vocabulary namespace.
/// Matches `gmeow_tools.config.LOGIC_NAMESPACE` exactly.
pub const LOGIC_NAMESPACE: &str = "https://blackcatinformatics.ca/logic/";

/// Sentinel rule IRI for asserted (input) facts.
/// The canonical assert-rule IRI (the recipe formerly carried by
/// `logic_materialize.py`, retired):
/// `f"{_LOGIC_NS}assert"` where `_LOGIC_NS = PREFIXES["logic"]`.
pub const ASSERT_RULE_IRI: &str = "https://blackcatinformatics.ca/logic/assert";

/// Prefix for reifier IRIs.
pub const REIFIER_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/reifier/";

/// Prefix for derivation IRIs.
pub const DERIVATION_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/derivation/";

// ── Bounded provenance algebras ───────────────────────────────────────────────

/// The checked algebraic plug-point shared by bounded provenance annotations.
///
/// `add` combines alternative derivations (`⊕`); `multiply` combines conjunctive
/// evidence (`⊗`).  The interface is deliberately smaller than a symbolic-lineage
/// system: recursive `N[X]` polynomials are not a materialization target.  Concrete
/// carriers stay bounded — [`MinProofHeightSemiring`] stores one small height and
/// [`ZWeightSemiring`] stores one signed counting weight.
///
/// Operations are fallible because a numeric carrier overflowing is a hard engine
/// error, never a saturated or wrapped provenance claim.
pub trait ProvenanceSemiring {
    /// One annotation value.
    type Element: Copy + Eq + std::fmt::Debug;

    /// Stable semantic identity used by annotated query/provider contracts.
    fn identity(self) -> &'static str;

    /// Canonical element encoding used by deterministic operational receipts.
    fn canonical_element(self, element: Self::Element) -> String;

    /// Additive identity: no derivation.
    fn zero(self) -> Self::Element;
    /// Multiplicative identity: asserted/unit evidence.
    fn one(self) -> Self::Element;
    /// Combine alternative derivations.
    fn add(self, left: Self::Element, right: Self::Element) -> gmeow_errors::Result<Self::Element>;
    /// Combine conjunctive premises.
    fn multiply(
        self,
        left: Self::Element,
        right: Self::Element,
    ) -> gmeow_errors::Result<Self::Element>;
}

impl<S> crate::annotation::TupleAnnotationAlgebra for S
where
    S: ProvenanceSemiring + Copy,
{
    type Element = S::Element;

    fn identity(&self) -> &str {
        ProvenanceSemiring::identity(*self)
    }

    fn canonical_element(&self, element: &Self::Element) -> String {
        ProvenanceSemiring::canonical_element(*self, *element)
    }

    fn zero(&self) -> Self::Element {
        ProvenanceSemiring::zero(*self)
    }

    fn one(&self) -> Self::Element {
        ProvenanceSemiring::one(*self)
    }

    fn add(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        ProvenanceSemiring::add(*self, *left, *right)
    }

    fn multiply(
        &self,
        left: &Self::Element,
        right: &Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        ProvenanceSemiring::multiply(*self, *left, *right)
    }
}

/// A provenance semiring with additive inverses (the signed Z-set carrier).
pub trait ProvenanceRing: ProvenanceSemiring {
    /// Additive inverse, used by retractions.
    fn negate(self, value: Self::Element) -> gmeow_errors::Result<Self::Element>;
}

/// Finite height of a selected minimal proof tree (`0` for an asserted fact).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProofHeight(NonZeroU32);

impl ProofHeight {
    /// An asserted fact is a proof leaf.
    pub const ASSERTED: Self = Self(NonZeroU32::MIN);

    /// Construct a finite proof height.
    ///
    /// # Errors
    ///
    /// Returns a typed provenance error if `value + 1` cannot fit the nonzero niche
    /// encoding. No height is saturated or wrapped.
    pub fn new(value: u32) -> gmeow_errors::Result<Self> {
        let encoded = value.checked_add(1).ok_or_else(|| {
            provenance_err(format!(
                "finite proof height {value} exceeds the niche-encoded u32 carrier"
            ))
        })?;
        Ok(Self(
            NonZeroU32::new(encoded).expect("checked height + 1 is nonzero"),
        ))
    }

    /// The finite height as a scalar.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get() - 1
    }

    /// Lift a conjunction of premise proofs through one rule firing.
    fn successor(self) -> gmeow_errors::Result<Self> {
        self.0
            .get()
            .checked_add(1)
            .and_then(NonZeroU32::new)
            .map(Self)
            .ok_or_else(|| {
                provenance_err("minimal proof-height annotation overflowed u32".to_owned())
            })
    }
}

/// The `N ∪ {∞}` carrier of the `(min, max)` idempotent semiring.
///
/// `Infinity` is additive identity/no derivation. `Finite(0)` is multiplicative
/// identity/asserted evidence. Alternative proofs choose `min`; a conjunction takes
/// `max`. A rule application then performs one checked successor, yielding the
/// Zhao/Subotić/Scholz recurrence `1 + max(body heights)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinProofHeight {
    /// A finite proof annotation.
    Finite(ProofHeight),
    /// No derivation (the additive identity).
    Infinity,
}

/// Minimal-proof-height provenance over the bounded `(min, max)` carrier.
#[derive(Debug, Clone, Copy, Default)]
pub struct MinProofHeightSemiring;

impl ProvenanceSemiring for MinProofHeightSemiring {
    type Element = MinProofHeight;

    fn identity(self) -> &'static str {
        "https://blackcatinformatics.ca/logic/algebra/min-proof-height-v1"
    }

    fn canonical_element(self, element: Self::Element) -> String {
        match element {
            MinProofHeight::Finite(height) => format!("finite:{}", height.get()),
            MinProofHeight::Infinity => "infinity".to_owned(),
        }
    }

    fn zero(self) -> Self::Element {
        MinProofHeight::Infinity
    }

    fn one(self) -> Self::Element {
        MinProofHeight::Finite(ProofHeight::ASSERTED)
    }

    fn add(self, left: Self::Element, right: Self::Element) -> gmeow_errors::Result<Self::Element> {
        Ok(match (left, right) {
            (MinProofHeight::Infinity, other) | (other, MinProofHeight::Infinity) => other,
            (MinProofHeight::Finite(a), MinProofHeight::Finite(b)) => {
                MinProofHeight::Finite(a.min(b))
            }
        })
    }

    fn multiply(
        self,
        left: Self::Element,
        right: Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        Ok(match (left, right) {
            (MinProofHeight::Infinity, _) | (_, MinProofHeight::Infinity) => {
                MinProofHeight::Infinity
            }
            (MinProofHeight::Finite(a), MinProofHeight::Finite(b)) => {
                MinProofHeight::Finite(a.max(b))
            }
        })
    }
}

impl MinProofHeightSemiring {
    /// Annotate one rule firing from its premise heights.
    ///
    /// An empty body folds to the multiplicative identity and therefore has height
    /// `1`; a non-empty body has `1 + max(premises)`. The finite iterator cannot
    /// produce `Infinity`, so reaching it is an internal algebra bug.
    pub fn derive(
        self,
        premises: impl IntoIterator<Item = ProofHeight>,
    ) -> gmeow_errors::Result<ProofHeight> {
        let mut product = self.one();
        for premise in premises {
            product = self.multiply(product, MinProofHeight::Finite(premise))?;
        }
        match product {
            MinProofHeight::Finite(height) => height.successor(),
            MinProofHeight::Infinity => Err(provenance_err(
                "finite proof premises unexpectedly folded to infinity".to_owned(),
            )),
        }
    }

    /// Choose the lower of two finite alternative proof heights.
    pub fn choose(
        self,
        left: ProofHeight,
        right: ProofHeight,
    ) -> gmeow_errors::Result<ProofHeight> {
        match self.add(MinProofHeight::Finite(left), MinProofHeight::Finite(right))? {
            MinProofHeight::Finite(height) => Ok(height),
            MinProofHeight::Infinity => Err(provenance_err(
                "two finite proof alternatives unexpectedly combined to infinity".to_owned(),
            )),
        }
    }
}

/// Signed integer counting provenance used by the incremental Z-set circuit.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZWeightSemiring;

impl ProvenanceSemiring for ZWeightSemiring {
    type Element = i64;

    fn identity(self) -> &'static str {
        "https://blackcatinformatics.ca/logic/algebra/z-weight-v1"
    }

    fn canonical_element(self, element: Self::Element) -> String {
        element.to_string()
    }

    fn zero(self) -> Self::Element {
        0
    }

    fn one(self) -> Self::Element {
        1
    }

    fn add(self, left: Self::Element, right: Self::Element) -> gmeow_errors::Result<Self::Element> {
        left.checked_add(right).ok_or_else(|| {
            provenance_err(format!(
                "signed counting-provenance addition overflow: {left} + {right}"
            ))
        })
    }

    fn multiply(
        self,
        left: Self::Element,
        right: Self::Element,
    ) -> gmeow_errors::Result<Self::Element> {
        left.checked_mul(right).ok_or_else(|| {
            provenance_err(format!(
                "signed counting-provenance multiplication overflow: {left} * {right}"
            ))
        })
    }
}

impl ProvenanceRing for ZWeightSemiring {
    fn negate(self, value: Self::Element) -> gmeow_errors::Result<Self::Element> {
        value.checked_neg().ok_or_else(|| {
            provenance_err(format!(
                "signed counting-provenance negation overflow: -({value})"
            ))
        })
    }
}

// ── SHA-1 helper ─────────────────────────────────────────────────────────────

/// The lowercase-hex SHA-1 of `s` — the content-addressing primitive the reifier,
/// derivation-id, and native reasoning-contract hashes all share.
pub fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── N3 serialization ─────────────────────────────────────────────────────────

fn validate_triple_term_predicates(term: &TermValue) -> gmeow_errors::Result<()> {
    let mut pending = vec![term];
    while let Some(term) = pending.pop() {
        if let TermValue::Triple { s, p, o } = term {
            if !matches!(p.as_ref(), TermValue::Iri(_)) {
                let predicate_kind = match p.as_ref() {
                    TermValue::Iri(_) => unreachable!("IRI predicates pass the validation guard"),
                    TermValue::Blank { .. } => "blank node",
                    TermValue::Literal { .. } => "literal",
                    TermValue::Triple { .. } => "triple term",
                };
                return Err(provenance_err(format!(
                    "RDF 1.2 triple-term predicate must be an IRI, got {predicate_kind}"
                )));
            }
            pending.push(o);
            pending.push(s);
        }
    }
    Ok(())
}

/// Serialize a native [`TermValue`] to rdflib `.n3()` form.
///
/// - `Iri(iri)` → `<iri>`
/// - `Blank` → not expected after Skolemization; serialized as `_:label`
/// - `Literal` → `xsd:string` / lang-less `rdf:langString` elide the datatype; a
///   lang-tagged literal renders `"lex"@lang` (lowercased); anything else renders
///   `"lex"^^<dt>`
/// - `Triple` → RDF 1.2 non-asserting triple term `<<( s p o )>>`, recursively.
///
/// # Errors
///
/// Returns an error when any nested triple term carries a non-IRI predicate.
pub fn term_n3(term: &TermValue) -> gmeow_errors::Result<String> {
    validate_triple_term_predicates(term)?;
    Ok(term_n3_unchecked(term))
}

/// Serialize an IRI string to rdflib `.n3()` form: `<iri>`.
pub fn named_node_n3(iri: &str) -> String {
    format!("<{}>", iri)
}

// ── mint_reifier ─────────────────────────────────────────────────────────────

/// Compute the reifier IRI for an `(S, P, O)` triple.
///
/// Mirrors the native statement-stage reifier recipe exactly:
/// ```text
/// canonical = s.n3() + " " + p.n3() + " " + o.n3()
/// digest    = sha1(canonical.encode("utf-8")).hexdigest()
/// iri       = f"{NAMESPACE}reifier/{digest}"
/// ```
///
/// # Arguments
///
/// - `s` — Subject term (as [`TermValue`]; IRIs after Skolemization).
/// - `p` — Predicate IRI string.
/// - `o` — Object term.
///
/// # Errors
///
/// Triple terms are serialized recursively in RDF 1.2 non-asserting form before
/// hashing, so distinct nested statements retain distinct content identities.
///
/// # Returns
///
/// The reifier IRI as a `String`.
pub fn mint_reifier(s: &TermValue, p: &str, o: &TermValue) -> gmeow_errors::Result<String> {
    let s_n3 = term_n3(s)?;
    let o_n3 = term_n3(o)?;
    let canonical = format!("{} {} {}", s_n3, named_node_n3(p), o_n3);
    let digest = sha1_hex(&canonical);
    Ok(format!("{}{}", REIFIER_PREFIX, digest))
}

/// Compute the reifier IRI from already-serialized N3 component strings.
///
/// `subject` and `predicate` are IRI strings (NOT N3-wrapped — this helper wraps
/// them in `<...>`); `obj_n3` is the object already in canonical N3 form (`<iri>`
/// for an IRI, `"lex"^^<dt>` for a literal, etc.) and is used **verbatim**.
///
/// The canonical reifier recipe (Python `_reifier_from_quad` in
/// `logic_explain.py` retired):
/// ```text
/// payload = f"<{subject}> <{predicate}> {obj_n3}"
/// digest  = sha1(payload.encode("utf-8")).hexdigest()
/// iri     = f"{NAMESPACE}reifier/{digest}"
/// ```
///
/// Used by the explanation engine ([`crate::explain`]), whose rows carry the
/// object already as an N3 string (it never re-parses the object term), and by the
/// `explain_quad` consumer surface, which computes a target quad's reifier from the
/// SAME canonical N3 object [`term_display`] produces for a [`crate::explain::Row`].
pub fn reifier_from_strings(subject: &str, predicate: &str, obj_n3: &str) -> String {
    let canonical = format!("<{}> <{}> {}", subject, predicate, obj_n3);
    let digest = sha1_hex(&canonical);
    format!("{}{}", REIFIER_PREFIX, digest)
}

// ── mint_nary_reifier ──────────────────────────────────────────────────────────

/// Prefix for n-ary reifier IRIs — the content-addressed node a fixed-arity n-ary
/// tuple reifies onto. Distinct from [`REIFIER_PREFIX`] so an n-ary reifier IRI is
/// never confused with a binary statement reifier.
pub const NARY_REIFIER_PREFIX: &str = "https://blackcatinformatics.ca/gmeow/reifier/nary/";

/// Compute the reifier IRI for a fixed-arity n-ary tuple `Rel(a₀,…,aₙ)`.
///
/// The reifier node is the single content-addressed IRI over which the flat-binary
/// reification (`logic:instanceOf(R, Rel) ∧ logic:naryArg0(R, a₀) ∧ …`) hangs. It
/// is keyed on the relation and the *ordered* arguments, so the same tuple — as a
/// ground fact or as any derivation of it — yields the same `R`, giving identity
/// and provenance parity with the binary [`mint_reifier`].
///
/// The recipe is deliberately additive and cannot collide with a binary
/// [`mint_reifier`] payload: it is domain-tagged with a leading `nary\n` (a binary
/// payload starts with `<`, never `n`) and every component is length-prefixed
/// (netstring `len:bytes,`), so the payload is injective in the relation and the
/// ordered argument list — no spacing or arity ambiguity can conflate two tuples.
///
/// ```text
/// payload = "nary\n"
///         + f"{len(<Rel>)}:{<Rel>},"
///         + Σᵢ f"{len(argᵢ.n3())}:{argᵢ.n3()},"
/// digest  = sha1(payload.encode("utf-8")).hexdigest()
/// iri     = f"{NARY_REIFIER_PREFIX}{digest}"
/// ```
///
/// # Errors
///
/// Triple-term arguments are serialized recursively through [`term_n3`], so native
/// RDF 1.2 tuple arguments retain their complete content identity.
pub fn mint_nary_reifier(relation: &str, args: &[TermValue]) -> gmeow_errors::Result<String> {
    let mut payload = String::from("nary\n");
    let rel = named_node_n3(relation);
    payload.push_str(&format!("{}:{},", rel.len(), rel));
    for a in args {
        let a_n3 = term_n3(a)?;
        payload.push_str(&format!("{}:{},", a_n3.len(), a_n3));
    }
    let digest = sha1_hex(&payload);
    Ok(format!("{}{}", NARY_REIFIER_PREFIX, digest))
}

// ── reified n-ary vocabulary ────────────────────────────────────────────────────

/// The `logic:instanceOf` predicate IRI — the reified-n-ary *typing* atom
/// `logic:instanceOf(R, Rel)` that types a reifier node `R` with its relation `Rel`.
///
/// This is the SINGLE source of the reified-n-ary vocabulary IRIs, shared by the native
/// restricted chase (`crate::physical::chase`) and the n-ary ingestion/lowering
/// (`crate::nary`), so the pre-reified EDB path and the chase-derived path agree on the
/// exact predicate surfaces (the encoding is doctrinal — `LOGIC-IR.md` §RelationalCore).
#[must_use]
pub fn instance_of_iri() -> String {
    format!("{LOGIC_NAMESPACE}instanceOf")
}

/// The `logic:naryArg{index}` positional-argument predicate IRI — the reified-n-ary atom
/// `logic:naryArg{i}(R, aᵢ)` binding the `i`-th argument `aᵢ` of the tuple reified onto `R`.
#[must_use]
pub fn nary_arg_predicate(index: usize) -> String {
    format!("{LOGIC_NAMESPACE}naryArg{index}")
}

/// Parse a `logic:naryArg{index}` predicate IRI back to its positional `index`, or `None`
/// if `predicate` is not a positional n-ary-argument predicate. The exact inverse of
/// [`nary_arg_predicate`].
#[must_use]
pub fn nary_arg_index(predicate: &str) -> Option<usize> {
    predicate
        .strip_prefix(&format!("{LOGIC_NAMESPACE}naryArg"))?
        .parse()
        .ok()
}

// ── mint_derivation_id ───────────────────────────────────────────────────────

/// Compute the derivation IRI for a rule firing.
///
/// The canonical derivation-id recipe (Python `derivation_id_iri` in
/// `gmeow_tools.logic_materialize` retired):
/// ```text
/// payload = raw_rule_identity + "\n" + "\n".join(sorted(source_reifier_iris))
/// digest  = sha1(payload.encode("utf-8")).hexdigest()
/// iri     = f"{NAMESPACE}derivation/{digest}"
/// ```
///
/// Sources are sorted (ascending lexicographic) for order-independence.
///
/// # Arguments
///
/// - `rule_iri` — The byte-exact firing identity: a full rule IRI, a native rule
///   label, or the assert sentinel. Callers must not canonicalize it before hashing.
/// - `source_reifier_iris` — The reifier IRIs of the consumed antecedent quads.
///
/// # Returns
///
/// The derivation IRI as a `String`.
pub fn mint_derivation_id(rule_iri: &str, source_reifier_iris: &[&str]) -> String {
    let mut sorted: Vec<&str> = source_reifier_iris.to_vec();
    sorted.sort_unstable();
    let joined = sorted.join("\n");
    let payload = format!("{}\n{}", rule_iri, joined);
    let digest = sha1_hex(&payload);
    format!("{}{}", DERIVATION_PREFIX, digest)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[path = "provenance.tests.rs"]
#[cfg(test)]
mod tests;
