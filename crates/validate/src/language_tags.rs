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

use gmeow_rdf::{parse_dataset, TermRef};

/// The GMEOW namespace prefix for term IRIs.
const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// RDF type predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

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
/// Scans individuals typed ``gmeow:Language``, ``gmeow:FormalLanguage``, and
/// ``gmeow:ProgrammingLanguage`` for ``gmeow:languageTag`` and
/// ``gmeow:bcp47Tag`` property values.
///
/// Each property must have exactly one distinct lexical value per individual:
/// - Missing either property → individual is silently skipped.
/// - More than one distinct value for either property → returns `Err`.
///
/// Returns ``{internal_tag: bcp47_tag}``.
pub fn load_tag_map(rdf_bytes: &[u8], format: &str) -> Result<HashMap<String, String>, String> {
    let media_type = media_type_for(format)?;

    // Parse straight into the gmeow-rdf IR via the native codecs: no oxigraph `io`
    // parser, and lenient private-use language tags by construction.
    let dataset =
        parse_dataset(rdf_bytes, media_type, None).map_err(|e| format!("RDF parse error: {e}"))?;

    build_tag_map(&dataset)
}

/// Build the tag map from an already-frozen `RdfDataset`, scanning all three
/// language classes (``Language``, ``FormalLanguage``, ``ProgrammingLanguage``).
///
/// Extracted for testability. Delegates to [`build_tag_map_for`].
fn build_tag_map(dataset: &gmeow_rdf::RdfDataset) -> Result<HashMap<String, String>, String> {
    let lang_class = format!("{NAMESPACE}Language");
    let formal_class = format!("{NAMESPACE}FormalLanguage");
    let prog_class = format!("{NAMESPACE}ProgrammingLanguage");
    build_tag_map_for(dataset, &[&lang_class, &formal_class, &prog_class])
}

