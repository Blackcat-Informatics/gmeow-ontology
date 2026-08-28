// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Language-tag policy core: the Rust authority for the ``x-gmeow-*`` private-use
//! tag discipline and the ``gmeow:Language`` → BCP-47 mapping.
//!
//! Principle 4 (one canonical source) + Principle 9 (co-equal, non-privileged
//! facets): canonical authored literals carry internal ``x-gmeow-*`` tags; public
//! projections emit BCP-47.  All policy logic lives here; the Python
//! ``language_tags`` module routes through these functions.

use std::collections::{BTreeSet, HashMap, HashSet};

use purrdf::{TermRef, parse_dataset};

/// The GMEOW namespace prefix for term IRIs.
const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// The `lang:` grounding-layer namespace prefix.
const LANG_NAMESPACE: &str = "https://blackcatinformatics.ca/lang/";
/// RDF type predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Look up `lang` in `tag_map`, trying the raw key first and then the
/// lowercased form. This handles mixed/upper-case internal tags (e.g.
/// ``X-GMEOW-FRENCH``) against a map whose keys are always lowercase.
fn internal_tag_mapping<'a>(
    lang: &str,
    tag_map: &'a HashMap<String, String>,
) -> Option<&'a String> {
    tag_map
        .get(lang)
        .or_else(|| tag_map.get(&lang.to_lowercase()))
}

/// Return whether `lang` is a GMEOW internal private-use tag (``x-gmeow-*``).
///
/// The pattern is ``^x-gmeow-[a-z0-9\-]+$``, matched case-insensitively.
pub fn is_internal_tag(lang: &str) -> bool {
    let lower = lang.to_lowercase();
    if !lower.starts_with("x-gmeow-") {
        return false;
    }
    let suffix = &lower["x-gmeow-".len()..];
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// Append the language-fallback presentation marker to `text`.
///
/// When `fallback` is true (the value was resolved via the carrier language
/// rather than a requested one), returns ``"{text} [fallback: {fallback_lang}]"``;
/// otherwise returns `text` unchanged. This is the presentation-side companion to
/// the selection logic that produces the `is_fallback` flag.
pub fn marked(text: &str, fallback: bool, fallback_lang: &str) -> String {
    if fallback {
        format!("{text} [fallback: {fallback_lang}]")
    } else {
        text.to_owned()
    }
}

/// The shared language-preference sort key.
///
/// Returns ``(0, lang.lower())`` for ``x-gmeow-english`` and
/// ``(1, lang.lower())`` for everything else, so the carrier language wins
/// deterministically in multilingual sorts.
pub fn rank_language(lang: &str) -> (u8, String) {
    let lower = lang.to_lowercase();
    let rank = if lower == "x-gmeow-english" { 0 } else { 1 };
    (rank, lower)
}

/// Parse `rdf_bytes` in `format` and build a mapping from GMEOW internal
/// language tag to BCP-47 tag.
///
/// Scans the framework carrier sign systems / varieties (``gmeow:Language`` and
/// ``lang:LanguageVariety``) for the internal ``lang:carrierTag`` and the
/// generated ``gmeow:bcp47Tag`` property values. Since the lang: graft, the
/// internal private-use tag rides ``lang:carrierTag`` on the three carriers
/// (``x-gmeow-english``/``french``/``mandarin``), never a per-language attribute;
/// the natural/formal/programming distinction moved to ``lang:signSystemKind``
/// individuals, so there is no longer a ``FormalLanguage``/``ProgrammingLanguage``
/// class to scan.
///
/// Each property must have exactly one distinct lexical value per individual:
/// - Missing either property → individual is silently skipped.
/// - More than one distinct value for either property → returns `Err`.
///
/// Returns ``{internal_tag: bcp47_tag}``.
pub fn load_tag_map(
    rdf_bytes: &[u8],
    format: &str,
) -> gmeow_errors::Result<HashMap<String, String>> {
    let media_type = media_type_for(format)?;

    // Parse straight into the purrdf IR via the native codecs: no oxigraph `io`
    // parser, and lenient private-use language tags by construction.
    let dataset = parse_dataset(rdf_bytes, media_type, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("RDF parse error: {e}"),
        })
    })?;

    load_tag_map_from_dataset(&dataset)
}

/// Build the tag map from an already-frozen `RdfDataset`, scanning the carrier
/// sign systems (``gmeow:Language``) and carrier varieties
/// (``lang:LanguageVariety``) — the only individuals that carry a
/// ``lang:carrierTag``.
///
/// This is the store-native counterpart to [`load_tag_map`]. Consumers that
/// already hold a parsed dataset should use it directly instead of serializing
/// the graph to N-Triples only to parse those bytes back into the same IR.
pub fn load_tag_map_from_dataset(
    dataset: &purrdf::RdfDataset,
) -> gmeow_errors::Result<HashMap<String, String>> {
    let lang_class = format!("{NAMESPACE}Language");
    let variety_class = format!("{LANG_NAMESPACE}LanguageVariety");
    build_tag_map_for(dataset, &[&lang_class, &variety_class])
}

/// Build the internal→BCP-47 tag map, restricting the scan to the carrier
/// `classes` (their IRI strings). Only the three framework carriers carry a
/// ``lang:carrierTag`` at all, so the scan is naturally confined to them.
///
/// Each individual typed as one of `classes` must carry exactly one distinct
/// ``lang:carrierTag`` and one distinct ``gmeow:bcp47Tag`` lexical value:
/// - Missing either → individual is silently skipped (authoring SHACL enforces
///   completeness; we never fabricate).
/// - More than one distinct value → returns `Err`.
/// - Two individuals mapping one internal tag to DIFFERENT BCP-47 tags → `Err`.
fn build_tag_map_for(
    dataset: &purrdf::RdfDataset,
    classes: &[&str],
) -> gmeow_errors::Result<HashMap<String, String>> {
    let tag_prop = format!("{LANG_NAMESPACE}carrierTag");
    let bcp_prop = format!("{NAMESPACE}bcp47Tag");

    // Collect the string-form subjects that are typed as one of the language
    // classes. We use string matching via quad_refs() since TermId is crate-private.
    let mut lang_subjects: HashSet<String> = HashSet::new();
    for qr in dataset.quad_refs() {
        if let (TermRef::Iri(p), TermRef::Iri(o)) = (qr.p, qr.o)
            && p == RDF_TYPE
            && classes.contains(&o)
            && let TermRef::Iri(s) = qr.s
        {
            lang_subjects.insert(s.to_owned());
        }
    }

    // For each subject IRI, collect distinct literal values for both properties.
    // Index: subject_iri → (prop_iri → set_of_lexical_values)
    let mut props: HashMap<String, HashMap<String, BTreeSet<String>>> = HashMap::new();
    for qr in dataset.quad_refs() {
        if let TermRef::Iri(s) = qr.s {
            if !lang_subjects.contains(s) {
                continue;
            }
            if let TermRef::Iri(p) = qr.p
                && (p == tag_prop || p == bcp_prop)
                && let TermRef::Literal { lexical, .. } = qr.o
            {
                props
                    .entry(s.to_owned())
                    .or_default()
                    .entry(p.to_owned())
                    .or_default()
                    .insert(lexical.to_owned());
            }
        }
    }

    let mut tag_map = HashMap::new();
    for subject in &lang_subjects {
        let subject_props = props.get(subject);
        let int_vals = subject_props
            .and_then(|m| m.get(&tag_prop))
            .cloned()
            .unwrap_or_default();
        let bcp_vals = subject_props
            .and_then(|m| m.get(&bcp_prop))
            .cloned()
            .unwrap_or_default();

        // Missing either → skip (SHACL enforces completeness at authoring time).
        if int_vals.is_empty() || bcp_vals.is_empty() {
            continue;
        }
        if int_vals.len() > 1 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::LanguageTag {
                detail: format!(
                    "individual <{subject}> has ambiguous carrierTag values: {int_vals:?}; \
                     tag-map projection requires a single canonical value"
                ),
            }));
        }
        if bcp_vals.len() > 1 {
            return Err(gmeow_errors::Diag::of_kind(crate::error::LanguageTag {
                detail: format!(
                    "individual <{subject}> has ambiguous bcp47Tag values: {bcp_vals:?}; \
                     tag-map projection requires a single canonical value"
                ),
            }));
        }
        let int_val = int_vals.into_iter().next().unwrap();
        let bcp_val = bcp_vals.into_iter().next().unwrap();
        // Two carrier individuals sharing one internal `carrierTag` but mapping it
        // to DIFFERENT `bcp47Tag`s is a nondeterministic conflict; per the
        // no-optionality/hard-fail doctrine, reject it rather than silently letting
        // the last writer win. (A repeated tag→SAME bcp47Tag is a harmless duplicate.)
        if let Some(existing) = tag_map.get(&int_val)
            && existing != &bcp_val
        {
            return Err(gmeow_errors::Diag::of_kind(crate::error::LanguageTag {
                detail: format!(
                    "conflicting bcp47Tag for internal carrierTag {int_val:?}: \
                     {existing:?} vs {bcp_val:?}; the tag-map projection requires a \
                     single canonical bcp47Tag per internal tag"
                ),
            }));
        }
        tag_map.insert(int_val, bcp_val);
    }

    Ok(tag_map)
}

