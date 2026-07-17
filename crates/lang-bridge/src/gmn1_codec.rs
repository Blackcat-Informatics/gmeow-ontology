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
//! 1. **IRIs** are represented as either a dictionary alias (`gmeow:gmnDictV3`, read from
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

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use purrdf::{RdfDataset, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfTerm, RdfTriple};
use unicode_normalization::is_nfc;
use unicode_security::skeleton;

use crate::emit::digest16;

// ── Well-known predicate IRIs the compact-record folder recognizes ─────────────────

#[cfg(test)]
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const CLASS_DENOTATION: &str = "https://blackcatinformatics.ca/lang/Denotation";
const CLASS_SCRIPT: &str = "https://blackcatinformatics.ca/lang/Script";
const CLASS_GMN_CODEBOOK: &str = "https://blackcatinformatics.ca/gmeow/GmnCodebook";
const CLASS_GMN_DICTIONARY: &str = "https://blackcatinformatics.ca/gmeow/GmnDictionary";
const CLASS_GMN_SYMBOL_CANDIDATE: &str = "https://blackcatinformatics.ca/gmeow/GmnSymbolCandidate";
const CURRENT_CODEBOOK: &str = "https://blackcatinformatics.ca/gmeow/gmnCodebookCurrent";
const DISPOSITION_ADOPTED_GLYPH: &str =
    "https://blackcatinformatics.ca/gmeow/gmnDispositionAdoptedGlyph";
const PRED_REFERENCES: &str = "https://blackcatinformatics.ca/gmeow/references";
const PRED_HAS_GRAPHEME: &str = "https://blackcatinformatics.ca/lang/hasGrapheme";
const PRED_DENOTED_FORM: &str = "https://blackcatinformatics.ca/lang/denotedForm";
const PRED_DENOTATION_TARGET: &str = "https://blackcatinformatics.ca/lang/denotationTarget";
const PRED_GMN_DENOTATION_GRAPHEME: &str =
    "https://blackcatinformatics.ca/gmeow/gmnDenotationGrapheme";
const PRED_GMN_CODEPOINTS: &str = "https://blackcatinformatics.ca/gmeow/gmnCodepoints";
const PRED_GMN_SIGIL_SCOPE: &str = "https://blackcatinformatics.ca/gmeow/gmnSigilScope";
const PRED_GMN_SIGIL_GLYPH: &str = "https://blackcatinformatics.ca/gmeow/gmnSigilGlyph";
const PRED_GMN_FIXITY: &str = "https://blackcatinformatics.ca/gmeow/gmnFixity";
const PRED_GMN_ARITY: &str = "https://blackcatinformatics.ca/gmeow/gmnArity";
const PRED_GMN_CANDIDATE_DENOTATION: &str =
    "https://blackcatinformatics.ca/gmeow/gmnCandidateDenotation";
const PRED_GMN_ASCII_FALLBACK: &str = "https://blackcatinformatics.ca/gmeow/gmnAsciiFallback";
const PRED_GMN_SYMBOL_DISPOSITION: &str =
    "https://blackcatinformatics.ca/gmeow/gmnSymbolDisposition";
const PRED_GMN_DICTIONARY_ENTRY: &str = "https://blackcatinformatics.ca/gmeow/gmnDictionaryEntry";
const PRED_GMN_DICTIONARY_ENTRY_TERM: &str =
    "https://blackcatinformatics.ca/gmeow/gmnDictionaryEntryTerm";
const PRED_GMN_DICTIONARY_ENTRY_ALIAS: &str =
    "https://blackcatinformatics.ca/gmeow/gmnDictionaryEntryAlias";
const PRED_GMN_DICTIONARY_VERSION: &str =
    "https://blackcatinformatics.ca/gmeow/gmnDictionaryVersion";
const PRED_GMN_GLYPH_TABLE_VERSION: &str =
    "https://blackcatinformatics.ca/gmeow/gmnGlyphTableVersion";

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
/// [`classify_literal`]).
const REF_PREFIX: &str = "r_";

const DIALECT_VERSION: &str = "1";
const DICTIONARY_VERSION: &str = "3";
const GLYPH_VERSION: &str = "2";

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

// ── The graph-derived glyph registry ───────────────────────────────────────────────

/// The executable operator signature carried by the canonical GMN form behind a
/// `lang:Denotation`. Constants have neither coordinate; operators have BOTH. Keeping
/// these coordinates in the registry key prevents typography alone from silently
/// conflating, for example, unary and binary uses of one glyph.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
struct GmnGlyphSignature {
    fixity: Option<String>,
    arity: Option<u32>,
}

/// The current codebook selection resolved from the graph. The loader follows
/// `gmeow:gmnCodebookCurrent` through `gmeow:references`; it never treats unrelated
/// historical dictionaries or codebooks in the same carrier as current inventory.
///
/// Public because it is one of the two clean carriers of codebook identity the
/// codebook-digest layer ([`crate::gmn1_digest::codebook_digest`]) hashes over — the
/// reference inventory, script graphemes, and pinned versions this struct holds are
/// Merkle leaves of the codebook's content address.
#[derive(Debug, Clone)]
pub struct CurrentCodebook {
    /// The codebook's `gmeow:references` inventory (dictionary, script, sigil roles).
    pub references: BTreeSet<String>,
    /// The pinned `gmeow:gmnDictionaryVersion`.
    pub dictionary_version: String,
    /// The pinned `gmeow:gmnGlyphTableVersion`.
    pub glyph_version: String,
    /// The current script's `lang:hasGrapheme` inventory.
    pub graphemes: BTreeSet<String>,
    /// The current dictionary's `gmeow:gmnDictionaryEntry` members.
    pub dictionary_entries: BTreeSet<String>,
}

/// The codec's pinned GMN-1 dialect (schema) version — the `v:` coordinate the
/// `@gmn{…}` header pins and the first Merkle leaf of the codebook digest. Exposed so
/// the digest layer folds over the SAME constant the writer emits, never a second copy.
#[must_use]
pub(crate) fn dialect_version() -> &'static str {
    DIALECT_VERSION
}

/// The executable GMN glyph table, derived from canonical, typed
/// `lang:Denotation` records in the current codebook's script.
///
/// Each key includes `(record sigil, term-or-glyph, fixity, arity)`: sigil scope is
/// still the primary disambiguation boundary, and the authored operator signature is
/// an additional, executable criterion. Bare record tokens remain legal only when
/// that scoped term/glyph has ONE authored signature. ASCII fallbacks are read aliases
/// on the same scoped signature and are never the writer's canonical spelling.
#[derive(Debug, Clone)]
pub struct GmnGlyphRegistry {
    version: String,
    term_to_glyph: BTreeMap<(String, String, GmnGlyphSignature), String>,
    glyph_to_term: BTreeMap<(String, String, GmnGlyphSignature), String>,
    fallback_to_term: BTreeMap<(String, String, GmnGlyphSignature), String>,
}

impl Default for GmnGlyphRegistry {
    fn default() -> Self {
        Self {
            version: GLYPH_VERSION.to_owned(),
            term_to_glyph: BTreeMap::new(),
            glyph_to_term: BTreeMap::new(),
            fallback_to_term: BTreeMap::new(),
        }
    }
}

/// A canonical glyph-table defect: an incomplete denotation, malformed codepoint
/// sequence, unknown scope, or collision inside one sigil scope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GlyphRegistryError(pub String);

impl GmnGlyphRegistry {
    /// Build the glyph table from the current codebook graph. A binding exists only
    /// when a node is explicitly typed `lang:Denotation`, carries both
    /// `lang:denotationTarget` and `gmeow:gmnDenotationGrapheme`, and is the adopted
    /// candidate's `gmeow:gmnCandidateDenotation`. The denoted form supplies the
    /// `gmeow:gmnFixity`/`gmeow:gmnArity` signature, the grapheme supplies canonical
    /// codepoints and scope, and the adopted candidate supplies the executable ASCII
    /// read fallback. No local-name convention or label parsing participates.
    pub fn from_dataset(ds: &RdfDataset) -> Result<Self, GlyphRegistryError> {
        let codebook = resolve_current_codebook(ds)?;
        let mut types = BTreeMap::<String, BTreeSet<String>>::new();
        let mut den_targets = BTreeMap::<String, BTreeSet<String>>::new();
        let mut den_graphemes = BTreeMap::<String, String>::new();
        let mut den_forms = BTreeMap::<String, BTreeSet<String>>::new();
        let mut grapheme_codepoints = BTreeMap::<String, String>::new();
        let mut grapheme_scopes = BTreeMap::<String, String>::new();
        let mut role_sigils = BTreeMap::<String, String>::new();
        let mut form_fixities = BTreeMap::<String, String>::new();
        let mut form_arities = BTreeMap::<String, u32>::new();
        let mut candidate_denotations = BTreeMap::<String, String>::new();
        let mut candidate_fallbacks = BTreeMap::<String, String>::new();
        let mut candidate_dispositions = BTreeMap::<String, String>::new();
        let mut candidate_arities = BTreeMap::<String, u32>::new();

        for quad in ds.owned_quads() {
            let RdfTerm::Iri(subject) = &quad.subject else {
                continue;
            };
            match quad.predicate.as_str() {
                RDF_TYPE => {
                    if let RdfTerm::Iri(class) = &quad.object {
                        types
                            .entry(subject.clone())
                            .or_default()
                            .insert(class.clone());
                    }
                }
                PRED_DENOTED_FORM => {
                    if let RdfTerm::Iri(form) = &quad.object {
                        den_forms
                            .entry(subject.clone())
                            .or_default()
                            .insert(form.clone());
                    }
                }
                PRED_DENOTATION_TARGET => {
                    if let RdfTerm::Iri(target) = &quad.object {
                        den_targets
                            .entry(subject.clone())
                            .or_default()
                            .insert(target.clone());
                    }
                }
                PRED_GMN_DENOTATION_GRAPHEME => {
                    if let RdfTerm::Iri(grapheme) = &quad.object {
                        insert_unique(
                            &mut den_graphemes,
                            subject,
                            grapheme,
                            "denotation grapheme",
                        )?;
                    }
                }
                PRED_GMN_CODEPOINTS => {
                    if let RdfTerm::Literal(literal) = &quad.object {
                        insert_unique(
                            &mut grapheme_codepoints,
                            subject,
                            &literal.lexical_form,
                            "grapheme codepoints",
                        )?;
                    }
                }
                PRED_GMN_SIGIL_SCOPE => {
                    if let RdfTerm::Iri(scope) = &quad.object {
                        insert_unique(&mut grapheme_scopes, subject, scope, "glyph scope")?;
                    }
                }
                PRED_GMN_SIGIL_GLYPH => {
                    if let RdfTerm::Literal(literal) = &quad.object {
                        insert_unique(
                            &mut role_sigils,
                            subject,
                            &literal.lexical_form,
                            "sigil glyph",
                        )?;
                    }
                }
                PRED_GMN_FIXITY => {
                    if let RdfTerm::Iri(fixity) = &quad.object {
                        insert_unique(&mut form_fixities, subject, fixity, "GMN fixity")?;
                    }
                }
                PRED_GMN_ARITY => {
                    if let RdfTerm::Literal(arity) = &quad.object {
                        insert_unique_u32(
                            &mut form_arities,
                            subject,
                            &arity.lexical_form,
                            "GMN form arity",
                        )?;
                        insert_unique_u32(
                            &mut candidate_arities,
                            subject,
                            &arity.lexical_form,
                            "GMN candidate arity",
                        )?;
                    }
                }
                PRED_GMN_CANDIDATE_DENOTATION => {
                    if let RdfTerm::Iri(denotation) = &quad.object {
                        insert_unique(
                            &mut candidate_denotations,
                            subject,
                            denotation,
                            "candidate denotation",
                        )?;
                    }
                }
                PRED_GMN_ASCII_FALLBACK => {
                    if let RdfTerm::Literal(fallback) = &quad.object {
                        insert_unique(
                            &mut candidate_fallbacks,
                            subject,
                            &fallback.lexical_form,
                            "candidate ASCII fallback",
                        )?;
                    }
                }
                PRED_GMN_SYMBOL_DISPOSITION => {
                    if let RdfTerm::Iri(disposition) = &quad.object {
                        insert_unique(
                            &mut candidate_dispositions,
                            subject,
                            disposition,
                            "candidate disposition",
                        )?;
                    }
                }
                _ => {}
            }
        }

        let current_denotations = den_graphemes
            .iter()
            .filter_map(|(denotation, grapheme)| {
                codebook
                    .graphemes
                    .contains(grapheme)
                    .then_some(denotation.as_str())
            })
            .collect::<BTreeSet<_>>();
        let mut adopted_by_denotation = BTreeMap::<String, String>::new();
        for (candidate, disposition) in &candidate_dispositions {
            if disposition != DISPOSITION_ADOPTED_GLYPH {
                continue;
            }
            if !has_type(&types, candidate, CLASS_GMN_SYMBOL_CANDIDATE) {
                return Err(GlyphRegistryError(format!(
                    "adopted glyph candidate {candidate} is not typed gmeow:GmnSymbolCandidate"
                )));
            }
            let denotation = candidate_denotations.get(candidate).ok_or_else(|| {
                GlyphRegistryError(format!(
                    "adopted glyph candidate {candidate} has no gmnCandidateDenotation"
                ))
            })?;
            if !current_denotations.contains(denotation.as_str()) {
                continue;
            }
            if let Some(prior) = adopted_by_denotation.insert(denotation.clone(), candidate.clone())
                && prior != *candidate
            {
                return Err(GlyphRegistryError(format!(
                    "denotation {denotation} is claimed by two adopted glyph candidates: {prior} and {candidate}"
                )));
            }
        }

        let mut registry = Self {
            version: codebook.glyph_version.clone(),
            ..Self::default()
        };
        let mut skeleton_to_glyph = BTreeMap::<(String, String), String>::new();
        let mut processed_candidates = BTreeSet::<String>::new();
        for (denotation, grapheme) in den_graphemes {
            if !codebook.graphemes.contains(&grapheme) {
                continue;
            }
            if !has_type(&types, &denotation, CLASS_DENOTATION) {
                return Err(GlyphRegistryError(format!(
                    "glyph denotation {denotation} is not typed lang:Denotation"
                )));
            }
            let targets = den_targets.get(&denotation).ok_or_else(|| {
                GlyphRegistryError(format!(
                    "glyph denotation {denotation} has no lang:denotationTarget"
                ))
            })?;
            if targets.len() != 1 {
                return Err(GlyphRegistryError(format!(
                    "glyph denotation {denotation} must have exactly one target, found {}",
                    targets.len()
                )));
            }
            let target = targets
                .iter()
                .next()
                .expect("the exactly-one target set is non-empty");
            let forms = den_forms.get(&denotation).ok_or_else(|| {
                GlyphRegistryError(format!(
                    "glyph denotation {denotation} has no lang:denotedForm"
                ))
            })?;
            if forms.len() != 1 {
                return Err(GlyphRegistryError(format!(
                    "glyph denotation {denotation} must have exactly one denoted form, found {}",
                    forms.len()
                )));
            }
            let form = forms
                .iter()
                .next()
                .expect("the exactly-one denoted-form set is non-empty");
            let signature = GmnGlyphSignature {
                fixity: form_fixities.get(form).cloned(),
                arity: form_arities.get(form).copied(),
            };
            if signature.fixity.is_some() != signature.arity.is_some() {
                return Err(GlyphRegistryError(format!(
                    "GMN form {form} must author gmnFixity and gmnArity together"
                )));
            }
            let codepoints = grapheme_codepoints.get(&grapheme).ok_or_else(|| {
                GlyphRegistryError(format!(
                    "glyph denotation {denotation} names grapheme {grapheme} with no gmnCodepoints"
                ))
            })?;
            let glyph = decode_codepoint_sequence(codepoints)?;
            validate_glyph_surface(&glyph)?;
            let sigil = match grapheme_scopes.get(&grapheme) {
                Some(scope) => {
                    if !codebook.references.contains(scope) {
                        return Err(GlyphRegistryError(format!(
                            "grapheme {grapheme} names sigil scope {scope} outside the current codebook"
                        )));
                    }
                    let sigil = role_sigils.get(scope).cloned().ok_or_else(|| {
                        GlyphRegistryError(format!(
                            "grapheme {grapheme} names unknown sigil scope {scope}"
                        ))
                    })?;
                    if !KNOWN_SIGILS.contains(&sigil.as_str()) {
                        return Err(GlyphRegistryError(format!(
                            "grapheme {grapheme} names unsupported GMN sigil {sigil:?}"
                        )));
                    }
                    sigil
                }
                None => String::new(),
            };
            let candidate = adopted_by_denotation.get(&denotation).ok_or_else(|| {
                GlyphRegistryError(format!(
                    "executable glyph denotation {denotation} has no adopted GmnSymbolCandidate"
                ))
            })?;
            let fallback = candidate_fallbacks.get(candidate).ok_or_else(|| {
                GlyphRegistryError(format!(
                    "adopted glyph candidate {candidate} has no gmnAsciiFallback"
                ))
            })?;
            if !is_identifier(fallback)
                || fallback.starts_with(BLANK_PREFIX)
                || fallback.starts_with(REF_PREFIX)
            {
                return Err(GlyphRegistryError(format!(
                    "adopted glyph candidate {candidate} has non-executable ASCII fallback {fallback:?}"
                )));
            }
            if candidate_arities.get(candidate).copied() != signature.arity {
                return Err(GlyphRegistryError(format!(
                    "adopted glyph candidate {candidate} arity {:?} disagrees with denoted form {form} arity {:?}",
                    candidate_arities.get(candidate),
                    signature.arity
                )));
            }

            let term_key = (sigil.clone(), target.clone(), signature.clone());
            let glyph_key = (sigil.clone(), glyph.clone(), signature.clone());
            let fallback_key = (sigil.clone(), fallback.clone(), signature.clone());
            let skeleton_key = (sigil.clone(), skeleton(&glyph).collect::<String>());
            if let Some(prior) = skeleton_to_glyph.insert(skeleton_key, glyph.clone())
                && prior != glyph
            {
                return Err(GlyphRegistryError(format!(
                    "glyph {glyph:?} is UTS #39-confusable with {prior:?} in scope {sigil:?}"
                )));
            }
            if let Some(prior) = registry.term_to_glyph.insert(term_key, glyph.clone())
                && prior != glyph
            {
                return Err(GlyphRegistryError(format!(
                    "term {target} has two glyphs in scope {sigil:?}: {prior:?} and {glyph:?}"
                )));
            }
            if let Some(prior) = registry.glyph_to_term.insert(glyph_key, target.clone())
                && prior != *target
            {
                return Err(GlyphRegistryError(format!(
                    "glyph {glyph:?} collides in scope {sigil:?}: {prior} and {target}"
                )));
            }
            if let Some(prior) = registry
                .fallback_to_term
                .insert(fallback_key, target.clone())
                && prior != *target
            {
                return Err(GlyphRegistryError(format!(
                    "ASCII fallback {fallback:?} collides in scope {sigil:?}: {prior} and {target}"
                )));
            }
            processed_candidates.insert(candidate.clone());
        }

        for candidate in adopted_by_denotation.values() {
            if !processed_candidates.contains(candidate) {
                return Err(GlyphRegistryError(format!(
                    "adopted glyph candidate {candidate} is not linked to a denotation/grapheme in the current codebook script"
                )));
            }
        }
        registry.reject_bare_signature_ambiguity()?;
        Ok(registry)
    }

    fn glyph_for(&self, iri: &str, sigil: &str) -> Option<&str> {
        self.unique_term_binding(iri, sigil)
            .map(|(_, glyph)| glyph.as_str())
    }

    fn term_for(&self, glyph: &str, sigil: &str) -> Option<&str> {
        self.unique_surface_binding(&self.glyph_to_term, glyph, sigil)
            .map(|(_, term)| term.as_str())
    }

    fn term_for_fallback(&self, fallback: &str, sigil: &str) -> Option<&str> {
        self.unique_surface_binding(&self.fallback_to_term, fallback, sigil)
            .map(|(_, term)| term.as_str())
    }

