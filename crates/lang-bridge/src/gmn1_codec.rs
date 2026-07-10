// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The GMN-0 ⇄ GMN-1 codec: the executed byte witness behind
//! `gmeow:gmnCorrNormalToGmn`'s `logic:SectionRetraction` / `logic:ExactPreservation` /
//! `logic:mnemomorphic true` declaration ([`LANG-GMN.md`](../../../slices/grounding/lang/design/LANG-GMN.md)).
//!
//! This module mirrors the byte-reconstruction discipline of
//! [`crate::superset`](../../pipeline/src/stages/superset.rs) (a sibling crate, not
//! imported here): a real writer ([`gmn1_write`]) plus a genuinely INDEPENDENT reader
//! ([`gmn1_read`]) whose round-trip over GMN-0 normal forms is executed and canonically
//! compared, never asserted on faith.
//!
//! # What "GMN-0" means here
//!
//! GMN-0 (`gmeow:gmnNormalForm`) is charter-defined as "the RDFC-1.0 canonically
//! blank-node-labeled, content-sorted term-table normal form of the bundle" — i.e. a
//! canonical *quad set*. [`Gmn0Model`] is that quad set, built from a [`purrdf::RdfDataset`]
//! and compared for canonical equality through [`purrdf::canonicalize`] — the SAME
//! canonical-comparison primitive the GTS/N-Quads byte-teeth gates use.
//!
//! # The record model (design decision, documented here because it is load-bearing)
//!
//! GMN-1's grammar (`grammars/gmn.ebnf`) has **no free-string production**: a `value` is
//! `identifier | number | list`, and `identifier` is `[A-Za-z_][A-Za-z0-9_.-]*` — no colon,
//! no arbitrary Unicode. Three consequences this codec resolves explicitly:
//!
//! 1. **IRIs** are represented as either a dictionary alias (`gmeow:gmnDictV1`, read from
//!    the compiled carrier — never hardcoded) OR a deterministic, injective
//!    prefix-mangling of the term's CURIE under the SAME prefix registry the rest of the
//!    pipeline treats as canonical (`gmeow_logic_compile::ingest::prefixes`): `prefix__local`
//!    (a `__` separator, illegal in both a real prefix and a real local name in this
//!    ontology, verified defensively). An IRI under no registered namespace is
//!    `lang:GmnUncoveredTerm`.
//! 2. **Blank nodes** are represented `_b<canonical-label>` (the RDFC-1.0 `c14nN` label),
//!    disjoint by construction from both dictionary aliases (curated, alpha-initial) and
//!    prefix-mangled tokens (never underscore-initial, since every registered prefix is
//!    alphabetic).
//! 3. **Literals** that are NOT identifier-shaped/canonical-number-shaped — arbitrary
//!    prose, `rdf:langString`, non-integer/decimal datatypes — ride **by reference**: the
//!    codec mints a content-addressed key (`r_<hash>`) and carries the literal's full
//!    lexical/datatype/language payload in the [`Gmn1Document`]'s reference table, which
//!    travels WITH the GMN-1 text as the out-of-band resolution store the charter's
//!    "by reference" language presupposes (the same idiom the envelope's digest/dictionary
//!    coordinates already use — resolution state riding beside the record text, not
//!    inlined in it). This keeps the codec TOTAL over arbitrary literal content instead of
//!    silently degrading to hard-fail on every prose string in the grounding slices.
//!
//! # Reader independence (the Section-Retraction false-tautology guard)
//!
//! [`gmn1_read`] is a genuine small parser: it tokenizes the GMN-1 TEXT byte-by-byte
//! (looking for `@gmn{`, `@claims[`, `@<sigil>{`, `,`, `:`, `}`), builds a `Record`/row IR
//! from what it reads via `parse_header`/`parse_sigil_record`/`parse_tabular_row`, and
//! only THEN decodes each field token back to a [`purrdf::RdfTerm`] via
//! `decode_reference`/`decode_value`. It never calls, matches on, or shares control flow
//! with [`gmn1_write`] — the two directions are independent code paths that happen to
//! agree because `decode_reference`/`decode_value` are the two-sided inverse of
//! `encode_reference`/`encode_value`, not because one function derives from the other.
//! Concretely: [`gmn1_read`] rejects malformed input [`gmn1_write`] could never produce
//! (a record missing its closing `}`, a key out of canonical order, a tabular row whose
//! value count does not match its declared column count, a token containing a byte
//! outside `[A-Za-z0-9_.-]`) — a property a mechanical `write.invert()` would not have,
//! because there would be nothing exercising those rejection paths.

use std::collections::BTreeMap;
use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm};

use crate::emit::digest16;

// ── Well-known predicate IRIs the compact-record folder recognizes ─────────────────

const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

const PRED_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const PRED_ACCORDING_TO: &str = "https://blackcatinformatics.ca/gmeow/accordingTo";
const PRED_EVIDENCE: &str = "https://blackcatinformatics.ca/gmeow/hasAvailableEvidence";
const PRED_MODAL_FORCE: &str = "https://blackcatinformatics.ca/gmeow/claimModalForce";
const PRED_OBSERVATION_METHOD: &str = "https://blackcatinformatics.ca/gmeow/observationMethod";
const PRED_OCCURRENT_BOUNDARY: &str = "https://blackcatinformatics.ca/logic/occurrentBoundary";
const PRED_OCCURRENCE_OF_SERIES: &str = "https://blackcatinformatics.ca/gmeow/occurrenceOfSeries";

const XSD_STRING: &str = "http://www.w3.org/2001/XMLSchema#string";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";

/// The `prefix__local` separator: illegal inside both a registered prefix and a genuine
/// local name in this ontology (checked defensively at mangle time), so the split back
/// apart on read is unambiguous.
const SEP: &str = "__";
/// The blank-node token prefix: underscore-initial, disjoint from every dictionary alias
/// and every prefix-mangled token (both always start with a letter).
const BLANK_PREFIX: &str = "_b";
/// The by-reference literal-key token prefix: reserved so a plain identifier-shaped
/// literal lexical form is never allowed to collide with a reference key (see
/// [`encode_literal`]).
const REF_PREFIX: &str = "r_";

const DICT_VERSION: &str = "1";
const DICT_ALIASES_ID: &str = "dict-v1";

// ── GMN-0: the canonical quad-set normal form ───────────────────────────────────────