/// Build the BCP-47 → internal mapping — the inverse of [`load_tag_map`].
///
/// Built from the carrier sign systems / varieties that carry a
/// ``lang:carrierTag`` (only the three natural-language carriers do). A BCP-47
/// tag that several carriers still share is dropped rather than guessed (the
/// no-fabrication discipline). Keys are lowercased.
pub fn load_inverse_tag_map(
    rdf_bytes: &[u8],
    format: &str,
) -> gmeow_errors::Result<HashMap<String, String>> {
    let media_type = media_type_for(format)?;
    let dataset = parse_dataset(rdf_bytes, media_type, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("RDF parse error: {e}"),
        })
    })?;

    load_inverse_tag_map_from_dataset(&dataset)
}

/// Build the BCP-47 → internal mapping from an already-frozen dataset.
///
/// This is the parsed-dataset counterpart to [`load_inverse_tag_map`], for resident
/// consumers that already authenticated and restored a bundle corpus.
pub fn load_inverse_tag_map_from_dataset(
    dataset: &purrdf::RdfDataset,
) -> gmeow_errors::Result<HashMap<String, String>> {
    let natural = load_tag_map_from_dataset(dataset)?;

    // Group internal tags by their lowercased BCP-47 value, then keep only the
    // BCP-47 keys that map to EXACTLY ONE internal tag (drop ambiguous).
    let mut by_bcp: HashMap<String, BTreeSet<String>> = HashMap::new();
    for (internal, bcp) in natural {
        by_bcp
            .entry(bcp.to_lowercase())
            .or_default()
            .insert(internal);
    }
    let mut inverse = HashMap::new();
    for (bcp, ints) in by_bcp {
        if ints.len() == 1 {
            inverse.insert(bcp, ints.into_iter().next().unwrap());
        }
    }
    Ok(inverse)
}

/// A resolved, validated user language request.
///
/// Holds the requested BCP-47 tags in precedence order and the set of tags known
/// to the current snapshot. All CLI/env resolution funnels through this so the
/// fold and graph paths agree by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LangSelector {
    /// The requested BCP-47 tags in precedence order (deduplicated, lowercased).
    pub requested: Vec<String>,
    /// The set of public BCP-47 tags known to the current snapshot (lowercased).
    pub available: BTreeSet<String>,
}

/// Raised when a requested language tag is not available in the tag map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownLanguage {
    /// The offending raw token, exactly as requested.
    pub tag: String,
    /// The available BCP-47 tags, ``en`` first then lexicographic.
    pub available: Vec<String>,
}

/// Sort a set of available BCP-47 tags into the user-facing order: the ``en``
/// carrier language first, then everything else lexicographically.
fn available_en_first(available: &BTreeSet<String>) -> Vec<String> {
    let mut out: Vec<String> = available.iter().cloned().collect();
    out.sort_by(|a, b| (a != "en", a).cmp(&(b != "en", b)));
    out
}

/// Resolve CLI/env language input into a [`LangSelector`].
///
/// * ``None``/empty → default ``["en"]``.
/// * Internal tags (``x-gmeow-english``) are normalized to their BCP-47 form via
///   `tag_map`; an internal tag with no mapping → `Err`.
/// * Public BCP-47 tags are lowercased.
/// * Comma-separated lists preserve order and are deduplicated.
/// * A normalized tag outside the available set → `Err`.
///
/// `available` defaults to the lowercased values of `tag_map` (the full mapped
/// catalog) when `None`.
pub fn resolve_lang_input(
    raw: Option<&str>,
    tag_map: &HashMap<String, String>,
    available: Option<&[String]>,
) -> Result<LangSelector, UnknownLanguage> {
    let available_set: BTreeSet<String> = match available {
        Some(list) => list.iter().map(|a| a.to_lowercase()).collect(),
        None => tag_map.values().map(|v| v.to_lowercase()).collect(),
    };

    let raw_trimmed = raw.map(str::trim).unwrap_or("");
    if raw_trimmed.is_empty() {
        return Ok(LangSelector {
            requested: vec!["en".to_owned()],
            available: available_set,
        });
    }

    let mut resolved: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for token in raw_trimmed.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }
        let normalized = if is_internal_tag(token) {
            match internal_tag_mapping(token, tag_map) {
                Some(bcp) => bcp.to_lowercase(),
                None => {
                    return Err(UnknownLanguage {
                        tag: token.to_owned(),
                        available: available_en_first(&available_set),
                    });
                }
            }
        } else {
            token.to_lowercase()
        };
        if seen.contains(&normalized) {
            continue;
        }
        if !available_set.contains(&normalized) {
            return Err(UnknownLanguage {
                tag: token.to_owned(),
                available: available_en_first(&available_set),
            });
        }
        seen.insert(normalized.clone());
        resolved.push(normalized);
    }

    if resolved.is_empty() {
        resolved.push("en".to_owned());
    }
    Ok(LangSelector {
        requested: resolved,
        available: available_set,
    })
}

/// A language-tagged (or untagged) literal, as the descriptor-based selection API
/// sees it. The selection functions never invent lexical content; they only choose
/// among the descriptors handed in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LitDesc {
    /// The literal's lexical form.
    pub lexical: String,
    /// The literal's language tag, if any (internal ``x-gmeow-*`` or public).
    pub language: Option<String>,
}

/// One selection verdict pointing back into the input slice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Selection {
    /// Index of the chosen literal in the input slice.
    pub index: usize,
    /// The public BCP-47 tag to retag the chosen literal to, when the original
    /// carried an internal tag WITH a map entry. `None` → return the literal
    /// unchanged.
    pub retag_to: Option<String>,
    /// Whether this selection is a fallback (no requested language matched).
    pub is_fallback: bool,
}

/// The public BCP-47 bucket key for a literal: a mapped internal tag's BCP-47,
/// else the lowercased public tag, else `""` for an untagged literal.
fn bucket_key(lang: &Option<String>, tag_map: &HashMap<String, String>) -> String {
    match lang {
        None => String::new(),
        Some(lang) => if is_internal_tag(lang) {
            internal_tag_mapping(lang, tag_map)
                .cloned()
                .unwrap_or_else(|| lang.clone())
        } else {
            lang.clone()
        }
        .to_lowercase(),
    }
}

/// The `retag_to` value for a chosen literal: `Some(bcp)` when it carried an
/// internal tag WITH a map entry (needs retagging to public), else `None`.
fn retag_to(lang: &Option<String>, tag_map: &HashMap<String, String>) -> Option<String> {
    match lang {
        Some(lang) if is_internal_tag(lang) => internal_tag_mapping(lang, tag_map).cloned(),
        _ => None,
    }
}