    /// Resolve a term only when the caller's explicit operator signature matches the
    /// authored denoted form. A wrong fixity or arity is a miss, never typography-based
    /// guessing. Sigil scope retains exact-then-global precedence.
    #[must_use]
    pub fn glyph_for_signature(
        &self,
        iri: &str,
        sigil: &str,
        fixity: Option<&str>,
        arity: Option<u32>,
    ) -> Option<&str> {
        let signature = GmnGlyphSignature {
            fixity: fixity.map(str::to_owned),
            arity,
        };
        self.term_to_glyph
            .get(&(sigil.to_owned(), iri.to_owned(), signature.clone()))
            .or_else(|| {
                self.term_to_glyph
                    .get(&(String::new(), iri.to_owned(), signature))
            })
            .map(String::as_str)
    }

    /// Resolve a glyph only when the caller's explicit fixity and arity match the
    /// authored denoted form. The ordinary reader uses the same table's unique bare
    /// scoped binding; this method exposes the signature criterion directly to parser
    /// integrations and acceptance tests.
    #[must_use]
    pub fn term_for_signature(
        &self,
        glyph: &str,
        sigil: &str,
        fixity: Option<&str>,
        arity: Option<u32>,
    ) -> Option<&str> {
        let signature = GmnGlyphSignature {
            fixity: fixity.map(str::to_owned),
            arity,
        };
        self.glyph_to_term
            .get(&(sigil.to_owned(), glyph.to_owned(), signature.clone()))
            .or_else(|| {
                self.glyph_to_term
                    .get(&(String::new(), glyph.to_owned(), signature))
            })
            .map(String::as_str)
    }

    /// The glyph-table version pinned in the GMN header.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The executable glyph bindings `(sigil, glyph, fixity, arity, term)` in stable
    /// `BTreeMap` key order, each coordinate flattened to an owned string (fixity/arity
    /// rendered `""` when absent). The codebook-digest layer folds these as one Merkle
    /// leaf; the private [`GmnGlyphSignature`] never crosses the module boundary.
    pub(crate) fn glyph_binding_rows(&self) -> Vec<(String, String, String, String, String)> {
        Self::binding_rows(&self.glyph_to_term)
    }

    /// The ASCII-fallback read bindings, in the same flattened shape as
    /// [`Self::glyph_binding_rows`] — a distinct Merkle leaf of the codebook digest.
    pub(crate) fn fallback_binding_rows(&self) -> Vec<(String, String, String, String, String)> {
        Self::binding_rows(&self.fallback_to_term)
    }

    fn binding_rows(
        table: &BTreeMap<(String, String, GmnGlyphSignature), String>,
    ) -> Vec<(String, String, String, String, String)> {
        table
            .iter()
            .map(|((sigil, surface, signature), term)| {
                (
                    sigil.clone(),
                    surface.clone(),
                    signature.fixity.clone().unwrap_or_default(),
                    signature.arity.map(|a| a.to_string()).unwrap_or_default(),
                    term.clone(),
                )
            })
            .collect()
    }

    /// The distinct executable glyph tokens, ordered for deterministic longest-match
    /// lexing (more Unicode scalar values first, then bytewise lexical order).
    #[must_use]
    pub fn glyph_tokens(&self) -> Vec<&str> {
        let mut glyphs: Vec<&str> = self
            .glyph_to_term
            .keys()
            .map(|(_, glyph, _)| glyph.as_str())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        glyphs.sort_by(|a, b| {
            b.chars()
                .count()
                .cmp(&a.chars().count())
                .then_with(|| a.cmp(b))
        });
        glyphs
    }

    /// Render the closed W3C-EBNF `glyphToken` production from the graph-derived
    /// registry. This is the one grammar inventory used by regeneration; removing a
    /// Denotation therefore removes the glyph from writer, reader, and generated EBNF.
    #[must_use]
    pub fn render_glyph_token_production(&self) -> String {
        let alternatives = self
            .glyph_tokens()
            .into_iter()
            .map(|glyph| format!("'{glyph}'"))
            .collect::<Vec<_>>()
            .join(" | ");
        format!("glyphToken ::= {alternatives}")
    }

    /// Render the graph-derived closed glyph production into the GMN EBNF template.
    /// Exactly one `glyphToken` production must exist: accepting zero would silently leave
    /// the grammar disconnected from the registry, while accepting two would make the
    /// grammar ambiguous about which inventory is authoritative.
    pub fn render_grammar(&self, source: &[u8]) -> Result<Vec<u8>, GlyphRegistryError> {
        let source = std::str::from_utf8(source)
            .map_err(|error| GlyphRegistryError(format!("GMN grammar is not UTF-8: {error}")))?;
        if self.term_to_glyph.is_empty() {
            return Err(GlyphRegistryError(
                "the executable glyph registry is empty; a closed glyphToken production cannot be rendered"
                    .to_owned(),
            ));
        }
        let mut found = 0usize;
        let production = self.render_glyph_token_production();
        let mut rendered = String::with_capacity(source.len() + production.len());
        for line in source.lines() {
            if line.trim_start().starts_with("glyphToken ::=") {
                found += 1;
                rendered.push_str(&production);
            } else {
                rendered.push_str(line);
            }
            rendered.push('\n');
        }
        if found != 1 {
            return Err(GlyphRegistryError(format!(
                "GMN grammar must contain exactly one glyphToken production, found {found}"
            )));
        }
        Ok(rendered.into_bytes())
    }

    /// Whether the graph-derived registry contains `glyph` in `sigil` scope (including
    /// a deliberately global fallback binding).
    #[must_use]
    pub fn contains_glyph(&self, glyph: &str, sigil: &str) -> bool {
        self.term_for(glyph, sigil).is_some()
    }

    /// Every `(sigil, glyph)` executable binding for one denoted term, in stable key
    /// order. Used by the independent glyph-optimality quality axis; the semantic
    /// round-trip coverage axis does not consume this view.
    #[must_use]
    pub fn bindings_for_term(&self, iri: &str) -> Vec<(&str, &str)> {
        self.term_to_glyph
            .iter()
            .filter_map(|((sigil, term, _), glyph)| {
                (term == iri).then_some((sigil.as_str(), glyph.as_str()))
            })
            .collect()
    }

    fn unique_term_binding(&self, iri: &str, sigil: &str) -> Option<(&GmnGlyphSignature, &String)> {
        let mut exact = self
            .term_to_glyph
            .iter()
            .filter(|((scope, term, _), _)| scope == sigil && term == iri)
            .map(|((_, _, signature), glyph)| (signature, glyph));
        match (exact.next(), exact.next()) {
            (Some(binding), None) => return Some(binding),
            (Some(_), Some(_)) => return None,
            (None, _) => {}
        }
        let mut global = self
            .term_to_glyph
            .iter()
            .filter(|((scope, term, _), _)| scope.is_empty() && term == iri)
            .map(|((_, _, signature), glyph)| (signature, glyph));
        match (global.next(), global.next()) {
            (Some(binding), None) => Some(binding),
            _ => None,
        }
    }

    fn unique_surface_binding<'a>(
        &'a self,
        table: &'a BTreeMap<(String, String, GmnGlyphSignature), String>,
        surface: &str,
        sigil: &str,
    ) -> Option<(&'a GmnGlyphSignature, &'a String)> {
        let mut exact = table
            .iter()
            .filter(|((scope, token, _), _)| scope == sigil && token == surface)
            .map(|((_, _, signature), term)| (signature, term));
        match (exact.next(), exact.next()) {
            (Some(binding), None) => return Some(binding),
            (Some(_), Some(_)) => return None,
            (None, _) => {}
        }
        let mut global = table
            .iter()
            .filter(|((scope, token, _), _)| scope.is_empty() && token == surface)
            .map(|((_, _, signature), term)| (signature, term));
        match (global.next(), global.next()) {
            (Some(binding), None) => Some(binding),
            _ => None,
        }
    }

    fn reject_bare_signature_ambiguity(&self) -> Result<(), GlyphRegistryError> {
        reject_ambiguous_groups(
            self.term_to_glyph
                .keys()
                .map(|(scope, term, signature)| (scope.as_str(), term.as_str(), signature)),
            "term",
        )?;
        reject_ambiguous_groups(
            self.glyph_to_term
                .keys()
                .map(|(scope, glyph, signature)| (scope.as_str(), glyph.as_str(), signature)),
            "glyph",
        )?;
        reject_ambiguous_groups(
            self.fallback_to_term
                .keys()
                .map(|(scope, fallback, signature)| (scope.as_str(), fallback.as_str(), signature)),
            "ASCII fallback",
        )
    }
}

fn reject_ambiguous_groups<'a>(
    keys: impl Iterator<Item = (&'a str, &'a str, &'a GmnGlyphSignature)>,
    kind: &str,
) -> Result<(), GlyphRegistryError> {
    let mut signatures = BTreeMap::<(String, String), BTreeSet<GmnGlyphSignature>>::new();
    for (scope, value, signature) in keys {
        signatures
            .entry((scope.to_owned(), value.to_owned()))
            .or_default()
            .insert(signature.clone());
    }
    if let Some(((scope, value), found)) = signatures.iter().find(|(_, found)| found.len() > 1) {
        return Err(GlyphRegistryError(format!(
            "{kind} {value:?} in scope {scope:?} has multiple authored fixity/arity signatures {found:?}; a bare GMN token would be ambiguous"
        )));
    }
    Ok(())
}

/// Resolve the current codebook selection from a carrier — the one loader that follows
/// `gmeow:gmnCodebookCurrent` through `gmeow:references`. Public so a caller (the CLI /
/// pipeline codebook-digest surface) can obtain a [`CurrentCodebook`] to hash, using the
/// SAME resolution the codec's dictionary/glyph loaders already trust.
///
/// # Errors
/// A [`GlyphRegistryError`] if the current codebook is absent, mistyped, references not
/// exactly one dictionary/script, or pins a version disagreeing with the codec's.
pub fn resolve_current_codebook(ds: &RdfDataset) -> Result<CurrentCodebook, GlyphRegistryError> {
    let mut types = BTreeMap::<String, BTreeSet<String>>::new();
    let mut references = BTreeMap::<String, BTreeSet<String>>::new();
    let mut dictionary_versions = BTreeMap::<String, BTreeSet<String>>::new();
    let mut glyph_versions = BTreeMap::<String, BTreeSet<String>>::new();
    let mut dictionary_entries = BTreeMap::<String, BTreeSet<String>>::new();
    let mut script_graphemes = BTreeMap::<String, BTreeSet<String>>::new();

    for quad in ds.owned_quads() {
        let RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        match quad.predicate.as_str() {
            RDF_TYPE => {
                if let RdfTerm::Iri(class) = &quad.object {
                    types
                        .entry(subject.clone())
                        .or_default()
                        .insert(class.clone());
                }
            }
            PRED_REFERENCES => {
                if let RdfTerm::Iri(object) = &quad.object {
                    references
                        .entry(subject.clone())
                        .or_default()
                        .insert(object.clone());
                }
            }
            PRED_GMN_DICTIONARY_VERSION => {
                if let RdfTerm::Literal(version) = &quad.object {
                    dictionary_versions
                        .entry(subject.clone())
                        .or_default()
                        .insert(version.lexical_form.clone());
                }
            }
            PRED_GMN_GLYPH_TABLE_VERSION => {
                if let RdfTerm::Literal(version) = &quad.object {
                    glyph_versions
                        .entry(subject.clone())
                        .or_default()
                        .insert(version.lexical_form.clone());
                }
            }
            PRED_GMN_DICTIONARY_ENTRY => {
                if let RdfTerm::Iri(entry) = &quad.object {
                    dictionary_entries
                        .entry(subject.clone())
                        .or_default()
                        .insert(entry.clone());
                }
            }
            PRED_HAS_GRAPHEME => {
                if let RdfTerm::Iri(grapheme) = &quad.object {
                    script_graphemes
                        .entry(subject.clone())
                        .or_default()
                        .insert(grapheme.clone());
                }
            }
            _ => {}
        }
    }

    if !has_type(&types, CURRENT_CODEBOOK, CLASS_GMN_CODEBOOK) {
        return Err(GlyphRegistryError(format!(
            "current codebook {CURRENT_CODEBOOK} is absent or not typed gmeow:GmnCodebook"
        )));
    }
    let current_references = references.get(CURRENT_CODEBOOK).cloned().ok_or_else(|| {
        GlyphRegistryError(format!(
            "current codebook {CURRENT_CODEBOOK} has no gmeow:references inventory"
        ))
    })?;
    let dictionary = exactly_one_typed_reference(
        &current_references,
        &types,
        CLASS_GMN_DICTIONARY,
        "gmeow:GmnDictionary",
    )?;
    let script =
        exactly_one_typed_reference(&current_references, &types, CLASS_SCRIPT, "lang:Script")?;
    let codebook_dictionary_version = exactly_one_literal(
        dictionary_versions.get(CURRENT_CODEBOOK),
        "current codebook gmnDictionaryVersion",
    )?;
    let dictionary_version = exactly_one_literal(
        dictionary_versions.get(&dictionary),
        &format!("current dictionary {dictionary} gmnDictionaryVersion"),
    )?;
    if codebook_dictionary_version != dictionary_version {
        return Err(GlyphRegistryError(format!(
            "current codebook dictionary version {codebook_dictionary_version:?} disagrees with referenced dictionary {dictionary} version {dictionary_version:?}"
        )));
    }
    if dictionary_version != DICTIONARY_VERSION {
        return Err(GlyphRegistryError(format!(
            "current dictionary version {dictionary_version:?} does not match codec version {DICTIONARY_VERSION:?}"
        )));
    }
    let glyph_version = exactly_one_literal(
        glyph_versions.get(CURRENT_CODEBOOK),
        "current codebook gmnGlyphTableVersion",
    )?;
    if glyph_version != GLYPH_VERSION {
        return Err(GlyphRegistryError(format!(
            "current glyph-table version {glyph_version:?} does not match codec version {GLYPH_VERSION:?}"
        )));
    }
    let graphemes = script_graphemes.get(&script).cloned().ok_or_else(|| {
        GlyphRegistryError(format!(
            "current codebook script {script} has no lang:hasGrapheme inventory"
        ))
    })?;

    Ok(CurrentCodebook {
        references: current_references,
        dictionary_entries: dictionary_entries
            .get(&dictionary)
            .cloned()
            .unwrap_or_default(),
        dictionary_version,
        glyph_version,
        graphemes,
    })
}

fn exactly_one_typed_reference(
    references: &BTreeSet<String>,
    types: &BTreeMap<String, BTreeSet<String>>,
    class: &str,
    label: &str,
) -> Result<String, GlyphRegistryError> {
    let matches = references
        .iter()
        .filter(|reference| has_type(types, reference, class))
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(GlyphRegistryError(format!(
            "current codebook must reference exactly one {label}, found {}",
            matches.len()
        )));
    }
    Ok((*matches[0]).clone())
}

fn exactly_one_literal(
    values: Option<&BTreeSet<String>>,
    label: &str,
) -> Result<String, GlyphRegistryError> {
    let values = values.cloned().unwrap_or_default();
    if values.len() != 1 {
        return Err(GlyphRegistryError(format!(
            "{label} must be declared exactly once, found {} values",
            values.len()
        )));
    }
    Ok(values
        .into_iter()
        .next()
        .expect("the exactly-one literal set is non-empty"))
}

fn has_type(types: &BTreeMap<String, BTreeSet<String>>, node: &str, class: &str) -> bool {
    types
        .get(node)
        .is_some_and(|classes| classes.contains(class))
}

fn insert_unique(
    map: &mut BTreeMap<String, String>,
    key: &str,
    value: &str,
    field: &str,
) -> Result<(), GlyphRegistryError> {
    if let Some(prior) = map.insert(key.to_owned(), value.to_owned())
        && prior != value
    {
        return Err(GlyphRegistryError(format!(
            "{field} is not functional for {key}: {prior:?} and {value:?}"
        )));
    }
    Ok(())
}

fn insert_unique_u32(
    map: &mut BTreeMap<String, u32>,
    key: &str,
    value: &str,
    field: &str,
) -> Result<(), GlyphRegistryError> {
    let value = value.parse::<u32>().map_err(|_| {
        GlyphRegistryError(format!(
            "{field} for {key} is not a non-negative 32-bit integer: {value:?}"
        ))
    })?;
    if let Some(prior) = map.insert(key.to_owned(), value)
        && prior != value
    {
        return Err(GlyphRegistryError(format!(
            "{field} is not functional for {key}: {prior} and {value}"
        )));
    }
    Ok(())
}

fn decode_codepoint_sequence(value: &str) -> Result<String, GlyphRegistryError> {
    let mut glyph = String::new();
    for token in value.split_whitespace() {
        let hex = token.strip_prefix("U+").ok_or_else(|| {
            GlyphRegistryError(format!(
                "codepoint token {token:?} does not use canonical U+XXXX form"
            ))
        })?;
        if !(4..=6).contains(&hex.len())
            || !hex
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b))
        {
            return Err(GlyphRegistryError(format!(
                "codepoint token {token:?} is not canonical uppercase U+XXXX hexadecimal"
            )));
        }
        let scalar = u32::from_str_radix(hex, 16).map_err(|_| {
            GlyphRegistryError(format!("codepoint token {token:?} is outside Unicode"))
        })?;
        let ch = char::from_u32(scalar).ok_or_else(|| {
            GlyphRegistryError(format!("codepoint token {token:?} is not a Unicode scalar"))
        })?;
        glyph.push(ch);
    }
    if glyph.is_empty() {
        return Err(GlyphRegistryError(
            "a glyph codepoint sequence cannot be empty".to_owned(),
        ));
    }
    Ok(glyph)
}

/// The Unicode security boundary of a GMN glyph token. The grammar is closed and
/// delimiter-driven, so whitespace/delimiters are never legal content; NFC keeps the
/// codepoint spelling canonical; bidi controls and default-ignorables are rejected rather
/// than allowed to alter display or disappear in review.
fn validate_glyph_surface(glyph: &str) -> Result<(), GlyphRegistryError> {
    if !is_nfc(glyph) {
        return Err(GlyphRegistryError(format!(
            "glyph {glyph:?} is not NFC-normalized"
        )));
    }
    for ch in glyph.chars() {
        if ch.is_whitespace() || matches!(ch, ',' | ':' | '{' | '}' | '[' | ']' | '\'' | '"' | '\\')
        {
            return Err(GlyphRegistryError(format!(
                "glyph {glyph:?} contains a GMN grammar delimiter or whitespace"
            )));
        }
        if is_bidi_control(ch) || is_default_ignorable(ch) {
            return Err(GlyphRegistryError(format!(
                "glyph {glyph:?} contains bidi/default-ignorable codepoint U+{:04X}",
                ch as u32
            )));
        }
    }
    Ok(())
}

fn is_bidi_control(ch: char) -> bool {
    matches!(
        ch as u32,
        0x061C | 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069
    )
}

/// Unicode Default_Ignorable_Code_Point ranges relevant to serialized source text.
/// Kept explicit so the security decision is inspectable and dependency-free; bidi
/// controls are named separately above for a more precise diagnostic.
fn is_default_ignorable(ch: char) -> bool {
    matches!(
        ch as u32,
        0x00AD
            | 0x034F
            | 0x061C
            | 0x115F..=0x1160
            | 0x17B4..=0x17B5
            | 0x180B..=0x180F
            | 0x200B..=0x200F
            | 0x202A..=0x202E
            | 0x2060..=0x206F
            | 0x3164
            | 0xFE00..=0xFE0F
            | 0xFEFF
            | 0xFFA0
            | 0x1BCA0..=0x1BCA3
            | 0x1D173..=0x1D17A
            | 0xE0000..=0xE0FFF
    )
}

// ── The dictionary bijection (`gmeow:gmnDictV3`, read from the carrier) ─────────────

/// The GMN alias-table bijection, read from the compiled carrier — never hardcoded.
/// Injective over its covered term set (checked at load time, defensively: the carrier's
/// own SHACL gate is the primary authority, this is the codec's own read-back safety net).
#[derive(Debug, Clone)]
pub struct GmnDictionary {
    version: String,
    term_to_alias: BTreeMap<String, String>,
    alias_to_term: BTreeMap<String, String>,
    glyphs: GmnGlyphRegistry,
}

/// An explicit empty dictionary at the codec's current coordinates. This exists for
/// fixture-scale prefix-only models; carrier loading never falls back to it —
/// [`GmnDictionary::from_dataset`] requires the current codebook's declarations.
impl Default for GmnDictionary {
    fn default() -> Self {
        Self {
            version: DICTIONARY_VERSION.to_owned(),
            term_to_alias: BTreeMap::new(),
            alias_to_term: BTreeMap::new(),
            glyphs: GmnGlyphRegistry::default(),
        }
    }
}

