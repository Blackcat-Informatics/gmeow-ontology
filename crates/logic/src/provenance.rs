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
//! `sha1(rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))).hexdigest()`
//! under `{NAMESPACE}derivation/`.
//! Sources are sorted for order-independence.

use purrdf::{RdfTextDirection, TermValue};
use sha1::{Digest, Sha1};
use std::num::NonZeroU32;

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

// ── XSD / RDF datatype IRIs ────────────────────────────────────────────────────

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const RDF_LANG_STRING: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString";

// ── SHA-1 helper ─────────────────────────────────────────────────────────────

/// The lowercase-hex SHA-1 of `s` — the content-addressing primitive the reifier,
/// derivation-id, and native reasoning-contract hashes all share.
pub fn sha1_hex(s: &str) -> String {
    let mut hasher = Sha1::new();
    hasher.update(s.as_bytes());
    format!("{:x}", hasher.finalize())
}

// ── N3 serialization ─────────────────────────────────────────────────────────

/// Escape a literal lexical form exactly as rdflib does in `.n3()`.
///
/// rdflib escapes: `\` → `\\`, `"` → `\"`, newline → `\n`, CR → `\r`, tab → `\t`.
/// No other escaping is applied.
fn escape_lexical(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 4);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out
}

/// Serialize a native literal (lexical form + datatype IRI + optional language) to
/// rdflib `.n3()` form.
///
/// Rules:
/// - `xsd:string` → `"lex"` (datatype elided)
/// - `rdf:langString` (lang-tagged) → `"lex"@lang` (datatype elided, lang kept)
/// - Any other datatype → `"lex"^^<datatype_iri>`
///
/// rdflib lowercases the BCP-47 language subtag.  The native IR lowercases the
/// language tag for the identity key, but a `TermValue` constructed directly may
/// carry an un-lowercased tag, so we lowercase here to stay in sync regardless.
fn literal_n3_parts(
    lexical_form: &str,
    datatype: &str,
    language: Option<&str>,
    direction: Option<RdfTextDirection>,
) -> String {
    let lex = escape_lexical(lexical_form);

    if let Some(lang) = language {
        // Language-tagged literal — rdflib elides the rdf:langString datatype.
        // rdflib lowercases the language tag; mirror that.
        let language = lang.to_lowercase();
        return match direction {
            Some(direction) => format!("\"{lex}\"@{language}--{}", direction.as_str()),
            None => format!("\"{lex}\"@{language}"),
        };
    }

    if datatype == XSD_STRING {
        // Plain xsd:string — rdflib elides the datatype.
        return format!("\"{}\"", lex);
    }
    if datatype == RDF_LANG_STRING {
        // rdf:langString without a language tag — treated like xsd:string by rdflib.
        // (In practice rdf:langString always has a lang tag; be defensive.)
        return format!("\"{}\"", lex);
    }

    // Typed literal with a non-elided datatype.
    format!("\"{}\"^^<{}>", lex, datatype)
}

#[derive(Clone, Copy)]
enum TermRenderStyle {
    N3,
    Display,
}