/// Build the public-tag buckets `{bcp: [(orig_index, orig_lang)]}`, each bucket
/// sorted by `(rank_language(orig_lang), lexical)` so the carrier language wins
/// and ties resolve deterministically.
fn bucket_literals<'a>(
    literals: &'a [LitDesc],
    tag_map: &HashMap<String, String>,
) -> HashMap<String, Vec<(usize, &'a LitDesc)>> {
    let mut by_bcp: HashMap<String, Vec<(usize, &'a LitDesc)>> = HashMap::new();
    for (index, lit) in literals.iter().enumerate() {
        let bucket = bucket_key(&lit.language, tag_map);
        by_bcp.entry(bucket).or_default().push((index, lit));
    }
    for bucket in by_bcp.values_mut() {
        bucket.sort_by(|(_, a), (_, b)| {
            let a_lang = a.language.clone().unwrap_or_default();
            let b_lang = b.language.clone().unwrap_or_default();
            (rank_language(&a_lang), &a.lexical).cmp(&(rank_language(&b_lang), &b.lexical))
        });
    }
    by_bcp
}

/// Build a [`Selection`] for the chosen literal at `index`.
fn selection_for(
    index: usize,
    literals: &[LitDesc],
    tag_map: &HashMap<String, String>,
    is_fallback: bool,
) -> Selection {
    Selection {
        index,
        retag_to: retag_to(&literals[index].language, tag_map),
        is_fallback,
    }
}

/// The lowest-`rank_language` non-empty tagged bucket key, by `(rank, key)`.
fn lowest_ranked_tagged(by_bcp: &HashMap<String, Vec<(usize, &LitDesc)>>) -> Option<String> {
    by_bcp
        .keys()
        .filter(|k| !k.is_empty())
        .min_by(|a, b| rank_language(a).cmp(&rank_language(b)))
        .cloned()
}

/// Select the single best literal for the `requested` languages.
///
/// Requested languages are tried in order; if none match, the fallback chain is:
/// the ``en`` bucket → the lowest-`rank_language` non-empty tagged bucket → the
/// ``""`` untagged bucket → `None`. The returned [`Selection`] carries the
/// retag-to tag for the chosen literal and whether it was a fallback.
pub fn select_literal(
    literals: &[LitDesc],
    requested: &[String],
    tag_map: &HashMap<String, String>,
) -> Option<Selection> {
    if literals.is_empty() {
        return None;
    }
    let by_bcp = bucket_literals(literals, tag_map);

    for req in requested {
        if let Some(bucket) = by_bcp.get(req)
            && let Some(&(index, _)) = bucket.first()
        {
            return Some(selection_for(index, literals, tag_map, false));
        }
    }

    if let Some(bucket) = by_bcp.get("en")
        && let Some(&(index, _)) = bucket.first()
    {
        return Some(selection_for(index, literals, tag_map, true));
    }
    if let Some(best_key) = lowest_ranked_tagged(&by_bcp)
        && let Some(&(index, _)) = by_bcp[&best_key].first()
    {
        return Some(selection_for(index, literals, tag_map, true));
    }
    if let Some(bucket) = by_bcp.get("")
        && let Some(&(index, _)) = bucket.first()
    {
        return Some(selection_for(index, literals, tag_map, true));
    }
    None
}

/// Return every literal of the matched requested buckets, or the single fallback.
///
/// When at least one requested language is present, returns ALL its literals (in
/// each matched requested bucket, requested-tag order) with `is_fallback=false`.
/// Otherwise returns a single-element fallback list following the same chain as
/// [`select_literal`] (``en`` → lowest-ranked tagged → ``""`` untagged → empty).
pub fn filter_literals(
    literals: &[LitDesc],
    requested: &[String],
    tag_map: &HashMap<String, String>,
) -> Vec<Selection> {
    if literals.is_empty() {
        return Vec::new();
    }
    let by_bcp = bucket_literals(literals, tag_map);

    let mut results: Vec<Selection> = Vec::new();
    for req in requested {
        if let Some(bucket) = by_bcp.get(req) {
            for &(index, _) in bucket {
                results.push(selection_for(index, literals, tag_map, false));
            }
        }
    }
    if !results.is_empty() {
        return results;
    }

    if let Some(bucket) = by_bcp.get("en")
        && let Some(&(index, _)) = bucket.first()
    {
        return vec![selection_for(index, literals, tag_map, true)];
    }
    if let Some(best_key) = lowest_ranked_tagged(&by_bcp)
        && let Some(&(index, _)) = by_bcp[&best_key].first()
    {
        return vec![selection_for(index, literals, tag_map, true)];
    }
    if let Some(bucket) = by_bcp.get("")
        && let Some(&(index, _)) = bucket.first()
    {
        return vec![selection_for(index, literals, tag_map, true)];
    }
    Vec::new()
}

// ── graph-level public selection ─────────────────────────────────────────────────

/// Select the canonical public-facing literal for `subject`/`predicate`.
///
/// The projection-boundary companion to [`select_literal`]: rather than honouring
/// a user language request, it picks the single canonical public value for a term
/// regardless of request. This mirrors the Python `public_literal`, so the
/// rdflib path and the fold view agree by construction. Preference order:
///
/// 1. An internal-tagged literal (`x-gmeow-*`) that has a BCP-47 mapping in
///    `tag_map`, taken in [`rank_language`] order (the `x-gmeow-english` carrier
///    wins) with ties broken by `(language, lexical)`, retagged to its public
///    BCP-47 tag.
/// 2. Otherwise the deterministic first literal by `(language, lexical)`, returned
///    with its original tag (external or untagged) unchanged.
/// 3. No literal object on the pair → `None`.
///
/// Only literal objects on `(subject, predicate)` in the default graph are
/// considered; IRI and blank-node objects are ignored. `subject` and `predicate`
/// are IRI strings.
pub fn public_literal(
    dataset: &purrdf::RdfDataset,
    subject: &str,
    predicate: &str,
    tag_map: &HashMap<String, String>,
) -> Option<LitDesc> {
    // Collect every literal object on (subject, predicate) in the default graph.
    let mut candidates: Vec<LitDesc> = Vec::new();
    for qr in dataset.quad_refs() {
        let (TermRef::Iri(s), TermRef::Iri(p)) = (qr.s, qr.p) else {
            continue;
        };
        if s != subject || p != predicate {
            continue;
        }
        if let TermRef::Literal {
            lexical, language, ..
        } = qr.o
        {
            candidates.push(LitDesc {
                lexical: lexical.to_owned(),
                language: language.map(str::to_owned),
            });
        }
    }
    if candidates.is_empty() {
        return None;
    }

    // Deterministic base order: (language, lexical). rdflib iteration order is
    // process-unstable, so we impose a total order before any ranked selection.
    candidates.sort_by(|a, b| {
        (a.language.clone().unwrap_or_default(), &a.lexical)
            .cmp(&(b.language.clone().unwrap_or_default(), &b.lexical))
    });

    // Prefer an internal-tagged, mapped literal in rank_language order (carrier
    // first). The stable sort preserves the (language, lexical) order within a
    // rank, so ties resolve identically to the fold path.
    let mut ranked: Vec<&LitDesc> = candidates.iter().collect();
    ranked.sort_by(|a, b| {
        rank_language(a.language.as_deref().unwrap_or_default())
            .cmp(&rank_language(b.language.as_deref().unwrap_or_default()))
    });
    for lit in ranked {
        if let Some(lang) = &lit.language
            && is_internal_tag(lang)
            && let Some(bcp) = internal_tag_mapping(lang, tag_map)
        {
            return Some(LitDesc {
                lexical: lit.lexical.clone(),
                language: Some(bcp.clone()),
            });
        }
    }

    // Fallback: the deterministic first candidate, tag preserved.
    candidates.into_iter().next()
}

/// Return the string value of the public-facing literal for `subject`/`predicate`,
/// or the empty string when there is none. Thin wrapper over [`public_literal`].
pub fn public_text(
    dataset: &purrdf::RdfDataset,
    subject: &str,
    predicate: &str,
    tag_map: &HashMap<String, String>,
) -> String {
    public_literal(dataset, subject, predicate, tag_map)
        .map(|lit| lit.lexical)
        .unwrap_or_default()
}

// ── graph passes ────────────────────────────────────────────────────────────────
//
// The graph passes operate on base triples (the default graph of an N-Triples
// input). RDF 1.2 quoted-triple terms and the statement layer (reifiers,
// annotations) are carried through verbatim; the language passes only touch
// annotation-literal objects, never the inside of a quoted triple.

/// Build the public BCP-47 retag for a literal carrying an internal tag WITH a map
/// entry, returning a swapped owned literal. Returns `None` when no swap applies.
fn retagged_literal(
    lit: &purrdf::RdfLiteral,
    tag_map: &HashMap<String, String>,
) -> Option<purrdf::RdfLiteral> {
    let lang = lit.language.as_deref()?;
    if !is_internal_tag(lang) {
        return None;
    }
    let bcp = internal_tag_mapping(lang, tag_map)?;
    Some(purrdf::RdfLiteral::language_tagged(
        lit.lexical_form.clone(),
        bcp.clone(),
    ))
}

/// Retag every internal-tagged literal in `rdf_bytes` to its public BCP-47 form.
///
/// Every literal whose language is an internal ``x-gmeow-*`` tag WITH a `tag_map`
/// entry is swapped to the mapped public tag (same lexical, ``rdf:langString``);
/// all other quads (IRIs, blank nodes, typed literals, untagged literals) are
/// copied verbatim. Returns N-Triples bytes.
pub fn retag_graph(
    rdf_bytes: &[u8],
    format: &str,
    tag_map: &HashMap<String, String>,
) -> gmeow_errors::Result<Vec<u8>> {
    rewrite_graph(rdf_bytes, format, |lit| retagged_literal(lit, tag_map))
}

/// Retag every public BCP-47 literal in `rdf_bytes` to its canonical
/// ``x-gmeow-*`` form — the inverse of [`retag_graph`].
///
/// Every literal whose language is NON-internal and whose lowercased language is a
/// key in `inverse_map` is swapped to the mapped internal tag (same lexical); all
/// other quads are copied verbatim. Returns N-Triples bytes.
pub fn retag_graph_to_internal(
    rdf_bytes: &[u8],
    format: &str,
    inverse_map: &HashMap<String, String>,
) -> gmeow_errors::Result<Vec<u8>> {
    rewrite_graph(rdf_bytes, format, |lit| {
        let lang = lit.language.as_deref()?;
        if is_internal_tag(lang) {
            return None;
        }
        let internal = inverse_map.get(&lang.to_lowercase())?;
        Some(purrdf::RdfLiteral::language_tagged(
            lit.lexical_form.clone(),
            internal.clone(),
        ))
    })
}

/// Parse `rdf_bytes`, rewrite each literal object through `rewrite` (when it
/// returns `Some`, the object is replaced), copy everything else verbatim, and
/// serialize back to N-Triples. The full RDF 1.2 statement layer (reifiers +
/// annotations) and quoted-triple terms are carried through — reifier statement
/// objects are updated when the underlying quad's literal was rewritten, keeping
/// the reifier's statement in sync with the base triple.
fn rewrite_graph<F>(rdf_bytes: &[u8], format: &str, rewrite: F) -> gmeow_errors::Result<Vec<u8>>
where
    F: Fn(&purrdf::RdfLiteral) -> Option<purrdf::RdfLiteral>,
{
    let media_type = media_type_for(format)?;
    let dataset = parse_dataset(rdf_bytes, media_type, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("RDF parse error: {e}"),
        })
    })?;

    // First pass: build the quad set and record which (subject, predicate, old_lit)
    // triples had their literal rewritten, so we can update matching reifier statements.
    let mut literal_rewrites: HashMap<(String, String, purrdf::RdfLiteral), purrdf::RdfLiteral> =
        HashMap::new();
    let mut builder = purrdf::RdfDatasetBuilder::new();
    for mut quad in dataset.owned_quads() {
        if let purrdf::RdfTerm::Literal(lit) = &quad.object
            && let Some(new_lit) = rewrite(lit)
        {
            let key = (
                quad.subject.to_string(),
                quad.predicate.clone(),
                lit.clone(),
            );
            literal_rewrites.insert(key, new_lit.clone());
            quad.object = purrdf::RdfTerm::Literal(new_lit);
        }
        builder.push_owned_quad(&quad);
    }

    // Second pass: copy reifiers, updating any whose statement object was rewritten.
    for mut reifier in dataset.owned_reifiers() {
        if let purrdf::RdfTerm::Literal(obj_lit) = &reifier.statement.object {
            let key = (
                reifier.statement.subject.to_string(),
                reifier.statement.predicate.clone(),
                obj_lit.clone(),
            );
            if let Some(new_lit) = literal_rewrites.get(&key) {
                reifier.statement.object = purrdf::RdfTerm::Literal(new_lit.clone());
            }
        }
        builder.push_owned_reifier(&reifier);
    }
    // Annotations reference the reifier IRI (not the statement literal), so the
    // reifier identity is unchanged after a retag — copy all annotations verbatim.
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }

    serialize_ntriples(builder)
}