/// A dictionary that fails to load: not a bijection, or an alias collides with a
/// reserved token shape ([`BLANK_PREFIX`] / [`REF_PREFIX`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryError(pub String);

impl GmnDictionary {
    /// Load the one dictionary referenced by `gmeow:gmnCodebookCurrent`: only its
    /// `gmeow:gmnDictionaryEntry` members participate, so historical dictionaries and
    /// unrelated entry records can coexist in the carrier without contaminating current
    /// alias resolution. Both the codebook and dictionary MUST explicitly declare the
    /// same supported version; absence is an error, never a default.
    pub fn from_dataset(ds: &RdfDataset) -> Result<Self, DictionaryError> {
        let codebook = resolve_current_codebook(ds).map_err(|error| DictionaryError(error.0))?;

        let mut terms: BTreeMap<String, String> = BTreeMap::new();
        let mut aliases: BTreeMap<String, String> = BTreeMap::new();
        for quad in ds.owned_quads() {
            if quad.predicate == PRED_GMN_DICTIONARY_ENTRY_TERM {
                let RdfTerm::Iri(subject) = &quad.subject else {
                    continue;
                };
                if !codebook.dictionary_entries.contains(subject) {
                    continue;
                }
                let RdfTerm::Iri(term) = &quad.object else {
                    continue;
                };
                insert_unique(&mut terms, subject, term, "dictionary-entry term")
                    .map_err(|error| DictionaryError(error.0))?;
            } else if quad.predicate == PRED_GMN_DICTIONARY_ENTRY_ALIAS {
                let RdfTerm::Iri(subject) = &quad.subject else {
                    continue;
                };
                if !codebook.dictionary_entries.contains(subject) {
                    continue;
                }
                let RdfTerm::Literal(lit) = &quad.object else {
                    continue;
                };
                insert_unique(
                    &mut aliases,
                    subject,
                    &lit.lexical_form,
                    "dictionary-entry alias",
                )
                .map_err(|error| DictionaryError(error.0))?;
            }
        }

        let mut term_to_alias = BTreeMap::new();
        let mut alias_to_term: BTreeMap<String, String> = BTreeMap::new();
        for entry in &codebook.dictionary_entries {
            let term = terms.get(entry).ok_or_else(|| {
                DictionaryError(format!("dictionary entry {entry} has an alias but no term"))
            })?;
            let alias = aliases.get(entry).ok_or_else(|| {
                DictionaryError(format!("dictionary entry {entry} has a term but no alias"))
            })?;
            if alias.starts_with(BLANK_PREFIX) || alias.starts_with(REF_PREFIX) {
                return Err(DictionaryError(format!(
                    "dictionary alias {alias:?} for {term} collides with a reserved token shape"
                )));
            }
            if !is_identifier(alias) {
                return Err(DictionaryError(format!(
                    "dictionary alias {alias:?} for {term} is outside the closed identifier grammar"
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

        let glyphs =
            GmnGlyphRegistry::from_dataset(ds).map_err(|error| DictionaryError(error.0))?;
        for ((_scope, glyph, _signature), target) in &glyphs.glyph_to_term {
            if let Some(alias_target) = alias_to_term.get(glyph) {
                return Err(DictionaryError(format!(
                    "executable glyph {glyph:?} for {target} collides with dictionary alias for {alias_target}"
                )));
            }
        }
        for ((_scope, fallback, _signature), target) in &glyphs.fallback_to_term {
            if let Some(alias_target) = alias_to_term.get(fallback) {
                return Err(DictionaryError(format!(
                    "executable ASCII fallback {fallback:?} for {target} collides with dictionary alias for {alias_target}"
                )));
            }
        }

        Ok(Self {
            version: codebook.dictionary_version,
            term_to_alias,
            alias_to_term,
            glyphs,
        })
    }

    fn alias_for(&self, iri: &str) -> Option<&str> {
        self.term_to_alias.get(iri).map(String::as_str)
    }

    fn term_for(&self, alias: &str) -> Option<&str> {
        self.alias_to_term.get(alias).map(String::as_str)
    }

    fn glyph_for(&self, iri: &str, sigil: &str) -> Option<&str> {
        self.glyphs.glyph_for(iri, sigil)
    }

    fn term_for_glyph(&self, glyph: &str, sigil: &str) -> Option<&str> {
        self.glyphs.term_for(glyph, sigil)
    }

    fn term_for_glyph_fallback(&self, fallback: &str, sigil: &str) -> Option<&str> {
        self.glyphs.term_for_fallback(fallback, sigil)
    }

    fn aliases_id(&self) -> String {
        format!("dict-v{}", self.version)
    }

    /// The dictionary version this codec's `@gmn{v: 1, aliases: …}` header pins.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// The alias bijection as `term → alias` pairs in stable `BTreeMap` (term) key order —
    /// the codebook digest's dictionary-alias Merkle leaf. Read-only; the map itself is
    /// the loaded, injectivity-checked bijection, never a rebuilt copy.
    pub(crate) fn alias_entries(&self) -> &BTreeMap<String, String> {
        &self.term_to_alias
    }

    /// The graph-derived scoped glyph registry carried beside the alias table.
    #[must_use]
    pub fn glyph_registry(&self) -> &GmnGlyphRegistry {
        &self.glyphs
    }
}

// ── Uncovered-term reporting (the pure error surface; ledger interning lives in
// `crate::error::GmnUncoveredTerm`, attached by the round-trip gate) ────────────────

/// A GMN-0 construct this codec cannot losslessly encode — the pure carrier of what
/// becomes a `lang:GmnUncoveredTerm` finding once interned into a
/// [`gmeow_errors::DiagLedger`] (never a silent drop, per the no-optionality rule).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UncoveredTerm(pub String);

/// A GMN-1 write or read failure — a TOTAL typed algebra in which every failure resolves
/// to EXACTLY ONE `lang:` validator-tier conformance class (no untyped residual). Each
/// variant maps to one `lang:LangConformanceFailure` subclass through [`Self::failure_class`]
/// ([`LANG-GMN.md`](../../../slices/grounding/lang/design/LANG-GMN.md) § "The four
/// validator-tier failure classes"; the classes themselves live in
/// `slices/grounding/lang/module.ttl`).
///
/// # Detection precedence (the linearization that makes "exactly one class" well-defined)
///
/// A single GMN-1 document can violate several rules at once; to keep the classification a
/// FUNCTION (exactly one class per input, never a set), [`gmn1_read`] applies its checks in a
/// fixed precedence — syntax before semantics — and returns the FIRST class the input trips:
///
/// 1. **lex/grammar** ([`NonDecodableGrammar`](Self::NonDecodableGrammar)) — the byte stream
///    must be structurally decodable at all (balanced braces, a known sigil, known keys, no
///    duplicate keys, a `@claims` schema its rows match). A grammar defect ANYWHERE dominates.
/// 2. **number-form** ([`MalformedNumber`](Self::MalformedNumber)) — every number-SHAPED value
///    token must be a canonical integer or exactly-two-digit decimal. Number well-formedness is
///    a LEXICAL property, decidable without the dialect header, so it precedes header-presence.
/// 3. **key-order** ([`NonCanonicalOrder`](Self::NonCanonicalOrder)) — a record's keys must be
///    in the canonical `s p o v id q st ev m ek bd it class` order.
/// 4. **header-presence** ([`UndeclaredDialectVersion`](Self::UndeclaredDialectVersion)) — the
///    `@gmn{…}` header must pin the dialect/dictionary version before any record.
/// 5. **dictionary-coverage** ([`Uncovered`](Self::Uncovered)) — every term must resolve
///    against the pinned dictionary / prefix registry.
///
/// [`NonDecodableGrammar`](Self::NonDecodableGrammar) is the RESIDUAL bucket: genuinely
/// unliftable grammar (and the codec's own internal round-trip-mismatch invariant), never a
/// catch-all for one of the four rules above.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Gmn1Error {
    /// `lang:GmnUncoveredTerm` — a grammar-valid term the pinned dictionary / prefix registry
    /// does not cover (an IRI under no registered namespace, or a dictionary alias `dict-v3`
    /// does not mint). Named so it is diagnosable. RDF 1.2 triple terms are NOT uncovered —
    /// the codec encodes them losslessly (see [`encode_triple_term`]); a named-graph quad is
    /// its own honest domain-boundary class ([`Self::NamedGraphOutOfDomain`]), never this
    /// "uncovered" residual (which would imply a bigger dictionary could cover it).
    Uncovered(UncoveredTerm),
    /// `lang:GmnGraphOutOfDomain` — a quad carries a named graph, which is OUTSIDE the
    /// default-graph GMN-0 normal-form domain (the grounding slices are authored as
    /// `text/turtle`, i.e. default-graph triples only). This is an HONEST domain boundary,
    /// NOT an [`Self::Uncovered`] residual: no larger dictionary or richer term grammar could
    /// ever bring a named-graph quad in-domain, because the GMN-1 record shape has no graph
    /// slot by charter. `graph` is the offending graph name's canonical rendering.
    NamedGraphOutOfDomain { graph: String },
    /// `lang:GmnNonCanonicalOrder` — a record's field keys are not in the canonical key order
    /// (`s p o v id q st ev m ek bd it class`), forfeiting the byte-comparability the digest discipline
    /// depends on.
    NonCanonicalOrder { detail: String },
    /// `lang:GmnMalformedNumber` — a number-shaped value token outside the grammar's number
    /// production (scientific notation like `9.5e-1`, or a fraction whose digit count is not
    /// exactly two like `0.951`). `token` is the offending lexeme.
    MalformedNumber { token: String },
    /// `lang:GmnUndeclaredDialectVersion` — the document reaches the reader without its dialect
    /// coordinates: no `@gmn{…}` header pinning the schema/dictionary version before the first
    /// record, or a header that fails to pin the expected version.
    UndeclaredDialectVersion { detail: String },
    /// `lang:GmnNonDecodableGrammar` — the residual for genuinely unliftable grammar: an
    /// unbalanced brace, an unknown sigil, an unknown or duplicate record key, a malformed
    /// field pair, a `@claims` schema its rows do not match, or the codec's own internal
    /// round-trip-mismatch invariant.
    NonDecodableGrammar { detail: String },
    /// `lang:GmnNonCanonicalCodepoint` — a literal's lexical form is not NFC-normalized.
    /// The GMN glyph discipline (`is_nfc` on every glyph surface) extended to literal
    /// content: a non-NFC lexical form is a non-canonical Unicode spelling, so two byte-
    /// distinct encodings of the "same" text would take different content digests and
    /// forfeit the byte-comparability the digest layer rests on. A HARD FAIL at encode
    /// time (no optionality), never a silent normalization. `lexical` is the offending
    /// form.
    NonNfcLiteral { lexical: String },
    /// `lang:GmnNonDecodableGrammar` — a PER-CLAIM localization of the whole-model
    /// round-trip failure [`round_trip_check`] already discharges: `decode(encode(GMN-0))`
    /// did not reproduce the canonical GMN-0 for the claim (canonical-subject group)
    /// `subject`. This is NOT a new failure class — it is the SAME mnemomorphic
    /// round-trip guarantee, localized to the offending canonical subject so a conformance
    /// witness can name WHICH claim diverged (the per-claim inversion witness).
    /// `subject` is the canonical-subject rendering (`<iri>`, `_:c14nN`, or the standalone
    /// leg's model-subject key) whose partition digest disagreed.
    PerClaimMismatch { subject: String },
}

impl Gmn1Error {
    /// The full `lang:` failure-class IRI (`https://blackcatinformatics.ca/lang/…`) — the
    /// SAME IRIs `slices/grounding/lang/module.ttl` mints under `lang:LangConformanceFailure`,
    /// the ONE canonical classifier the on-gate GMN gate, `run.rs`'s ledger split, and the
    /// shipped-projection lint all consume (never a second, drift-prone classifier).
    pub const CLASS_UNCOVERED_TERM: &'static str =
        "https://blackcatinformatics.ca/lang/GmnUncoveredTerm";
    /// See [`Self::CLASS_UNCOVERED_TERM`]. The named-graph domain-boundary class — a quad
    /// with a named graph is outside the default-graph GMN-0 normal-form domain.
    pub const CLASS_GRAPH_OUT_OF_DOMAIN: &'static str =
        "https://blackcatinformatics.ca/lang/GmnGraphOutOfDomain";
    /// See [`Self::CLASS_UNCOVERED_TERM`].
    pub const CLASS_NON_CANONICAL_ORDER: &'static str =
        "https://blackcatinformatics.ca/lang/GmnNonCanonicalOrder";
    /// See [`Self::CLASS_UNCOVERED_TERM`].
    pub const CLASS_MALFORMED_NUMBER: &'static str =
        "https://blackcatinformatics.ca/lang/GmnMalformedNumber";
    /// See [`Self::CLASS_UNCOVERED_TERM`].
    pub const CLASS_UNDECLARED_DIALECT_VERSION: &'static str =
        "https://blackcatinformatics.ca/lang/GmnUndeclaredDialectVersion";
    /// See [`Self::CLASS_UNCOVERED_TERM`].
    pub const CLASS_NON_DECODABLE_GRAMMAR: &'static str =
        "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar";
    /// The non-NFC literal failure REUSES the existing `lang:GmnNonCanonicalCodepoint`
    /// class — the vocabulary's one Unicode-canonicity failure (a non-canonical codepoint
    /// spelling). A non-NFC literal lexical form is exactly that: a non-canonical Unicode
    /// spelling. No second, parallel normalization class is minted (GREENFIELD, one
    /// classifier).
    pub const CLASS_NON_CANONICAL_CODEPOINT: &'static str =
        "https://blackcatinformatics.ca/lang/GmnNonCanonicalCodepoint";

    /// The full `lang:` failure-class IRI this failure resolves to. This match is EXHAUSTIVE
    /// with no wildcard arm — the compile-time totality witness (mirroring
    /// [`Gmn1ConstructCategory::all_covered_by_match`]): a new [`Gmn1Error`] variant added
    /// without an IRI mapping HERE fails to compile, so the typed algebra can never grow an
    /// untyped residual.
    #[must_use]
    pub fn failure_class(&self) -> &'static str {
        match self {
            Self::Uncovered(_) => Self::CLASS_UNCOVERED_TERM,
            Self::NamedGraphOutOfDomain { .. } => Self::CLASS_GRAPH_OUT_OF_DOMAIN,
            Self::NonCanonicalOrder { .. } => Self::CLASS_NON_CANONICAL_ORDER,
            Self::MalformedNumber { .. } => Self::CLASS_MALFORMED_NUMBER,
            Self::UndeclaredDialectVersion { .. } => Self::CLASS_UNDECLARED_DIALECT_VERSION,
            Self::NonDecodableGrammar { .. } => Self::CLASS_NON_DECODABLE_GRAMMAR,
            Self::NonNfcLiteral { .. } => Self::CLASS_NON_CANONICAL_CODEPOINT,
            // A per-claim mismatch REUSES the whole-model round-trip class — it is the
            // SAME mnemomorphic guarantee localized to one canonical subject, never a
            // second vocabulary class (GREENFIELD, one classifier).
            Self::PerClaimMismatch { .. } => Self::CLASS_NON_DECODABLE_GRAMMAR,
        }
    }
}

impl std::fmt::Display for Gmn1Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Uncovered(u) => write!(f, "lang:GmnUncoveredTerm: {}", u.0),
            Self::NamedGraphOutOfDomain { graph } => write!(
                f,
                "lang:GmnGraphOutOfDomain: quad in named graph {graph} is outside the \
                 default-graph GMN-0 normal-form domain (the GMN-1 record shape has no \
                 graph slot)"
            ),
            Self::NonCanonicalOrder { detail } => write!(f, "lang:GmnNonCanonicalOrder: {detail}"),
            Self::MalformedNumber { token } => write!(
                f,
                "lang:GmnMalformedNumber: number-shaped token {token:?} is not a canonical \
                 integer or exactly-two-digit decimal"
            ),
            Self::UndeclaredDialectVersion { detail } => {
                write!(f, "lang:GmnUndeclaredDialectVersion: {detail}")
            }
            Self::NonDecodableGrammar { detail } => {
                write!(f, "lang:GmnNonDecodableGrammar: {detail}")
            }
            Self::NonNfcLiteral { lexical } => write!(
                f,
                "lang:GmnNonCanonicalCodepoint: literal lexical form {lexical:?} is not \
                 NFC-normalized"
            ),
            Self::PerClaimMismatch { subject } => write!(
                f,
                "lang:GmnNonDecodableGrammar: per-claim round-trip mismatch at canonical \
                 subject {subject} (decode(encode(GMN-0)) did not reproduce this claim)"
            ),
        }
    }
}

impl std::error::Error for Gmn1Error {}

/// The residual `lang:GmnNonDecodableGrammar` constructor — a one-line helper so the reader's
/// many structural-defect sites all mint the SAME variant.
fn non_decodable(detail: String) -> Gmn1Error {
    Gmn1Error::NonDecodableGrammar { detail }
}

// ── Construct-category classification (the coverage-completeness audit's
// vocabulary) ─────────────────────────────────────────────────────────────

/// The codec's own closed set of GMN-0 "construct categories" a WRITE-side term can
/// classify into. This is not a second notion of coverage: [`classify_iri`],
/// [`classify_literal`], [`classify_reference`], and [`classify_value`] are the SAME
/// dispatch [`encode_reference`]/[`encode_value`] call (each is a one-line wrapper
/// around its classifier), so a category label can never drift from what [`gmn1_write`] really
/// does to the same term. [`classify_model`] and [`ConstructCoverageTally`] compose these
/// into the per-quad, corpus-wide audit `crates/pipeline/src/stages/gmn1_gate.rs`'s
/// `check_gmn1_construct_coverage` runs over the real grounding slices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Gmn1ConstructCategory {
    /// An IRI rendered through a scoped, graph-derived GMN glyph Denotation.
    IriGlyph,
    /// An IRI resolved via the `gmeow:gmnDictV3` alias table (a dictionary hit).
    IriDictAlias,
    /// An IRI resolved via `prefix__local` mangling with no `/` in the stripped local
    /// part (the common case).
    IriPrefixMangled,
    /// An IRI resolved via `prefix__local` mangling whose local part needed the `/` →
    /// [`SLASH_ESCAPE`] reversible escape (a multi-segment path IRI under a registered
    /// namespace, e.g. `http://lexvo.org/id/iso639-3/eng`).
    IriPrefixMangledSlashEscaped,
    /// An IRI that IS a registered namespace's own bare root (empty local part).
    IriBareNamespaceRoot,
    /// A blank node in a reference-position slot.
    BlankNode,
    /// An `xsd:string` literal, identifier-shaped, inlined directly as a GMN-1
    /// identifier token.
    LiteralIdentifier,
    /// A canonical-shaped `xsd:integer` literal, inlined directly as a GMN-1 integer
    /// token.
    LiteralInteger,
    /// A canonical two-digit-fraction `xsd:decimal` literal, inlined directly as a
    /// GMN-1 number token.
    LiteralDecimal,
    /// Any other literal (an `rdf:langString`/language-tagged literal, a
    /// non-canonical-shaped number, arbitrary prose, or any other datatype), riding by
    /// reference through the document's reference table.
    LiteralByReference,
    /// An RDF 1.2 triple term (`( s p o )` over three nested terms) in the object slot —
    /// the codec's lossless RDF-star surface. Deliberately OUTSIDE [`Self::ALL`] (see its
    /// doc): triple terms are supported for RDF-1.2 completeness but are not part of the
    /// default-graph grounding-slice fragment the corpus-completeness audit is scoped to,
    /// exactly as a named-graph quad is out-of-domain and has no category at all.
    TripleTerm,
}