/// GMN-0: the RDFC-1.0 canonically blank-node-labeled, content-sorted quad set this
/// codec round-trips. A thin, deterministic wrapper over [`purrdf::RdfQuad`] — GMN-0
/// mints no new canonical object (per the charter), so this type carries no data the
/// carrier does not already have.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gmn0Model {
    /// The quad set, in a fixed deterministic order ([`quad_sort_key`]).
    pub quads: Vec<RdfQuad>,
}

impl Gmn0Model {
    /// Build a GMN-0 model from every quad a dataset carries, deduplicated and sorted
    /// into the codec's canonical iteration order (a superset of, and independent from,
    /// RDFC-1.0's own blank-label canonicalization — that happens at comparison time via
    /// [`canonical_nquads`](Self::canonical_nquads)).
    #[must_use]
    pub fn from_dataset(ds: &RdfDataset) -> Self {
        let mut quads: Vec<RdfQuad> = ds.owned_quads().collect();
        quads.sort_by_key(quad_sort_key);
        quads.dedup_by(|a, b| quad_sort_key(a) == quad_sort_key(b));
        Self { quads }
    }

    /// Rebuild a frozen [`RdfDataset`] carrying exactly this model's quads.
    #[must_use]
    pub fn to_dataset(&self) -> Arc<RdfDataset> {
        let mut builder = RdfDatasetBuilder::new();
        for quad in &self.quads {
            builder.push_owned_quad(quad);
        }
        builder
            .freeze()
            .expect("a Gmn0Model built from valid RdfQuads freezes cleanly")
    }

    /// The RDFC-1.0 canonical N-Quads form — the single canonical-comparison primitive
    /// this codec's round-trip gate (and the GTS/N-Quads byte-teeth gates) share.
    #[must_use]
    pub fn canonical_nquads(&self) -> String {
        purrdf::canonicalize(&self.to_dataset()).nquads
    }
}

/// Two GMN-0 models are the SAME model iff their RDFC-1.0 canonical N-Quads agree —
/// the codec's round-trip gate's sole equality oracle.
#[must_use]
pub fn gmn0_canonically_equal(a: &Gmn0Model, b: &Gmn0Model) -> bool {
    a.canonical_nquads() == b.canonical_nquads()
}

/// A deterministic, total sort key over an [`RdfQuad`] (string-keyed; blank labels sort
/// by their pre-canonicalization label, which is only used for *this* codec's own stable
/// iteration order, never for cross-model comparison — that is RDFC-1.0's job).
fn quad_sort_key(q: &RdfQuad) -> (u8, String, String, u8, String, String) {
    (
        term_kind_tag(&q.subject),
        term_sort_string(&q.subject),
        q.predicate.clone(),
        term_kind_tag(&q.object),
        term_sort_string(&q.object),
        q.graph_name
            .as_ref()
            .map(term_sort_string)
            .unwrap_or_default(),
    )
}

fn term_kind_tag(t: &RdfTerm) -> u8 {
    match t {
        RdfTerm::BlankNode(_) => 0,
        RdfTerm::Iri(_) => 1,
        RdfTerm::Literal(_) => 2,
        RdfTerm::Triple(_) => 3,
    }
}

fn term_sort_string(t: &RdfTerm) -> String {
    match t {
        RdfTerm::Iri(s) | RdfTerm::BlankNode(s) => s.clone(),
        RdfTerm::Literal(l) => format!(
            "{}\u{1f}{}\u{1f}{}",
            l.lexical_form,
            l.datatype.as_deref().unwrap_or(""),
            l.language.as_deref().unwrap_or("")
        ),
        RdfTerm::Triple(t) => format!(
            "{}\u{1f}{}\u{1f}{}",
            term_sort_string(&t.subject),
            t.predicate,
            term_sort_string(&t.object)
        ),
    }
}

// ── The dictionary bijection (`gmeow:gmnDictV1`, read from the carrier) ─────────────

/// The GMN alias-table bijection, read from the compiled carrier — never hardcoded.
/// Injective over its covered term set (checked at load time, defensively: the carrier's
/// own SHACL gate is the primary authority, this is the codec's own read-back safety net).
#[derive(Debug, Clone, Default)]
pub struct GmnDictionary {
    version: String,
    term_to_alias: BTreeMap<String, String>,
    alias_to_term: BTreeMap<String, String>,
}

/// A dictionary that fails to load: not a bijection, or an alias collides with a
/// reserved token shape ([`BLANK_PREFIX`] / [`REF_PREFIX`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryError(pub String);

impl GmnDictionary {
    /// Load `gmeow:gmnDictV1` (the shipped dictionary version) from `ds`: every
    /// `gmeow:GmnDictionaryEntry` reachable via `gmeow:gmnDictionaryEntryTerm` /
    /// `gmeow:gmnDictionaryEntryAlias`, verified injective and reserved-shape-safe.
    pub fn from_dataset(ds: &RdfDataset) -> Result<Self, DictionaryError> {
        let term_pred = format!("{GMEOW_NS}gmnDictionaryEntryTerm");
        let alias_pred = format!("{GMEOW_NS}gmnDictionaryEntryAlias");

        let mut terms: BTreeMap<String, String> = BTreeMap::new();
        let mut aliases: BTreeMap<String, String> = BTreeMap::new();
        for quad in ds.owned_quads() {
            if quad.predicate == term_pred {
                let RdfTerm::Iri(subject) = &quad.subject else {
                    continue;
                };
                let RdfTerm::Iri(term) = &quad.object else {
                    continue;
                };
                terms.insert(subject.clone(), term.clone());
            } else if quad.predicate == alias_pred {
                let RdfTerm::Iri(subject) = &quad.subject else {
                    continue;
                };
                let RdfTerm::Literal(lit) = &quad.object else {
                    continue;
                };
                aliases.insert(subject.clone(), lit.lexical_form.clone());
            }
        }

        let mut term_to_alias = BTreeMap::new();
        let mut alias_to_term: BTreeMap<String, String> = BTreeMap::new();
        for (entry, term) in &terms {
            let Some(alias) = aliases.get(entry) else {
                continue;
            };
            if alias.starts_with(BLANK_PREFIX) || alias.starts_with(REF_PREFIX) {
                return Err(DictionaryError(format!(
                    "dictionary alias {alias:?} for {term} collides with a reserved token shape"
                )));
            }
            if let Some(prior) = alias_to_term.insert(alias.clone(), term.clone())
                && prior != *term
            {
                return Err(DictionaryError(format!(
                    "dictionary alias {alias:?} is not injective: both {prior} and {term} claim it"
                )));
            }
            term_to_alias.insert(term.clone(), alias.clone());
        }

        Ok(Self {
            version: DICT_VERSION.to_owned(),
            term_to_alias,
            alias_to_term,
        })
    }