enum TermRenderTask<'term> {
    Term(&'term TermValue),
    Text(&'static str),
}

fn render_term(term: &TermValue, style: TermRenderStyle) -> String {
    let mut rendered = String::new();
    let mut tasks = vec![TermRenderTask::Term(term)];
    while let Some(task) = tasks.pop() {
        let term = match task {
            TermRenderTask::Term(term) => term,
            TermRenderTask::Text(text) => {
                rendered.push_str(text);
                continue;
            }
        };
        match term {
            TermValue::Iri(iri) => {
                rendered.push('<');
                rendered.push_str(iri);
                rendered.push('>');
            }
            TermValue::Blank { label, scope } => {
                rendered.push_str("_:");
                rendered.push_str(&scope.qualify_label(label));
            }
            TermValue::Literal {
                lexical_form,
                datatype,
                language,
                direction,
            } => match style {
                TermRenderStyle::N3 => rendered.push_str(&literal_n3_parts(
                    lexical_form,
                    datatype,
                    language.as_deref(),
                    *direction,
                )),
                TermRenderStyle::Display => {
                    let lex = escape_lexical(lexical_form);
                    if let Some(lang) = language {
                        rendered.push('"');
                        rendered.push_str(&lex);
                        rendered.push_str("\"@");
                        rendered.push_str(lang);
                        if let Some(direction) = direction {
                            rendered.push_str("--");
                            rendered.push_str(direction.as_str());
                        }
                    } else if datatype == XSD_STRING || datatype == RDF_LANG_STRING {
                        rendered.push('"');
                        rendered.push_str(&lex);
                        rendered.push('"');
                    } else {
                        rendered.push('"');
                        rendered.push_str(&lex);
                        rendered.push_str("\"^^<");
                        rendered.push_str(datatype);
                        rendered.push('>');
                    }
                }
            },
            TermValue::Triple { s, p, o } => {
                tasks.push(TermRenderTask::Text(" )>>"));
                tasks.push(TermRenderTask::Term(o));
                tasks.push(TermRenderTask::Text(" "));
                tasks.push(TermRenderTask::Term(p));
                tasks.push(TermRenderTask::Text(" "));
                tasks.push(TermRenderTask::Term(s));
                tasks.push(TermRenderTask::Text("<<( "));
            }
        }
    }
    rendered
}

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
/// - `Literal` → delegated to [`literal_n3_parts`]
/// - `Triple` → RDF 1.2 non-asserting triple term `<<( s p o )>>`, recursively.
///
/// # Errors
///
/// Returns an error when any nested triple term carries a non-IRI predicate.
pub fn term_n3(term: &TermValue) -> gmeow_errors::Result<String> {
    validate_triple_term_predicates(term)?;
    Ok(render_term(term, TermRenderStyle::N3))
}

/// Serialize an IRI string to rdflib `.n3()` form: `<iri>`.
pub fn named_node_n3(iri: &str) -> String {
    format!("<{}>", iri)
}

/// Render a [`TermValue`] in oxigraph's Turtle term Display form — the exact byte
/// form the prior `Term::to_string()` produced. This is the canonical-surface used
/// for content-addressed dedup keys and sort keys (`rule_ir`, `foundation`) and for
/// the verify finding detail, so it MUST stay byte-identical to oxigraph's Display.
///
/// Unlike [`term_n3`] this does **not** lowercase the language tag (oxigraph's
/// Display preserves the stored tag verbatim) and renders a triple term in the
/// RDF 1.2 non-asserting triple-term form `<<( s p o )>>`.
///
/// - `Iri` → `<iri>`
/// - `Blank` → `_:label`
/// - `Literal` xsd:string / rdf:langString → `"lex"` ; lang → `"lex"@lang` ;
///   typed → `"lex"^^<dt>`
/// - `Triple` → `<<( s p o )>>` (iteratively traversed, including nested triples)
pub fn term_display(term: &TermValue) -> String {
    render_term(term, TermRenderStyle::Display)
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
/// payload = rule_iri + "\n" + "\n".join(sorted(source_reifier_iris))
/// digest  = sha1(payload.encode("utf-8")).hexdigest()
/// iri     = f"{NAMESPACE}derivation/{digest}"
/// ```
///
/// Sources are sorted (ascending lexicographic) for order-independence.
///
/// # Arguments
///
/// - `rule_iri` — The IRI of the fired rule (or the assert-sentinel).
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn min_proof_height_uses_max_for_premises_and_min_for_alternatives() {
        let algebra = MinProofHeightSemiring;
        let direct = algebra
            .derive([ProofHeight::new(0).unwrap(), ProofHeight::new(2).unwrap()])
            .expect("finite proof heights must combine");
        let indirect = algebra
            .derive([ProofHeight::new(3).unwrap()])
            .expect("finite proof height must lift");

        assert_eq!(direct.get(), 3, "rule height is 1 + max(body heights)");
        assert_eq!(indirect.get(), 4);
        assert_eq!(
            algebra
                .choose(indirect, direct)
                .expect("finite alternatives must combine"),
            direct,
            "alternative derivations select the minimal proof height"
        );
        assert_eq!(
            algebra
                .derive([])
                .expect("a bodyless derived rule has one rule level")
                .get(),
            1
        );
    }

    #[test]
    fn bounded_provenance_overflow_hard_fails() {
        assert!(
            ProofHeight::new(u32::MAX).is_err(),
            "the niche constructor must refuse an unencodable finite value"
        );
        let height_err = MinProofHeightSemiring
            .derive([ProofHeight::new(u32::MAX - 1).unwrap()])
            .expect_err("proof height must not saturate or wrap");
        assert!(height_err.to_string().contains("proof-height"));

        let add_err = ZWeightSemiring
            .add(i64::MAX, 1)
            .expect_err("Z-weight addition must be checked");
        assert!(add_err.to_string().contains("addition overflow"));

        let mul_err = ZWeightSemiring
            .multiply(i64::MAX, 2)
            .expect_err("Z-weight multiplication must be checked");
        assert!(mul_err.to_string().contains("multiplication overflow"));

        let neg_err = ZWeightSemiring
            .negate(i64::MIN)
            .expect_err("Z-weight negation must be checked");
        assert!(neg_err.to_string().contains("negation overflow"));
    }

    #[test]
    fn provenance_algebras_obey_identity_and_absorption_contracts() {
        let min_height = MinProofHeightSemiring;
        let two = MinProofHeight::Finite(ProofHeight::new(2).unwrap());
        assert_eq!(min_height.add(two, min_height.zero()).unwrap(), two);
        assert_eq!(min_height.multiply(two, min_height.one()).unwrap(), two);
        assert_eq!(
            min_height.multiply(two, min_height.zero()).unwrap(),
            MinProofHeight::Infinity
        );

        let z = ZWeightSemiring;
        assert_eq!(z.add(7, z.zero()).unwrap(), 7);
        assert_eq!(z.multiply(7, z.one()).unwrap(), 7);
        assert_eq!(z.multiply(7, z.zero()).unwrap(), 0);
        assert_eq!(z.add(7, z.negate(7).unwrap()).unwrap(), 0);
    }

    #[test]
    fn optional_proof_height_uses_the_nonzero_niche() {
        assert_eq!(
            std::mem::size_of::<Option<ProofHeight>>(),
            std::mem::size_of::<u32>(),
            "an absent annotation must not widen neutral provenance rows"
        );
    }

    /// Test-only helper mirroring `literal_n3_parts` over a constructed `TermValue`.
    fn literal_n3(term: &TermValue) -> String {
        term_n3(term).expect("literal term must serialize")
    }

    // ── literal_n3 ────────────────────────────────────────────────────────────

    #[test]
    fn literal_n3_plain_string_elides_datatype() {
        // xsd:string datatype must be elided — matches rdflib .n3()
        let lit = TermValue::simple_literal("plain string");
        assert_eq!(literal_n3(&lit), "\"plain string\"");
    }

    #[test]
    fn literal_n3_language_tagged_lowercased() {
        // rdflib lowercases lang tags; we must mirror that
        let lit = TermValue::lang_literal("hello", "en");
        assert_eq!(literal_n3(&lit), "\"hello\"@en");
    }

    #[test]
    fn literal_n3_uppercase_lang_lowercased() {
        // Upper-case lang tag must be lowercased
        let lit = TermValue::lang_literal("Bonjour", "FR");
        assert_eq!(literal_n3(&lit), "\"Bonjour\"@fr");
    }

    #[test]
    fn literal_n3_decimal_not_elided() {
        // xsd:decimal must NOT be elided — only xsd:string and rdf:langString are
        let lit = TermValue::typed_literal("1.0", "http://www.w3.org/2001/XMLSchema#decimal");
        assert_eq!(
            literal_n3(&lit),
            "\"1.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>"
        );
    }

    #[test]
    fn literal_n3_escape_backslash() {
        let lit = TermValue::simple_literal("a\\b");
        assert_eq!(literal_n3(&lit), "\"a\\\\b\"");
    }

    #[test]
    fn literal_n3_escape_quote() {
        let lit = TermValue::simple_literal("say \"hi\"");
        assert_eq!(literal_n3(&lit), "\"say \\\"hi\\\"\"");
    }

    #[test]
    fn literal_n3_escape_newline() {
        let lit = TermValue::simple_literal("line1\nline2");
        assert_eq!(literal_n3(&lit), "\"line1\\nline2\"");
    }

    #[test]
    fn literal_n3_escape_tab() {
        let lit = TermValue::simple_literal("col1\tcol2");
        assert_eq!(literal_n3(&lit), "\"col1\\tcol2\"");
    }

    // ── term_n3 ───────────────────────────────────────────────────────────────

    #[test]
    fn term_n3_iri() {
        let term = TermValue::iri("http://example.org/a");
        assert_eq!(term_n3(&term).unwrap(), "<http://example.org/a>");
    }

    #[test]
    fn term_n3_literal_string() {
        let term = TermValue::simple_literal("hello");
        assert_eq!(term_n3(&term).unwrap(), "\"hello\"");
    }

    #[test]
    fn term_n3_and_reifier_preserve_recursive_rdf12_triple_terms() {
        let triple = TermValue::Triple {
            s: Box::new(TermValue::iri("http://example.org/s")),
            p: Box::new(TermValue::iri("http://example.org/p")),
            o: Box::new(TermValue::iri("http://example.org/o")),
        };
        assert_eq!(
            term_n3(&triple).unwrap(),
            "<<( <http://example.org/s> <http://example.org/p> <http://example.org/o> )>>"
        );
        let nested = mint_reifier(
            &TermValue::iri("http://example.org/holder"),
            "http://example.org/mentions",
            &triple,
        )
        .expect("RDF 1.2 triple-term reifier");
        let flat = mint_reifier(
            &TermValue::iri("http://example.org/holder"),
            "http://example.org/mentions",
            &TermValue::iri("http://example.org/o"),
        )
        .expect("flat reifier");
        assert_ne!(nested, flat);
    }

    #[test]
    fn deeply_nested_rdf12_triple_terms_render_without_call_stack_recursion() {
        const DEPTH: usize = 4_096;
        let mut nested = TermValue::iri("http://example.org/leaf");
        for _ in 0..DEPTH {
            nested = TermValue::Triple {
                s: Box::new(TermValue::iri("http://example.org/s")),
                p: Box::new(TermValue::iri("http://example.org/p")),
                o: Box::new(nested),
            };
        }

        let n3 = term_n3(&nested).expect("iterative N3 renderer");
        let display = term_display(&nested);
        assert_eq!(n3, display);
        assert_eq!(n3.matches("<<( ").count(), DEPTH);
        assert!(n3.contains("<http://example.org/leaf>"));

        // Box's recursive destructor is outside the renderer contract under test.
        std::mem::forget(nested);
    }

    #[test]
    fn term_n3_rejects_non_iri_predicates_at_every_nesting_depth() {
        let invalid = TermValue::Triple {
            s: Box::new(TermValue::iri("http://example.org/s")),
            p: Box::new(TermValue::simple_literal("not-an-iri")),
            o: Box::new(TermValue::iri("http://example.org/o")),
        };
        assert!(
            term_n3(&invalid).is_err(),
            "direct non-IRI predicate must fail closed"
        );
        let nested = TermValue::Triple {
            s: Box::new(TermValue::iri("http://example.org/outer-s")),
            p: Box::new(TermValue::iri("http://example.org/outer-p")),
            o: Box::new(invalid),
        };

        let error = term_n3(&nested).expect_err("nested non-IRI predicate must fail closed");
        assert!(
            error
                .to_string()
                .contains("RDF 1.2 triple-term predicate must be an IRI")
        );
        assert!(
            mint_reifier(
                &TermValue::iri("http://example.org/holder"),
                "http://example.org/mentions",
                &nested,
            )
            .is_err(),
            "invalid triple terms must not mint provenance identities"
        );
    }

    #[test]
    fn directional_language_is_part_of_the_provenance_surface() {
        let literal = TermValue::Literal {
            lexical_form: "مرحبا".to_owned(),
            datatype: RDF_LANG_STRING.to_owned(),
            language: Some("AR".to_owned()),
            direction: Some(RdfTextDirection::Rtl),
        };
        assert_eq!(term_n3(&literal).unwrap(), "\"مرحبا\"@ar--rtl");
        assert_eq!(term_display(&literal), "\"مرحبا\"@AR--rtl");
    }

    // ── mint_reifier goldens ─────────────────────────────────────────────────

    /// Golden 1: three plain IRI terms.
    /// Python oracle: sha1("<http://example.org/a> <http://example.org/related> <http://example.org/b>")
    ///             = 10d9bdab72fe25cf3b81fe842b3a105077d98a6a
    #[test]
    fn mint_reifier_golden_1_iri_triple() {
        let s = TermValue::iri("http://example.org/a");
        let p = "http://example.org/related";
        let o = TermValue::iri("http://example.org/b");
        let got = mint_reifier(&s, p, &o).expect("IRI terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
            "mint_reifier golden-1 mismatch"
        );
    }

    /// Golden 2: language-tagged literal object (lang tag lowercased).
    /// Python oracle: sha1("<http://example.org/x> <http://www.w3.org/2000/01/rdf-schema#label> \"hello\"@en")
    ///             = 61194b8ccffff3db1bbb81df91c55b7776ee4064
    #[test]
    fn mint_reifier_golden_2_lang_literal() {
        let s = TermValue::iri("http://example.org/x");
        let p = "http://www.w3.org/2000/01/rdf-schema#label";
        let o = TermValue::lang_literal("hello", "en");
        let got = mint_reifier(&s, p, &o).expect("lang literal terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
            "mint_reifier golden-2 mismatch"
        );
    }

    /// Golden 3: xsd:decimal literal — datatype NOT elided.
    /// Python oracle: sha1("<http://example.org/m> <http://example.org/value> \"1.0\"^^<http://www.w3.org/2001/XMLSchema#decimal>")
    ///             = efbda8fbbb765e64c7f8ca2d690489a1ba70e569
    #[test]
    fn mint_reifier_golden_3_xsd_decimal() {
        let s = TermValue::iri("http://example.org/m");
        let p = "http://example.org/value";
        let o = TermValue::typed_literal("1.0", "http://www.w3.org/2001/XMLSchema#decimal");
        let got = mint_reifier(&s, p, &o).expect("typed literal terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/efbda8fbbb765e64c7f8ca2d690489a1ba70e569",
            "mint_reifier golden-3 mismatch"
        );
    }

    /// Golden 4: plain string literal — xsd:string datatype ELIDED.
    /// Python oracle: sha1("<http://example.org/n> <http://example.org/name> \"plain string\"")
    ///             = 784c486d79b869539405a3f90f21126477b07f26
    #[test]
    fn mint_reifier_golden_4_plain_string() {
        let s = TermValue::iri("http://example.org/n");
        let p = "http://example.org/name";
        let o = TermValue::simple_literal("plain string");
        let got = mint_reifier(&s, p, &o).expect("plain literal terms must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/784c486d79b869539405a3f90f21126477b07f26",
            "mint_reifier golden-4 mismatch"
        );
    }

    // ── mint_nary_reifier goldens ─────────────────────────────────────────────

    /// Nary golden A: ternary all-IRI tuple mul(a, b, c).
    /// payload = "nary\n" + "24:<http://example.org/mul>," + "22:<http://example.org/a>,"
    ///                    + "22:<http://example.org/b>," + "22:<http://example.org/c>,"
    #[test]
    fn mint_nary_reifier_golden_a_ternary_iri() {
        let got = mint_nary_reifier(
            "http://example.org/mul",
            &[
                TermValue::iri("http://example.org/a"),
                TermValue::iri("http://example.org/b"),
                TermValue::iri("http://example.org/c"),
            ],
        )
        .expect("IRI args must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/nary/5fae1c051af0f9e8d679a7b7b0b97fdc5261cec2",
            "mint_nary_reifier golden-A mismatch"
        );
    }

    /// Nary golden B: unary tuple T(x) — the arity-1 reifier recipe.
    #[test]
    fn mint_nary_reifier_golden_b_unary() {
        let got = mint_nary_reifier(
            "http://example.org/T",
            &[TermValue::iri("http://example.org/x")],
        )
        .expect("unary IRI arg must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/nary/18606f3f26d824d2930b42f058b999d930a6d081",
            "mint_nary_reifier golden-B mismatch"
        );
    }

    /// Nary golden C: ternary tuple with a typed-literal argument (xsd:integer).
    #[test]
    fn mint_nary_reifier_golden_c_typed_literal() {
        let got = mint_nary_reifier(
            "http://example.org/mul",
            &[
                TermValue::iri("http://example.org/a"),
                TermValue::iri("http://example.org/b"),
                TermValue::typed_literal("6", "http://www.w3.org/2001/XMLSchema#integer"),
            ],
        )
        .expect("typed-literal arg must not fail");
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/reifier/nary/f504fca5fb1c626e2719dbbb3d79bcb851718d9f",
            "mint_nary_reifier golden-C mismatch"
        );
    }

    /// Argument order is significant: swapping two args yields a different reifier.
    #[test]
    fn mint_nary_reifier_is_order_sensitive() {
        let abc = mint_nary_reifier(
            "http://example.org/mul",
            &[
                TermValue::iri("http://example.org/a"),
                TermValue::iri("http://example.org/b"),
                TermValue::iri("http://example.org/c"),
            ],
        )
        .unwrap();
        let acb = mint_nary_reifier(
            "http://example.org/mul",
            &[
                TermValue::iri("http://example.org/a"),
                TermValue::iri("http://example.org/c"),
                TermValue::iri("http://example.org/b"),
            ],
        )
        .unwrap();
        assert_ne!(
            abc, acb,
            "distinct argument orders must mint distinct reifiers"
        );
    }

    /// The n-ary recipe is domain-separated from the binary [`mint_reifier`]: a
    /// binary payload starts with `<`, the n-ary payload with `nary\n`, so their
    /// digests live in different prefixes and can never collide.
    #[test]
    fn mint_nary_reifier_never_collides_with_binary() {
        let binary = mint_reifier(
            &TermValue::iri("http://example.org/a"),
            "http://example.org/mul",
            &TermValue::iri("http://example.org/b"),
        )
        .unwrap();
        let nary = mint_nary_reifier(
            "http://example.org/mul",
            &[
                TermValue::iri("http://example.org/a"),
                TermValue::iri("http://example.org/b"),
            ],
        )
        .unwrap();
        assert!(binary.starts_with(REIFIER_PREFIX) && !binary.starts_with(NARY_REIFIER_PREFIX));
        assert!(nary.starts_with(NARY_REIFIER_PREFIX));
        assert_ne!(binary, nary);
    }

    // ── mint_derivation_id goldens ────────────────────────────────────────────

    /// Golden 5: two-source rule firing (sources are sorted before hashing).
    /// payload = "https://blackcatinformatics.ca/logic/rules/transitivity\n
    ///            https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a\n
    ///            https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064"
    /// sha1 = e1379d93fd46357cc6a3be9e057528bb0d589f68
    #[test]
    fn mint_derivation_id_golden_5_two_sources() {
        let rule_iri = "https://blackcatinformatics.ca/logic/rules/transitivity";
        let sources = [
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
        ];
        let got = mint_derivation_id(rule_iri, &sources);
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/derivation/e1379d93fd46357cc6a3be9e057528bb0d589f68",
            "mint_derivation_id golden-5 mismatch"
        );
    }

    /// Golden 5b: same sources in reversed order → same result (sorted).
    #[test]
    fn mint_derivation_id_golden_5_order_independent() {
        let rule_iri = "https://blackcatinformatics.ca/logic/rules/transitivity";
        let sources_fwd = [
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
        ];
        let sources_rev = [
            "https://blackcatinformatics.ca/gmeow/reifier/61194b8ccffff3db1bbb81df91c55b7776ee4064",
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
        ];
        assert_eq!(
            mint_derivation_id(rule_iri, &sources_fwd),
            mint_derivation_id(rule_iri, &sources_rev),
            "mint_derivation_id must be order-independent"
        );
    }

    /// Golden 6: assert-sentinel derivation (self-reifier as only source).
    /// payload = "https://blackcatinformatics.ca/logic/assert\n
    ///            https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a"
    /// sha1 = 5dd2eeebb9812618b81b5053f662c0756c57b2e6
    #[test]
    fn mint_derivation_id_golden_6_assert_sentinel() {
        let rule_iri = "https://blackcatinformatics.ca/logic/assert";
        let sources = [
            "https://blackcatinformatics.ca/gmeow/reifier/10d9bdab72fe25cf3b81fe842b3a105077d98a6a",
        ];
        let got = mint_derivation_id(rule_iri, &sources);
        assert_eq!(
            got,
            "https://blackcatinformatics.ca/gmeow/derivation/5dd2eeebb9812618b81b5053f662c0756c57b2e6",
            "mint_derivation_id golden-6 mismatch"
        );
    }

    // ── Goldens parity: load from JSON fixture ────────────────────────────────

    /// Load the authoritative goldens JSON and verify all entries match.
    ///
    /// This test is the normative gate: it reads the same file the Python oracle
    /// writes and asserts that every IRI the Rust engine would produce is
    /// byte-identical.
    #[test]
    fn goldens_parity_from_json_fixture() {
        // Path relative to the crate root (where Cargo.toml lives).
        // `include_str!` is relative to the source file, so use a path that
        // goes up from src/ to the repo root then down to the fixture.
        let json_text = include_str!("../../../tests/fixtures/logic/determinism-goldens.json");

        let root: serde_json::Value =
            serde_json::from_str(json_text).expect("determinism-goldens.json must be valid JSON");

        // ── Quad-reifier goldens ──────────────────────────────────────────────
        let reifier_goldens = root["quad_reifier_goldens"]
            .as_array()
            .expect("quad_reifier_goldens must be an array");

        for entry in reifier_goldens {
            let id = entry["_id"].as_str().unwrap_or("?");
            let subj_iri = entry["subject"].as_str().expect("subject");
            let pred_iri = entry["predicate"].as_str().expect("predicate");
            let expected_reifier = entry["reifier_iri"].as_str().expect("reifier_iri");
            let is_literal = entry["object_is_literal"].as_bool().unwrap_or(false);

            let s = TermValue::iri(subj_iri);

            let o: TermValue = if is_literal {
                let lex = entry["object"].as_str().expect("object lexical");
                if let Some(lang) = entry["object_lang"].as_str() {
                    TermValue::lang_literal(lex, lang)
                } else if let Some(dt_iri) = entry["object_datatype"].as_str() {
                    TermValue::typed_literal(lex, dt_iri)
                } else {
                    // Plain xsd:string
                    TermValue::simple_literal(lex)
                }
            } else {
                let obj_iri = entry["object"].as_str().expect("object IRI");
                TermValue::iri(obj_iri)
            };

            let got = mint_reifier(&s, pred_iri, &o)
                .unwrap_or_else(|e| panic!("{id}: mint_reifier failed: {e}"));
            assert_eq!(
                got, expected_reifier,
                "goldens parity FAIL for {id}: got {got:?}, expected {expected_reifier:?}"
            );
        }

        // ── Derivation-ID goldens ─────────────────────────────────────────────
        let derivation_goldens = root["derivation_id_goldens"]
            .as_array()
            .expect("derivation_id_goldens must be an array");

        for entry in derivation_goldens {
            let id = entry["_id"].as_str().unwrap_or("?");
            let rule_iri = entry["rule_iri"].as_str().expect("rule_iri");
            let expected_derivation = entry["derivation_iri"].as_str().expect("derivation_iri");
            let sources: Vec<&str> = entry["source_reifier_iris"]
                .as_array()
                .expect("source_reifier_iris")
                .iter()
                .map(|v| v.as_str().expect("source IRI"))
                .collect();

            let got = mint_derivation_id(rule_iri, &sources);
            assert_eq!(
                got, expected_derivation,
                "goldens parity FAIL for {id}: got {got:?}, expected {expected_derivation:?}"
            );
        }
    }
}
