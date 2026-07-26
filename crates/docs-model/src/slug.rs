// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Slug, display-name and facet helpers — the pure naming layer.
//!
//! These are the only parts of the documentation renderer that the *model* half
//! of this crate needs: `rdf.rs` builds the documentation graph from them,
//! `card.rs` mints Pydantic module/class names, and `model.rs` resolves term
//! slugs. They carry no `include_bytes!`, no site assembly and no template
//! state, so they sit below the renderer rather than inside it.
//!
//! Hoisted out of `render.rs` so `crates/docs-model` can depend on them without
//! dragging in `render.rs`/`vendored_asset.rs` — the two files that embed the
//! vendored wasm engines. Keeping them in the renderer would close a
//! `docs-model -> docs -> docs-model` cycle through exactly the modules the
//! split exists to move.

use std::collections::BTreeMap;

use crate::model::{DocConcern, DocSlice, DocTerm, DocTermCategory, DocsModel};

/// The generated `gmeow_models` module slug for a slice IRI (the last IRI segment,
/// lowercased, non-identifier chars → `_`) — the same routing the Pydantic emitter
/// uses, so `gmeow_models.<slice>` resolves to the term's model module.
pub fn pydantic_module_slug(slice_iri: &str) -> String {
    let local = slice_iri.rsplit(['#', '/']).next().unwrap_or(slice_iri);
    let mut out = String::new();
    for ch in local.chars() {
        out.push(if ch == '_' || ch.is_ascii_alphanumeric() {
            ch.to_ascii_lowercase()
        } else {
            '_'
        });
    }
    while out.contains("__") {
        out = out.replace("__", "_");
    }
    out.trim_matches('_').to_string()
}

/// The generated Pydantic class name for a class IRI: the CamelCase of its local
/// name (mirroring the emitter's `sanitize_type`), guarded against a leading digit.
pub fn pydantic_class_name(iri: &str) -> String {
    let local = iri.rsplit(['#', '/']).next().unwrap_or(iri);
    let mut ident = String::new();
    for ch in local.chars() {
        ident.push(if ch == '_' || ch.is_ascii_alphanumeric() {
            ch
        } else {
            '_'
        });
    }
    while ident.contains("__") {
        ident = ident.replace("__", "_");
    }
    ident = ident.trim_matches('_').to_string();
    let mut chars = ident.chars();
    let name = match chars.next() {
        Some(c) => format!("{}{}", c.to_ascii_uppercase(), chars.as_str()),
        None => "GmeowModel".to_string(),
    };
    if name.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        format!("N{name}")
    } else {
        name
    }
}

/// The coarse-grain provenance chain for a durable page: the producing-stage path
/// walked BACKWARD over `gmeow:dataflowConsumes` from `start_local` (the stage
/// whose local name is `start_local`, default `stage-docs-render`), following the
/// lexicographically-smallest consumed producer at each step until a source-reading
/// stage (one that consumes nothing in-DAG) is reached. Cycle-safe (visited set).
/// Returns the stage local names in consumer→producer order, or empty when the
/// start stage is absent.
pub fn provenance_chain(
    pipeline: &crate::model::DocPipeline,
    start_local: &str,
) -> Vec<String> {
    use std::collections::BTreeSet;
    let by_iri: BTreeMap<&str, &crate::model::DocStage> = pipeline
        .stages
        .iter()
        .map(|s| (s.iri.as_str(), s))
        .collect();
    let Some(mut current) = pipeline
        .stages
        .iter()
        .find(|s| local_name(&s.iri) == start_local)
    else {
        return Vec::new();
    };
    let mut chain = vec![local_name(&current.iri).to_string()];
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    visited.insert(current.iri.as_str());
    // `next_iri` is cloned to an owned String so the borrow of `current.consumes`
    // ends before `current` is reassigned in the body (the condition temporary must
    // not outlive the reassignment).
    while let Some(next_iri) = current
        .consumes
        .iter()
        .filter(|p| !visited.contains(p.as_str()))
        .min()
        .cloned()
    {
        let Some(next) = by_iri.get(next_iri.as_str()) else {
            break;
        };
        chain.push(local_name(&next.iri).to_string());
        visited.insert(next.iri.as_str());
        current = *next;
    }
    chain
}

/// The INJECTIVE documentation-entry slug of a term — the single source of the
/// `documentation/term/{slug}` doc-entry IRI, the page URL, and every cross-page
/// link. Returns the term's resolved [`DocTerm::slug`] (assigned once from the
/// whole term set by [`resolve_term_slugs`] at model build), so the doc-entry
/// subject is collision-free and its coverage incidence can never be conflated.
///
/// A hand-built term (a unit-test fixture that never went through model
/// resolution) carries an empty `slug`; it then falls back to the base slug —
/// safe because such tiny models never collide, and the real model always carries
/// a resolved slug, so this is one function with one answer, never two that can
/// disagree.
pub fn term_slug(term: &DocTerm) -> String {
    if term.slug.is_empty() {
        slugify(local_name(&term.iri))
    } else {
        term.slug.clone()
    }
}