    fn alias_for(&self, iri: &str) -> Option<&str> {
        self.term_to_alias.get(iri).map(String::as_str)
    }

    fn term_for(&self, alias: &str) -> Option<&str> {
        self.alias_to_term.get(alias).map(String::as_str)
    }

    /// The dictionary version this codec's `@gmn{v: 1, aliases: …}` header pins.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }
}

// ── Uncovered-term reporting (the pure error surface; ledger interning lives in
// `crate::error::GmnUncoveredTerm`, attached by the round-trip gate) ────────────────

/// A GMN-0 construct this codec cannot losslessly encode — the pure carrier of what
/// becomes a `lang:GmnUncoveredTerm` finding once interned into a
/// [`gmeow_errors::DiagLedger`] (never a silent drop, per the no-optionality rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncoveredTerm(pub String);

/// A GMN-1 write or read failure: either an uncovered construct or a malformed-text
/// parse defect (the latter only ever raised by [`gmn1_read`] on hand-corrupted input —
/// [`gmn1_write`] never produces malformed text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gmn1Error {
    /// [`UncoveredTerm`] — the construct is named so the failure is diagnosable.
    Uncovered(UncoveredTerm),
    /// A parse defect in GMN-1 text that is not an uncovered-term hard fail (a
    /// structurally malformed record, e.g. an unterminated brace).
    Malformed(String),
}

impl std::fmt::Display for Gmn1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uncovered(u) => write!(f, "lang:GmnUncoveredTerm: {}", u.0),
            Self::Malformed(m) => write!(f, "GMN-1 parse error: {m}"),
        }
    }
}

impl std::error::Error for Gmn1Error {}

// ── Term ⇄ token codec (shared by every record field position) ─────────────────────

/// A by-reference literal payload the reference table carries (langString, arbitrary
/// prose, or any non-integer/decimal datatype).
#[derive(Debug, Clone, PartialEq, Eq)]
struct RefPayload {
    lexical: String,
    datatype: Option<String>,
    language: Option<String>,
}

/// The GMN-1 artifact this codec's writer produces: the record TEXT plus the
/// content-addressed reference table the "by reference" literals/annotations
/// resolve through — the out-of-band resolution store the charter's rate–fidelity
/// contract presupposes (the same idiom the envelope's dictionary/digest coordinates
/// already use). Both travel together; the round-trip gate reads both back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Gmn1Document {
    /// The GMN-1 surface text: the `@gmn` header followed by one record (or tabular
    /// batch row) per line.
    pub text: String,
    /// Content-addressed key -> literal payload, for every "by reference" literal the
    /// text's tokens name via an `r_<hash>` key.
    refs: BTreeMap<String, RefPayload>,
}

/// Encode a REFERENCE-position term (`s p o st ev m ek bd it`: an IRI or blank node
/// naming an entity) as a GMN-1 token. Deliberately does NOT accept a literal — the
/// grammar draws no syntactic distinction between a reference and a value token, but the
/// codec's OWN field semantics do (this is the fix for the `open`-the-string vs.
/// `logic:Open`-the-dictionary-alias collision this module's tests caught): a reference
/// token never consults the numeric/reference-table decode paths, so a plain literal
/// lexical form that happens to collide with a dictionary alias (e.g. the identifier
/// literal `"open"` vs. the `bd` slot's dictionary alias `open` for `logic:Open`) can
/// never be misread as a reference, because it is never ENCODED as one — [`encode_value`]
/// handles every literal, unconditionally, through the reference table when it is not
/// safely inlinable, and NEVER through the dictionary.
fn encode_reference(
    term: &RdfTerm,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
) -> Result<String, UncoveredTerm> {
    match term {
        RdfTerm::Iri(iri) => iri_to_token(iri, dict, ns_to_prefix)
            .ok_or_else(|| UncoveredTerm(format!("IRI under no registered namespace: {iri}"))),
        RdfTerm::BlankNode(label) => {
            if !is_safe_token_body(label) {
                return Err(UncoveredTerm(format!(
                    "blank node label is not GMN-1 identifier-safe: {label}"
                )));
            }
            Ok(format!("{BLANK_PREFIX}{label}"))
        }
        RdfTerm::Literal(lit) => Err(UncoveredTerm(format!(
            "a reference-position slot (s/p/o/st/ev/m/ek/bd/it) cannot carry a literal: {lit:?}"
        ))),
        RdfTerm::Triple(_) => Err(UncoveredTerm(
            "RDF 1.2 quoted triple terms are outside this codec's covered fragment".to_owned(),
        )),
    }
}

/// Encode a VALUE-position term (`v q`: the object's own literal payload, or an asserted
/// confidence) as a GMN-1 token — a canonical number when the shape/datatype allow it,
/// otherwise a content-addressed by-reference key. Deliberately does NOT consult the
/// dictionary (see [`encode_reference`]'s doc comment for why that would be unsound).
fn encode_value(
    term: &RdfTerm,
    refs: &mut BTreeMap<String, RefPayload>,
) -> Result<String, UncoveredTerm> {
    match term {
        RdfTerm::Literal(lit) => Ok(encode_literal(lit, refs)),
        other => Err(UncoveredTerm(format!(
            "a value-position slot (v/q) must carry a literal, got: {other:?}"
        ))),
    }
}

/// `prefix__local` if `iri` starts with a registered namespace and the local part is
/// GMN-1 identifier-safe; the dictionary alias takes precedence when present (shorter,
/// and the charter's witness-carried bijection).
fn iri_to_token(
    iri: &str,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
) -> Option<String> {
    if let Some(alias) = dict.alias_for(iri) {
        return Some(alias.to_owned());
    }
    for (ns, prefix) in ns_to_prefix {
        if let Some(local) = iri.strip_prefix(ns.as_str())
            && !local.is_empty()
            && !prefix.contains(SEP)
            && let Some(mangled) = mangle_local(local)
        {
            return Some(format!("{prefix}{SEP}{mangled}"));
        }
        // The bare namespace root itself (e.g. the ontology's own base IRI, used as an
        // `owl:imports` object with no trailing slash): the local part is empty, so
        // there is nothing to mangle — the prefix ALONE is the token (still injective:
        // it is disjoint from every `prefix__local` token, which always contains SEP).
        if iri == ns.trim_end_matches('/') {
            return Some(prefix.clone());
        }
    }
    None
}