impl Gmn1ConstructCategory {
    /// The **default-graph grounding-slice** construct categories the corpus-completeness
    /// audit ([`ConstructCoverageTally::unexercised_categories`]) requires the real slices
    /// to exercise at least once. A category MISSING from this list is blind to that audit,
    /// so [`Self::all_covered_by_match`] is a compile-time witness that every in-domain
    /// variant has a match arm and this list stays complete over them.
    ///
    /// [`Self::TripleTerm`] is DELIBERATELY excluded: RDF 1.2 triple terms are encoded
    /// losslessly by the codec but do not occur in the default-graph grounding fragment the
    /// audit is scoped to (the slices author no reifiers), exactly as a named-graph quad is
    /// out-of-domain and carries no category at all. Their round-trip is proven by the
    /// codec's own fixtures, not by demanding the grounding corpus emit one.
    pub const ALL: &'static [Self] = &[
        Self::IriGlyph,
        Self::IriDictAlias,
        Self::IriPrefixMangled,
        Self::IriPrefixMangledSlashEscaped,
        Self::IriBareNamespaceRoot,
        Self::BlankNode,
        Self::LiteralIdentifier,
        Self::LiteralInteger,
        Self::LiteralDecimal,
        Self::LiteralByReference,
    ];

    // Never called; exists so an exhaustive match over `Self` fails to compile the moment a
    // new variant is added without a matching arm HERE. Every in-domain arm is also a member
    // of `ALL`; the out-of-domain `TripleTerm` arm is matched here (so this stays exhaustive)
    // but intentionally omitted from `ALL` (see its doc).
    #[allow(dead_code)]
    fn all_covered_by_match(self) {
        match self {
            Self::IriGlyph
            | Self::IriDictAlias
            | Self::IriPrefixMangled
            | Self::IriPrefixMangledSlashEscaped
            | Self::IriBareNamespaceRoot
            | Self::BlankNode
            | Self::LiteralIdentifier
            | Self::LiteralInteger
            | Self::LiteralDecimal
            | Self::LiteralByReference
            | Self::TripleTerm => {}
        }
    }
}

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