/// Retain only the language-selected literals for `predicates`.
///
/// For every ``(subject, predicate)`` group where `predicate` is in `predicates`
/// and the objects include language-tagged literals, the objects are replaced by
/// the literals selected by [`filter_literals`] (with their public retag applied).
/// If the chosen literal set equals the current set, the group is left UNCHANGED
/// (set-equality skip). All non-matching quads are copied verbatim. Returns
/// N-Triples bytes.
pub fn filter_graph(
    rdf_bytes: &[u8],
    format: &str,
    tag_map: &HashMap<String, String>,
    requested: &[String],
    predicates: &[String],
) -> gmeow_errors::Result<Vec<u8>> {
    let media_type = media_type_for(format)?;
    let dataset = parse_dataset(rdf_bytes, media_type, None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: format!("RDF parse error: {e}"),
        })
    })?;

    let target_preds: HashSet<&str> = predicates.iter().map(String::as_str).collect();

    // Group language-tagged literal objects by (subject-key, predicate) for the
    // target predicates. The subject key is the owned term's canonical rendering,
    // which is stable for IRIs and blank nodes alike.
    type GroupKey = (String, String);
    let mut group_lits: HashMap<GroupKey, Vec<purrdf::RdfLiteral>> = HashMap::new();
    for quad in dataset.owned_quads() {
        if !target_preds.contains(quad.predicate.as_str()) {
            continue;
        }
        if let purrdf::RdfTerm::Literal(lit) = &quad.object
            && lit.language.is_some()
        {
            let key = (quad.subject.to_string(), quad.predicate.clone());
            group_lits.entry(key).or_default().push(lit.clone());
        }
    }

    // For each group, compute the chosen literal set. If it equals the current
    // set, mark the group as a no-op (skip); else record the replacement set.
    //
    // `stmt_remap` captures the old→new literal mapping for replaced groups, keyed by
    // (subject_str, predicate, old_literal). Value `Some(new_lit)` means the old literal
    // was retained but retagged; `None` means it was dropped entirely. This is used below
    // to keep reifier statements in sync with the filtered quad set.
    let mut group_replacement: HashMap<GroupKey, Vec<purrdf::RdfLiteral>> = HashMap::new();
    let mut group_skip: HashSet<GroupKey> = HashSet::new();
    let mut stmt_remap: HashMap<(String, String, purrdf::RdfLiteral), Option<purrdf::RdfLiteral>> =
        HashMap::new();
    for (key, literals) in &group_lits {
        let descs: Vec<LitDesc> = literals
            .iter()
            .map(|lit| LitDesc {
                lexical: lit.lexical_form.clone(),
                language: lit.language.clone(),
            })
            .collect();
        let selections = filter_literals(&descs, requested, tag_map);
        if selections.is_empty() {
            group_skip.insert(key.clone());
            continue;
        }
        let chosen: Vec<purrdf::RdfLiteral> = selections
            .iter()
            .map(|sel| match &sel.retag_to {
                Some(bcp) => purrdf::RdfLiteral::language_tagged(
                    literals[sel.index].lexical_form.clone(),
                    bcp.clone(),
                ),
                None => literals[sel.index].clone(),
            })
            .collect();

        // Set-equality skip: if the chosen set equals the current set, leave the
        // group untouched (byte-identical output for an already-satisfied group).
        let current: HashSet<&purrdf::RdfLiteral> = literals.iter().collect();
        let chosen_set: HashSet<&purrdf::RdfLiteral> = chosen.iter().collect();
        if current == chosen_set {
            group_skip.insert(key.clone());
        } else {
            // Build stmt_remap entries for this replaced group.
            // Start by marking all old literals as dropped (None).
            for old_lit in literals {
                stmt_remap.insert((key.0.clone(), key.1.clone(), old_lit.clone()), None);
            }
            // Overwrite with Some(new_lit) for each selection that survived.
            for (i, sel) in selections.iter().enumerate() {
                stmt_remap.insert(
                    (key.0.clone(), key.1.clone(), literals[sel.index].clone()),
                    Some(chosen[i].clone()),
                );
            }
            group_replacement.insert(key.clone(), chosen);
        }
    }

    // Rebuild the dataset: drop the original language-tagged literals of replaced
    // groups, emit the chosen set once per group, copy everything else verbatim.
    let mut builder = purrdf::RdfDatasetBuilder::new();
    let mut emitted_groups: HashSet<GroupKey> = HashSet::new();
    for quad in dataset.owned_quads() {
        let is_target = target_preds.contains(quad.predicate.as_str());
        if is_target
            && let purrdf::RdfTerm::Literal(lit) = &quad.object
            && lit.language.is_some()
        {
            let key = (quad.subject.to_string(), quad.predicate.clone());
            if group_skip.contains(&key) {
                // Unchanged group: copy this object verbatim.
                builder.push_owned_quad(&quad);
                continue;
            }
            if let Some(chosen) = group_replacement.get(&key) {
                // Replaced group: emit the chosen set once for the group.
                if emitted_groups.insert(key.clone()) {
                    for new_lit in chosen {
                        let mut new_quad = quad.clone();
                        new_quad.object = purrdf::RdfTerm::Literal(new_lit.clone());
                        builder.push_owned_quad(&new_quad);
                    }
                }
                // Original language-tagged object of a replaced group is dropped.
                continue;
            }
        }
        builder.push_owned_quad(&quad);
    }

    // Handle the RDF 1.2 statement layer. For replaced groups:
    // - If a reifier's statement object was dropped, the reifier itself is dropped.
    // - If a reifier's statement object was retagged, the reifier statement is updated.
    // - Reifiers on unchanged quads (skip groups or non-target quads) pass through.
    // Annotations whose reifier was dropped are also removed.
    let mut dropped_reifier_ids: HashSet<String> = HashSet::new();
    for mut reifier in dataset.owned_reifiers() {
        if let purrdf::RdfTerm::Literal(obj_lit) = &reifier.statement.object {
            let remap_key = (
                reifier.statement.subject.to_string(),
                reifier.statement.predicate.clone(),
                obj_lit.clone(),
            );
            if let Some(remap) = stmt_remap.get(&remap_key) {
                match remap {
                    Some(new_lit) => {
                        // Retagged: update the statement object and keep the reifier.
                        reifier.statement.object = purrdf::RdfTerm::Literal(new_lit.clone());
                        builder.push_owned_reifier(&reifier);
                    }
                    None => {
                        // Dropped: record this reifier's ID so its annotations are pruned.
                        dropped_reifier_ids.insert(reifier.reifier.to_string());
                    }
                }
                continue;
            }
        }
        builder.push_owned_reifier(&reifier);
    }
    for annotation in dataset.owned_annotations() {
        if dropped_reifier_ids.contains(&annotation.reifier.to_string()) {
            continue;
        }
        builder.push_owned_annotation(&annotation);
    }

    serialize_ntriples(builder)
}