/// The reversible escape for a literal `/` inside a mangled local name — an external
/// multi-segment path IRI (e.g. `http://lexvo.org/id/iso639-3/eng` under the registered
/// `http://lexvo.org/` namespace) has segment separators the GMN-1 grammar's identifier
/// production cannot carry verbatim. Chosen to be vanishingly unlikely to occur inside a
/// genuine local name; [`mangle_local`] defensively refuses to mangle a local name that
/// already contains this sequence rather than risk an ambiguous round-trip.
const SLASH_ESCAPE: &str = "-2f-";

/// Escape every `/` in a stripped-namespace local name into [`SLASH_ESCAPE`] and verify
/// every remaining byte is a legal `nameChar`; `None` if the local name contains a byte
/// this scheme cannot carry losslessly (never a lossy best-effort escape).
fn mangle_local(local: &str) -> Option<String> {
    if local.contains(SEP) || local.contains(SLASH_ESCAPE) {
        return None;
    }
    let mut out = String::with_capacity(local.len());
    for c in local.chars() {
        if c == '/' {
            out.push_str(SLASH_ESCAPE);
        } else if c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-') {
            out.push(c);
        } else {
            return None;
        }
    }
    Some(out)
}

/// The inverse of [`mangle_local`]: unescape [`SLASH_ESCAPE`] back to `/`.
fn unmangle_local(mangled: &str) -> String {
    mangled.replace(SLASH_ESCAPE, "/")
}

/// Every byte is a legal GMN-1 `nameChar` and the string carries no [`SEP`] occurrence
/// (which would make the `prefix__local` split ambiguous on read-back).
fn is_safe_token_body(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(SEP)
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
}

/// Whether `s` is a legal GMN-1 identifier in its own right (`nameStart nameChar*`).
fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

/// Whether `s` is a canonical GMN-1 `integer` token (`-?[0-9]+`).
fn is_integer_token(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// Whether `s` is a canonical GMN-1 `number` token with the grammar's exactly-two-digit
/// fraction (`-?[0-9]+\.[0-9]{2}`).
fn is_decimal_token(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    let Some((int_part, frac_part)) = s.split_once('.') else {
        return false;
    };
    !int_part.is_empty()
        && int_part.bytes().all(|b| b.is_ascii_digit())
        && frac_part.len() == 2
        && frac_part.bytes().all(|b| b.is_ascii_digit())
}

/// Encode a literal: inline as an identifier or canonical number when the datatype and
/// shape allow it losslessly; otherwise mint a content-addressed `r_<hash>` reference
/// and carry the full payload in `refs`.
fn encode_literal(lit: &RdfLiteral, refs: &mut BTreeMap<String, RefPayload>) -> String {
    let plain_string = lit.language.is_none()
        && lit.direction.is_none()
        && lit.datatype.as_deref() == Some(XSD_STRING);
    if plain_string
        && is_identifier(&lit.lexical_form)
        && !lit.lexical_form.starts_with(BLANK_PREFIX)
        && !lit.lexical_form.starts_with(REF_PREFIX)
    {
        return lit.lexical_form.clone();
    }
    let integer_or_decimal = lit.language.is_none() && lit.direction.is_none();
    if integer_or_decimal
        && lit.datatype.as_deref() == Some(XSD_INTEGER)
        && is_integer_token(&lit.lexical_form)
    {
        return lit.lexical_form.clone();
    }
    if integer_or_decimal
        && lit.datatype.as_deref() == Some(XSD_DECIMAL)
        && is_decimal_token(&lit.lexical_form)
    {
        return lit.lexical_form.clone();
    }
    // By reference: content-addressed on the full payload, so two occurrences of the
    // same literal share one reference-table entry.
    let key = format!(
        "{REF_PREFIX}{}",
        digest16(
            "gmn1-literal",
            &format!(
                "{}\u{1f}{}\u{1f}{}",
                lit.lexical_form,
                lit.datatype.as_deref().unwrap_or(""),
                lit.language.as_deref().unwrap_or("")
            )
        )
    );
    refs.entry(key.clone()).or_insert_with(|| RefPayload {
        lexical: lit.lexical_form.clone(),
        datatype: lit.datatype.clone(),
        language: lit.language.clone(),
    });
    key
}

/// Decode a REFERENCE-position token (`s p o st ev m ek bd it`) back to an [`RdfTerm`] —
/// the two-sided inverse of [`encode_reference`]: blank prefix, dictionary alias, or a
/// prefix-mangled IRI. Deliberately never resolves a number or a by-reference literal
/// key — a reference token can only ever name an entity (see [`encode_reference`]'s doc
/// comment for the ambiguity this split forecloses).
fn decode_reference(
    token: &str,
    dict: &GmnDictionary,
    prefix_to_ns: &BTreeMap<String, String>,
) -> Result<RdfTerm, Gmn1Error> {
    if let Some(label) = token.strip_prefix(BLANK_PREFIX) {
        if !is_safe_token_body(label) {
            return Err(Gmn1Error::Malformed(format!(
                "malformed blank-node token: {token}"
            )));
        }
        return Ok(RdfTerm::BlankNode(label.to_owned()));
    }
    if let Some(term) = dict.term_for(token) {
        return Ok(RdfTerm::Iri(term.to_owned()));
    }
    if let Some((prefix, local)) = token.split_once(SEP)
        && let Some(ns) = prefix_to_ns.get(prefix)
    {
        return Ok(RdfTerm::Iri(format!("{ns}{}", unmangle_local(local))));
    }
    // The bare namespace-root form (see `iri_to_token`): a token with no SEP that
    // exactly names a registered prefix decodes to that namespace's root IRI (trailing
    // slash stripped).
    if let Some(ns) = prefix_to_ns.get(token) {
        return Ok(RdfTerm::Iri(ns.trim_end_matches('/').to_owned()));
    }
    Err(Gmn1Error::Uncovered(UncoveredTerm(format!(
        "reference token is not a blank-node token, a dictionary alias, or a \
         prefix-mangled IRI: {token}"
    ))))
}

/// Decode a VALUE-position token (`v q`) back to an [`RdfTerm::Literal`] — the
/// two-sided inverse of [`encode_value`]: a canonical number, a by-reference key
/// resolved against the document's reference table, or a bare identifier-shaped
/// `xsd:string`. Deliberately never consults the dictionary (see [`encode_value`]'s doc
/// comment).
fn decode_value(token: &str, refs: &BTreeMap<String, RefPayload>) -> Result<RdfTerm, Gmn1Error> {
    if is_integer_token(token) {
        return Ok(RdfTerm::Literal(RdfLiteral::typed(token, XSD_INTEGER)));
    }
    if is_decimal_token(token) {
        return Ok(RdfTerm::Literal(RdfLiteral::typed(token, XSD_DECIMAL)));
    }
    if token.starts_with(REF_PREFIX) {
        return match refs.get(token) {
            Some(payload) => Ok(payload_to_term(payload)),
            None => Err(Gmn1Error::Uncovered(UncoveredTerm(format!(
                "dangling by-reference token with no reference-table entry: {token}"
            )))),
        };
    }
    if is_identifier(token) {
        return Ok(RdfTerm::Literal(RdfLiteral::typed(token, XSD_STRING)));
    }
    Err(Gmn1Error::Uncovered(UncoveredTerm(format!(
        "value token is not identifier-shaped, not a canonical number, and not a \
         known by-reference key: {token}"
    ))))
}

fn payload_to_term(payload: &RefPayload) -> RdfTerm {
    RdfTerm::Literal(RdfLiteral {
        lexical_form: payload.lexical.clone(),
        datatype: payload.datatype.clone(),
        language: payload.language.clone(),
        direction: None,
    })
}

// ── Records: the s/p/o/v/q/st/ev/m/ek/bd/it field model ────────────────────────────

/// The canonical GMN-1 field key order (`s p o v q st ev m ek`, plus the `@p`-only
/// `bd it` pair), per `LANG-GMN.md` § "Record form, tabular form, and canonical order".
const KEY_ORDER: [&str; 11] = ["s", "p", "o", "v", "q", "st", "ev", "m", "ek", "bd", "it"];

const SIGIL_CLAIM: &str = "@c";
const SIGIL_PROCESS: &str = "@p";

/// One GMN-1 record: a sigil plus the ordered, sparse field map — the codec's internal
/// working representation shared by both the record-form and tabular-form writer, and
/// by the reader's parse result.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Record {
    sigil: &'static str,
    fields: BTreeMap<&'static str, String>,
}