/// Build the internal→BCP-47 tag map, restricting the scan to the language
/// `classes` (their IRI strings). The natural-`Language`-only restriction is the
/// inverse-map path: a programming language's code is tagged ``en`` too, so
/// including those classes would make the ``en`` reverse ambiguous.
///
/// Each individual typed as one of `classes` must carry exactly one distinct
/// ``gmeow:languageTag`` and one distinct ``gmeow:bcp47Tag`` lexical value:
/// - Missing either → individual is silently skipped (authoring SHACL enforces
///   completeness; we never fabricate).
/// - More than one distinct value → returns `Err`.
/// - Two individuals mapping one internal tag to DIFFERENT BCP-47 tags → `Err`.
fn build_tag_map_for(
    dataset: &gmeow_rdf::RdfDataset,
    classes: &[&str],
) -> Result<HashMap<String, String>, String> {
    let tag_prop = format!("{NAMESPACE}languageTag");
    let bcp_prop = format!("{NAMESPACE}bcp47Tag");

    // Collect the string-form subjects that are typed as one of the language
    // classes. We use string matching via quad_refs() since TermId is crate-private.
    let mut lang_subjects: HashSet<String> = HashSet::new();
    for qr in dataset.quad_refs() {
        if let (TermRef::Iri(p), TermRef::Iri(o)) = (qr.p, qr.o) {
            if p == RDF_TYPE && classes.contains(&o) {
                if let TermRef::Iri(s) = qr.s {
                    lang_subjects.insert(s.to_owned());
                }
            }
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
            if let TermRef::Iri(p) = qr.p {
                if p == tag_prop || p == bcp_prop {
                    if let TermRef::Literal { lexical, .. } = qr.o {
                        props
                            .entry(s.to_owned())
                            .or_default()
                            .entry(p.to_owned())
                            .or_default()
                            .insert(lexical.to_owned());
                    }
                }
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
            return Err(format!(
                "individual <{subject}> has ambiguous languageTag values: {int_vals:?}; \
                 tag-map projection requires a single canonical value"
            ));
        }
        if bcp_vals.len() > 1 {
            return Err(format!(
                "individual <{subject}> has ambiguous bcp47Tag values: {bcp_vals:?}; \
                 tag-map projection requires a single canonical value"
            ));
        }
        let int_val = int_vals.into_iter().next().unwrap();
        let bcp_val = bcp_vals.into_iter().next().unwrap();
        // Two `gmeow:Language` individuals sharing one internal `languageTag` but
        // mapping it to DIFFERENT `bcp47Tag`s is a nondeterministic conflict; per the
        // no-optionality/hard-fail doctrine, reject it rather than silently letting
        // the last writer win. (A repeated tag→SAME bcp47Tag is a harmless duplicate.)
        if let Some(existing) = tag_map.get(&int_val) {
            if existing != &bcp_val {
                return Err(format!(
                    "conflicting bcp47Tag for internal languageTag {int_val:?}: \
                     {existing:?} vs {bcp_val:?}; the tag-map projection requires a \
                     single canonical bcp47Tag per internal tag"
                ));
            }
        }
        tag_map.insert(int_val, bcp_val);
    }

    Ok(tag_map)
}

/// Build the BCP-47 → internal mapping — the inverse of [`load_tag_map`].
///
/// Built from **natural** ``gmeow:Language`` individuals only (NOT formal or
/// programming languages): a programming language's code carries an ``en`` BCP-47
/// tag too, so including them would make the ``en`` reverse ambiguous. A BCP-47
/// tag that several natural languages still share is dropped rather than guessed
/// (the no-fabrication discipline). Keys are lowercased.
pub fn load_inverse_tag_map(
    rdf_bytes: &[u8],
    format: &str,
) -> Result<HashMap<String, String>, String> {
    let media_type = media_type_for(format)?;
    let dataset =
        parse_dataset(rdf_bytes, media_type, None).map_err(|e| format!("RDF parse error: {e}"))?;

    let lang_class = format!("{NAMESPACE}Language");
    let natural = build_tag_map_for(&dataset, &[&lang_class])?;

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
            match tag_map.get(token) {
                Some(bcp) => bcp.to_lowercase(),
                None => {
                    return Err(UnknownLanguage {
                        tag: token.to_owned(),
                        available: available_en_first(&available_set),
                    })
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
            tag_map.get(lang).cloned().unwrap_or_else(|| lang.clone())
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
        Some(lang) if is_internal_tag(lang) => tag_map.get(lang).cloned(),
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
        if let Some(bucket) = by_bcp.get(req) {
            if let Some(&(index, _)) = bucket.first() {
                return Some(selection_for(index, literals, tag_map, false));
            }
        }
    }

    if let Some(bucket) = by_bcp.get("en") {
        if let Some(&(index, _)) = bucket.first() {
            return Some(selection_for(index, literals, tag_map, true));
        }
    }
    if let Some(best_key) = lowest_ranked_tagged(&by_bcp) {
        if let Some(&(index, _)) = by_bcp[&best_key].first() {
            return Some(selection_for(index, literals, tag_map, true));
        }
    }
    if let Some(bucket) = by_bcp.get("") {
        if let Some(&(index, _)) = bucket.first() {
            return Some(selection_for(index, literals, tag_map, true));
        }
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

    if let Some(bucket) = by_bcp.get("en") {
        if let Some(&(index, _)) = bucket.first() {
            return vec![selection_for(index, literals, tag_map, true)];
        }
    }
    if let Some(best_key) = lowest_ranked_tagged(&by_bcp) {
        if let Some(&(index, _)) = by_bcp[&best_key].first() {
            return vec![selection_for(index, literals, tag_map, true)];
        }
    }
    if let Some(bucket) = by_bcp.get("") {
        if let Some(&(index, _)) = bucket.first() {
            return vec![selection_for(index, literals, tag_map, true)];
        }
    }
    Vec::new()
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
    lit: &gmeow_rdf::RdfLiteral,
    tag_map: &HashMap<String, String>,
) -> Option<gmeow_rdf::RdfLiteral> {
    let lang = lit.language.as_deref()?;
    if !is_internal_tag(lang) {
        return None;
    }
    let bcp = tag_map.get(lang)?;
    Some(gmeow_rdf::RdfLiteral::language_tagged(
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
) -> Result<Vec<u8>, String> {
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
) -> Result<Vec<u8>, String> {
    rewrite_graph(rdf_bytes, format, |lit| {
        let lang = lit.language.as_deref()?;
        if is_internal_tag(lang) {
            return None;
        }
        let internal = inverse_map.get(&lang.to_lowercase())?;
        Some(gmeow_rdf::RdfLiteral::language_tagged(
            lit.lexical_form.clone(),
            internal.clone(),
        ))
    })
}

/// Parse `rdf_bytes`, rewrite each literal object through `rewrite` (when it
/// returns `Some`, the object is replaced), copy everything else verbatim, and
/// serialize back to N-Triples. The full RDF 1.2 statement layer (reifiers +
/// annotations) and quoted-triple terms are carried through unchanged.
fn rewrite_graph<F>(rdf_bytes: &[u8], format: &str, rewrite: F) -> Result<Vec<u8>, String>
where
    F: Fn(&gmeow_rdf::RdfLiteral) -> Option<gmeow_rdf::RdfLiteral>,
{
    let media_type = media_type_for(format)?;
    let dataset =
        parse_dataset(rdf_bytes, media_type, None).map_err(|e| format!("RDF parse error: {e}"))?;

    let mut builder = gmeow_rdf::RdfDatasetBuilder::new();
    for mut quad in dataset.owned_quads() {
        if let gmeow_rdf::RdfTerm::Literal(lit) = &quad.object {
            if let Some(new_lit) = rewrite(lit) {
                quad.object = gmeow_rdf::RdfTerm::Literal(new_lit);
            }
        }
        builder.push_owned_quad(&quad);
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
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
) -> Result<Vec<u8>, String> {
    let media_type = media_type_for(format)?;
    let dataset =
        parse_dataset(rdf_bytes, media_type, None).map_err(|e| format!("RDF parse error: {e}"))?;

    let target_preds: HashSet<&str> = predicates.iter().map(String::as_str).collect();

    // Group language-tagged literal objects by (subject-key, predicate) for the
    // target predicates. The subject key is the owned term's canonical rendering,
    // which is stable for IRIs and blank nodes alike.
    type GroupKey = (String, String);
    let mut group_lits: HashMap<GroupKey, Vec<gmeow_rdf::RdfLiteral>> = HashMap::new();
    for quad in dataset.owned_quads() {
        if !target_preds.contains(quad.predicate.as_str()) {
            continue;
        }
        if let gmeow_rdf::RdfTerm::Literal(lit) = &quad.object {
            if lit.language.is_some() {
                let key = (quad.subject.to_string(), quad.predicate.clone());
                group_lits.entry(key).or_default().push(lit.clone());
            }
        }
    }

    // For each group, compute the chosen literal set. If it equals the current
    // set, mark the group as a no-op (skip); else record the replacement set.
    let mut group_replacement: HashMap<GroupKey, Vec<gmeow_rdf::RdfLiteral>> = HashMap::new();
    let mut group_skip: HashSet<GroupKey> = HashSet::new();
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
        let chosen: Vec<gmeow_rdf::RdfLiteral> = selections
            .iter()
            .map(|sel| match &sel.retag_to {
                Some(bcp) => gmeow_rdf::RdfLiteral::language_tagged(
                    literals[sel.index].lexical_form.clone(),
                    bcp.clone(),
                ),
                None => literals[sel.index].clone(),
            })
            .collect();

        // Set-equality skip: if the chosen set equals the current set, leave the
        // group untouched (byte-identical output for an already-satisfied group).
        let current: HashSet<&gmeow_rdf::RdfLiteral> = literals.iter().collect();
        let chosen_set: HashSet<&gmeow_rdf::RdfLiteral> = chosen.iter().collect();
        if current == chosen_set {
            group_skip.insert(key.clone());
        } else {
            group_replacement.insert(key.clone(), chosen);
        }
    }

    // Rebuild the dataset: drop the original language-tagged literals of replaced
    // groups, emit the chosen set once per group, copy everything else verbatim.
    let mut builder = gmeow_rdf::RdfDatasetBuilder::new();
    let mut emitted_groups: HashSet<GroupKey> = HashSet::new();
    for quad in dataset.owned_quads() {
        let is_target = target_preds.contains(quad.predicate.as_str());
        if is_target {
            if let gmeow_rdf::RdfTerm::Literal(lit) = &quad.object {
                if lit.language.is_some() {
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
                                new_quad.object = gmeow_rdf::RdfTerm::Literal(new_lit.clone());
                                builder.push_owned_quad(&new_quad);
                            }
                        }
                        // Original language-tagged object of a replaced group is dropped.
                        continue;
                    }
                }
            }
        }
        builder.push_owned_quad(&quad);
    }
    for reifier in dataset.owned_reifiers() {
        builder.push_owned_reifier(&reifier);
    }
    for annotation in dataset.owned_annotations() {
        builder.push_owned_annotation(&annotation);
    }

    serialize_ntriples(builder)
}

/// Freeze `builder` and serialize the result to N-Triples bytes (default graph).
fn serialize_ntriples(builder: gmeow_rdf::RdfDatasetBuilder) -> Result<Vec<u8>, String> {
    let dataset = builder
        .freeze()
        .map_err(|e| format!("dataset freeze error: {e}"))?;
    gmeow_rdf::serialize_dataset(
        &dataset,
        "application/n-triples",
        gmeow_rdf::SerializeGraph::DefaultGraph,
    )
    .map_err(|e| format!("RDF serialize error: {e}"))
}

// ── helpers ─────────────────────────────────────────────────────────────────────

/// Map a format string (legacy short ids or media types) to a native-codec media
/// type understood by [`parse_dataset`].
fn media_type_for(format: &str) -> Result<&'static str, String> {
    match format.to_ascii_lowercase().as_str() {
        "turtle" | "text/turtle" | "ttl" => Ok("text/turtle"),
        "n-triples" | "ntriples" | "nt" | "application/n-triples" => Ok("application/n-triples"),
        "n-quads" | "nquads" | "nq" | "application/n-quads" => Ok("application/n-quads"),
        "trig" | "application/trig" => Ok("application/trig"),
        _ => Err(format!("unsupported RDF format: {format:?}")),
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
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    gmeow:languageTag "x-gmeow-french" ;
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
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:languageTag "x-gmeow-english-alt" ;
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
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishAlt a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en-GB" .
"#;
        let err = load_tag_map(ttl.as_bytes(), "turtle").expect_err("conflict must error");
        assert!(err.contains("conflicting bcp47Tag"), "{err}");
    }

    #[test]
    fn load_tag_map_duplicate_identical_ok() {
        // The SAME internal tag → SAME bcp47Tag from two individuals is a harmless
        // duplicate, not a conflict.
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishCopy a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
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
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert!(map.is_empty(), "incomplete individual must be skipped");
    }

    #[test]
    fn load_tag_map_formal_and_prog_language() {
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:Rust a gmeow:ProgrammingLanguage ;
    gmeow:languageTag "x-gmeow-rust" ;
    gmeow:bcp47Tag "en" .

gmeow:Prolog a gmeow:FormalLanguage ;
    gmeow:languageTag "x-gmeow-prolog" ;
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
<https://blackcatinformatics.ca/gmeow/languageTag> \
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
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    gmeow:languageTag "x-gmeow-french" ;
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
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:EnglishUk a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english-uk" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    gmeow:languageTag "x-gmeow-french" ;
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
    fn dataset_iri(dataset: &gmeow_rdf::RdfDataset, datatype: gmeow_rdf::TermId) -> String {
        match dataset.resolve(datatype) {
            TermRef::Iri(iri) => iri.to_owned(),
            _ => String::new(),
        }
    }
}