/// The category discriminator segment appended to a contended base slug.
pub fn category_slug(category: DocTermCategory) -> &'static str {
    match category {
        DocTermCategory::Class => "class",
        DocTermCategory::Property => "property",
        DocTermCategory::Individual => "individual",
        DocTermCategory::Datatype => "datatype",
        DocTermCategory::Other => "other",
    }
}

/// A short, stable IRI discriminator: the first 12 hex chars of the full IRI's
/// BLAKE3 digest — a deterministic, order-independent tiebreak for the rare case
/// where two distinct terms share BOTH a base slug and a category.
pub fn short_iri_digest(iri: &str) -> String {
    blake3::hash(iri.as_bytes()).to_hex()[..12].to_owned()
}

/// Resolve the disambiguated `documentation/term/{slug}` slug for every term whose
/// base slug COLLIDES — a deterministic pure function of the term set, keyed by
/// term IRI. Terms whose base slug is already unique are ABSENT from the map (they
/// keep the base slug via [`term_slug`]'s fallback), so the returned entries are
/// exactly the colliders — the minority that must change.
///
/// # Scheme (minimal churn, no blank nodes)
///
/// 1. **Base slug** = [`slugify`] of the IRI's local name (the historical slug).
///    A base slug carried by exactly ONE term is kept verbatim — the non-colliding
///    terms' IRIs / URLs / links are unchanged (and they are not in the map).
/// 2. **Category disambiguation** — a base slug shared by ≥2 distinct terms (the
///    `slugify` case/punctuation fold is lossy, e.g. class `AcceptanceStatus` and
///    property `acceptanceStatus` both fold to `acceptancestatus`) gets its
///    category appended (`-class` / `-property` / `-individual` / `-datatype` /
///    `-other`).
/// 3. **Digest tiebreak** — a residual collision (same base AND category, or a
///    disambiguated slug that would clash with a reserved base) appends
///    [`short_iri_digest`] of the full IRI; a further clash appends an incrementing
///    suffix. The full slug set (unique bases ∪ resolved) is asserted injective — a
///    HARD FAIL otherwise, never silent conflation.
///
/// Contended terms are processed in IRI-sorted order, so the assignment is a total
/// function of the (unordered) term set: the same terms always yield the same map.
pub fn resolve_term_slugs(terms: &[DocTerm]) -> BTreeMap<String, String> {
    use std::collections::{HashMap, HashSet};

    // Distinct terms by IRI (first occurrence in IRI-sorted order). A term IRI that
    // appears more than once in the list (e.g. lifted by two scans) is ONE doc-entry
    // subject, so it resolves to ONE slug — the injectivity target is distinct IRIs,
    // not list positions.
    let mut order: Vec<&DocTerm> = terms.iter().collect();
    order.sort_by(|a, b| a.iri.cmp(&b.iri));
    let mut seen: HashSet<&str> = HashSet::new();
    let distinct: Vec<&DocTerm> = order
        .into_iter()
        .filter(|t| seen.insert(t.iri.as_str()))
        .collect();

    // Base slug per distinct term IRI + how many distinct terms share each base.
    let base_of: HashMap<&str, String> = distinct
        .iter()
        .map(|t| (t.iri.as_str(), slugify(local_name(&t.iri))))
        .collect();
    let mut base_count: HashMap<&str, usize> = HashMap::new();
    for base in base_of.values() {
        *base_count.entry(base.as_str()).or_default() += 1;
    }

    // Every uncontended base is reserved (kept verbatim, absent from the map).
    let mut used: HashSet<String> = HashSet::new();
    for term in &distinct {
        let base = &base_of[term.iri.as_str()];
        if base_count[base.as_str()] == 1 {
            used.insert(base.clone());
        }
    }

    // Disambiguate the contended terms (already in IRI-sorted order → determinism).
    let mut out: BTreeMap<String, String> = BTreeMap::new();
    for term in &distinct {
        let base = &base_of[term.iri.as_str()];
        if base_count[base.as_str()] == 1 {
            continue;
        }
        let cat = category_slug(term.category);
        let mut cand = format!("{base}-{cat}");
        if used.contains(&cand) {
            cand = format!("{base}-{cat}-{}", short_iri_digest(&term.iri));
        }
        let mut n = 2;
        while used.contains(&cand) {
            cand = format!("{base}-{cat}-{}-{n}", short_iri_digest(&term.iri));
            n += 1;
        }
        used.insert(cand.clone());
        out.insert(term.iri.clone(), cand);
    }

    // Injectivity is the whole point: distinct IRIs → distinct slugs across the
    // WHOLE surface (unique bases ∪ resolved). `used` grew by exactly one per
    // reserved base and per resolved slug, so its size must equal the distinct-IRI
    // count — a HARD FAIL otherwise, never silent conflation.
    assert_eq!(
        used.len(),
        distinct.len(),
        "resolve_term_slugs produced a non-injective slug surface"
    );
    out
}