impl Record {
    fn ordered_fields(&self) -> Vec<(&'static str, &str)> {
        KEY_ORDER
            .iter()
            .filter_map(|k| self.fields.get(k).map(|v| (*k, v.as_str())))
            .collect()
    }

    fn render_line(&self) -> String {
        let body = self
            .ordered_fields()
            .into_iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("{}{{{body}}}", self.sigil)
    }
}

/// Group a subject's quads into records, folding the recognized annotation predicates
/// into the ONE primary triple's record when — and only when — exactly one candidate
/// primary triple exists for that subject (the safe-fold guard: with zero or ≥2 primary
/// candidates the fold is ambiguous, so every triple, annotation included, falls back to
/// its own flat `@c` record — always lossless either way, per module doc "The record
/// model").
fn quads_to_records(
    quads: &[RdfQuad],
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
) -> Result<Vec<Record>, UncoveredTerm> {
    // Group by (graph, subject) preserving the model's already-sorted order.
    let mut groups: Vec<(Option<RdfTerm>, RdfTerm, Vec<&RdfQuad>)> = Vec::new();
    for q in quads {
        if let Some((g, s, bucket)) = groups.last_mut()
            && *g == q.graph_name
            && *s == q.subject
        {
            bucket.push(q);
            continue;
        }
        groups.push((q.graph_name.clone(), q.subject.clone(), vec![q]));
    }

    let mut records = Vec::new();
    for (graph, _subject, bucket) in groups {
        if graph.is_some() {
            return Err(UncoveredTerm(
                "named-graph-scoped quads are outside this codec's covered record model \
                 (no graph slot in the GMN-1 record shape)"
                    .to_owned(),
            ));
        }
        let (primary, annotation): (Vec<&&RdfQuad>, Vec<&&RdfQuad>) = bucket
            .iter()
            .partition(|q| annotation_slot(&q.predicate).is_none());

        if primary.len() == 1 {
            let host = primary[0];
            let mut fields = BTreeMap::new();
            fields.insert("s", encode_reference(&host.subject, dict, ns_to_prefix)?);
            fields.insert(
                "p",
                encode_reference(&RdfTerm::Iri(host.predicate.clone()), dict, ns_to_prefix)?,
            );
            let (obj_key, obj_tok) = encode_object(&host.object, dict, ns_to_prefix, refs)?;
            fields.insert(obj_key, obj_tok);

            let mut sigil = SIGIL_CLAIM;
            for q in &annotation {
                let slot = annotation_slot(&q.predicate).expect("partitioned as annotation");
                if slot == "bd" || slot == "it" {
                    sigil = SIGIL_PROCESS;
                }
                let tok = if slot == "q" {
                    encode_value(&q.object, refs)?
                } else {
                    encode_reference(&q.object, dict, ns_to_prefix)?
                };
                fields.insert(slot, tok);
            }
            records.push(Record { sigil, fields });
        } else {
            for q in &bucket {
                let mut fields = BTreeMap::new();
                fields.insert("s", encode_reference(&q.subject, dict, ns_to_prefix)?);
                fields.insert(
                    "p",
                    encode_reference(&RdfTerm::Iri(q.predicate.clone()), dict, ns_to_prefix)?,
                );
                let (obj_key, obj_tok) = encode_object(&q.object, dict, ns_to_prefix, refs)?;
                fields.insert(obj_key, obj_tok);
                records.push(Record {
                    sigil: SIGIL_CLAIM,
                    fields,
                });
            }
        }
    }
    Ok(records)
}

/// Encode an object term into its `(key, token)` pair: `v` (value) for a literal, `o`
/// (reference) otherwise — the o-vs-v slot split.
fn encode_object(
    object: &RdfTerm,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
) -> Result<(&'static str, String), UncoveredTerm> {
    if matches!(object, RdfTerm::Literal(_)) {
        Ok(("v", encode_value(object, refs)?))
    } else {
        Ok(("o", encode_reference(object, dict, ns_to_prefix)?))
    }
}

/// The compact-record annotation slot a well-known predicate folds into, or `None` for
/// an ordinary (primary) predicate.
fn annotation_slot(predicate: &str) -> Option<&'static str> {
    match predicate {
        PRED_CONFIDENCE => Some("q"),
        PRED_ACCORDING_TO => Some("st"),
        PRED_EVIDENCE => Some("ev"),
        PRED_MODAL_FORCE => Some("m"),
        PRED_OBSERVATION_METHOD => Some("ek"),
        PRED_OCCURRENT_BOUNDARY => Some("bd"),
        PRED_OCCURRENCE_OF_SERIES => Some("it"),
        _ => None,
    }
}