impl Gmn1Document {
    /// A self-contained GMN-1 surface with an EMPTY out-of-band reference table — the reader
    /// entry point for raw external text. Any document whose tokens resolve entirely through
    /// the pinned dictionary / prefix registry (carrying no `r_<hash>` by-reference literals)
    /// reads back through [`gmn1_read`] from this constructor; a document that DOES name a
    /// by-reference token the empty table cannot resolve hard-fails as `lang:GmnUncoveredTerm`,
    /// never a silent drop. This is the only way a caller outside the codec can present raw
    /// GMN-1 text to the reader (the writer's [`gmn1_write`] is the other source of a document).
    #[must_use]
    pub fn from_text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            refs: BTreeMap::new(),
        }
    }
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
fn classify_reference(
    term: &RdfTerm,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<(String, Gmn1ConstructCategory), UncoveredTerm> {
    match term {
        RdfTerm::Iri(iri) => classify_iri(iri, dict, ns_to_prefix, sigil)
            .ok_or_else(|| UncoveredTerm(format!("IRI under no registered namespace: {iri}"))),
        RdfTerm::BlankNode(label) => {
            if !is_safe_token_body(label) {
                return Err(UncoveredTerm(format!(
                    "blank node label is not GMN-1 identifier-safe: {label}"
                )));
            }
            Ok((
                format!("{BLANK_PREFIX}{label}"),
                Gmn1ConstructCategory::BlankNode,
            ))
        }
        RdfTerm::Literal(lit) => Err(UncoveredTerm(format!(
            "a reference-position slot (s/p/o/st/ev/m/ek/bd/it) cannot carry a literal: {lit:?}"
        ))),
        // RDF 1.2 triple term (`s rdf:reifies <<( a b c )>>`): a FIRST-CLASS object the
        // codec encodes losslessly as the compact `( s p o )` surface over three nested
        // terms, recursively through this same reference/value dispatch — never a hard fail.
        RdfTerm::Triple(triple) => Ok((
            encode_triple_term(triple, dict, ns_to_prefix, refs, sigil)?,
            Gmn1ConstructCategory::TripleTerm,
        )),
    }
}

/// The token-only view of [`classify_reference`] — a one-line wrapper so the write path
/// and the classification path can never disagree about what a reference-position term
/// encodes to.
fn encode_reference(
    term: &RdfTerm,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<String, UncoveredTerm> {
    classify_reference(term, dict, ns_to_prefix, refs, sigil).map(|(token, _)| token)
}

/// Encode an RDF 1.2 triple term (`<<( s p o )>>`) as the compact GMN-1 object-slot
/// surface `( s p o )` — a single whitespace/comma/colon-free-ish token whose three
/// nested terms invert through the SAME term grammar the codec uses everywhere else. The
/// parens delimit and space-separate the three components; every nested leaf token
/// (identifier, `prefix__local`, `_b<label>`, glyph, `r_<hash>`, integer/decimal) is
/// itself space-free, so [`decode_triple_term`]'s paren-depth-aware split recovers them
/// exactly. Triple terms nest (`( a b ( c d e ) )`) via the recursion.
fn encode_triple_term(
    triple: &RdfTriple,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<String, UncoveredTerm> {
    let subject = encode_embedded_term(&triple.subject, dict, ns_to_prefix, refs, sigil)?;
    let predicate = encode_embedded_term(
        &RdfTerm::Iri(triple.predicate.clone()),
        dict,
        ns_to_prefix,
        refs,
        sigil,
    )?;
    let object = encode_embedded_term(&triple.object, dict, ns_to_prefix, refs, sigil)?;
    Ok(format!("( {subject} {predicate} {object} )"))
}

/// Encode one term embedded INSIDE a triple term. Unlike the top-level object slot (which
/// disambiguates a literal via the `v` vs `o` key), a triple term's three positions are
/// slot-free, so a literal MUST NOT inline as a bare identifier — a bare token there is
/// indistinguishable from a dictionary alias / prefix reference. Integers and decimals are
/// still inlined (disjoint from every reference token by shape), while every other literal
/// rides by reference as `r_<hash>` (disjoint by the reserved `r_` prefix). IRIs, blanks,
/// and nested triple terms encode through the reference dispatch.
fn encode_embedded_term(
    term: &RdfTerm,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<String, UncoveredTerm> {
    match term {
        RdfTerm::Literal(lit) => Ok(encode_embedded_literal(lit, refs)),
        other => encode_reference(other, dict, ns_to_prefix, refs, sigil),
    }
}

/// A literal in an embedded (slot-free) triple-term position: canonical integer/decimal
/// inline (unambiguous by shape), otherwise by reference (never a bare identifier, which
/// would collide with a reference token). Shares the by-reference minting with
/// [`classify_literal`] via [`intern_literal_ref`].
fn encode_embedded_literal(lit: &RdfLiteral, refs: &mut BTreeMap<String, RefPayload>) -> String {
    let numeric = lit.language.is_none() && lit.direction.is_none();
    if numeric
        && lit.datatype.as_deref() == Some(XSD_INTEGER)
        && is_integer_token(&lit.lexical_form)
    {
        return lit.lexical_form.clone();
    }
    if numeric
        && lit.datatype.as_deref() == Some(XSD_DECIMAL)
        && is_decimal_token(&lit.lexical_form)
    {
        return lit.lexical_form.clone();
    }
    intern_literal_ref(lit, refs)
}

/// Encode a VALUE-position term (`v q`: the object's own literal payload, or an asserted
/// confidence) as a GMN-1 token — a canonical number when the shape/datatype allow it,
/// otherwise a content-addressed by-reference key. Deliberately does NOT consult the
/// dictionary (see [`encode_reference`]'s doc comment for why that would be unsound).
fn classify_value(
    term: &RdfTerm,
    refs: &mut BTreeMap<String, RefPayload>,
) -> Result<(String, Gmn1ConstructCategory), UncoveredTerm> {
    match term {
        RdfTerm::Literal(lit) => Ok(classify_literal(lit, refs)),
        other => Err(UncoveredTerm(format!(
            "a value-position slot (v/q) must carry a literal, got: {other:?}"
        ))),
    }
}

/// The token-only view of [`classify_value`] — a one-line wrapper so the write path and
/// the classification path can never disagree about what a value-position term encodes
/// to.
fn encode_value(
    term: &RdfTerm,
    refs: &mut BTreeMap<String, RefPayload>,
) -> Result<String, UncoveredTerm> {
    classify_value(term, refs).map(|(token, _)| token)
}

/// `prefix__local` if `iri` starts with a registered namespace and the local part is
/// GMN-1 identifier-safe; the dictionary alias takes precedence when present (shorter,
/// and the charter's witness-carried bijection). Also tags WHICH construct category the
/// resolution took, for [`classify_reference`]/[`classify_model`]'s audit use — the
/// classification is computed inline (never a second, drift-prone re-derivation of the
/// same branches).
fn classify_iri(
    iri: &str,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    sigil: &str,
) -> Option<(String, Gmn1ConstructCategory)> {
    if let Some(glyph) = dict.glyph_for(iri, sigil) {
        return Some((glyph.to_owned(), Gmn1ConstructCategory::IriGlyph));
    }
    if let Some(alias) = dict.alias_for(iri) {
        return Some((alias.to_owned(), Gmn1ConstructCategory::IriDictAlias));
    }
    for (ns, prefix) in ns_to_prefix {
        if let Some(local) = iri.strip_prefix(ns.as_str())
            && !local.is_empty()
            && !prefix.contains(SEP)
            && let Some(mangled) = mangle_local(local)
        {
            let category = if local.contains('/') {
                Gmn1ConstructCategory::IriPrefixMangledSlashEscaped
            } else {
                Gmn1ConstructCategory::IriPrefixMangled
            };
            return Some((format!("{prefix}{SEP}{mangled}"), category));
        }
        // The bare namespace root itself (e.g. the ontology's own base IRI, used as an
        // `owl:imports` object with no trailing slash): the local part is empty, so
        // there is nothing to mangle — the prefix ALONE is the token (still injective:
        // it is disjoint from every `prefix__local` token, which always contains SEP).
        if iri == ns.trim_end_matches('/') {
            return Some((prefix.clone(), Gmn1ConstructCategory::IriBareNamespaceRoot));
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

/// Whether `s` is NUMBER-SHAPED — a token that is unambiguously ATTEMPTING to be a numeric
/// literal, so a malformed one is a `lang:GmnMalformedNumber` rather than a mis-typed
/// identifier. A token is number-shaped iff it is NOT identifier-shaped AND (its first byte
/// is an ASCII digit, OR a leading sign/`.` is immediately followed by a digit).
///
/// The `!is_identifier` guard is load-bearing: identifiers legitimately contain `e`/`E`
/// (`open`, `state`, `sensorCrew`), so a naive "contains `e`" number test would misclassify
/// them — an identifier-shaped token is NEVER number-shaped and falls through to dictionary
/// coverage ([`Gmn1Error::Uncovered`]).
fn is_number_shaped(s: &str) -> bool {
    if is_identifier(s) {
        return false;
    }
    let mut bytes = s.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_digit() => true,
        Some(b'-' | b'+' | b'.') => bytes.next().is_some_and(|b| b.is_ascii_digit()),
        _ => false,
    }
}

/// Whether `s` is number-shaped but NOT a canonical GMN-1 number — the exact
/// `lang:GmnMalformedNumber` predicate: a fraction whose digit count is not exactly two
/// (`0.951`) or a scientific-notation lex (`9.5e-1`) is malformed, while `0.95`/`50`/`-1.00`
/// pass, and identifier-shaped tokens (never number-shaped) are left to dictionary coverage.
fn is_malformed_number(s: &str) -> bool {
    is_number_shaped(s) && !is_integer_token(s) && !is_decimal_token(s)
}

/// Classify + encode a literal: inline as an identifier or canonical number when the
/// datatype and shape allow it losslessly; otherwise mint a content-addressed `r_<hash>`
/// reference and carry the full payload in `refs`. [`classify_value`] delegates to this
/// (its token-only view) — never a second, drift-prone re-derivation of the same
/// branches.
fn classify_literal(
    lit: &RdfLiteral,
    refs: &mut BTreeMap<String, RefPayload>,
) -> (String, Gmn1ConstructCategory) {
    let plain_string = lit.language.is_none()
        && lit.direction.is_none()
        && lit.datatype.as_deref() == Some(XSD_STRING);
    if plain_string
        && is_identifier(&lit.lexical_form)
        && !lit.lexical_form.starts_with(BLANK_PREFIX)
        && !lit.lexical_form.starts_with(REF_PREFIX)
    {
        return (
            lit.lexical_form.clone(),
            Gmn1ConstructCategory::LiteralIdentifier,
        );
    }
    let integer_or_decimal = lit.language.is_none() && lit.direction.is_none();
    if integer_or_decimal
        && lit.datatype.as_deref() == Some(XSD_INTEGER)
        && is_integer_token(&lit.lexical_form)
    {
        return (
            lit.lexical_form.clone(),
            Gmn1ConstructCategory::LiteralInteger,
        );
    }
    if integer_or_decimal
        && lit.datatype.as_deref() == Some(XSD_DECIMAL)
        && is_decimal_token(&lit.lexical_form)
    {
        return (
            lit.lexical_form.clone(),
            Gmn1ConstructCategory::LiteralDecimal,
        );
    }
    // By reference: content-addressed on the full payload, so two occurrences of the
    // same literal share one reference-table entry.
    (
        intern_literal_ref(lit, refs),
        Gmn1ConstructCategory::LiteralByReference,
    )
}

/// Mint (or reuse) the content-addressed `r_<hash>` by-reference key for a literal and
/// register its full payload in `refs`. The one place a by-reference literal key is
/// formed — shared by [`classify_literal`] (top-level value slot) and
/// [`encode_embedded_literal`] (embedded triple-term position) so the two never drift.
fn intern_literal_ref(lit: &RdfLiteral, refs: &mut BTreeMap<String, RefPayload>) -> String {
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
    refs: &BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<RdfTerm, Gmn1Error> {
    // RDF 1.2 triple term (`( s p o )`): the two-sided inverse of [`encode_triple_term`].
    if token.starts_with('(') {
        return Ok(RdfTerm::Triple(Box::new(decode_triple_term(
            token,
            dict,
            prefix_to_ns,
            refs,
            sigil,
        )?)));
    }
    if let Some(label) = token.strip_prefix(BLANK_PREFIX) {
        if !is_safe_token_body(label) {
            return Err(non_decodable(format!(
                "malformed blank-node token: {token}"
            )));
        }
        return Ok(RdfTerm::BlankNode(label.to_owned()));
    }
    if let Some(term) = dict.term_for(token) {
        return Ok(RdfTerm::Iri(term.to_owned()));
    }
    if let Some(term) = dict.term_for_glyph(token, sigil) {
        return Ok(RdfTerm::Iri(term.to_owned()));
    }
    if let Some(term) = dict.term_for_glyph_fallback(token, sigil) {
        return Ok(RdfTerm::Iri(term.to_owned()));
    }
    if let Some((prefix, local)) = token.split_once(SEP)
        && let Some(ns) = prefix_to_ns.get(prefix)
    {
        return Ok(RdfTerm::Iri(format!("{ns}{}", unmangle_local(local))));
    }
    // The bare namespace-root form (see `classify_iri`): a token with no SEP that
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
    // A number-SHAPED token that reached here is not a canonical integer/decimal — a
    // `lang:GmnMalformedNumber` (scientific notation, a non-two-digit fraction), never a
    // silently-dropped `Uncovered` (the reader's number-form pass classifies these first;
    // this is the defensive belt so a value token is classified correctly in isolation too).
    if is_number_shaped(token) {
        return Err(Gmn1Error::MalformedNumber {
            token: token.to_owned(),
        });
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

/// Decode a `( s p o )` triple-term token back to an [`RdfTriple`] — the two-sided inverse
/// of [`encode_triple_term`]. Subject and predicate ride the reference dispatch (the
/// predicate must resolve to an IRI, per the RDF data model); the object rides the
/// embedded-term dispatch (it may be a literal, an IRI/blank, or a nested triple term).
fn decode_triple_term(
    token: &str,
    dict: &GmnDictionary,
    prefix_to_ns: &BTreeMap<String, String>,
    refs: &BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<RdfTriple, Gmn1Error> {
    let inner = token
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(')'))
        .ok_or_else(|| {
            non_decodable(format!("malformed triple-term token (no `( … )`): {token}"))
        })?;
    let [subject_tok, predicate_tok, object_tok] = split_triple_components(inner)?;
    let subject = decode_reference(&subject_tok, dict, prefix_to_ns, refs, sigil)?;
    let predicate = match decode_reference(&predicate_tok, dict, prefix_to_ns, refs, sigil)? {
        RdfTerm::Iri(iri) => iri,
        other => {
            return Err(non_decodable(format!(
                "triple-term predicate must decode to an IRI, got {other:?} from token \
                 {predicate_tok}"
            )));
        }
    };
    let object = decode_embedded_term(&object_tok, dict, prefix_to_ns, refs, sigil)?;
    Ok(RdfTriple::new(subject, predicate, object))
}

/// Decode one term embedded inside a triple term — the inverse of [`encode_embedded_term`].
/// A leading `(` is a nested triple; a number-shaped or `r_<hash>` token is a value literal
/// (disjoint by shape / reserved prefix from every reference token); everything else is a
/// reference (IRI/blank/dictionary alias).
fn decode_embedded_term(
    token: &str,
    dict: &GmnDictionary,
    prefix_to_ns: &BTreeMap<String, String>,
    refs: &BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<RdfTerm, Gmn1Error> {
    if token.starts_with('(') {
        return decode_reference(token, dict, prefix_to_ns, refs, sigil);
    }
    if is_integer_token(token) || is_decimal_token(token) || token.starts_with(REF_PREFIX) {
        return decode_value(token, refs);
    }
    decode_reference(token, dict, prefix_to_ns, refs, sigil)
}

/// Split the inner text of a `( … )` triple term into its three top-level components,
/// respecting nested parens (`( a b ( c d e ) )` splits into `a`, `b`, `( c d e )`). A
/// component count other than three, or an unbalanced paren, is `lang:GmnNonDecodableGrammar`.
fn split_triple_components(inner: &str) -> Result<[String; 3], Gmn1Error> {
    let mut components: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth: i32 = 0;
    for ch in inner.chars() {
        match ch {
            '(' => {
                depth += 1;
                current.push(ch);
            }
            ')' => {
                depth -= 1;
                if depth < 0 {
                    return Err(non_decodable(format!(
                        "unbalanced `)` inside triple term: {inner}"
                    )));
                }
                current.push(ch);
            }
            c if c.is_whitespace() && depth == 0 => {
                if !current.is_empty() {
                    components.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if depth != 0 {
        return Err(non_decodable(format!(
            "unbalanced `(` inside triple term: {inner}"
        )));
    }
    if !current.is_empty() {
        components.push(current);
    }
    components.try_into().map_err(|found: Vec<String>| {
        non_decodable(format!(
            "triple term must have exactly three components, found {}: {inner}",
            found.len()
        ))
    })
}

// ── Records: the s/p/o/v/q/st/ev/m/ek/bd/it field model ────────────────────────────

/// The canonical GMN-1 field key order, per `LANG-GMN.md` § "Record form, tabular form,
/// and canonical order". The primary-triple slots (`s p o v`) lead, then the in-band
/// repair TARGET id (`id`), then the folded qualifier slots (`q st ev m ek`, plus the
/// `@p`-only `bd it` pair), and finally the `@err` failure-class name (`class`).
///
/// The two repair keys sit in a fixed canonical position consistent with the charter's
/// worked examples (`@err{id: …, class: …}`, `@patch{id: …, q: …}`, `@retract{id: …}`):
/// `id` is FIRST among the repair fields (immediately after the primary-triple slots, so
/// it precedes a restated `q` — `@patch{id, q}` — as the charter shows), and `class` is
/// LAST (after every qualifier, so `@err{id, class}` renders id-before-class). The patched
/// payload fields (`q`, …) reuse the existing qualifier slots — a repair record introduces
/// no rival key for a value it already has a canonical slot for.
const KEY_ORDER: [&str; 13] = [
    "s", "p", "o", "v", "id", "q", "st", "ev", "m", "ek", "bd", "it", "class",
];

const SIGIL_CLAIM: &str = "@c";
const SIGIL_EVIDENCE: &str = "@e";
const SIGIL_STANDPOINT: &str = "@s";
const SIGIL_PROCESS: &str = "@p";
const SIGIL_PROOF: &str = "@π";
const SIGIL_DEFEATER: &str = "@d";
const SIGIL_MODAL: &str = "@m";
const SIGIL_MATH: &str = "@μ";
const SIGIL_LANG_AST: &str = "@λ";
const SIGIL_LOGIC: &str = "@ℒ";
/// The three in-band repair sigils (`LANG-GMN.md` § "In-band repair"). Each is a
/// claim-about-claims — a reified NEW record naming a stable TARGET record id, never an
/// in-place mutation. `@err` reports a rejected record's failure class, `@patch` restates
/// fields of an identified record, `@retract` withdraws one.
const SIGIL_ERR: &str = "@err";
const SIGIL_PATCH: &str = "@patch";
const SIGIL_RETRACT: &str = "@retract";

const KNOWN_SIGILS: [&str; 13] = [
    SIGIL_CLAIM,
    SIGIL_EVIDENCE,
    SIGIL_STANDPOINT,
    SIGIL_PROCESS,
    SIGIL_PROOF,
    SIGIL_DEFEATER,
    SIGIL_MODAL,
    SIGIL_MATH,
    SIGIL_LANG_AST,
    SIGIL_LOGIC,
    SIGIL_ERR,
    SIGIL_PATCH,
    SIGIL_RETRACT,
];

/// The `gmeow:` class IRIs the three repair sigils map to (Task 1's vocabulary): a
/// repair record is a GMN-0 subject typed with exactly one of these three
/// `gmeow:StandpointClaim` subclasses.
const CLASS_GMN_ERR: &str = "https://blackcatinformatics.ca/gmeow/GmnErr";
const CLASS_GMN_PATCH: &str = "https://blackcatinformatics.ca/gmeow/GmnPatch";
const CLASS_GMN_RETRACT: &str = "https://blackcatinformatics.ca/gmeow/GmnRetract";
/// The stable TARGET record id a repair record names (a datatype property → literal).
const PRED_GMN_REPAIR_ID: &str = "https://blackcatinformatics.ca/gmeow/gmnRepairId";
/// The `lang:LangConformanceFailure` subclass an `@err` names (an object property → IRI).
const PRED_GMN_REPAIR_CLASS: &str = "https://blackcatinformatics.ca/gmeow/gmnRepairClass";

/// The repair sigil a repair-class IRI maps to, or `None` for a non-repair class. The
/// single write-side classifier shared by [`try_repair_record`].
fn repair_sigil_for_class(class: &str) -> Option<&'static str> {
    match class {
        CLASS_GMN_ERR => Some(SIGIL_ERR),
        CLASS_GMN_PATCH => Some(SIGIL_PATCH),
        CLASS_GMN_RETRACT => Some(SIGIL_RETRACT),
        _ => None,
    }
}

/// The repair-class IRI a repair sigil reconstructs, or `None` for a non-repair sigil.
/// The two-sided inverse of [`repair_sigil_for_class`] — the single read-side classifier.
fn repair_class_for_sigil(sigil: &str) -> Option<&'static str> {
    match sigil {
        SIGIL_ERR => Some(CLASS_GMN_ERR),
        SIGIL_PATCH => Some(CLASS_GMN_PATCH),
        SIGIL_RETRACT => Some(CLASS_GMN_RETRACT),
        _ => None,
    }
}

/// Choose a semantic record role from the quad itself. Exact `rdf:type` roles win,
/// followed by the process annotation pair, then the three grounding namespaces.
/// The choice changes only the scoped surface vocabulary; every role reconstructs the
/// same RDF quad shape.
fn sigil_for_quad(quad: &RdfQuad, has_process_annotations: bool) -> &'static str {
    if quad.predicate == RDF_TYPE
        && let RdfTerm::Iri(class) = &quad.object
    {
        match class.as_str() {
            "https://blackcatinformatics.ca/gmeow/EvidenceSpan" => return SIGIL_EVIDENCE,
            "https://blackcatinformatics.ca/gmeow/Standpoint" => return SIGIL_STANDPOINT,
            "https://blackcatinformatics.ca/logic/Process" => return SIGIL_PROCESS,
            "https://blackcatinformatics.ca/math/Proof" => return SIGIL_PROOF,
            "https://blackcatinformatics.ca/gmeow/Defeater" => return SIGIL_DEFEATER,
            "https://blackcatinformatics.ca/gmeow/ModalForce" => return SIGIL_MODAL,
            _ => {}
        }
    }
    if has_process_annotations {
        return SIGIL_PROCESS;
    }
    let iris = [
        match &quad.subject {
            RdfTerm::Iri(iri) => Some(iri.as_str()),
            _ => None,
        },
        Some(quad.predicate.as_str()),
        match &quad.object {
            RdfTerm::Iri(iri) => Some(iri.as_str()),
            _ => None,
        },
    ];
    if iris.iter().flatten().any(|iri| iri.starts_with(MATH_NS)) {
        SIGIL_MATH
    } else if iris.iter().flatten().any(|iri| iri.starts_with(LANG_NS)) {
        SIGIL_LANG_AST
    } else if iris.iter().flatten().any(|iri| iri.starts_with(LOGIC_NS)) {
        SIGIL_LOGIC
    } else {
        SIGIL_CLAIM
    }
}

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

type QuadGroup<'a> = (Option<RdfTerm>, RdfTerm, Vec<&'a RdfQuad>);

/// Group a canonically ordered quad slice by `(graph, subject)`. The writer and
/// coverage audit share this helper so record context cannot drift between them.
fn group_quads(quads: &[RdfQuad]) -> Vec<QuadGroup<'_>> {
    let mut groups = Vec::<QuadGroup<'_>>::new();
    for quad in quads {
        if let Some((graph, subject, bucket)) = groups.last_mut()
            && *graph == quad.graph_name
            && *subject == quad.subject
        {
            bucket.push(quad);
            continue;
        }
        groups.push((quad.graph_name.clone(), quad.subject.clone(), vec![quad]));
    }
    groups
}

/// Return the one safe folded-record host and its selected sigil. A group with
/// zero or multiple primary quads has no fold context and must use flat records.
fn folded_record_context<'a>(bucket: &[&'a RdfQuad]) -> Option<(&'a RdfQuad, &'static str)> {
    let mut primary = bucket
        .iter()
        .copied()
        .filter(|quad| annotation_slot(&quad.predicate).is_none());
    let host = primary.next()?;
    if primary.next().is_some() {
        return None;
    }
    let has_process_annotations = bucket
        .iter()
        .any(|quad| matches!(annotation_slot(&quad.predicate), Some("bd" | "it")));
    Some((host, sigil_for_quad(host, has_process_annotations)))
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
) -> Result<Vec<Record>, Gmn1Error> {
    let mut records = Vec::new();
    for (graph, subject, bucket) in group_quads(quads) {
        // A named-graph quad is OUT OF DOMAIN, not "uncovered": the GMN-0 normal form is
        // default-graph by charter (the grounding slices are authored as `text/turtle`),
        // and the GMN-1 record shape has no graph slot. An honest typed domain boundary,
        // never a term-coverage residual a bigger dictionary could resolve.
        if let Some(graph) = graph {
            return Err(Gmn1Error::NamedGraphOutOfDomain {
                graph: term_sort_string(&graph),
            });
        }
        // In-band repair records (`@err`/`@patch`/`@retract`) are claims-about-claims —
        // a subject typed with one of the three `gmeow:` repair classes, carrying a
        // stable TARGET record id (`gmnRepairId`) plus the `@err` failure class or the
        // `@patch` restated payload. They fold to their own sigil ONLY when the whole
        // group matches the repair shape exactly; a repair-typed group that carries any
        // foreign predicate falls through to the flat/folded logic below (always
        // lossless), never a lossy repair fold.
        if let Some(record) = try_repair_record(&subject, &bucket, dict, ns_to_prefix, refs)? {
            records.push(record);
            continue;
        }
        if let Some((host, sigil)) = folded_record_context(&bucket) {
            let mut fields = BTreeMap::new();
            fields.insert(
                "s",
                encode_reference(&host.subject, dict, ns_to_prefix, refs, sigil)
                    .map_err(Gmn1Error::Uncovered)?,
            );
            fields.insert(
                "p",
                encode_reference(
                    &RdfTerm::Iri(host.predicate.clone()),
                    dict,
                    ns_to_prefix,
                    refs,
                    sigil,
                )
                .map_err(Gmn1Error::Uncovered)?,
            );
            let (obj_key, obj_tok) = encode_object(&host.object, dict, ns_to_prefix, refs, sigil)
                .map_err(Gmn1Error::Uncovered)?;
            fields.insert(obj_key, obj_tok);

            for q in bucket
                .iter()
                .copied()
                .filter(|quad| annotation_slot(&quad.predicate).is_some())
            {
                let slot = annotation_slot(&q.predicate).expect("partitioned as annotation");
                let tok = if slot == "q" {
                    encode_value(&q.object, refs).map_err(Gmn1Error::Uncovered)?
                } else {
                    encode_reference(&q.object, dict, ns_to_prefix, refs, sigil)
                        .map_err(Gmn1Error::Uncovered)?
                };
                fields.insert(slot, tok);
            }
            records.push(Record { sigil, fields });
        } else {
            for q in &bucket {
                let sigil = sigil_for_quad(q, false);
                let mut fields = BTreeMap::new();
                fields.insert(
                    "s",
                    encode_reference(&q.subject, dict, ns_to_prefix, refs, sigil)
                        .map_err(Gmn1Error::Uncovered)?,
                );
                fields.insert(
                    "p",
                    encode_reference(
                        &RdfTerm::Iri(q.predicate.clone()),
                        dict,
                        ns_to_prefix,
                        refs,
                        sigil,
                    )
                    .map_err(Gmn1Error::Uncovered)?,
                );
                let (obj_key, obj_tok) = encode_object(&q.object, dict, ns_to_prefix, refs, sigil)
                    .map_err(Gmn1Error::Uncovered)?;
                fields.insert(obj_key, obj_tok);
                records.push(Record { sigil, fields });
            }
        }
    }
    Ok(records)
}

/// Fold a `(subject, bucket)` group into an in-band repair record (`@err`/`@patch`/
/// `@retract`) when — and only when — the whole group matches the repair shape EXACTLY:
///
/// * exactly one `rdf:type` quad, whose object is one of the three `gmeow:` repair
///   classes (this selects the sigil);
/// * exactly one `gmnRepairId` quad carrying a literal (the stable TARGET record id,
///   carried verbatim into the `id` slot — never resolved to another record);
/// * for `@err`, at most one `gmnRepairClass` quad → the `class` slot (the failure
///   class's compact name, via the ordinary IRI/alias rendering);
/// * for `@patch`, any folded qualifier predicate (`confidence` → `q`, …) → its slot,
///   the restated payload;
/// * NO other predicate.
///
/// Returns `Ok(None)` (not an error) when the group is not a clean repair record, so the
/// caller falls through to the always-lossless flat/folded `@c`-family logic. The
/// subject rides the `s` slot ONLY when it is an IRI whose identity must be preserved; a
/// blank-node repair subject (the reified claim's own fresh identity, as the charter's
/// worked examples show) is omitted and minted fresh on read, canonically equal under
/// RDFC-1.0's blank-label canonicalization.
fn try_repair_record(
    subject: &RdfTerm,
    bucket: &[&RdfQuad],
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
) -> Result<Option<Record>, Gmn1Error> {
    // Exactly one rdf:type quad, naming exactly one repair class, selects the sigil.
    let type_quads: Vec<&&RdfQuad> = bucket.iter().filter(|q| q.predicate == RDF_TYPE).collect();
    let [type_quad] = type_quads.as_slice() else {
        return Ok(None);
    };
    let RdfTerm::Iri(class) = &type_quad.object else {
        return Ok(None);
    };
    let Some(sigil) = repair_sigil_for_class(class) else {
        return Ok(None);
    };

    let mut fields = BTreeMap::new();
    let mut repair_id_seen = false;
    for q in bucket {
        match q.predicate.as_str() {
            RDF_TYPE => {} // the single repair-class type, already consumed for the sigil.
            PRED_GMN_REPAIR_ID => {
                if repair_id_seen || !matches!(q.object, RdfTerm::Literal(_)) {
                    return Ok(None);
                }
                fields.insert(
                    "id",
                    encode_value(&q.object, refs).map_err(Gmn1Error::Uncovered)?,
                );
                repair_id_seen = true;
            }
            PRED_GMN_REPAIR_CLASS if sigil == SIGIL_ERR => {
                if fields.contains_key("class") || !matches!(q.object, RdfTerm::Iri(_)) {
                    return Ok(None);
                }
                fields.insert(
                    "class",
                    encode_reference(&q.object, dict, ns_to_prefix, refs, sigil)
                        .map_err(Gmn1Error::Uncovered)?,
                );
            }
            other => {
                // A `@patch` restates folded qualifier fields (confidence → q, …); any
                // other predicate (or a qualifier on a non-`@patch` repair) means this is
                // not a clean repair record — fall through to the flat/folded logic.
                match annotation_slot(other) {
                    Some(slot) if sigil == SIGIL_PATCH => {
                        if fields.contains_key(slot) {
                            return Ok(None);
                        }
                        let tok = if slot == "q" {
                            encode_value(&q.object, refs).map_err(Gmn1Error::Uncovered)?
                        } else {
                            encode_reference(&q.object, dict, ns_to_prefix, refs, sigil)
                                .map_err(Gmn1Error::Uncovered)?
                        };
                        fields.insert(slot, tok);
                    }
                    _ => return Ok(None),
                }
            }
        }
    }

    // A repair record MUST name its target — a repair-typed group without a `gmnRepairId`
    // is not a well-formed repair record; fall through so it round-trips as flat records.
    if !repair_id_seen {
        return Ok(None);
    }

    if let RdfTerm::Iri(_) = subject {
        fields.insert(
            "s",
            encode_reference(subject, dict, ns_to_prefix, refs, sigil)
                .map_err(Gmn1Error::Uncovered)?,
        );
    }
    Ok(Some(Record { sigil, fields }))
}

/// Encode an object term into its `(key, token)` pair: `v` (value) for a literal, `o`
/// (reference) otherwise — the o-vs-v slot split. An RDF 1.2 triple term is a non-literal
/// object, so it rides the `o` slot as the `( s p o )` surface (see [`encode_triple_term`]).
fn encode_object(
    object: &RdfTerm,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
    sigil: &str,
) -> Result<(&'static str, String), UncoveredTerm> {
    if matches!(object, RdfTerm::Literal(_)) {
        Ok(("v", encode_value(object, refs)?))
    } else {
        Ok((
            "o",
            encode_reference(object, dict, ns_to_prefix, refs, sigil)?,
        ))
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
    fresh_index: usize,
) -> Result<Vec<RdfQuad>, Gmn1Error> {
    // In-band repair records reconstruct their own GMN-0 quad shape (type + repair id +
    // failure class / restated payload), never the primary `s p o/v` triple.
    if repair_class_for_sigil(record.sigil).is_some() {
        return repair_record_to_quads(record, dict, prefix_to_ns, refs, fresh_index);
    }
    // The repair-only keys are legal ONLY inside a repair record; a non-repair record
    // carrying one would silently drop it below (a lost quad, no signal) — a HARD FAIL.
    for repair_key in ["id", "class"] {
        if record.fields.contains_key(repair_key) {
            return Err(non_decodable(format!(
                "non-repair record ({}) carries the repair-only key '{repair_key}'",
                record.sigil
            )));
        }
    }
    let s_tok = record
        .fields
        .get("s")
        .ok_or_else(|| non_decodable("record is missing required key 's'".to_owned()))?;
    let p_tok = record
        .fields
        .get("p")
        .ok_or_else(|| non_decodable("record is missing required key 'p'".to_owned()))?;
    let subject = decode_reference(s_tok, dict, prefix_to_ns, refs, record.sigil)?;
    let RdfTerm::Iri(predicate) = decode_reference(p_tok, dict, prefix_to_ns, refs, record.sigil)?
    else {
        return Err(non_decodable(format!(
            "'p' slot must decode to an IRI, got token {p_tok}"
        )));
    };
    let object = match (record.fields.get("o"), record.fields.get("v")) {
        (Some(o_tok), None) => decode_reference(o_tok, dict, prefix_to_ns, refs, record.sigil)?,
        (None, Some(v_tok)) => decode_value(v_tok, refs)?,
        (None, None) => {
            return Err(non_decodable(
                "record carries neither 'o' nor 'v'".to_owned(),
            ));
        }
        (Some(_), Some(_)) => {
            return Err(non_decodable(
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
                decode_reference(tok, dict, prefix_to_ns, refs, record.sigil)?
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

/// Reconstruct a repair record's GMN-0 quads — the reader's inverse of
/// [`try_repair_record`]: the `rdf:type` → repair-class quad, the `gmnRepairId` → target
/// id quad, and the `@err` `gmnRepairClass` / `@patch` restated payload. HARD-FAILS
/// (typed [`Gmn1Error::NonDecodableGrammar`]) on a repair record missing its mandatory
/// `id`, or carrying a key outside the sigil's allowed set (a primary-triple `p`/`o`/`v`
/// on any repair record, a `class` on a non-`@err`, or a restated qualifier on a
/// non-`@patch`).
///
/// The subject rides the `s` slot when present (an IRI whose identity was preserved);
/// otherwise a fresh blank node is minted — `fresh_index` makes distinct repair records
/// take distinct labels, and RDFC-1.0's blank-label canonicalization makes the mint
/// canonically equal to whatever blank node the original carried.
fn repair_record_to_quads(
    record: &Record,
    dict: &GmnDictionary,
    prefix_to_ns: &BTreeMap<String, String>,
    refs: &BTreeMap<String, RefPayload>,
    fresh_index: usize,
) -> Result<Vec<RdfQuad>, Gmn1Error> {
    let class_iri =
        repair_class_for_sigil(record.sigil).expect("dispatched here only for a repair sigil");

    // Reject any key outside this sigil's allowed set — a silent drop would lose a quad.
    let class_allowed = record.sigil == SIGIL_ERR;
    let payload_allowed = record.sigil == SIGIL_PATCH;
    for key in record.fields.keys() {
        let allowed = match *key {
            "s" | "id" => true,
            "class" => class_allowed,
            "q" | "st" | "ev" | "m" | "ek" | "bd" | "it" => payload_allowed,
            _ => false,
        };
        if !allowed {
            return Err(non_decodable(format!(
                "repair record ({}) carries the key '{key}', which is outside its allowed field set",
                record.sigil
            )));
        }
    }

    let id_tok = record.fields.get("id").ok_or_else(|| {
        non_decodable(format!(
            "repair record ({}) is missing its required target-id key 'id'",
            record.sigil
        ))
    })?;
    let id_object = decode_value(id_tok, refs)?;

    let subject = match record.fields.get("s") {
        Some(s_tok) => decode_reference(s_tok, dict, prefix_to_ns, refs, record.sigil)?,
        None => RdfTerm::BlankNode(format!("gmnRepair{fresh_index}")),
    };

    let mut quads = vec![
        RdfQuad {
            subject: subject.clone(),
            predicate: RDF_TYPE.to_owned(),
            object: RdfTerm::Iri(class_iri.to_owned()),
            graph_name: None,
            location: None,
        },
        RdfQuad {
            subject: subject.clone(),
            predicate: PRED_GMN_REPAIR_ID.to_owned(),
            object: id_object,
            graph_name: None,
            location: None,
        },
    ];

    if let Some(class_tok) = record.fields.get("class") {
        quads.push(RdfQuad {
            subject: subject.clone(),
            predicate: PRED_GMN_REPAIR_CLASS.to_owned(),
            object: decode_reference(class_tok, dict, prefix_to_ns, refs, record.sigil)?,
            graph_name: None,
            location: None,
        });
    }

    for slot in ["q", "st", "ev", "m", "ek", "bd", "it"] {
        if let Some(tok) = record.fields.get(slot) {
            let pred =
                annotation_predicate_for_slot(slot).expect("every qualifier slot has a predicate");
            let object = if slot == "q" {
                decode_value(tok, refs)?
            } else {
                decode_reference(tok, dict, prefix_to_ns, refs, record.sigil)?
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

/// The literal-lexical-form NFC gate — the point where every literal ENTERS encoding.
///
/// The GMN glyph discipline already refuses a non-NFC glyph surface ([`validate_glyph_surface`]'s
/// `is_nfc` check); this extends the SAME discipline to literal content: a literal object
/// (an object `v` slot, or an annotation `q` confidence) whose lexical form is not
/// NFC-normalized HARD-FAILS as [`Gmn1Error::NonNfcLiteral`] before any record is built.
/// No optionality: the codec never silently normalizes, because normalizing the lexical
/// form would change the underlying RDF term (a different `xsd:string` value), and a
/// non-NFC form is a non-canonical Unicode spelling that would make one text take two
/// content digests. A literal only ever occupies the object position of a GMN-0 quad, so
/// scanning object slots covers every literal the writer would encode.
fn assert_model_literals_nfc(model: &Gmn0Model) -> Result<(), Gmn1Error> {
    for quad in &model.quads {
        if let RdfTerm::Literal(lit) = &quad.object
            && !is_nfc(&lit.lexical_form)
        {
            return Err(Gmn1Error::NonNfcLiteral {
                lexical: lit.lexical_form.clone(),
            });
        }
    }
    Ok(())
}

/// GMN-0 → GMN-1: the forward/put leg. Total over any [`Gmn0Model`] whose quads are
/// default-graph triples under a registered namespace (the grounding slices' fragment)
/// PLUS RDF 1.2 triple-term objects, which round-trip losslessly. Hard-fails, never a
/// silent drop, on: a named-graph quad ([`Gmn1Error::NamedGraphOutOfDomain`] — an honest
/// domain boundary) or an IRI under no registered namespace ([`Gmn1Error::Uncovered`]).
pub fn gmn1_write(model: &Gmn0Model, dict: &GmnDictionary) -> Result<Gmn1Document, Gmn1Error> {
    assert_model_literals_nfc(model)?;
    let ns_to_prefix = ns_to_prefix_table();
    let mut refs = BTreeMap::new();
    let records = quads_to_records(&model.quads, dict, &ns_to_prefix, &mut refs)?;

    let mut lines = vec![format!(
        "@gmn{{v: {DIALECT_VERSION}, aliases: {}, glyphs: {}}}",
        dict.aliases_id(),
        dict.glyphs.version()
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
    assert_model_literals_nfc(model)?;
    let ns_to_prefix = ns_to_prefix_table();
    let mut refs = BTreeMap::new();
    let records = quads_to_records(&model.quads, dict, &ns_to_prefix, &mut refs)?;

    let mut lines = vec![format!(
        "@gmn{{v: {DIALECT_VERSION}, aliases: {}, glyphs: {}}}",
        dict.aliases_id(),
        dict.glyphs.version()
    )];

    let uniform_schema: Option<Vec<&'static str>> = records.first().and_then(|first| {
        if first.sigil != SIGIL_CLAIM {
            return None;
        }
        // A triple-term token (`( s p o )`) carries interior spaces; a tabular row is
        // whitespace-delimited, so such a value would corrupt the row lexer's column
        // count. Fall back to record form (always correct) when any field is space-bearing.
        if records
            .iter()
            .any(|r| r.fields.values().any(|v| v.contains(' ')))
        {
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

/// A record lexed from GMN-1 text in READ order — the intermediate the detection-precedence
/// passes ([`Gmn1Error`]'s precedence doc) consume, BEFORE number-form, key-order, header, or
/// dictionary validation. Keeping the read-order `pairs` (not the canonicalizing
/// [`Record`]'s `BTreeMap`) is what lets the key-order pass see the ACTUAL order the tokens
/// appeared in.
struct LexedRecord {
    sigil: &'static str,
    /// `(canonical-key, value)` pairs in the order read from the line.
    pairs: Vec<(&'static str, String)>,
    /// Sigil records enforce canonical key order (the writer emits it); a schema-driven
    /// tabular row does not (its keys come from the once-declared `@claims` schema).
    enforce_key_order: bool,
}

/// GMN-1 → GMN-0: the backward/get leg. A hand-written, table-driven parser applying the
/// [`Gmn1Error`] DETECTION-PRECEDENCE passes in order (lex/grammar → number-form → key-order
/// → header-presence → dictionary-coverage), so every failure resolves to EXACTLY ONE typed
/// `lang:` conformance class — see the module documentation for the reader-independence
/// argument and [`Gmn1Error`] for the precedence rationale.
pub fn gmn1_read(doc: &Gmn1Document, dict: &GmnDictionary) -> Result<Gmn0Model, Gmn1Error> {
    let prefix_to_ns = prefix_to_ns_table();

    // Non-empty, trimmed lines in order. A canonical document opens with the `@gmn{…}`
    // header; whether it is present/valid is decided in the HEADER-PRESENCE pass, so here we
    // only SEPARATE the (optional) header from the record lines. `@gmn{` uniquely marks the
    // header: no value token can contain `{`.
    let raw_lines: Vec<&str> = doc
        .text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let (header_line, record_lines): (Option<&str>, &[&str]) = match raw_lines.split_first() {
        Some((first, rest)) if first.starts_with("@gmn{") => (Some(*first), rest),
        _ => (None, &raw_lines[..]),
    };

    // ── Pass 1 — lex/grammar (`GmnNonDecodableGrammar`) ────────────────────────────────
    let mut lexed: Vec<LexedRecord> = Vec::new();
    let mut pending_columns: Option<Vec<&'static str>> = None;
    for line in record_lines {
        if let Some(rest) = line.strip_prefix("@claims[") {
            let cols_str = rest
                .strip_suffix(']')
                .ok_or_else(|| non_decodable(format!("unterminated @claims header: {line}")))?;
            pending_columns = Some(lex_columns(cols_str)?);
            continue;
        }
        if line.starts_with('@') && line.contains('{') {
            pending_columns = None;
            lexed.push(lex_sigil_record(line)?);
            continue;
        }
        if let Some(cols) = &pending_columns {
            lexed.push(lex_tabular_row(cols, line)?);
            continue;
        }
        return Err(non_decodable(format!(
            "line matches neither a sigil record, a @claims header, nor a pending tabular row: {line}"
        )));
    }

    // ── Pass 2 — number-form (`GmnMalformedNumber`) ────────────────────────────────────
    // A number-SHAPED value token that is not a canonical integer / two-digit decimal is a
    // malformed number, decided before header-presence because number well-formedness is a
    // lexical property independent of the dialect version.
    for record in &lexed {
        for (_key, value) in &record.pairs {
            if is_malformed_number(value) {
                return Err(Gmn1Error::MalformedNumber {
                    token: value.clone(),
                });
            }
        }
    }

    // ── Pass 3 — key-order (`GmnNonCanonicalOrder`) ────────────────────────────────────
    for record in &lexed {
        if !record.enforce_key_order {
            continue;
        }
        let mut last_rank: Option<usize> = None;
        for (key, _value) in &record.pairs {
            let rank = KEY_ORDER
                .iter()
                .position(|candidate| candidate == key)
                .expect("the lexer resolved every key to a KEY_ORDER member");
            if last_rank.is_some_and(|last| rank < last) {
                return Err(Gmn1Error::NonCanonicalOrder {
                    detail: format!(
                        "record key '{key}' precedes an earlier key — records must follow the \
                         canonical key order (s p o v id q st ev m ek bd it class)"
                    ),
                });
            }
            last_rank = Some(rank);
        }
    }

    // ── Pass 4 — header-presence (`GmnUndeclaredDialectVersion`) ───────────────────────
    match header_line {
        Some(line) => parse_header(line, dict)?,
        None => {
            return Err(Gmn1Error::UndeclaredDialectVersion {
                detail: "GMN-1 text must open with an @gmn{...} header pinning the schema and \
                         dictionary version before the first record"
                    .to_owned(),
            });
        }
    }

    // ── Pass 5 — dictionary-coverage (`GmnUncoveredTerm`) + quad reconstruction ─────────
    let mut quads = Vec::new();
    for (fresh_index, record) in lexed.iter().enumerate() {
        let assembled = Record {
            sigil: record.sigil,
            fields: record.pairs.iter().cloned().collect(),
        };
        // `fresh_index` disambiguates the blank node minted for a repair record whose
        // subject rode no `s` slot — distinct records take distinct labels, and RDFC-1.0
        // canonicalization erases the label at comparison time.
        quads.extend(record_to_quads(
            &assembled,
            dict,
            &prefix_to_ns,
            &doc.refs,
            fresh_index,
        )?);
    }
    quads.sort_by_key(quad_sort_key);
    quads.dedup_by(|a, b| quad_sort_key(a) == quad_sort_key(b));
    Ok(Gmn0Model { quads })
}

/// Validate `@gmn{v: 1, aliases: dict-v3, glyphs: 2}` by explicit token scanning — never by re-deriving
/// from what [`gmn1_write`] would emit. A header that fails to open/close or fails to pin the
/// expected version is `lang:GmnUndeclaredDialectVersion` (the dialect coordinates the reader
/// refuses to guess).
fn parse_header(line: &str, dict: &GmnDictionary) -> Result<(), Gmn1Error> {
    let line = line.trim();
    let body = line
        .strip_prefix("@gmn{")
        .and_then(|s| s.strip_suffix('}'))
        .ok_or_else(|| Gmn1Error::UndeclaredDialectVersion {
            detail: format!("GMN-1 text must open with an @gmn{{...}} header, got: {line}"),
        })?;
    let mut version_ok = false;
    let mut aliases_ok = false;
    let mut glyphs_ok = false;
    for pair in body.split(',') {
        let (k, v) = pair
            .split_once(':')
            .ok_or_else(|| Gmn1Error::UndeclaredDialectVersion {
                detail: format!("malformed @gmn header pair: {pair}"),
            })?;
        let (k, v) = (k.trim(), v.trim());
        match k {
            "v" => version_ok = v == DIALECT_VERSION,
            "aliases" => aliases_ok = v == dict.aliases_id(),
            "glyphs" => glyphs_ok = v == dict.glyphs.version(),
            other => {
                return Err(Gmn1Error::UndeclaredDialectVersion {
                    detail: format!("unrecognized @gmn header key: {other}"),
                });
            }
        }
    }
    if !version_ok || !aliases_ok || !glyphs_ok {
        return Err(Gmn1Error::UndeclaredDialectVersion {
            detail: format!(
                "@gmn header does not pin the expected schema/dictionary/glyph-table version: {line}"
            ),
        });
    }
    Ok(())
}

/// Lex one `@sigil{k: v, k: v, ...}` record line into read-order pairs — pass-1 grammar only:
/// an unbalanced brace, an unknown sigil, an unknown key, a duplicate key, or a malformed
/// field pair is `lang:GmnNonDecodableGrammar`. Number-form and key-order are LATER passes.
fn lex_sigil_record(line: &str) -> Result<LexedRecord, Gmn1Error> {
    let brace = line
        .find('{')
        .ok_or_else(|| non_decodable(format!("record line has no '{{': {line}")))?;
    let sigil_str = line[..brace].trim();
    let sigil = KNOWN_SIGILS
        .iter()
        .copied()
        .find(|known| *known == sigil_str)
        .ok_or_else(|| {
            non_decodable(format!(
                "sigil {sigil_str} is outside the GMN-1 record grammar"
            ))
        })?;
    let body = line
        .strip_suffix('}')
        .map(|s| &s[brace + 1..])
        .ok_or_else(|| non_decodable(format!("unterminated record body: {line}")))?;

    let mut pairs: Vec<(&'static str, String)> = Vec::new();
    if !body.trim().is_empty() {
        for pair in body.split(',') {
            let (k, v) = pair
                .split_once(':')
                .ok_or_else(|| non_decodable(format!("malformed field pair: {pair}")))?;
            let (k, v) = (k.trim(), v.trim());
            let key = KEY_ORDER
                .iter()
                .copied()
                .find(|candidate| *candidate == k)
                .ok_or_else(|| {
                    non_decodable(format!("record key '{k}' is outside the canonical key set"))
                })?;
            if pairs.iter().any(|(existing, _)| *existing == key) {
                return Err(non_decodable(format!(
                    "record key '{key}' appears more than once in one record"
                )));
            }
            pairs.push((key, v.to_owned()));
        }
    }
    Ok(LexedRecord {
        sigil,
        pairs,
        enforce_key_order: true,
    })
}

/// Lex the `@claims[...]` column schema, resolving each column to its canonical key — an
/// unknown column is `lang:GmnNonDecodableGrammar`, and so is a REPEATED canonical column:
/// [`lex_tabular_row`] zips columns positionally against row values, and pass-5 assembly
/// (`record.pairs.iter().cloned().collect()` into a `BTreeMap`) would otherwise silently keep
/// only the LAST occurrence's value and drop every earlier one with no error — a quad lost
/// with no signal, mirroring [`lex_sigil_record`]'s duplicate-key guard for the sigil form.
fn lex_columns(cols_str: &str) -> Result<Vec<&'static str>, Gmn1Error> {
    let mut cols: Vec<&'static str> = Vec::new();
    for col in cols_str.split(' ').filter(|s| !s.is_empty()) {
        let key = KEY_ORDER
            .iter()
            .copied()
            .find(|candidate| *candidate == col)
            .ok_or_else(|| {
                non_decodable(format!(
                    "tabular column '{col}' is outside the canonical key set"
                ))
            })?;
        if cols.contains(&key) {
            return Err(non_decodable(format!(
                "tabular column '{key}' appears more than once in the schema"
            )));
        }
        cols.push(key);
    }
    Ok(cols)
}

/// Lex one bare tabular row against the pending `@claims[...]` column schema — a value count
/// that does not match the declared schema is `lang:GmnNonDecodableGrammar`.
fn lex_tabular_row(cols: &[&'static str], line: &str) -> Result<LexedRecord, Gmn1Error> {
    let values: Vec<&str> = line.split_whitespace().collect();
    if values.len() != cols.len() {
        return Err(non_decodable(format!(
            "tabular row has {} value(s) but the declared schema has {} column(s): {line}",
            values.len(),
            cols.len()
        )));
    }
    let pairs = cols
        .iter()
        .copied()
        .zip(values.into_iter().map(str::to_owned))
        .collect();
    Ok(LexedRecord {
        sigil: SIGIL_CLAIM,
        pairs,
        enforce_key_order: false,
    })
}

// ── Coverage measurement (the axis primitive, distinct from the round-trip gate) ───

/// A per-construct coverage measurement over a [`Gmn0Model`] — the fraction of
/// `model`'s quads whose subject, predicate, and object each encode losslessly to a
/// GMN-1 token, WITHOUT hard-failing the whole model on the first uncovered quad.
///
/// This is deliberately NOT [`round_trip_check`]: the round-trip gate (Task 6,
/// `crates/pipeline/src/stages/gmn1_gate.rs`) is total-or-hard-fail over the
/// grounding slices' GMN-0 (no optionality within that domain); this report is the
/// MEASUREMENT primitive `gmeow-slice-quality`'s `gmn1_coverage_axis` composes over
/// every OTHER slice's GMN-0 vocabulary (Task 7's convergence-contract axis) — a
/// bounded `[0,1]` fraction, never an unbounded ratio.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoverageReport {
    /// Quads whose subject, predicate, and object each encode losslessly.
    pub covered: usize,
    /// Every quad measured (the report's denominator).
    pub total: usize,
}

impl CoverageReport {
    /// The bounded `[0,1]` coverage fraction. The vacuous empty model (no quads to
    /// cover) scores `1.0` — nothing uncovered is trivially fully covered, mirroring
    /// every other vacuous-slice convention in the slice-quality rubric.
    #[must_use]
    pub fn fraction(&self) -> f64 {
        if self.total == 0 {
            1.0
        } else {
            #[allow(clippy::cast_precision_loss)]
            let f = self.covered as f64 / self.total as f64;
            f
        }
    }
}

/// Measure [`CoverageReport`] over every quad in `model` against `dict`, reusing the
/// SAME grouped record context [`gmn1_write`] uses — never a second, duplicated
/// notion of "coverable".
#[must_use]
pub fn measure_coverage(model: &Gmn0Model, dict: &GmnDictionary) -> CoverageReport {
    let covered = classify_model(model, dict)
        .into_iter()
        .filter(|coverage| matches!(coverage, QuadCoverage::Covered { .. }))
        .count();
    CoverageReport {
        covered,
        total: model.quads.len(),
    }
}

// ── Per-quad construct classification (the coverage-completeness audit) ────────────

/// One quad's classification against this codec's own write-side dispatch — see
/// [`Gmn1ConstructCategory`]'s doc for why this can never drift from what [`gmn1_write`]
/// actually does to the same quad.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuadCoverage {
    /// Every slot (subject, predicate, object) encodes losslessly; the categories name
    /// which codec construct each slot fell into. The object's category is whichever
    /// [`classify_value`] (a literal object — the `v` slot) or [`classify_reference`]
    /// (an IRI/blank-node object — the `o` slot) produced, mirroring [`encode_object`]'s
    /// o-vs-v split.
    Covered {
        subject: Gmn1ConstructCategory,
        predicate: Gmn1ConstructCategory,
        object: Gmn1ConstructCategory,
    },
    /// This quad hits a construct outside the covered category set: a named-graph quad
    /// (out-of-domain; [`gmn1_write`] raises [`Gmn1Error::NamedGraphOutOfDomain`] for it),
    /// an IRI under no registered namespace, an unsafe blank-node label, or a
    /// non-literal/non-reference term in a slot that requires the other shape. An RDF 1.2
    /// triple-term object is NOT here — it classifies as [`Gmn1ConstructCategory::TripleTerm`]
    /// under [`QuadCoverage::Covered`], mirroring [`gmn1_write`]'s lossless encoding.
    Uncovered(UncoveredTerm),
}

/// Classify one quad under the sigil selected for its writer record.
fn classify_quad_in_record(
    quad: &RdfQuad,
    dict: &GmnDictionary,
    ns_to_prefix: &[(String, String)],
    refs: &mut BTreeMap<String, RefPayload>,
    sigil: &str,
) -> QuadCoverage {
    if quad.graph_name.is_some() {
        return QuadCoverage::Uncovered(UncoveredTerm(
            "named-graph-scoped quads are outside this codec's covered record model \
             (no graph slot in the GMN-1 record shape)"
                .to_owned(),
        ));
    }
    let subject = match classify_reference(&quad.subject, dict, ns_to_prefix, refs, sigil) {
        Ok((_, category)) => category,
        Err(e) => return QuadCoverage::Uncovered(e),
    };
    let predicate = match classify_reference(
        &RdfTerm::Iri(quad.predicate.clone()),
        dict,
        ns_to_prefix,
        refs,
        sigil,
    ) {
        Ok((_, category)) => category,
        Err(e) => return QuadCoverage::Uncovered(e),
    };
    let object_result = if matches!(quad.object, RdfTerm::Literal(_)) {
        classify_value(&quad.object, refs)
    } else {
        classify_reference(&quad.object, dict, ns_to_prefix, refs, sigil)
    };
    let object = match object_result {
        Ok((_, category)) => category,
        Err(e) => return QuadCoverage::Uncovered(e),
    };
    QuadCoverage::Covered {
        subject,
        predicate,
        object,
    }
}

/// Classify every quad with the same grouped record context and selected sigil as
/// [`quads_to_records`]. The returned vector is in `model.quads` order and has the
/// same length. Exactly-one-primary groups inherit their folded host's sigil,
/// including `@p` selected by sibling `bd`/`it` annotations; ambiguous groups use
/// the writer's flat per-quad fallback.
#[must_use]
pub fn classify_model(model: &Gmn0Model, dict: &GmnDictionary) -> Vec<QuadCoverage> {
    let ns_to_prefix = ns_to_prefix_table();
    let mut refs = BTreeMap::new();
    let mut classifications = Vec::with_capacity(model.quads.len());

    for (graph, _subject, bucket) in group_quads(&model.quads) {
        if graph.is_some() {
            classifications.extend(bucket.into_iter().map(|quad| {
                classify_quad_in_record(quad, dict, &ns_to_prefix, &mut refs, SIGIL_CLAIM)
            }));
        } else if let Some((_host, sigil)) = folded_record_context(&bucket) {
            classifications.extend(
                bucket.into_iter().map(|quad| {
                    classify_quad_in_record(quad, dict, &ns_to_prefix, &mut refs, sigil)
                }),
            );
        } else {
            classifications.extend(bucket.into_iter().map(|quad| {
                let sigil = sigil_for_quad(quad, false);
                classify_quad_in_record(quad, dict, &ns_to_prefix, &mut refs, sigil)
            }));
        }
    }

    classifications
}

/// Per-[`Gmn1ConstructCategory`] occurrence tally over a quad corpus — the
/// coverage-COMPLETENESS primitive, distinct from
/// [`CoverageReport`] (which measures the FRACTION of quads covered). This measures
/// whether each of the codec's own [`Gmn1ConstructCategory::ALL`] categories was hit AT
/// LEAST ONCE by the corpus: a category with zero real occurrences is a category no
/// round-trip test — however large the corpus it runs over — could ever have actually
/// exercised, so a latent bug in that dispatch branch would go undetected indefinitely.
/// Proving every category's count is nonzero over the REAL grounding slices turns
/// "the round-trip gate happens to pass" into "every codec construct category is
/// genuinely proven against production content."
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConstructCoverageTally {
    counts: BTreeMap<Gmn1ConstructCategory, usize>,
    /// Every quad this tally found uncovered, in encounter order. A non-empty list here
    /// is the round-trip gate's own job to catch (an `Uncovered` construct hard-fails
    /// `gmn1_write`); this tally surfaces it too purely so a caller can cross-check the
    /// two audits agree, never as a substitute for the round-trip gate's hard fail.
    pub uncovered: Vec<UncoveredTerm>,
}

impl ConstructCoverageTally {
    /// Fold every quad of `model` into this tally, via [`classify_model`].
    pub fn absorb(&mut self, model: &Gmn0Model, dict: &GmnDictionary) {
        for coverage in classify_model(model, dict) {
            match coverage {
                QuadCoverage::Covered {
                    subject,
                    predicate,
                    object,
                } => {
                    *self.counts.entry(subject).or_insert(0) += 1;
                    *self.counts.entry(predicate).or_insert(0) += 1;
                    *self.counts.entry(object).or_insert(0) += 1;
                }
                QuadCoverage::Uncovered(u) => self.uncovered.push(u),
            }
        }
    }

    /// The count for one category (0 if never observed).
    #[must_use]
    pub fn count(&self, category: Gmn1ConstructCategory) -> usize {
        self.counts.get(&category).copied().unwrap_or(0)
    }

    /// Every category in [`Gmn1ConstructCategory::ALL`] this tally never observed, in
    /// `ALL`'s order — a non-empty result is the completeness gap this audit exists to
    /// catch: falsifiable by construction (see the type-level doc and this crate's
    /// `construct_coverage_audit_is_falsifiable_on_a_missing_category` test).
    #[must_use]
    pub fn unexercised_categories(&self) -> Vec<Gmn1ConstructCategory> {
        Gmn1ConstructCategory::ALL
            .iter()
            .copied()
            .filter(|c| self.count(*c) == 0)
            .collect()
    }
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
        Err(non_decodable(format!(
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

    fn glyph_registry_fixture(rows: &str, version: &str) -> Arc<RdfDataset> {
        let ttl = format!(
            r#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .

gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ;
    gmeow:references ex:dict, ex:script, ex:mathRole, ex:logicRole ;
    gmeow:gmnDictionaryVersion "3" ;
    gmeow:gmnGlyphTableVersion "{version}" .
ex:dict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
ex:script a lang:Script ;
    lang:hasGrapheme ex:g, ex:g1, ex:g2, ex:gPlus, ex:gNot .
ex:mathRole gmeow:gmnSigilGlyph "@μ" .
ex:logicRole gmeow:gmnSigilGlyph "@ℒ" .
{rows}
"#
        );
        parse_dataset(ttl.as_bytes(), "text/turtle", None).expect("glyph fixture parses")
    }

    #[test]
    fn glyph_registry_is_graph_derived_scoped_and_longest_match_ordered() {
        let ds = glyph_registry_fixture(
            r#"
ex:g1 gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f1 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ;
    lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ;
    gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "add" ;
    gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+00AC U+00AC" ; gmeow:gmnSigilScope ex:logicRole .
ex:f2 gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ;
    lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ;
    gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "not" ;
    gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let registry = GmnGlyphRegistry::from_dataset(&ds).expect("registry loads");
        assert_eq!(
            registry.glyph_for(&format!("{MATH_NS}Addition"), "@μ"),
            Some("+")
        );
        assert_eq!(
            registry.term_for("+", "@μ"),
            Some(format!("{MATH_NS}Addition").as_str())
        );
        assert_eq!(
            registry.glyph_for(&format!("{MATH_NS}Addition"), "@ℒ"),
            None
        );
        assert_eq!(
            registry.glyph_for_signature(
                &format!("{MATH_NS}Addition"),
                "@μ",
                Some(&format!("{GMEOW_NS}gmnFixityInfix")),
                Some(2),
            ),
            Some("+")
        );
        assert_eq!(
            registry.glyph_for_signature(
                &format!("{MATH_NS}Addition"),
                "@μ",
                Some(&format!("{GMEOW_NS}gmnFixityPrefix")),
                Some(2),
            ),
            None,
            "wrong fixity must not resolve"
        );
        assert_eq!(
            registry.term_for_signature(
                "+",
                "@μ",
                Some(&format!("{GMEOW_NS}gmnFixityInfix")),
                Some(1),
            ),
            None,
            "wrong arity must not resolve"
        );
        assert_eq!(registry.glyph_tokens(), vec!["¬¬", "+"]);
        assert_eq!(
            registry.render_glyph_token_production(),
            "glyphToken ::= '¬¬' | '+'"
        );
        let grammar = registry
            .render_grammar(b"referenceToken ::= identifier | glyphToken\nglyphToken ::= 'stale'\n")
            .expect("the graph-derived production renders");
        assert_eq!(
            String::from_utf8(grammar).unwrap(),
            "referenceToken ::= identifier | glyphToken\nglyphToken ::= '¬¬' | '+'\n"
        );
    }

    #[test]
    fn removing_a_denotation_removes_writer_reader_and_generated_grammar_binding() {
        let full = glyph_registry_fixture(
            r#"
ex:gPlus gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:fPlus gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:dPlus a lang:Denotation ; lang:denotedForm ex:fPlus ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:gPlus .
ex:cPlus a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:dPlus ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:gNot gmeow:gmnCodepoints "U+00AC" ; gmeow:gmnSigilScope ex:logicRole .
ex:fNot gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:dNot a lang:Denotation ; lang:denotedForm ex:fNot ; lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ; gmeow:gmnDenotationGrapheme ex:gNot .
ex:cNot a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:dNot ; gmeow:gmnAsciiFallback "not" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let pruned = glyph_registry_fixture(
            r#"
ex:gPlus gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:gNot gmeow:gmnCodepoints "U+00AC" ; gmeow:gmnSigilScope ex:logicRole .
ex:fNot gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:dNot a lang:Denotation ; lang:denotedForm ex:fNot ; lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ; gmeow:gmnDenotationGrapheme ex:gNot .
ex:cNot a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:dNot ; gmeow:gmnAsciiFallback "not" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let full_dict = GmnDictionary::from_dataset(&full).expect("full dictionary loads");
        let pruned_dict = GmnDictionary::from_dataset(&pruned).expect("pruned dictionary loads");

        let mut builder = RdfDatasetBuilder::new();
        let subject = builder.intern_iri(&format!("{MATH_NS}Expression"));
        let predicate = builder.intern_iri(&format!("{MATH_NS}operator"));
        let addition = builder.intern_iri(&format!("{MATH_NS}Addition"));
        builder.push_quad(subject, predicate, addition, None);
        let model = Gmn0Model::from_dataset(&builder.freeze().expect("freeze"));

        let full_doc = gmn1_write(&model, &full_dict).expect("full writer uses glyph");
        assert!(full_doc.text.contains("o: +"), "{}", full_doc.text);
        let pruned_doc = gmn1_write(&model, &pruned_dict).expect("fallback writer remains total");
        assert!(
            pruned_doc.text.contains("o: math__Addition"),
            "{}",
            pruned_doc.text
        );
        assert!(
            matches!(
                gmn1_read(&full_doc, &pruned_dict),
                Err(Gmn1Error::Uncovered(_))
            ),
            "the pruned reader must reject the now-unknown scoped glyph"
        );

        let template = b"referenceToken ::= identifier | glyphToken\nglyphToken ::= 'stale'\n";
        let full_grammar = String::from_utf8(
            full_dict
                .glyph_registry()
                .render_grammar(template)
                .expect("full grammar"),
        )
        .unwrap();
        let pruned_grammar = String::from_utf8(
            pruned_dict
                .glyph_registry()
                .render_grammar(template)
                .expect("pruned grammar"),
        )
        .unwrap();
        assert!(full_grammar.contains("'+'"));
        assert!(!pruned_grammar.contains("'+'"));
    }

    #[test]
    fn glyph_registry_rejects_scope_collision_and_uts39_confusable_pair() {
        let exact = glyph_registry_fixture(
            r#"
ex:g1 gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f1 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f2 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ; lang:denotationTarget <https://blackcatinformatics.ca/math/PositiveSign> ; gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "positive" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let error = GmnGlyphRegistry::from_dataset(&exact).expect_err("exact collision rejects");
        assert!(error.0.contains("collides in scope"));

        let confusable = glyph_registry_fixture(
            r#"
ex:g1 gmeow:gmnCodepoints "U+0041" ; gmeow:gmnSigilScope ex:mathRole .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ; lang:denotationTarget <https://blackcatinformatics.ca/math/LatinA> ; gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "latinA" ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+0391" ; gmeow:gmnSigilScope ex:mathRole .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ; lang:denotationTarget <https://blackcatinformatics.ca/math/GreekAlpha> ; gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "greekAlpha" ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let error = GmnGlyphRegistry::from_dataset(&confusable)
            .expect_err("a UTS #39 skeleton collision rejects");
        assert!(error.0.contains("UTS #39-confusable"), "{}", error.0);
    }

    #[test]
    fn glyph_registry_rejects_bare_token_signature_ambiguity() {
        let ambiguous = glyph_registry_fixture(
            r#"
ex:g1 gmeow:gmnCodepoints "U+2212" ; gmeow:gmnSigilScope ex:mathRole .
ex:f1 gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:d1 a lang:Denotation ; lang:denotedForm ex:f1 ; lang:denotationTarget <https://blackcatinformatics.ca/math/Negation> ; gmeow:gmnDenotationGrapheme ex:g1 .
ex:c1 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d1 ; gmeow:gmnAsciiFallback "neg" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
ex:g2 gmeow:gmnCodepoints "U+2212" ; gmeow:gmnSigilScope ex:mathRole .
ex:f2 gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d2 a lang:Denotation ; lang:denotedForm ex:f2 ; lang:denotationTarget <https://blackcatinformatics.ca/math/Subtraction> ; gmeow:gmnDenotationGrapheme ex:g2 .
ex:c2 a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d2 ; gmeow:gmnAsciiFallback "sub" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let error = GmnGlyphRegistry::from_dataset(&ambiguous)
            .expect_err("bare scoped minus cannot choose between two signatures");
        assert!(
            error.0.contains("bare GMN token would be ambiguous"),
            "{}",
            error.0
        );
    }

    #[test]
    fn glyph_registry_rejects_noncanonical_unicode_and_version_drift() {
        for (codepoints, needle) in [
            ("U+03c0", "canonical uppercase"),
            ("U+0065 U+0301", "not NFC-normalized"),
            ("U+2066 U+00AC", "bidi/default-ignorable"),
        ] {
            let rows = format!(
                r#"ex:g gmeow:gmnCodepoints "{codepoints}" ; gmeow:gmnSigilScope ex:logicRole .
ex:f gmeow:gmnFixity gmeow:gmnFixityPrefix ; gmeow:gmnArity 1 .
ex:d a lang:Denotation ; lang:denotedForm ex:f ; lang:denotationTarget <https://blackcatinformatics.ca/logic/not> ; gmeow:gmnDenotationGrapheme ex:g .
ex:c a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d ; gmeow:gmnAsciiFallback "not" ; gmeow:gmnArity 1 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph ."#
            );
            let ds = glyph_registry_fixture(&rows, GLYPH_VERSION);
            let error = GmnGlyphRegistry::from_dataset(&ds).expect_err("invalid glyph rejects");
            assert!(
                error.0.contains(needle),
                "expected {needle:?} in {}",
                error.0
            );
        }

        let ds = glyph_registry_fixture("", "1");
        let error = GmnGlyphRegistry::from_dataset(&ds).expect_err("version drift rejects");
        assert!(error.0.contains("does not match codec version"));
    }

    #[test]
    fn glyph_registry_rejects_scope_outside_the_closed_sigil_set() {
        let unsupported_scope = glyph_registry_fixture(
            r#"
gmeow:gmnCodebookCurrent gmeow:references ex:deadRole .
ex:script lang:hasGrapheme ex:gDead .
ex:deadRole gmeow:gmnSigilGlyph "@x" .
ex:gDead gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:deadRole .
ex:fDead gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:dDead a lang:Denotation ; lang:denotedForm ex:fDead ;
    lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ;
    gmeow:gmnDenotationGrapheme ex:gDead .
ex:cDead a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:dDead ; gmeow:gmnAsciiFallback "add" ;
    gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let error = GmnGlyphRegistry::from_dataset(&unsupported_scope)
            .expect_err("a role outside the reader/writer's closed sigil set must reject");
        assert!(
            error.0.contains("unsupported GMN sigil \"@x\""),
            "{}",
            error.0
        );
    }

    #[test]
    fn glyph_registry_requires_typed_denotation_and_complete_operator_signature() {
        let untyped = glyph_registry_fixture(
            r#"
ex:g gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:d lang:denotedForm ex:f ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:g .
ex:c a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let error = GmnGlyphRegistry::from_dataset(&untyped)
            .expect_err("an untyped denotation must not enter executable resolution");
        assert!(error.0.contains("not typed lang:Denotation"), "{}", error.0);

        let incomplete_signature = glyph_registry_fixture(
            r#"
ex:g gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:f gmeow:gmnFixity gmeow:gmnFixityInfix .
ex:d a lang:Denotation ; lang:denotedForm ex:f ; lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ; gmeow:gmnDenotationGrapheme ex:g .
ex:c a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:d ; gmeow:gmnAsciiFallback "add" ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let error = GmnGlyphRegistry::from_dataset(&incomplete_signature)
            .expect_err("fixity without arity must not create a partial executable key");
        assert!(
            error
                .0
                .contains("must author gmnFixity and gmnArity together"),
            "{}",
            error.0
        );
    }

    #[test]
    fn current_codebook_versions_are_required_and_unrelated_history_is_ignored() {
        let missing_glyph_version = parse_dataset(
            br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .
gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ; gmeow:references ex:dict, ex:script ; gmeow:gmnDictionaryVersion "3" .
ex:dict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
ex:script a lang:Script ; lang:hasGrapheme ex:g .
"#,
            "text/turtle",
            None,
        )
        .expect("fixture parses");
        let error = GmnDictionary::from_dataset(&missing_glyph_version)
            .expect_err("a missing current glyph version must never default");
        assert!(
            error
                .0
                .contains("gmnGlyphTableVersion must be declared exactly once"),
            "{}",
            error.0
        );

        let missing_dictionary_version = parse_dataset(
            br#"@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex: <https://example.test/> .
gmeow:gmnCodebookCurrent a gmeow:GmnCodebook ; gmeow:references ex:dict, ex:script ; gmeow:gmnGlyphTableVersion "2" .
ex:dict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "3" .
ex:script a lang:Script ; lang:hasGrapheme ex:g .
"#,
            "text/turtle",
            None,
        )
        .expect("fixture parses");
        let error = GmnDictionary::from_dataset(&missing_dictionary_version)
            .expect_err("a missing current dictionary version must never default");
        assert!(
            error
                .0
                .contains("gmnDictionaryVersion must be declared exactly once"),
            "{}",
            error.0
        );

        let with_history = glyph_registry_fixture(
            r#"
ex:oldCodebook a gmeow:GmnCodebook ; gmeow:references ex:oldDict, ex:oldScript, ex:oldRole ; gmeow:gmnDictionaryVersion "1" ; gmeow:gmnGlyphTableVersion "1" .
ex:oldDict a gmeow:GmnDictionary ; gmeow:gmnDictionaryVersion "1" ; gmeow:gmnDictionaryEntry ex:oldEntry .
ex:oldEntry gmeow:gmnDictionaryEntryTerm <https://blackcatinformatics.ca/math/Obsolete> ; gmeow:gmnDictionaryEntryAlias "obsolete" .
ex:oldScript a lang:Script ; lang:hasGrapheme ex:oldGlyph .
ex:oldRole gmeow:gmnSigilGlyph "@μ" .
ex:oldGlyph gmeow:gmnCodepoints "U+2212" ; gmeow:gmnSigilScope ex:oldRole .
ex:oldForm gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:oldDenotation a lang:Denotation ; lang:denotedForm ex:oldForm ; lang:denotationTarget <https://blackcatinformatics.ca/math/Obsolete> ; gmeow:gmnDenotationGrapheme ex:oldGlyph .
ex:oldCandidate a gmeow:GmnSymbolCandidate ; gmeow:gmnCandidateDenotation ex:oldDenotation ; gmeow:gmnAsciiFallback "obsoleteOp" ; gmeow:gmnArity 2 ; gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .
"#,
            GLYPH_VERSION,
        );
        let dictionary = GmnDictionary::from_dataset(&with_history)
            .expect("unrelated historical versions do not contaminate current resolution");
        assert_eq!(dictionary.version(), DICTIONARY_VERSION);
        assert!(dictionary.term_for("obsolete").is_none());
        assert!(dictionary.glyph_registry().glyph_tokens().is_empty());
    }

    #[test]
    fn unrelated_denotation_functionality_does_not_poison_current_registry() {
        let dataset = glyph_registry_fixture(
            r#"
ex:g gmeow:gmnCodepoints "U+002B" ; gmeow:gmnSigilScope ex:mathRole .
ex:currentForm gmeow:gmnFixity gmeow:gmnFixityInfix ; gmeow:gmnArity 2 .
ex:currentDenotation a lang:Denotation ;
    lang:denotedForm ex:currentForm ;
    lang:denotationTarget <https://blackcatinformatics.ca/math/Addition> ;
    gmeow:gmnDenotationGrapheme ex:g .
ex:currentCandidate a gmeow:GmnSymbolCandidate ;
    gmeow:gmnCandidateDenotation ex:currentDenotation ;
    gmeow:gmnAsciiFallback "add" ;
    gmeow:gmnArity 2 ;
    gmeow:gmnSymbolDisposition gmeow:gmnDispositionAdoptedGlyph .

# Slice-quality carriers merge conformance fixtures with the module. A fixture
# denotation outside the current script may be deliberately non-functional and
# must not contaminate executable glyph-registry construction.
ex:fixtureDenotation a lang:Denotation ;
    lang:denotedForm ex:fixtureFormOne, ex:fixtureFormTwo .
"#,
            GLYPH_VERSION,
        );

        let registry = GmnGlyphRegistry::from_dataset(&dataset)
            .expect("unrelated fixture denotation is outside current glyph inventory");
        assert_eq!(
            registry.glyph_for(&format!("{MATH_NS}Addition"), "@μ"),
            Some("+")
        );
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
    fn writer_and_reader_cover_all_thirteen_declared_sigils() {
        let mut builder = RdfDatasetBuilder::new();
        let rdf_type = builder.intern_iri(RDF_TYPE);
        for (local, class) in [
            ("evidence", format!("{GMEOW_NS}EvidenceSpan")),
            ("standpoint", format!("{GMEOW_NS}Standpoint")),
            ("process", format!("{LOGIC_NS}Process")),
            ("proof", format!("{MATH_NS}Proof")),
            ("defeater", format!("{GMEOW_NS}Defeater")),
            ("modal", format!("{GMEOW_NS}ModalForce")),
        ] {
            let subject = builder.intern_iri(&format!("{GMEOW_NS}{local}SigilProbe"));
            let object = builder.intern_iri(&class);
            builder.push_quad(subject, rdf_type, object, None);
        }
        // The three in-band repair sigils: each probe is a repair-class-typed subject
        // that also names its target id (and, for @err, its failure class), so the
        // writer folds it to its repair sigil rather than to flat `@c` records.
        for (local, class) in [
            ("err", CLASS_GMN_ERR),
            ("patch", CLASS_GMN_PATCH),
            ("retract", CLASS_GMN_RETRACT),
        ] {
            let subject = builder.intern_iri(&format!("{GMEOW_NS}{local}SigilProbe"));
            let object = builder.intern_iri(class);
            builder.push_quad(subject, rdf_type, object, None);
            let repair_id = builder.intern_iri(PRED_GMN_REPAIR_ID);
            let target = builder.intern_literal(RdfLiteral::typed("t1", XSD_STRING));
            builder.push_quad(subject, repair_id, target, None);
        }
        let err_probe = builder.intern_iri(&format!("{GMEOW_NS}errSigilProbe"));
        let repair_class = builder.intern_iri(PRED_GMN_REPAIR_CLASS);
        let failure_class = builder.intern_iri(&format!("{LANG_NS}GmnMalformedNumber"));
        builder.push_quad(err_probe, repair_class, failure_class, None);
        for (local, predicate, object) in [
            (
                "claim",
                format!("{GMEOW_NS}hasState"),
                format!("{GMEOW_NS}State"),
            ),
            (
                "math",
                format!("{MATH_NS}operatorDomain"),
                format!("{MATH_NS}realNumbers"),
            ),
            (
                "lang",
                format!("{LANG_NS}denotedForm"),
                format!("{LANG_NS}Form"),
            ),
            (
                "logic",
                format!("{LOGIC_NS}and"),
                format!("{LOGIC_NS}Formula"),
            ),
        ] {
            let subject = builder.intern_iri(&format!("{GMEOW_NS}{local}Probe"));
            let predicate = builder.intern_iri(&predicate);
            let object = builder.intern_iri(&object);
            builder.push_quad(subject, predicate, object, None);
        }
        let model = Gmn0Model::from_dataset(&builder.freeze().expect("freeze"));
        let dictionary = real_dict();
        let document = gmn1_write(&model, &dictionary).expect("all semantic roles write");
        for sigil in KNOWN_SIGILS {
            assert!(
                document
                    .text
                    .lines()
                    .any(|line| line.starts_with(&format!("{sigil}{{"))),
                "writer did not emit {sigil}:\n{}",
                document.text
            );
        }
        round_trip_check(&model, &dictionary).expect("all thirteen sigils read back exactly");
    }

    #[test]
    fn real_writer_uses_scoped_grounding_glyphs_and_wrong_scope_hard_fails() {
        let mut builder = RdfDatasetBuilder::new();
        let pi = builder.intern_iri(&format!("{MATH_NS}pi"));
        let math_predicate = builder.intern_iri(&format!("{MATH_NS}operatorDomain"));
        let addition = builder.intern_iri(&format!("{MATH_NS}Addition"));
        builder.push_quad(pi, math_predicate, addition, None);
        let formula = builder.intern_iri(&format!("{LOGIC_NS}Formula"));
        let not = builder.intern_iri(&format!("{LOGIC_NS}not"));
        let operand = builder.intern_iri(&format!("{LOGIC_NS}AtomicFormula"));
        builder.push_quad(formula, not, operand, None);
        let model = Gmn0Model::from_dataset(&builder.freeze().expect("freeze"));
        let dictionary = real_dict();
        let document = gmn1_write(&model, &dictionary).expect("grounding glyphs write");
        assert!(document.text.contains("@μ{s: π"), "{}", document.text);
        assert!(document.text.contains("o: +"), "{}", document.text);
        assert!(document.text.contains("@ℒ{"), "{}", document.text);
        assert!(document.text.contains("p: ¬"), "{}", document.text);
        round_trip_check(&model, &dictionary).expect("glyph-bearing records round-trip");

        let fallback_text = document
            .text
            .replace("s: π", "s: pi")
            .replace("o: +", "o: add")
            .replace("p: ¬", "p: not");
        let fallback_document = Gmn1Document::from_text(fallback_text);
        let fallback_back = gmn1_read(&fallback_document, &dictionary)
            .expect("authored adopted-glyph ASCII fallbacks decode");
        assert!(
            gmn0_canonically_equal(&model, &fallback_back),
            "ASCII fallbacks must have semantic parity with their canonical glyphs"
        );

        let wrong_scope = Gmn1Document::from_text(
            "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@ℒ{s: logic__Formula, p: +, o: logic__Formula}\n",
        );
        assert!(
            matches!(
                gmn1_read(&wrong_scope, &dictionary),
                Err(Gmn1Error::Uncovered(_))
            ),
            "a math-scoped glyph must not decode in @logic scope"
        );
        let wrong_fallback_scope = Gmn1Document::from_text(
            "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@ℒ{s: logic__Formula, p: add, o: logic__Formula}\n",
        );
        assert!(
            matches!(
                gmn1_read(&wrong_fallback_scope, &dictionary),
                Err(Gmn1Error::Uncovered(_))
            ),
            "an adopted glyph's ASCII fallback must preserve the same sigil scope"
        );
    }

    #[test]
    fn coverage_uses_grouped_process_record_context() {
        let mut builder = RdfDatasetBuilder::new();
        let subject = builder.intern_iri(&format!("{GMEOW_NS}processCoverageProbe"));
        let predicate = builder.intern_iri(&format!("{GMEOW_NS}hasState"));
        let addition = builder.intern_iri(&format!("{MATH_NS}Addition"));
        builder.push_quad(subject, predicate, addition, None);

        let boundary = builder.intern_iri(PRED_OCCURRENT_BOUNDARY);
        let open = builder.intern_iri(&format!("{LOGIC_NS}Open"));
        builder.push_quad(subject, boundary, open, None);

        let model = Gmn0Model::from_dataset(&builder.freeze().expect("freeze"));
        let dictionary = real_dict();
        let document = gmn1_write(&model, &dictionary).expect("process record writes");
        assert!(document.text.contains("@p{"), "{}", document.text);
        assert!(
            document.text.contains("o: math__Addition"),
            "the @p record must not use math:Addition's @mu-only glyph:\n{}",
            document.text
        );
        assert!(!document.text.contains("o: +"), "{}", document.text);

        let classifications = classify_model(&model, &dictionary);
        let primary = model
            .quads
            .iter()
            .zip(&classifications)
            .find(|(quad, _)| quad.predicate == format!("{GMEOW_NS}hasState"))
            .map(|(_, coverage)| coverage)
            .expect("primary quad is classified");
        assert!(
            matches!(
                primary,
                QuadCoverage::Covered {
                    object: Gmn1ConstructCategory::IriPrefixMangled,
                    ..
                }
            ),
            "coverage must use the writer's @p sigil, not classify the object as an @mu glyph: {primary:?}"
        );

        let mut tally = ConstructCoverageTally::default();
        tally.absorb(&model, &dictionary);
        assert_eq!(
            tally.count(Gmn1ConstructCategory::IriGlyph),
            0,
            "the tally must not claim an @mu glyph the grouped @p writer did not emit"
        );
        round_trip_check(&model, &dictionary).expect("process record round-trips");
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
            dict.term_to_alias.len() >= 30,
            "expected at least the 30 authored dict-v3 entries, got {}",
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
        assert_eq!(dict.alias_for(&format!("{LANG_NS}Denotation")), Some("den"));
        assert_eq!(dict.alias_for(&format!("{MATH_NS}Division")), Some("div"));
        assert_eq!(dict.alias_for(&format!("{LOGIC_NS}forall")), Some("fa"));
    }

    #[test]
    fn boundary_predicate_uses_the_logic_namespace() {
        // Sanity: the constant matches the real logic: namespace, not a typo.
        assert!(PRED_OCCURRENT_BOUNDARY.starts_with("https://blackcatinformatics.ca/logic/"));
    }

    #[test]
    fn coverage_report_fraction_is_vacuously_full_on_empty_model() {
        let report = CoverageReport {
            covered: 0,
            total: 0,
        };
        assert_eq!(report.fraction(), 1.0, "nothing to cover is vacuously 1.0");
    }

    #[test]
    fn measure_coverage_is_full_over_a_covered_model() {
        let mut b = RdfDatasetBuilder::new();
        let s = b.intern_iri(&format!("{GMEOW_NS}gate1"));
        let p = b.intern_iri(&format!("{GMEOW_NS}hasState"));
        let o = b.intern_iri(&format!("{GMEOW_NS}doorGate1"));
        b.push_quad(s, p, o, None);
        let ds = b.freeze().expect("freeze");
        let model = Gmn0Model::from_dataset(&ds);
        let dict = empty_dict();
        let report = measure_coverage(&model, &dict);
        assert_eq!(report.total, 1);
        assert_eq!(report.covered, 1);
        assert_eq!(report.fraction(), 1.0);
    }

    #[test]
    fn measure_coverage_counts_an_uncovered_quad_without_hard_failing() {
        let mut b = RdfDatasetBuilder::new();
        let s1 = b.intern_iri(&format!("{GMEOW_NS}gate1"));
        let p1 = b.intern_iri(&format!("{GMEOW_NS}hasState"));
        let o1 = b.intern_iri(&format!("{GMEOW_NS}doorGate1"));
        b.push_quad(s1, p1, o1, None);
        // A second, deliberately uncovered quad: an IRI under no registered namespace.
        let s2 = b.intern_iri("https://not-registered.example/subject");
        let p2 = b.intern_iri(&format!("{GMEOW_NS}hasState"));
        let o2 = b.intern_iri(&format!("{GMEOW_NS}doorGate1"));
        b.push_quad(s2, p2, o2, None);
        let ds = b.freeze().expect("freeze");
        let model = Gmn0Model::from_dataset(&ds);
        let dict = empty_dict();
        let report = measure_coverage(&model, &dict);
        assert_eq!(report.total, 2, "both quads are measured");
        assert_eq!(
            report.covered, 1,
            "only the registered-namespace quad is covered"
        );
        assert!((report.fraction() - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_header_tolerates_trailing_cr_and_incidental_whitespace() {
        let dict = empty_dict();
        // GMN-1 is an LLM-first interchange dialect: the reader must parse text an
        // external author/tool emits, not only gmn1_write's own canonical whitespace.
        // A trailing '\r' can survive std::str::lines()'s built-in CRLF handling (e.g.
        // a doubled '\r\r\n', or a lone trailing '\r' with no following '\n') — before
        // the fix, parse_header matched the un-trimmed line exactly against
        // "@gmn{...}" and hard-failed as Malformed on any such residue.
        assert!(parse_header("@gmn{v: 1, aliases: dict-v3, glyphs: 2}\r", &dict).is_ok());
        assert!(parse_header("  @gmn{v: 1, aliases: dict-v3, glyphs: 2}  ", &dict).is_ok());
        assert!(parse_header("@gmn{v: 1, aliases: dict-v3, glyphs: 2}", &dict).is_ok());
    }

    #[test]
    fn gmn1_read_parses_externally_authored_whitespace_variants() {
        // Hand-written text simulating an externally-authored GMN-1 document —
        // deliberately NOT produced via this crate's own `gmn1_write` (which always
        // emits canonical single-space, LF-only whitespace) — so this proves the
        // reader tolerates real external variance, not merely its own writer's output.
        let dict = empty_dict();
        let text = concat!(
            // A doubled '\r' before the line feed: std::str::lines() strips exactly
            // one trailing '\r' per line, so one '\r' still reaches parse_header
            // un-trimmed without the fix.
            "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\r\r\n",
            // A stray space between the sigil and its opening brace.
            "@c {s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1}\n",
            "@claims[s p o]\n",
            // A tab-delimited tabular row (mixed with normal spacing elsewhere).
            "gmeow__gate2\tgmeow__hasState\tgmeow__doorGate2\n",
        );
        let doc = Gmn1Document {
            text: text.to_owned(),
            refs: BTreeMap::new(),
        };
        let model = gmn1_read(&doc, &dict)
            .expect("CRLF header, spaced sigil, and tab-delimited row must all parse");
        assert_eq!(
            model.quads.len(),
            2,
            "one @c record plus one tabular row decode to two quads"
        );
    }

    #[test]
    fn gmn1_read_unknown_sigil_hard_fails_as_non_decodable_grammar_after_trim() {
        // Negative control: trimming whitespace must never broaden the grammar. An unknown
        // sigil is not a dictionary-coverage gap (that is a grammar-VALID token the alias
        // table does not mint) — it is a structural defect the parse table has no production
        // for, so it is `lang:GmnNonDecodableGrammar`, never silently parsed.
        let dict = empty_dict();
        let text = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@x{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1}\n";
        let doc = Gmn1Document {
            text: text.to_owned(),
            refs: BTreeMap::new(),
        };
        let err = gmn1_read(&doc, &dict).expect_err("an unknown sigil must still hard-fail");
        assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
        assert!(matches!(err, Gmn1Error::NonDecodableGrammar { .. }));
    }

    #[test]
    fn failure_class_returns_the_exact_lang_iri_for_every_variant() {
        assert_eq!(
            Gmn1Error::Uncovered(UncoveredTerm("x".to_owned())).failure_class(),
            "https://blackcatinformatics.ca/lang/GmnUncoveredTerm"
        );
        assert_eq!(
            Gmn1Error::NonCanonicalOrder {
                detail: "x".to_owned()
            }
            .failure_class(),
            "https://blackcatinformatics.ca/lang/GmnNonCanonicalOrder"
        );
        assert_eq!(
            Gmn1Error::MalformedNumber {
                token: "0.951".to_owned()
            }
            .failure_class(),
            "https://blackcatinformatics.ca/lang/GmnMalformedNumber"
        );
        assert_eq!(
            Gmn1Error::UndeclaredDialectVersion {
                detail: "x".to_owned()
            }
            .failure_class(),
            "https://blackcatinformatics.ca/lang/GmnUndeclaredDialectVersion"
        );
        assert_eq!(
            Gmn1Error::NonDecodableGrammar {
                detail: "x".to_owned()
            }
            .failure_class(),
            "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar"
        );
        // A per-claim mismatch REUSES the whole-model round-trip class, never a new one.
        assert_eq!(
            Gmn1Error::PerClaimMismatch {
                subject: "<https://blackcatinformatics.ca/gmeow/gate2>".to_owned()
            }
            .failure_class(),
            "https://blackcatinformatics.ca/lang/GmnNonDecodableGrammar"
        );
    }

    #[test]
    fn number_shape_predicate_separates_numbers_from_identifiers() {
        // Number-shaped tokens (a malformed one is GmnMalformedNumber):
        assert!(is_number_shaped("9.5e-1"));
        assert!(is_number_shaped("0.951"));
        assert!(is_number_shaped("0.95"));
        assert!(is_number_shaped("50"));
        assert!(is_number_shaped("-1.00"));
        assert!(is_number_shaped(".5"));
        // Identifier-shaped tokens containing `e`/`E` must NOT be number-shaped (the naive
        // "contains e" test is wrong) — they stay dictionary-coverage (Uncovered):
        assert!(!is_number_shaped("open"));
        assert!(!is_number_shaped("state"));
        assert!(!is_number_shaped("sensorCrew"));
        assert!(!is_number_shaped("gate1"));
        assert!(!is_number_shaped("e12"));

        // The malformed-number predicate: exactly the non-canonical number lexemes.
        assert!(is_malformed_number("9.5e-1"));
        assert!(is_malformed_number("0.951"));
        assert!(!is_malformed_number("0.95"));
        assert!(!is_malformed_number("50"));
        assert!(!is_malformed_number("-1.00"));
        assert!(!is_malformed_number("open"));
    }

    /// A value token drives the number-form classification end-to-end through the reader:
    /// `9.5e-1` and `0.951` become `GmnMalformedNumber`, `0.95` decodes, and an identifier
    /// not in the dictionary stays `GmnUncoveredTerm`.
    #[test]
    fn reader_classifies_malformed_numbers_and_keeps_identifiers_uncovered() {
        let dict = empty_dict();
        let read = |q: &str| {
            let text = format!(
                "@gmn{{v: 1, aliases: dict-v3, glyphs: 2}}\n@c{{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1, q: {q}}}\n"
            );
            gmn1_read(
                &Gmn1Document {
                    text,
                    refs: BTreeMap::new(),
                },
                &dict,
            )
        };
        assert_eq!(
            read("9.5e-1")
                .expect_err("scientific notation")
                .failure_class(),
            Gmn1Error::CLASS_MALFORMED_NUMBER
        );
        assert_eq!(
            read("0.951")
                .expect_err("three-digit fraction")
                .failure_class(),
            Gmn1Error::CLASS_MALFORMED_NUMBER
        );
        read("0.95").expect("a canonical two-digit confidence decodes");
        read("50").expect("a canonical integer decodes");

        // A grammar-valid identifier in an object slot the empty dictionary does not cover
        // stays Uncovered (dictionary-coverage), NOT MalformedNumber.
        let text = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: unregistered, p: unregistered, o: unregistered}\n";
        let err = gmn1_read(
            &Gmn1Document {
                text: text.to_owned(),
                refs: BTreeMap::new(),
            },
            &dict,
        )
        .expect_err("an uncovered identifier must still hard-fail as Uncovered");
        assert_eq!(err.failure_class(), Gmn1Error::CLASS_UNCOVERED_TERM);
    }

    /// The detection-precedence linearization: an input that violates ≥2 classes resolves to
    /// exactly the higher-precedence one. Here a document with NO `@gmn` header (header-
    /// presence) AND a three-fractional-digit confidence (number-form) resolves to
    /// `GmnMalformedNumber`, because number-form precedes header-presence — number
    /// well-formedness is lexical, decidable without the dialect version.
    #[test]
    fn detection_precedence_number_form_wins_over_missing_header() {
        let dict = empty_dict();
        let text = concat!(
            "@c{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1, q: 9.5e-1}\n",
            "@c{s: gmeow__gate2, p: gmeow__hasState, o: gmeow__doorGate2, q: 0.951}\n",
        );
        let err = gmn1_read(
            &Gmn1Document {
                text: text.to_owned(),
                refs: BTreeMap::new(),
            },
            &dict,
        )
        .expect_err("both no-header and a malformed number are violated");
        assert_eq!(
            err.failure_class(),
            Gmn1Error::CLASS_MALFORMED_NUMBER,
            "number-form (pass 2) must win over header-presence (pass 4)"
        );

        // Control: the SAME headerless document with only canonical numbers resolves to the
        // lower-precedence header-presence class, proving the precedence is real (not that
        // MalformedNumber always wins).
        let ok_numbers = "@c{s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1, q: 0.95}\n";
        let err = gmn1_read(
            &Gmn1Document {
                text: ok_numbers.to_owned(),
                refs: BTreeMap::new(),
            },
            &dict,
        )
        .expect_err("a headerless document still fails");
        assert_eq!(
            err.failure_class(),
            Gmn1Error::CLASS_UNDECLARED_DIALECT_VERSION
        );
    }

    /// Grammar (pass 1) dominates key-order (pass 3): a record with BOTH a non-canonical key
    /// order AND a duplicate key resolves to `GmnNonDecodableGrammar`; a clean-grammar
    /// misordered record resolves to `GmnNonCanonicalOrder`.
    #[test]
    fn detection_precedence_grammar_wins_over_key_order() {
        let dict = empty_dict();
        // Non-canonical order (q before s) is the sole defect → NonCanonicalOrder.
        let misordered = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{q: 0.95, s: gmeow__gate1, p: gmeow__hasState, o: gmeow__doorGate1}\n";
        let err = gmn1_read(
            &Gmn1Document {
                text: misordered.to_owned(),
                refs: BTreeMap::new(),
            },
            &dict,
        )
        .expect_err("q before s is non-canonical");
        assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_CANONICAL_ORDER);

        // A duplicate key is a grammar defect that dominates the misorder.
        let duplicate = "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n@c{s: gmeow__gate1, s: gmeow__gate2, p: gmeow__hasState, o: gmeow__doorGate1}\n";
        let err = gmn1_read(
            &Gmn1Document {
                text: duplicate.to_owned(),
                refs: BTreeMap::new(),
            },
            &dict,
        )
        .expect_err("a duplicate key is non-decodable grammar");
        assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
    }

    /// A tabular `@claims[...]` schema that repeats a column (e.g. `o` twice) must
    /// hard-fail as `GmnNonDecodableGrammar`, mirroring `lex_sigil_record`'s duplicate-key
    /// guard. Without this guard, `lex_tabular_row` zips the repeated column against two
    /// DIFFERENT row values, and pass-5 assembly's `.collect()` into a `BTreeMap` silently
    /// keeps only the last one — a quad is dropped with no error.
    #[test]
    fn tabular_schema_with_duplicate_column_is_non_decodable_grammar() {
        let dict = empty_dict();
        let text = concat!(
            "@gmn{v: 1, aliases: dict-v3, glyphs: 2}\n",
            "@claims[s p o o]\n",
            "gmeow__gate1 gmeow__hasState gmeow__doorGate1 gmeow__doorGate2\n",
        );
        let err = gmn1_read(
            &Gmn1Document {
                text: text.to_owned(),
                refs: BTreeMap::new(),
            },
            &dict,
        )
        .expect_err("a duplicate tabular column must not silently drop a quad");
        assert_eq!(err.failure_class(), Gmn1Error::CLASS_NON_DECODABLE_GRAMMAR);
        assert!(matches!(err, Gmn1Error::NonDecodableGrammar { .. }));
    }
}