/// Freeze `builder` and serialize the result to N-Triples bytes (default graph).
fn serialize_ntriples(builder: purrdf::RdfDatasetBuilder) -> gmeow_errors::Result<Vec<u8>> {
    let dataset = builder.freeze().map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Serialize {
            detail: format!("dataset freeze error: {e}"),
        })
    })?;
    purrdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        purrdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Serialize {
            detail: format!("RDF serialize error: {e}"),
        })
    })
}

// ── helpers ─────────────────────────────────────────────────────────────────────

/// Map a format string (legacy short ids or media types) to a native-codec media
/// type understood by [`parse_dataset`].
fn media_type_for(format: &str) -> gmeow_errors::Result<&'static str> {
    match format.to_ascii_lowercase().as_str() {
        "turtle" | "text/turtle" | "ttl" => Ok("text/turtle"),
        "n-triples" | "ntriples" | "nt" | "application/n-triples" => Ok("application/n-triples"),
        "n-quads" | "nquads" | "nq" | "application/n-quads" => Ok("application/n-quads"),
        "trig" | "application/trig" => Ok("application/trig"),
        _ => Err(gmeow_errors::Diag::of_kind(crate::error::Format {
            detail: format!("unsupported RDF format: {format:?}"),
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_internal_tag_basic() {
        assert!(is_internal_tag("x-gmeow-english"));
        assert!(is_internal_tag("x-gmeow-mandarin"));
        assert!(is_internal_tag("X-GMEOW-FRENCH"));
        assert!(is_internal_tag("x-gmeow-foo-bar"));
        assert!(!is_internal_tag("en"));
        assert!(!is_internal_tag("fr"));
        assert!(!is_internal_tag("x-gmeow-")); // empty suffix
        assert!(!is_internal_tag("xx-gmeow-no")); // wrong prefix
        assert!(!is_internal_tag("x-gmeow")); // no suffix segment
    }

    #[test]
    fn marked_appends_fallback_marker() {
        assert_eq!(marked("Hello", false, "en"), "Hello");
        assert_eq!(marked("Hello", true, "en"), "Hello [fallback: en]");
        assert_eq!(marked("Bonjour", true, "fr"), "Bonjour [fallback: fr]");
    }

    #[test]
    fn rank_language_carrier_wins() {
        let (r_en, _) = rank_language("x-gmeow-english");
        let (r_fr, _) = rank_language("x-gmeow-french");
        let (r_bcp, _) = rank_language("en");
        assert_eq!(r_en, 0);
        assert_eq!(r_fr, 1);
        assert_eq!(r_bcp, 1);
    }

    #[test]
    fn rank_language_case_insensitive() {
        let (r, key) = rank_language("X-GMEOW-ENGLISH");
        assert_eq!(r, 0);
        assert_eq!(key, "x-gmeow-english");
    }

    #[test]
    fn load_tag_map_parses_turtle() {
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    lang:carrierTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
        assert_eq!(map.get("x-gmeow-french"), Some(&"fr".to_owned()));
    }

    #[test]
    fn load_tag_map_ambiguous_err() {
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    lang:carrierTag "x-gmeow-english-alt" ;
    gmeow:bcp47Tag "en" .
"#;
        assert!(load_tag_map(ttl.as_bytes(), "turtle").is_err());
    }

    #[test]
    fn load_tag_map_conflicting_duplicate_err() {
        // Two individuals mapping the SAME internal tag to DIFFERENT bcp47Tags is a
        // nondeterministic conflict and must hard-fail, not silently last-writer-win.
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishAlt a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en-GB" .
"#;
        let err = load_tag_map(ttl.as_bytes(), "turtle").expect_err("conflict must error");
        assert!(err.is::<crate::error::LanguageTag>());
        assert!(
            err.message().contains("conflicting bcp47Tag"),
            "{}",
            err.message()
        );
    }

    #[test]
    fn load_tag_map_duplicate_identical_ok() {
        // The SAME internal tag → SAME bcp47Tag from two individuals is a harmless
        // duplicate, not a conflict.
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishCopy a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("identical duplicate ok");
        assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
    }

    #[test]
    fn load_tag_map_missing_tag_skipped() {
        // An individual with only one of the two required properties is silently
        // skipped (SHACL enforces completeness; we don't fabricate).
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert!(map.is_empty(), "incomplete individual must be skipped");
    }

    #[test]
    fn load_tag_map_formal_and_prog_language() {
        // The former gmeow:FormalLanguage / gmeow:ProgrammingLanguage subclasses are
        // retired: a formal or programming language is now a gmeow:Language
        // distinguished by lang:signSystemKind. Any gmeow:Language carrying a
        // lang:carrierTag + gmeow:bcp47Tag pair is picked up regardless of its kind.
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:Rust a gmeow:Language ;
    lang:signSystemKind lang:programmingLanguageKind ;
    lang:carrierTag "x-gmeow-rust" ;
    gmeow:bcp47Tag "en" .

gmeow:Prolog a gmeow:Language ;
    lang:signSystemKind lang:formalLanguageKind ;
    lang:carrierTag "x-gmeow-prolog" ;
    gmeow:bcp47Tag "en" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert!(map.contains_key("x-gmeow-rust"));
        assert!(map.contains_key("x-gmeow-prolog"));
    }

    #[test]
    fn load_tag_map_ntriples_format() {
        let nt = "\
<https://blackcatinformatics.ca/gmeow/English> \
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
<https://blackcatinformatics.ca/gmeow/Language> .\n\
<https://blackcatinformatics.ca/gmeow/English> \
<https://blackcatinformatics.ca/lang/carrierTag> \
\"x-gmeow-english\" .\n\
<https://blackcatinformatics.ca/gmeow/English> \
<https://blackcatinformatics.ca/gmeow/bcp47Tag> \
\"en\" .\n";
        let map = load_tag_map(nt.as_bytes(), "ntriples").expect("parse");
        assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
    }

    // ── shared fixtures ──────────────────────────────────────────────────────

    /// A small internal→BCP-47 tag map: english, french, mandarin.
    fn sample_tag_map() -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert("x-gmeow-english".to_owned(), "en".to_owned());
        m.insert("x-gmeow-french".to_owned(), "fr".to_owned());
        m.insert("x-gmeow-mandarin".to_owned(), "zh".to_owned());
        m
    }

    fn desc(lexical: &str, language: Option<&str>) -> LitDesc {
        LitDesc {
            lexical: lexical.to_owned(),
            language: language.map(str::to_owned),
        }
    }

    // ── resolve_lang_input ──────────────────────────────────────────────────

    #[test]
    fn resolve_lang_input_defaults_to_en() {
        let tm = sample_tag_map();
        let none = resolve_lang_input(None, &tm, None).expect("none");
        assert_eq!(none.requested, vec!["en".to_owned()]);
        let empty = resolve_lang_input(Some("   "), &tm, None).expect("empty");
        assert_eq!(empty.requested, vec!["en".to_owned()]);
    }

    #[test]
    fn resolve_lang_input_accepts_public_bcp47() {
        let tm = sample_tag_map();
        let sel = resolve_lang_input(Some("fr"), &tm, None).expect("fr");
        assert_eq!(sel.requested, vec!["fr".to_owned()]);
    }

    #[test]
    fn resolve_lang_input_accepts_internal_tag() {
        let tm = sample_tag_map();
        let sel = resolve_lang_input(Some("x-gmeow-french"), &tm, None).expect("internal");
        assert_eq!(sel.requested, vec!["fr".to_owned()]);
    }

    #[test]
    fn resolve_lang_input_preserves_order_and_dedupes() {
        let tm = sample_tag_map();
        let sel = resolve_lang_input(Some("fr,en,fr,zh"), &tm, None).expect("list");
        assert_eq!(
            sel.requested,
            vec!["fr".to_owned(), "en".to_owned(), "zh".to_owned()]
        );
    }

    #[test]
    fn resolve_lang_input_rejects_unknown_public_tag() {
        let tm = sample_tag_map();
        let err = resolve_lang_input(Some("de"), &tm, None).expect_err("unknown");
        assert_eq!(err.tag, "de");
        // Available is en-first then lexicographic.
        assert_eq!(err.available.first(), Some(&"en".to_owned()));
        assert!(err.available.contains(&"fr".to_owned()));
    }

    #[test]
    fn resolve_lang_input_rejects_unknown_internal_tag() {
        let tm = sample_tag_map();
        let err = resolve_lang_input(Some("x-gmeow-klingon"), &tm, None).expect_err("unknown");
        assert_eq!(err.tag, "x-gmeow-klingon");
    }

    // ── mixed/upper-case internal-tag normalisation ──────────────────────────

    #[test]
    fn resolve_lang_input_mixed_case_internal_tag_resolves_same_as_lowercase() {
        // A mixed/upper-case internal tag must NOT raise UnknownLanguage; it must
        // resolve to the same BCP-47 value as the canonical lowercase form.
        let tm = sample_tag_map();

        let lower = resolve_lang_input(Some("x-gmeow-french"), &tm, None)
            .expect("lowercase internal tag must resolve");
        let upper = resolve_lang_input(Some("X-GMEOW-FRENCH"), &tm, None)
            .expect("UPPER-CASE internal tag must also resolve (Gap H2)");
        let mixed = resolve_lang_input(Some("X-Gmeow-French"), &tm, None)
            .expect("Mixed-Case internal tag must also resolve (Gap H2)");

        assert_eq!(
            lower.requested, upper.requested,
            "X-GMEOW-FRENCH must resolve to the same BCP-47 as x-gmeow-french"
        );
        assert_eq!(
            lower.requested, mixed.requested,
            "X-Gmeow-French must resolve to the same BCP-47 as x-gmeow-french"
        );
        assert_eq!(
            lower.requested,
            vec!["fr".to_owned()],
            "resolved tag must be fr"
        );
    }

    #[test]
    fn retag_graph_mixed_case_internal_tag_retagged() {
        // A literal whose language tag is an upper/mixed-case internal tag must be
        // retagged to the public BCP-47 form, not left unchanged (bucket_key and
        // retagged_literal must both normalise the case).
        let tm = sample_tag_map();

        // Use X-GMEOW-ENGLISH (all caps) — maps to "en".
        let nt = nt_lang("https://e/s", "https://e/label", "Hello", "X-GMEOW-ENGLISH");
        let out = retag_graph(&nt, "ntriples", &tm).expect("retag must succeed for upper-case tag");
        let text = String::from_utf8(out).expect("utf8");
        assert!(
            text.contains("\"Hello\"@en"),
            "upper-case internal tag must be retagged to @en: {text}"
        );
        assert!(
            !text.contains("@X-GMEOW-ENGLISH"),
            "upper-case internal tag must not appear in output: {text}"
        );
    }

    #[test]
    fn bucket_key_mixed_case_internal_tag_resolves() {
        // bucket_key must map a mixed-case internal tag to its BCP-47 bucket, not
        // leave it as an unmapped raw tag (which would fall through as an unknown
        // bucket and cause silent loss in select/filter paths).
        let tm = sample_tag_map();
        let lang = Some("X-Gmeow-Mandarin".to_owned());
        let key = bucket_key(&lang, &tm);
        assert_eq!(
            key, "zh",
            "mixed-case internal tag must resolve to its BCP-47 bucket key"
        );
    }

    #[test]
    fn resolve_lang_input_respects_custom_available() {
        let tm = sample_tag_map();
        let avail = vec!["en".to_owned(), "fr".to_owned()];
        let sel = resolve_lang_input(Some("fr"), &tm, Some(&avail)).expect("custom");
        assert_eq!(sel.requested, vec!["fr".to_owned()]);
        assert_eq!(
            sel.available,
            BTreeSet::from(["en".to_owned(), "fr".to_owned()])
        );
    }

    #[test]
    fn resolve_lang_input_rejects_tag_outside_custom_available() {
        let tm = sample_tag_map();
        let avail = vec!["en".to_owned(), "fr".to_owned()];
        // zh is in the tag_map but NOT in the custom available set → Err.
        let err = resolve_lang_input(Some("zh"), &tm, Some(&avail)).expect_err("outside");
        assert_eq!(err.tag, "zh");
    }

    // ── select_literal ──────────────────────────────────────────────────────

    #[test]
    fn select_literal_prefers_requested_language() {
        let tm = sample_tag_map();
        let literals = vec![
            desc("Bonjour", Some("x-gmeow-french")),
            desc("Hello", Some("x-gmeow-english")),
        ];
        let sel = select_literal(&literals, &["fr".to_owned()], &tm).expect("match");
        assert_eq!(sel.index, 0);
        assert_eq!(sel.retag_to, Some("fr".to_owned()));
        assert!(!sel.is_fallback);
    }

    #[test]
    fn select_literal_falls_back_to_english() {
        let tm = sample_tag_map();
        let literals = vec![desc("Hello", Some("x-gmeow-english"))];
        let sel = select_literal(&literals, &["zh".to_owned()], &tm).expect("fallback");
        assert_eq!(sel.index, 0);
        assert_eq!(sel.retag_to, Some("en".to_owned()));
        assert!(sel.is_fallback);
    }

    #[test]
    fn select_literal_prefers_internal_over_external_same_language() {
        let tm = sample_tag_map();
        // Two literals both land in the `en` public bucket: the internal-tagged one
        // (rank 0 carrier) must win over the external `en`-tagged one.
        let literals = vec![
            desc("external", Some("en")),
            desc("canonical", Some("x-gmeow-english")),
        ];
        let sel = select_literal(&literals, &["en".to_owned()], &tm).expect("match");
        assert_eq!(sel.index, 1);
        assert_eq!(sel.retag_to, Some("en".to_owned()));
    }

    // ── filter_literals ─────────────────────────────────────────────────────

    #[test]
    fn filter_literals_returns_all_requested_values() {
        let tm = sample_tag_map();
        let literals = vec![
            desc("a", Some("x-gmeow-french")),
            desc("b", Some("x-gmeow-french")),
            desc("c", Some("x-gmeow-english")),
        ];
        let sels = filter_literals(&literals, &["fr".to_owned()], &tm);
        assert_eq!(sels.len(), 2);
        let indices: BTreeSet<usize> = sels.iter().map(|s| s.index).collect();
        assert_eq!(indices, BTreeSet::from([0, 1]));
        assert!(sels.iter().all(|s| !s.is_fallback));
    }

    #[test]
    fn filter_literals_falls_back_to_english() {
        let tm = sample_tag_map();
        let literals = vec![desc("Hello", Some("x-gmeow-english"))];
        let sels = filter_literals(&literals, &["zh".to_owned()], &tm);
        assert_eq!(sels.len(), 1);
        assert_eq!(sels[0].index, 0);
        assert!(sels[0].is_fallback);
    }

    // ── load_inverse_tag_map ────────────────────────────────────────────────

    #[test]
    fn load_inverse_tag_map_recovers_natural_tags() {
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    lang:carrierTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"#;
        let inv = load_inverse_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert_eq!(inv.get("en"), Some(&"x-gmeow-english".to_owned()));
        assert_eq!(inv.get("fr"), Some(&"x-gmeow-french".to_owned()));
    }

    #[test]
    fn load_inverse_tag_map_drops_ambiguous_bcp47() {
        // Two natural languages mapping to the SAME bcp47 `en` → `en` is dropped
        // (no fabrication), while the unambiguous `fr` survives.
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    lang:carrierTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishUk a gmeow:Language ;
    lang:carrierTag "x-gmeow-english-uk" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    lang:carrierTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"#;
        let inv = load_inverse_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert!(!inv.contains_key("en"), "ambiguous en must be dropped");
        assert_eq!(inv.get("fr"), Some(&"x-gmeow-french".to_owned()));
    }

    // ── graph passes ────────────────────────────────────────────────────────

    /// Build a one-triple NT byte buffer: `<s> <p> "lex"@lang`.
    fn nt_lang(subject: &str, predicate: &str, lexical: &str, lang: &str) -> Vec<u8> {
        format!("<{subject}> <{predicate}> \"{lexical}\"@{lang} .\n").into_bytes()
    }

    #[test]
    fn retag_graph_to_internal_lifts_public_tags() {
        let mut inv = HashMap::new();
        inv.insert("en".to_owned(), "x-gmeow-english".to_owned());
        inv.insert("fr".to_owned(), "x-gmeow-french".to_owned());

        let mut nt = nt_lang("https://e/s", "https://e/label", "Hello", "en");
        nt.extend(nt_lang("https://e/s", "https://e/label", "Bonjour", "fr"));

        let out = retag_graph_to_internal(&nt, "ntriples", &inv).expect("retag");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("@x-gmeow-english"), "{text}");
        assert!(text.contains("@x-gmeow-french"), "{text}");
        assert!(!text.contains("\"Hello\"@en"), "{text}");
    }

    #[test]
    fn filter_graph_keeps_only_selected_language() {
        let tm = sample_tag_map();
        let mut nt = nt_lang(
            "https://e/s",
            "https://e/label",
            "Bonjour",
            "x-gmeow-french",
        );
        nt.extend(nt_lang(
            "https://e/s",
            "https://e/label",
            "Hello",
            "x-gmeow-english",
        ));

        let preds = vec!["https://e/label".to_owned()];
        let out = filter_graph(&nt, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("\"Bonjour\"@fr"), "{text}");
        assert!(
            !text.contains("Hello"),
            "english must be filtered out: {text}"
        );
    }

    #[test]
    fn filter_graph_second_predicate_falls_back_to_english() {
        let tm = sample_tag_map();
        // Predicate `label` has fr+en; predicate `note` has ONLY en. A `fr` request
        // keeps fr on `label` and re-adds the en fallback on `note`.
        let mut nt = nt_lang(
            "https://e/s",
            "https://e/label",
            "Bonjour",
            "x-gmeow-french",
        );
        nt.extend(nt_lang(
            "https://e/s",
            "https://e/label",
            "Hello",
            "x-gmeow-english",
        ));
        nt.extend(nt_lang(
            "https://e/s",
            "https://e/note",
            "Note",
            "x-gmeow-english",
        ));

        let preds = vec!["https://e/label".to_owned(), "https://e/note".to_owned()];
        let out = filter_graph(&nt, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
        let text = String::from_utf8(out).expect("utf8");
        assert!(text.contains("\"Bonjour\"@fr"), "{text}");
        assert!(
            text.contains("\"Note\"@en"),
            "english fallback on note: {text}"
        );
        assert!(!text.contains("\"Hello\""), "label english dropped: {text}");
    }

    #[test]
    fn filter_graph_noop_is_byte_identical() {
        let tm = sample_tag_map();
        // The literal is already public `fr` and `fr` is requested → set-equality
        // skip leaves the group untouched, so the re-serialized output matches a
        // plain round-trip of the same input.
        let nt = nt_lang("https://e/s", "https://e/label", "Bonjour", "fr");
        let preds = vec!["https://e/label".to_owned()];
        let filtered =
            filter_graph(&nt, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
        // A no-op filter must equal a plain parse→serialize round-trip.
        let roundtrip = retag_graph(&nt, "ntriples", &tm).expect("roundtrip");
        assert_eq!(filtered, roundtrip, "no-op filter must be byte-identical");
    }

    #[test]
    fn retag_graph_preserves_typed_literal_and_bnode() {
        // A typed literal on a blank-node subject must survive retag_graph (a no-op
        // for it: no internal language tag) with no datatype loss and bnode intact.
        let tm = sample_tag_map();
        let nt = concat!(
            "_:b0 <https://e/count> \"5\"^^<http://www.w3.org/2001/XMLSchema#integer> .\n",
            "_:b0 <https://e/label> \"Hi\"@x-gmeow-english .\n",
        )
        .as_bytes()
        .to_vec();

        let out = retag_graph(&nt, "ntriples", &tm).expect("retag");
        let text = String::from_utf8(out.clone()).expect("utf8");
        assert!(
            text.contains("XMLSchema#integer"),
            "typed literal datatype must survive: {text}"
        );
        assert!(text.contains("_:"), "blank node must survive: {text}");
        assert!(
            text.contains("@en"),
            "internal english retagged to public: {text}"
        );

        // Round-trip survives a re-parse with the datatype + bnode structure intact.
        let reparsed =
            parse_dataset(&out, "application/n-triples", None).expect("re-parse retagged output");
        let has_typed = reparsed.quad_refs().any(|qr| {
            matches!(
                qr.o,
                TermRef::Literal { datatype, .. }
                    if dataset_iri(&reparsed, datatype) == "http://www.w3.org/2001/XMLSchema#integer"
            )
        });
        assert!(has_typed, "re-parsed typed literal must keep xsd:integer");
        let has_bnode = reparsed
            .quad_refs()
            .any(|qr| matches!(qr.s, TermRef::Blank { .. }));
        assert!(has_bnode, "re-parsed bnode subject must survive");
    }

    /// Resolve a `TermRef` datatype id to its IRI string (test helper).
    fn dataset_iri(dataset: &purrdf::RdfDataset, datatype: purrdf::TermId) -> String {
        match dataset.resolve(datatype) {
            TermRef::Iri(iri) => iri.to_owned(),
            _ => String::new(),
        }
    }

    // ── reifier/annotation sync tests ───────────────────────────────────────

    /// Build N-Triples bytes for a dataset that includes a quad, a reifier on it,
    /// and an annotation on the reifier. Returns the serialized bytes.
    fn build_nt_with_reifier(
        subject: &str,
        predicate: &str,
        lexical: &str,
        lang: &str,
        reifier_iri: &str,
        annotation_predicate: &str,
        annotation_value: &str,
    ) -> Vec<u8> {
        use purrdf::{
            RdfAnnotation, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple,
        };

        let subject_term = RdfTerm::iri(subject);
        let object_lit = RdfLiteral::language_tagged(lexical, lang);
        let object_term = RdfTerm::Literal(object_lit);
        let stmt = RdfTriple::new(subject_term.clone(), predicate, object_term.clone());

        let reifier_term = RdfTerm::iri(reifier_iri);
        let reifier = RdfReifier::new(reifier_term.clone(), stmt);
        let annotation = RdfAnnotation::new(
            reifier_term,
            annotation_predicate,
            RdfTerm::literal(RdfLiteral::simple(annotation_value)),
        );

        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(&RdfQuad::new(subject_term, predicate, object_term));
        builder.push_owned_reifier(&reifier);
        builder.push_owned_annotation(&annotation);

        let dataset = builder.freeze().expect("freeze");
        purrdf::serialize_dataset(
            &dataset,
            "application/n-triples",
            purrdf::SerializeGraph::DefaultGraph,
        )
        .expect("serialize")
    }

    #[test]
    fn rewrite_graph_retag_updates_reifier_statement() {
        let tm = sample_tag_map();

        // Build a dataset: one quad with @x-gmeow-english, a reifier on it, and an
        // annotation on the reifier.
        let input = build_nt_with_reifier(
            "https://e/s",
            "https://e/label",
            "Hello",
            "x-gmeow-english",
            "https://e/r1",
            "https://e/confidence",
            "high",
        );

        let out = retag_graph(&input, "ntriples", &tm).expect("retag");
        let text = String::from_utf8(out.clone()).expect("utf8");

        // The base quad must be retagged to @en.
        assert!(text.contains("\"Hello\"@en"), "base quad retagged: {text}");
        assert!(
            !text.contains("@x-gmeow-english"),
            "internal tag must be gone: {text}"
        );

        // The reifier's statement object must also be updated to @en.
        // We check by parsing the output and inspecting owned_reifiers().
        let reparsed = parse_dataset(&out, "application/n-triples", None).expect("reparse");
        let reifier_stmt_updated = reparsed.owned_reifiers().any(|r| {
            if let purrdf::RdfTerm::Literal(lit) = &r.statement.object {
                lit.language.as_deref() == Some("en") && lit.lexical_form == "Hello"
            } else {
                false
            }
        });
        assert!(
            reifier_stmt_updated,
            "reifier statement object must be updated to @en; output:\n{text}"
        );

        // The annotation on <https://e/r1> must still be present.
        let has_annotation = reparsed
            .owned_annotations()
            .any(|a| a.reifier.to_string() == "<https://e/r1>");
        assert!(has_annotation, "annotation on r1 must survive: {text}");
    }

    #[test]
    fn filter_graph_drops_reifier_for_dropped_literal() {
        use purrdf::{
            RdfAnnotation, RdfDatasetBuilder, RdfLiteral, RdfQuad, RdfReifier, RdfTerm, RdfTriple,
        };

        // Dataset: two quads (English + French) on the same (s, p), with a reifier
        // only on the English quad.
        let subject_term = RdfTerm::iri("https://e/s");
        let pred = "https://e/label";
        let en_lit = RdfLiteral::language_tagged("Hello", "x-gmeow-english");
        let fr_lit = RdfLiteral::language_tagged("Bonjour", "x-gmeow-french");

        let en_stmt = RdfTriple::new(subject_term.clone(), pred, RdfTerm::Literal(en_lit.clone()));
        let reifier_term = RdfTerm::iri("https://e/r_en");
        let reifier = RdfReifier::new(reifier_term.clone(), en_stmt);
        let annotation = RdfAnnotation::new(
            reifier_term,
            "https://e/confidence",
            RdfTerm::literal(RdfLiteral::simple("high")),
        );

        let mut builder = RdfDatasetBuilder::new();
        builder.push_owned_quad(&RdfQuad::new(
            subject_term.clone(),
            pred,
            RdfTerm::Literal(en_lit),
        ));
        builder.push_owned_quad(&RdfQuad::new(subject_term, pred, RdfTerm::Literal(fr_lit)));
        builder.push_owned_reifier(&reifier);
        builder.push_owned_annotation(&annotation);

        let dataset = builder.freeze().expect("freeze");
        let input = purrdf::serialize_dataset(
            &dataset,
            "application/n-triples",
            purrdf::SerializeGraph::DefaultGraph,
        )
        .expect("serialize");

        let tm = sample_tag_map();
        let preds = vec![pred.to_owned()];
        // Request only French — English quad is dropped.
        let out =
            filter_graph(&input, "ntriples", &tm, &["fr".to_owned()], &preds).expect("filter");
        let text = String::from_utf8(out.clone()).expect("utf8");

        // French quad survives, retagged to @fr.
        assert!(text.contains("\"Bonjour\"@fr"), "french survives: {text}");
        // English quad is gone.
        assert!(!text.contains("Hello"), "english dropped: {text}");

        let reparsed = parse_dataset(&out, "application/n-triples", None).expect("reparse");

        // The reifier on the dropped English quad must not appear.
        let reifier_present = reparsed
            .owned_reifiers()
            .any(|r| r.reifier.to_string() == "<https://e/r_en>");
        assert!(
            !reifier_present,
            "reifier for dropped literal must be absent: {text}"
        );

        // The annotation on the dropped reifier must also be gone.
        let annotation_present = reparsed
            .owned_annotations()
            .any(|a| a.reifier.to_string() == "<https://e/r_en>");
        assert!(
            !annotation_present,
            "annotation for dropped reifier must be absent: {text}"
        );
    }

    #[test]
    fn filter_graph_retag_updates_reifier_statement() {
        // Dataset: one quad with @x-gmeow-english, a reifier on it.
        // Request "en" → the quad is retagged to @en, and the reifier statement
        // must follow.
        let input = build_nt_with_reifier(
            "https://e/s",
            "https://e/label",
            "Hello",
            "x-gmeow-english",
            "https://e/r_en",
            "https://e/note",
            "tested",
        );

        let tm = sample_tag_map();
        let preds = vec!["https://e/label".to_owned()];
        let out =
            filter_graph(&input, "ntriples", &tm, &["en".to_owned()], &preds).expect("filter");
        let text = String::from_utf8(out.clone()).expect("utf8");

        // The quad must be retagged to @en.
        assert!(
            text.contains("\"Hello\"@en"),
            "quad retagged to @en: {text}"
        );
        assert!(
            !text.contains("@x-gmeow-english"),
            "internal tag gone: {text}"
        );

        let reparsed = parse_dataset(&out, "application/n-triples", None).expect("reparse");

        // The reifier's statement object must be @en, not @x-gmeow-english.
        let reifier_updated = reparsed.owned_reifiers().any(|r| {
            if let purrdf::RdfTerm::Literal(lit) = &r.statement.object {
                lit.language.as_deref() == Some("en") && lit.lexical_form == "Hello"
            } else {
                false
            }
        });
        assert!(
            reifier_updated,
            "reifier statement must be updated to @en: {text}"
        );
    }

    // ── public_literal / public_text ────────────────────────────────────────

    /// Parse N-Triples bytes into a dataset for the graph-level selection tests.
    fn parse_nt(nt: &[u8]) -> std::sync::Arc<purrdf::RdfDataset> {
        parse_dataset(nt, "application/n-triples", None).expect("parse")
    }

    #[test]
    fn public_literal_retags_internal_carrier() {
        // An internal x-gmeow-english literal WITH a map entry wins and is retagged
        // to its public BCP-47 form.
        let tm = sample_tag_map();
        let nt = nt_lang("https://e/s", "https://e/label", "Hello", "x-gmeow-english");
        let ds = parse_nt(&nt);
        let lit =
            public_literal(&ds, "https://e/s", "https://e/label", &tm).expect("literal present");
        assert_eq!(lit.lexical, "Hello");
        assert_eq!(lit.language, Some("en".to_owned()));
        assert_eq!(
            public_text(&ds, "https://e/s", "https://e/label", &tm),
            "Hello"
        );
    }

    #[test]
    fn public_literal_prefers_carrier_over_other_internal() {
        // Two mapped internal literals: the x-gmeow-english carrier (rank 0) wins
        // over x-gmeow-french (rank 1) and is retagged to en.
        let tm = sample_tag_map();
        let mut nt = nt_lang(
            "https://e/s",
            "https://e/label",
            "Bonjour",
            "x-gmeow-french",
        );
        nt.extend(nt_lang(
            "https://e/s",
            "https://e/label",
            "Hello",
            "x-gmeow-english",
        ));
        let ds = parse_nt(&nt);
        let lit = public_literal(&ds, "https://e/s", "https://e/label", &tm).expect("literal");
        assert_eq!(lit.lexical, "Hello");
        assert_eq!(lit.language, Some("en".to_owned()));
    }

    #[test]
    fn public_literal_falls_back_to_external_tag_unchanged() {
        // No internal-mapped literal: the deterministic first (language, lexical)
        // candidate is returned with its original external tag preserved.
        let tm = sample_tag_map();
        let mut nt = nt_lang("https://e/s", "https://e/label", "Servus", "de");
        nt.extend(nt_lang("https://e/s", "https://e/label", "Hola", "es"));
        let ds = parse_nt(&nt);
        let lit = public_literal(&ds, "https://e/s", "https://e/label", &tm).expect("literal");
        // (language, lexical) order: ("de","Servus") < ("es","Hola").
        assert_eq!(lit.lexical, "Servus");
        assert_eq!(lit.language, Some("de".to_owned()));
    }

    #[test]
    fn public_literal_none_when_no_literal_object() {
        let tm = sample_tag_map();
        let nt = nt_lang("https://e/s", "https://e/label", "Hi", "x-gmeow-english");
        let ds = parse_nt(&nt);
        // Different predicate → no candidate.
        assert!(public_literal(&ds, "https://e/s", "https://e/other", &tm).is_none());
        assert_eq!(public_text(&ds, "https://e/s", "https://e/other", &tm), "");
    }
}