fn annotation_predicate_for_slot(slot: &str) -> Option<&'static str> {
    match slot {
        "q" => Some(PRED_CONFIDENCE),
        "st" => Some(PRED_ACCORDING_TO),
        "ev" => Some(PRED_EVIDENCE),
        "m" => Some(PRED_MODAL_FORCE),
        "ek" => Some(PRED_OBSERVATION_METHOD),
        "bd" => Some(PRED_OCCURRENT_BOUNDARY),
        "it" => Some(PRED_OCCURRENCE_OF_SERIES),
        _ => None,
    }
}

/// Reconstruct the record's constituent GMN-0 quads — the reader's inverse of
/// [`quads_to_records`]'s fold: one primary quad plus one quad per annotation slot.
fn record_to_quads(
    record: &Record,
    dict: &GmnDictionary,
    prefix_to_ns: &BTreeMap<String, String>,
    refs: &BTreeMap<String, RefPayload>,
) -> Result<Vec<RdfQuad>, Gmn1Error> {
    let s_tok = record
        .fields
        .get("s")
        .ok_or_else(|| Gmn1Error::Malformed("record is missing required key 's'".to_owned()))?;
    let p_tok = record
        .fields
        .get("p")
        .ok_or_else(|| Gmn1Error::Malformed("record is missing required key 'p'".to_owned()))?;
    let subject = decode_reference(s_tok, dict, prefix_to_ns)?;
    let RdfTerm::Iri(predicate) = decode_reference(p_tok, dict, prefix_to_ns)? else {
        return Err(Gmn1Error::Malformed(format!(
            "'p' slot must decode to an IRI, got token {p_tok}"
        )));
    };
    let object = match (record.fields.get("o"), record.fields.get("v")) {
        (Some(o_tok), None) => decode_reference(o_tok, dict, prefix_to_ns)?,
        (None, Some(v_tok)) => decode_value(v_tok, refs)?,
        (None, None) => {
            return Err(Gmn1Error::Malformed(
                "record carries neither 'o' nor 'v'".to_owned(),
            ));
        }
        (Some(_), Some(_)) => {
            return Err(Gmn1Error::Malformed(
                "record carries BOTH 'o' and 'v' — exactly one object slot is legal".to_owned(),
            ));
        }
    };

    let mut quads = vec![RdfQuad {
        subject: subject.clone(),
        predicate,
        object,
        graph_name: None,
        location: None,
    }];
    for slot in ["q", "st", "ev", "m", "ek", "bd", "it"] {
        if let Some(tok) = record.fields.get(slot) {
            let pred = annotation_predicate_for_slot(slot)
                .expect("every KEY_ORDER annotation slot has a predicate");
            let object = if slot == "q" {
                decode_value(tok, refs)?
            } else {
                decode_reference(tok, dict, prefix_to_ns)?
            };
            quads.push(RdfQuad {
                subject: subject.clone(),
                predicate: pred.to_owned(),
                object,
                graph_name: None,
                location: None,
            });
        }
    }
    Ok(quads)
}

// ── The prefix registry adapter ─────────────────────────────────────────────────────

/// Supplemental `(namespace, prefix)` pairs this codec adds ON TOP OF the pipeline-wide
/// prefix registry: the slice-document identity IRIs (`rdfs:isDefinedBy
/// <https://blackcatinformatics.ca/gmeow/slices/lang>` and kin) live one path segment
/// below the bare `gmeow:` namespace, so the generic registry's `gmeow` prefix alone
/// cannot mangle them (the local part `slices/lang` contains `/`, illegal in a GMN-1
/// identifier). This is NOT a rival naming scheme — every slice base name here (`lang`,
/// `logic`, `math`, …) is already a safe bare identifier, so `slice__lang` is exactly
/// the SAME `prefix__local` scheme one namespace segment deeper, never a second
/// convention.
const SUPPLEMENTAL_NAMESPACES: &[(&str, &str)] = &[
    ("https://blackcatinformatics.ca/gmeow/slices/", "slice"),
    // The example-instance base every slice's `examples/*.ttl` demonstrator individuals
    // live under (the same `http://example.org/...` convention `EXAMPLE_BASE`s across the
    // bridge/registry producers already use) — `mangle_local`'s generic `/`-escaping
    // covers any per-slice sub-path (`math/causalComplement`, `lang/grammar/…`) beneath it.
    ("http://example.org/", "ex"),
];

/// `(namespace, prefix)` pairs, longest-namespace-first, matching the pipeline-wide
/// canonical prefix authority (`gmeow_logic_compile::ingest::prefixes::registry_pairs`),
/// so `prefix__local` mangling is never a rival naming scheme.
fn ns_to_prefix_table() -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = gmeow_logic_compile::ingest::prefixes::registry_pairs()
        .into_iter()
        .map(|(prefix, ns)| (ns, prefix))
        .chain(
            SUPPLEMENTAL_NAMESPACES
                .iter()
                .map(|(ns, prefix)| ((*ns).to_owned(), (*prefix).to_owned())),
        )
        .collect();
    pairs.sort_by_key(|(ns, _)| std::cmp::Reverse(ns.len()));
    pairs
}

fn prefix_to_ns_table() -> BTreeMap<String, String> {
    gmeow_logic_compile::ingest::prefixes::registry_pairs()
        .into_iter()
        .chain(
            SUPPLEMENTAL_NAMESPACES
                .iter()
                .map(|(ns, prefix)| ((*prefix).to_owned(), (*ns).to_owned())),
        )
        .collect()
}

// ── The writer ───────────────────────────────────────────────────────────────────────

/// GMN-0 → GMN-1: the forward/put leg. Total over any [`Gmn0Model`] whose quads are
/// default-graph, plain IRI/blank/literal-object triples under a registered namespace
/// (the grounding slices' fragment) — hard-fails as [`Gmn1Error::Uncovered`] on a
/// named-graph quad, a quoted-triple term, or an IRI under no registered namespace,
/// never a silent drop.
pub fn gmn1_write(model: &Gmn0Model, dict: &GmnDictionary) -> Result<Gmn1Document, Gmn1Error> {
    let ns_to_prefix = ns_to_prefix_table();
    let mut refs = BTreeMap::new();
    let records = quads_to_records(&model.quads, dict, &ns_to_prefix, &mut refs)
        .map_err(Gmn1Error::Uncovered)?;

    let mut lines = vec![format!(
        "@gmn{{v: {DICT_VERSION}, aliases: {DICT_ALIASES_ID}}}"
    )];
    for record in &records {
        lines.push(record.render_line());
    }
    Ok(Gmn1Document {
        text: lines.join("\n") + "\n",
        refs,
    })
}