/// A filesystem-safe slug from a slice IRI's last path segment.
pub fn slice_slug(slice: &DocSlice) -> String {
    slice_slug_of_iri(&slice.iri)
}

/// The slice slug derived directly from a slice IRI — the same slug
/// [`slice_slug`] yields, without needing a materialized [`DocSlice`]. Used by
/// [`crate::model::DocMarkdownDocument`] collection during model build, before the
/// owning `DocSlice` is fully assembled.
pub fn slice_slug_of_iri(iri: &str) -> String {
    slugify(local_name(iri))
}

/// A filesystem-safe slug from a concern IRI's last path segment.
pub fn concern_slug(concern: &DocConcern) -> String {
    slugify(local_name(&concern.iri))
}

/// The local name of an IRI: the tail after the last `/` or `#`.
pub fn local_name(iri: &str) -> &str {
    let cut = iri.rfind(['/', '#']).map(|i| i + 1).unwrap_or(0);
    &iri[cut..]
}

/// Lowercase + collapse to `[a-z0-9-]`, with non-alphanumerics becoming `-`,
/// runs collapsed, and leading/trailing dashes trimmed. Empty input → `unnamed`.
pub fn slugify(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut prev_dash = false;
    for ch in name.chars() {
        let lc = ch.to_ascii_lowercase();
        if lc.is_ascii_alphanumeric() {
            out.push(lc);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed.to_string()
    }
}

/// Short relation tag for an alignment predicate IRI — the local name after the
/// final `#`/`/` (`skos:closeMatch` → `closeMatch`, `owl:equivalentClass` →
/// `equivalentClass`), mirroring the SSSOM-style tags the Python projection used.
pub fn align_tag(predicate: &str) -> String {
    predicate
        .rsplit(['#', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(predicate)
        .to_string()
}

/// The display name for a slice: its title, then label, then IRI local name.
pub fn slice_display(slice: &DocSlice) -> String {
    slice
        .title
        .clone()
        .or_else(|| slice.label.clone())
        .unwrap_or_else(|| local_name(&slice.iri).to_string())
}

/// The display name for a concern: its label, else its IRI local name.
pub fn concern_display(concern: &DocConcern) -> String {
    concern
        .label
        .clone()
        .unwrap_or_else(|| local_name(&concern.iri).to_string())
}

/// The flattened advice facet for a term — its English advisory carriers in a
/// stable field order (scope, use-when, avoid-when, how-to-use). Empty when the
/// term carries no advice. Lets search match on advisory prose, not just label.
pub fn term_advice_facet(term: &DocTerm) -> Vec<String> {
    term.scope_notes
        .iter()
        .chain(term.use_when.iter())
        .chain(term.avoid_when.iter())
        .chain(term.how_to_use.iter())
        .cloned()
        .collect()
}

/// Maps each subject IRI to its sorted+deduped `tag:object` alignment tokens.
/// Borrows the subject IRIs from the model, so it is lifetime-bound to it.
pub type AlignmentFacets<'a> = std::collections::HashMap<&'a str, Vec<String>>;

/// Precompute alignment facets for all terms in one pass: maps each subject IRI
/// to a sorted+deduped `tag:object` token list. Avoids the O(N×M) per-term
/// linear scan of `model.linkages` when rendering the search and llms surfaces.
pub fn precompute_alignment_facets(model: &DocsModel) -> AlignmentFacets<'_> {
    let mut map: std::collections::HashMap<&str, Vec<String>> = std::collections::HashMap::new();
    for l in &model.linkages {
        map.entry(l.subject.as_str()).or_default().push(format!(
            "{}:{}",
            align_tag(&l.predicate),
            local_name(&l.object)
        ));
    }
    for tags in map.values_mut() {
        tags.sort_unstable();
        tags.dedup();
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_is_filesystem_safe() {
        assert_eq!(slugify("HasOwner"), "hasowner");
        assert_eq!(slugify("Cat 9 Lives!"), "cat-9-lives");
        assert_eq!(slugify("--weird--"), "weird");
        assert_eq!(slugify(""), "unnamed");
    }

    #[test]
    fn align_tag_handles_trailing_separators() {
        assert_eq!(
            align_tag("http://www.w3.org/2004/02/skos/core#closeMatch"),
            "closeMatch"
        );
        assert_eq!(
            align_tag("http://www.w3.org/2002/07/owl#equivalentClass"),
            "equivalentClass"
        );
        // trailing separator must not yield an empty tag
        assert_eq!(align_tag("http://example.org/vocab#"), "vocab");
        assert_eq!(align_tag("http://example.org/vocab/"), "vocab");
        // no separator at all -> whole predicate
        assert_eq!(align_tag("bareword"), "bareword");
    }
}