/// GMN-0 → GMN-1, tabular form: emits `@c`-only records sharing an identical key schema
/// as one schema-once `@claims[...]` batch when the WHOLE model's records are uniform;
/// any non-uniform model (mixed sigils, mixed key schemas) degrades to the record form,
/// which is always correct — tabular is a token-economy OPTIMIZATION over the same
/// records, never a distinct semantic surface. Used by the round-trip fixture proving
/// "two GMN-1 surfaces canonicalize to one GMN-0."
pub fn gmn1_write_tabular(
    model: &Gmn0Model,
    dict: &GmnDictionary,
) -> Result<Gmn1Document, Gmn1Error> {
    let ns_to_prefix = ns_to_prefix_table();
    let mut refs = BTreeMap::new();
    let records = quads_to_records(&model.quads, dict, &ns_to_prefix, &mut refs)
        .map_err(Gmn1Error::Uncovered)?;

    let mut lines = vec![format!(
        "@gmn{{v: {DICT_VERSION}, aliases: {DICT_ALIASES_ID}}}"
    )];

    let uniform_schema: Option<Vec<&'static str>> = records.first().and_then(|first| {
        if first.sigil != SIGIL_CLAIM {
            return None;
        }
        let schema: Vec<&'static str> =
            first.ordered_fields().into_iter().map(|(k, _)| k).collect();
        let all_match = records.iter().all(|r| {
            r.sigil == SIGIL_CLAIM
                && r.ordered_fields()
                    .into_iter()
                    .map(|(k, _)| k)
                    .collect::<Vec<_>>()
                    == schema
        });
        all_match.then_some(schema)
    });

    match uniform_schema {
        Some(schema) if !records.is_empty() => {
            lines.push(format!("@claims[{}]", schema.join(" ")));
            for record in &records {
                let row = record
                    .ordered_fields()
                    .into_iter()
                    .map(|(_, v)| v.to_owned())
                    .collect::<Vec<_>>()
                    .join(" ");
                lines.push(row);
            }
        }
        _ => {
            for record in &records {
                lines.push(record.render_line());
            }
        }
    }

    Ok(Gmn1Document {
        text: lines.join("\n") + "\n",
        refs,
    })
}

// ── The reader (a genuinely independent parser — see module docs) ──────────────────

/// GMN-1 → GMN-0: the backward/get leg. A hand-written, table-driven line parser — see
/// the module documentation for the concrete reader-independence argument. Hard-fails as
/// [`Gmn1Error::Malformed`] on structurally invalid text and as [`Gmn1Error::Uncovered`]
/// on a token this codec's covered fragment does not decode.
pub fn gmn1_read(doc: &Gmn1Document, dict: &GmnDictionary) -> Result<Gmn0Model, Gmn1Error> {
    let prefix_to_ns = prefix_to_ns_table();
    let mut lines = doc.text.lines();

    let header = lines
        .next()
        .ok_or_else(|| Gmn1Error::Malformed("empty GMN-1 document: no @gmn header".to_owned()))?;
    parse_header(header)?;

    let mut records: Vec<Record> = Vec::new();
    let mut pending_columns: Option<Vec<String>> = None;
    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(rest) = line.strip_prefix("@claims[") {
            let cols = rest.strip_suffix(']').ok_or_else(|| {
                Gmn1Error::Malformed(format!("unterminated @claims header: {line}"))
            })?;
            pending_columns = Some(
                cols.split(' ')
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
            );
            continue;
        }
        if line.starts_with('@') && line.contains('{') {
            pending_columns = None;
            records.push(parse_sigil_record(line)?);
            continue;
        }
        if let Some(cols) = &pending_columns {
            records.push(parse_tabular_row(cols, line)?);
            continue;
        }
        return Err(Gmn1Error::Malformed(format!(
            "line matches neither a sigil record, a @claims header, nor a pending tabular row: {line}"
        )));
    }

    let mut quads = Vec::new();
    for record in &records {
        quads.extend(record_to_quads(record, dict, &prefix_to_ns, &doc.refs)?);
    }
    quads.sort_by_key(quad_sort_key);
    quads.dedup_by(|a, b| quad_sort_key(a) == quad_sort_key(b));
    Ok(Gmn0Model { quads })
}

/// Parse `@gmn{v: 1, aliases: dict-v1}` by explicit token scanning — never by
/// re-deriving from what [`gmn1_write`] would emit.
fn parse_header(line: &str) -> Result<(), Gmn1Error> {
    let body = line
        .strip_prefix("@gmn{")
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| {
            Gmn1Error::Malformed(format!(
                "GMN-1 text must open with an @gmn{{...}} header, got: {line}"
            ))
        })?;
    let mut version_ok = false;
    let mut aliases_ok = false;
    for pair in body.split(',') {
        let (k, v) = pair
            .split_once(':')
            .ok_or_else(|| Gmn1Error::Malformed(format!("malformed header pair: {pair}")))?;
        let (k, v) = (k.trim(), v.trim());
        match k {
            "v" => version_ok = v == DICT_VERSION,
            "aliases" => aliases_ok = v == DICT_ALIASES_ID,
            other => {
                return Err(Gmn1Error::Malformed(format!(
                    "unrecognized @gmn header key: {other}"
                )));
            }
        }
    }
    if !version_ok || !aliases_ok {
        return Err(Gmn1Error::Malformed(format!(
            "@gmn header does not pin the expected schema/dictionary version: {line}"
        )));
    }
    Ok(())
}

/// Parse one `@sigil{k: v, k: v, ...}` record line.
fn parse_sigil_record(line: &str) -> Result<Record, Gmn1Error> {
    let brace = line
        .find('{')
        .ok_or_else(|| Gmn1Error::Malformed(format!("record line has no '{{': {line}")))?;
    let sigil_str = &line[..brace];
    let sigil = match sigil_str {
        "@c" => SIGIL_CLAIM,
        "@p" => SIGIL_PROCESS,
        other => {
            return Err(Gmn1Error::Uncovered(UncoveredTerm(format!(
                "sigil {other} is outside this codec's covered record model"
            ))));
        }
    };
    let body = line
        .strip_suffix('}')
        .map(|s| &s[brace + 1..])
        .ok_or_else(|| Gmn1Error::Malformed(format!("unterminated record body: {line}")))?;

    let mut fields = BTreeMap::new();
    let mut last_key_rank: Option<usize> = None;
    if !body.trim().is_empty() {
        for pair in body.split(',') {
            let (k, v) = pair
                .split_once(':')
                .ok_or_else(|| Gmn1Error::Malformed(format!("malformed field pair: {pair}")))?;
            let (k, v) = (k.trim(), v.trim());
            let rank = KEY_ORDER
                .iter()
                .position(|candidate| *candidate == k)
                .ok_or_else(|| {
                    Gmn1Error::Uncovered(UncoveredTerm(format!(
                        "record key '{k}' is outside the canonical key order"
                    )))
                })?;
            if let Some(last) = last_key_rank
                && rank <= last
            {
                return Err(Gmn1Error::Malformed(format!(
                    "record key '{k}' violates the canonical key order (s p o v q st ev m ek bd it): {line}"
                )));
            }
            last_key_rank = Some(rank);
            fields.insert(KEY_ORDER[rank], v.to_owned());
        }
    }
    Ok(Record { sigil, fields })
}

/// Parse one bare tabular row against the pending `@claims[...]` column schema.
fn parse_tabular_row(cols: &[String], line: &str) -> Result<Record, Gmn1Error> {
    let values: Vec<&str> = line.split(' ').filter(|s| !s.is_empty()).collect();
    if values.len() != cols.len() {
        return Err(Gmn1Error::Malformed(format!(
            "tabular row has {} value(s) but the declared schema has {} column(s): {line}",
            values.len(),
            cols.len()
        )));
    }
    let mut fields = BTreeMap::new();
    for (col, val) in cols.iter().zip(values) {
        let key = KEY_ORDER
            .iter()
            .find(|candidate| **candidate == col.as_str())
            .ok_or_else(|| {
                Gmn1Error::Uncovered(UncoveredTerm(format!(
                    "tabular column '{col}' is outside the canonical key order"
                )))
            })?;
        fields.insert(*key, val.to_owned());
    }
    Ok(Record {
        sigil: SIGIL_CLAIM,
        fields,
    })
}

// ── The round-trip check (the codec's own pure gate primitive) ─────────────────────

/// Run `gmn1_read(gmn1_write(model))` and assert canonical equality via
/// [`purrdf::canonicalize`] — the pure primitive the executed pipeline gate
/// (`crates/pipeline/src/stages/gmn1_gate.rs`) and this crate's own fixture tests both
/// call. `Ok(())` iff the round-trip is exact; `Err` names the concrete failure
/// (write-side uncovered term, read-side parse/uncovered defect, or a canonical
/// mismatch between the original and reconstructed model).
pub fn round_trip_check(model: &Gmn0Model, dict: &GmnDictionary) -> Result<(), Gmn1Error> {
    let doc = gmn1_write(model, dict)?;
    let reconstructed = gmn1_read(&doc, dict)?;
    if gmn0_canonically_equal(model, &reconstructed) {
        Ok(())
    } else {
        Err(Gmn1Error::Malformed(format!(
            "round-trip canonical mismatch:\n--- original ---\n{}\n--- reconstructed ---\n{}",
            model.canonical_nquads(),
            reconstructed.canonical_nquads()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::parse_dataset;

    fn empty_dict() -> GmnDictionary {
        GmnDictionary::default()
    }

    fn lang_module_dataset() -> Arc<RdfDataset> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../slices/grounding/lang/module.ttl"
        );
        let bytes = std::fs::read(path).expect("lang module.ttl is readable");
        parse_dataset(&bytes, "text/turtle", None).expect("lang module.ttl parses")
    }

    fn real_dict() -> GmnDictionary {
        GmnDictionary::from_dataset(&lang_module_dataset()).expect("dictionary loads")
    }

    #[test]
    fn identifier_shape_recognizes_grammar_productions() {
        assert!(is_identifier("gate1"));
        assert!(is_identifier("_leading"));
        assert!(!is_identifier(""));
        assert!(!is_identifier("1leading"));
        assert!(!is_identifier("has:colon"));
        assert!(!is_identifier("has space"));
    }

    #[test]
    fn number_tokens_recognize_grammar_productions() {
        assert!(is_integer_token("42"));
        assert!(is_integer_token("-3"));
        assert!(!is_integer_token("3.14"));
        assert!(is_decimal_token("0.95"));
        assert!(is_decimal_token("-1.00"));
        assert!(!is_decimal_token("0.9"));
        assert!(!is_decimal_token("0.951"));
        assert!(!is_decimal_token("9.5e-1"));
    }

    #[test]
    fn simple_triple_round_trips_with_empty_dictionary() {
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri(&format!("{GMEOW_NS}gate1"));
        let p = b.intern_iri(&format!("{GMEOW_NS}hasState"));
        let o = b.intern_iri(&format!("{GMEOW_NS}doorGate1"));
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("freeze");
        let model = Gmn0Model::from_dataset(&ds);
        let dict = empty_dict();
        round_trip_check(&model, &dict).expect("plain triple round-trips");
    }

    #[test]
    fn iri_under_no_registered_namespace_is_uncovered() {
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri("https://not-registered.example/subject");
        let p = b.intern_iri(&format!("{GMEOW_NS}hasState"));
        let o = b.intern_iri(&format!("{GMEOW_NS}doorGate1"));
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("freeze");
        let model = Gmn0Model::from_dataset(&ds);
        let dict = empty_dict();
        let err = round_trip_check(&model, &dict).expect_err("must hard-fail, not drop");
        match err {
            Gmn1Error::Uncovered(_) => {}
            other => panic!("expected Uncovered, got {other:?}"),
        }
    }

    #[test]
    fn real_dictionary_loads_and_is_injective() {
        let dict = real_dict();
        assert!(
            dict.term_to_alias.len() >= 15,
            "expected at least the 15 authored dict-v1 entries, got {}",
            dict.term_to_alias.len()
        );
        assert_eq!(
            dict.alias_for(&format!("{GMEOW_NS}modalForceNecessary")),
            Some("nec")
        );
        assert_eq!(
            dict.term_for("nec"),
            Some(format!("{GMEOW_NS}modalForceNecessary").as_str())
        );
    }

    #[test]
    fn boundary_predicate_uses_the_logic_namespace() {
        // Sanity: the constant matches the real logic: namespace, not a typo.
        assert!(PRED_OCCURRENT_BOUNDARY.starts_with("https://blackcatinformatics.ca/logic/"));
    }
}
