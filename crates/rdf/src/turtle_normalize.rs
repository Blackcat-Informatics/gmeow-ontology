// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! A canonical, review-friendly Turtle serializer over the **gmeow-rdf IR** (#819 Task 9).
//!
//! Replaces rdflib's `longturtle` as the on-disk normalizer (`gmeow normalize`).
//! Per the #819 thesis, the IR ([`RdfDataset`]) — not oxigraph — is the
//! representation that is read, ordered, and rendered here; the native
//! [`parse_dataset`](crate::parse_dataset) codec (#909) appears only as the text
//! *parser* at the ingest edge. Every triple is interned into the IR verbatim
//! (RDF-star triple terms stay triple-term objects, NOT split into reifier
//! tables — the native parser folds the statement layer, so the ingest flattens it
//! back to the un-folded flat quad stream before re-interning), so the rendered
//! graph is identical to the input.
//!
//! The output is a pure function of the graph and improves on `longturtle`:
//!
//! - **Inline blank nodes** `[ … ]` for once-referenced anonymous nodes (an OWL
//!   restriction reads as one nested block, not a dangling `_:b`).
//! - **RDF collection** `( … )` syntax for well-formed `rdf:List`s.
//! - **`a`-first**, predicates then objects sorted, one object per line so a
//!   single value add/remove is a single-line diff.
//! - **Native literal syntax** where lossless (`xsd:integer`/`decimal`/`double`/
//!   `boolean`, bare `xsd:string`, language tags), `"""…"""` for multi-line.
//! - **Deterministic, idempotent** blank labels: inline where possible; the rare
//!   shared/cyclic blank gets a structural-signature-derived `_:bN` label.

#![cfg(feature = "oxigraph")]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;

use crate::ir::{RdfDataset, RdfDatasetBuilder, TermId, TermRef};
use crate::oxigraph::flat_rdf_quads_from_dataset;
use crate::{parse_dataset, BlankScope, NativeRdfFormat, RdfTerm};

const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

fn rdf(local: &str) -> String {
    format!("{RDF}{local}")
}
fn xsd(local: &str) -> String {
    format!("{XSD}{local}")
}

/// Parse a Turtle document and re-serialize it as canonical, review-friendly
/// Turtle. `extra_prefixes` supplies prefix bindings (the project's standard set);
/// only those actually used appear in the header.
pub fn canonical_turtle(
    input: &[u8],
    extra_prefixes: &[(String, String)],
) -> Result<String, String> {
    let dataset = ingest(input)?;
    Ok(render(&dataset, extra_prefixes))
}

/// Ingest: the native codec parses the Turtle text at the edge into the IR, which
/// is flattened back to the un-folded flat quad stream (RDF-star triple terms
/// preserved as triple-term objects, nothing reclassified) and re-interned verbatim
/// into a fresh builder so the rendered graph is identical to the input.
fn ingest(input: &[u8]) -> Result<Arc<RdfDataset>, String> {
    let parsed = parse_dataset(input, NativeRdfFormat::Turtle.media_type(), None)
        .map_err(|e| format!("Turtle parse error: {e}"))?;
    let mut builder = RdfDatasetBuilder::new();
    for quad in flat_rdf_quads_from_dataset(&parsed) {
        let s = intern_term(&mut builder, &quad.subject)?;
        let p = builder.intern_iri(quad.predicate);
        let o = intern_term(&mut builder, &quad.object)?;
        builder.push_quad(s, p, o, None);
    }
    builder.freeze().map_err(|e| e.to_string())
}

fn intern_term(builder: &mut RdfDatasetBuilder, term: &RdfTerm) -> Result<TermId, String> {
    Ok(match term {
        RdfTerm::Iri(n) => builder.intern_iri(n.clone()),
        RdfTerm::BlankNode(b) => builder.intern_blank(b.clone(), BlankScope::DEFAULT),
        RdfTerm::Literal(l) => builder.intern_literal(l.clone()),
        RdfTerm::Triple(t) => {
            let s = intern_term(builder, &t.subject)?;
            let p = builder.intern_iri(t.predicate.clone());
            let o = intern_term(builder, &t.object)?;
            builder.intern_triple(s, p, o)
        }
    })
}

/// Render a frozen dataset as canonical Turtle.
pub fn render(dataset: &RdfDataset, prefixes: &[(String, String)]) -> String {
    // Longest-namespace-first so the most specific prefix wins on abbreviation.
    let mut prefixes: Vec<(String, String)> = prefixes.to_vec();
    prefixes.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(&b.0)));
    Renderer::new(dataset, prefixes).render()
}

/// A predicate→objects map (both ordered) for one subject.
type Props = BTreeMap<TermId, BTreeSet<ObjKey>>;

struct Renderer<'a> {
    dataset: &'a RdfDataset,
    prefixes: Vec<(String, String)>,
    /// Each subject's properties.
    by_subject: HashMap<TermId, Props>,
    /// Times each blank `TermId` appears as an object.
    object_refs: HashMap<TermId, usize>,
    /// `_:bN` labels for shared/cyclic blanks that cannot inline.
    shared_labels: HashMap<TermId, String>,
    /// Prefixes actually used during rendering.
    used_prefixes: RefCell<BTreeSet<String>>,
    /// The well-known predicate ids, or `None` when the term table has no such
    /// IRI (#837: `None` replaces the former out-of-range `TermId` sentinel — the
    /// `NonZeroU32` niche makes the absent case free).
    rdf_type: Option<TermId>,
    rdf_first: Option<TermId>,
    rdf_rest: Option<TermId>,
    rdf_nil_iri: String,
}

impl<'a> Renderer<'a> {
    fn new(dataset: &'a RdfDataset, prefixes: Vec<(String, String)>) -> Self {
        // Phase 1: collect each subject's predicate→object multiset as raw `TermId`s,
        // and count blank object references. We deliberately do NOT order objects yet:
        // the ordering of blank/triple objects is a pure function of their subtree
        // CONTENT (computed in phase 2), never of `TermId` interning order — that is
        // what makes the render idempotent regardless of how the parser interned terms.
        let mut raw: HashMap<TermId, BTreeMap<TermId, Vec<TermId>>> = HashMap::new();
        let mut object_refs: HashMap<TermId, usize> = HashMap::new();
        for q in dataset.quads() {
            raw.entry(q.s)
                .or_default()
                .entry(q.p)
                .or_default()
                .push(q.o);
            if matches!(dataset.resolve(q.o), TermRef::Blank { .. }) {
                *object_refs.entry(q.o).or_default() += 1;
            }
        }

        // Phase 2: a content-derived ordering key for every blank/triple term, walked
        // recursively over `raw` (the sorted (predicate, object-key) pairs of a blank's
        // properties), bounded against cycles. Grounded objects use their own lexical
        // key, so the result distinguishes sibling restrictions by their actual content
        // (`owl:onProperty`/`owl:someValuesFrom`/…) rather than by interning order.
        let content = ContentKeys::new(dataset, &raw);

        // Materialize the ordered `Props`: grounded objects keep their lexical key,
        // blank/triple objects sort by the content key computed above.
        let by_subject: HashMap<TermId, Props> = raw
            .iter()
            .map(|(&s, preds)| {
                let props: Props = preds
                    .iter()
                    .map(|(&p, objs)| {
                        let set: BTreeSet<ObjKey> = objs
                            .iter()
                            .map(|&o| ObjKey::new(dataset, o, &content))
                            .collect();
                        (p, set)
                    })
                    .collect();
                (s, props)
            })
            .collect();

        // Deterministic labels for blanks that cannot inline (referenced 0 or >1
        // times as an object), ordered by a structural signature so the labeling is
        // idempotent and stable under graph isomorphism for non-symmetric graphs.
        let mut shared: Vec<TermId> = by_subject
            .keys()
            .copied()
            .chain(object_refs.keys().copied())
            .filter(|id| matches!(dataset.resolve(*id), TermRef::Blank { .. }))
            .filter(|id| object_refs.get(id).copied().unwrap_or(0) != 1)
            .collect();
        shared.sort();
        shared.dedup();
        let sigs = blank_signatures(dataset, &by_subject, &shared);
        // Order labels by the structural-signature hash; on a hash tie fall back to the
        // fuller content key (still pure graph content), NEVER to `id.index()`, so the
        // `_:bN` numbering is idempotent under any interning order.
        shared.sort_by_cached_key(|id| (sigs.get(id).copied().unwrap_or(0), content.key_for(*id)));
        let shared_labels = shared
            .into_iter()
            .enumerate()
            .map(|(i, id)| (id, format!("_:b{i}")))
            .collect();

        let mut r = Self {
            dataset,
            prefixes,
            by_subject,
            object_refs,
            shared_labels,
            used_prefixes: RefCell::new(BTreeSet::new()),
            rdf_type: None,
            rdf_first: None,
            rdf_rest: None,
            rdf_nil_iri: rdf("nil"),
        };
        // Resolve the well-known predicate ids by scanning the term table (they may
        // be absent, in which case the sentinel never matches a real predicate).
        r.rdf_type = r.find_iri(&rdf("type"));
        r.rdf_first = r.find_iri(&rdf("first"));
        r.rdf_rest = r.find_iri(&rdf("rest"));
        r
    }

    /// The `TermId` of an interned IRI, or `None` if the term table has no such IRI.
    fn find_iri(&self, iri: &str) -> Option<TermId> {
        for i in 0..self.dataset.term_count() {
            let id = TermId::from_index(i as u32);
            if let TermRef::Iri(v) = self.dataset.resolve(id) {
                if v == iri {
                    return Some(id);
                }
            }
        }
        None
    }

    fn is_inline_bnode(&self, id: TermId) -> bool {
        matches!(self.dataset.resolve(id), TermRef::Blank { .. })
            && self.object_refs.get(&id).copied().unwrap_or(0) == 1
    }

    fn render(&self) -> String {
        let mut tops: Vec<TermId> = self
            .by_subject
            .keys()
            .copied()
            .filter(|id| !self.is_inline_bnode(*id))
            .collect();
        tops.sort_by_cached_key(|id| self.subject_sort_key(*id));

        let mut body = String::new();
        for (i, subj) in tops.iter().enumerate() {
            if i > 0 {
                body.push('\n');
            }
            body.push_str(&self.term_label(*subj));
            body.push('\n');
            self.render_props(*subj, 1, &mut body, true);
        }

        let used = self.used_prefixes.borrow();
        let mut header = String::new();
        for (p, ns) in &self.prefixes {
            if used.contains(p) {
                header.push_str(&format!("@prefix {p}: <{ns}> .\n"));
            }
        }
        if header.is_empty() {
            body
        } else {
            format!("{header}\n{body}")
        }
    }

    fn render_props(&self, subj: TermId, depth: usize, out: &mut String, top: bool) {
        let indent = "    ".repeat(depth);
        let Some(props) = self.by_subject.get(&subj) else {
            return;
        };
        let mut preds: Vec<TermId> = props.keys().copied().collect();
        preds.sort_by_cached_key(|p| (Some(*p) != self.rdf_type, self.iri_of(*p)));

        let last_pred = preds.len().saturating_sub(1);
        for (pi, pred) in preds.iter().enumerate() {
            let objs: Vec<&ObjKey> = props[pred].iter().collect();
            let pred_str = if Some(*pred) == self.rdf_type {
                "a".to_string()
            } else {
                self.term_label(*pred)
            };
            let last_obj = objs.len().saturating_sub(1);
            for (oi, obj) in objs.iter().enumerate() {
                // First object sits on the predicate line (indent `depth`);
                // continuation objects sit one level deeper, so a nested `[ … ]`
                // closes in alignment with its own opening line.
                let obj_depth = if oi == 0 { depth } else { depth + 1 };
                let rendered = self.render_object(obj.id, obj_depth);
                let terminator = if pi == last_pred && oi == last_obj {
                    if top {
                        " ."
                    } else {
                        " ;"
                    }
                } else if oi == last_obj {
                    " ;"
                } else {
                    " ,"
                };
                if oi == 0 {
                    out.push_str(&format!("{indent}{pred_str} {rendered}{terminator}\n"));
                } else {
                    out.push_str(&format!("{indent}    {rendered}{terminator}\n"));
                }
            }
        }
    }

    fn render_object(&self, id: TermId, depth: usize) -> String {
        match self.dataset.resolve(id) {
            TermRef::Iri(iri) => self.iri(iri),
            TermRef::Literal {
                lexical,
                datatype,
                language,
                direction,
            } => self.literal(lexical, datatype, language, direction),
            TermRef::Blank { .. } => {
                if self.is_inline_bnode(id) {
                    if let Some(list) = self.try_collection(id) {
                        return self.render_collection(&list, depth);
                    }
                    self.render_inline_bnode(id, depth)
                } else {
                    self.shared_labels
                        .get(&id)
                        .cloned()
                        .unwrap_or_else(|| "[]".to_string())
                }
            }
            TermRef::Triple { s, p, o } => {
                // RDF-1.2 quoted triple: `<< s p o >>`.
                format!(
                    "<< {} {} {} >>",
                    self.render_object(s, depth),
                    self.term_label(p),
                    self.render_object(o, depth)
                )
            }
        }
    }

    fn render_inline_bnode(&self, id: TermId, depth: usize) -> String {
        if self
            .by_subject
            .get(&id)
            .map(BTreeMap::is_empty)
            .unwrap_or(true)
        {
            return "[]".to_string();
        }
        let mut inner = String::new();
        self.render_props(id, depth + 1, &mut inner, false);
        let close_indent = "    ".repeat(depth);
        format!("[\n{inner}{close_indent}]")
    }

    /// A well-formed `rdf:List` headed by `id`: a chain of inline blanks each with
    /// exactly `rdf:first` + `rdf:rest`, ending in `rdf:nil`. Returns the elements.
    fn try_collection(&self, id: TermId) -> Option<Vec<TermId>> {
        // No `rdf:first`/`rdf:rest` IRI in the table ⇒ no list can exist (#837: the
        // niche `None` replaces the former out-of-range-sentinel check).
        let (rdf_first, rdf_rest) = (self.rdf_first?, self.rdf_rest?);
        let mut items = Vec::new();
        let mut cur = id;
        let mut seen = BTreeSet::new();
        loop {
            if !seen.insert(cur) {
                return None;
            }
            let props = self.by_subject.get(&cur)?;
            if props.len() != 2 || !props.contains_key(&rdf_first) || !props.contains_key(&rdf_rest)
            {
                return None;
            }
            let firsts = &props[&rdf_first];
            let rests = &props[&rdf_rest];
            if firsts.len() != 1 || rests.len() != 1 {
                return None;
            }
            items.push(firsts.iter().next().unwrap().id);
            let rest = rests.iter().next().unwrap().id;
            match self.dataset.resolve(rest) {
                TermRef::Iri(iri) if iri == self.rdf_nil_iri => return Some(items),
                TermRef::Blank { .. } if self.is_inline_bnode(rest) => cur = rest,
                _ => return None,
            }
        }
    }

    fn render_collection(&self, items: &[TermId], depth: usize) -> String {
        if items.is_empty() {
            return "()".to_string();
        }
        let rendered: Vec<String> = items
            .iter()
            .map(|t| self.render_object(*t, depth))
            .collect();
        format!("( {} )", rendered.join(" "))
    }

    // ── term formatting ──────────────────────────────────────────────────────

    /// The IRI string of an interned predicate/IRI term.
    fn iri_of(&self, id: TermId) -> String {
        match self.dataset.resolve(id) {
            TermRef::Iri(iri) => iri.to_owned(),
            _ => String::new(),
        }
    }

    /// A subject/predicate term's label (abbreviated IRI or shared blank label).
    fn term_label(&self, id: TermId) -> String {
        match self.dataset.resolve(id) {
            TermRef::Iri(iri) => self.iri(iri),
            TermRef::Blank { .. } => self
                .shared_labels
                .get(&id)
                .cloned()
                .unwrap_or_else(|| "[]".to_string()),
            _ => "[]".to_string(),
        }
    }

    fn iri(&self, iri: &str) -> String {
        for (prefix, ns) in &self.prefixes {
            if let Some(local) = iri.strip_prefix(ns.as_str()) {
                if !local.is_empty() && local.chars().all(is_pn_local_char) {
                    self.used_prefixes.borrow_mut().insert(prefix.clone());
                    return format!("{prefix}:{local}");
                }
            }
        }
        format!("<{}>", escape_iri(iri))
    }

    fn literal(
        &self,
        lexical: &str,
        datatype: TermId,
        language: Option<&str>,
        direction: Option<crate::RdfTextDirection>,
    ) -> String {
        if let Some(lang) = language {
            // RDF 1.2 base direction renders as `"text"@lang--ltr` / `--rtl`; a base
            // direction requires a language tag, so it only appears on this branch.
            return match direction {
                Some(dir) => format!("{}@{}--{}", quote(lexical), lang, dir.as_str()),
                None => format!("{}@{}", quote(lexical), lang),
            };
        }
        let dt = self.iri_of(datatype);
        if dt == xsd("string") {
            return quote(lexical);
        }
        if dt == xsd("boolean") && (lexical == "true" || lexical == "false") {
            return lexical.to_owned();
        }
        if dt == xsd("integer") && is_turtle_integer(lexical) {
            return lexical.to_owned();
        }
        if dt == xsd("decimal") && is_turtle_decimal(lexical) {
            return lexical.to_owned();
        }
        if dt == xsd("double") && is_turtle_double(lexical) {
            return lexical.to_owned();
        }
        format!("{}^^{}", quote(lexical), self.iri(&dt))
    }

    fn subject_sort_key(&self, id: TermId) -> (u8, String) {
        match self.dataset.resolve(id) {
            TermRef::Iri(iri) => (0, self.abbrev_for_sort(iri)),
            TermRef::Blank { .. } => (1, self.shared_labels.get(&id).cloned().unwrap_or_default()),
            _ => (2, String::new()),
        }
    }

    /// Abbreviation used only for ORDERING (does not record prefix usage).
    fn abbrev_for_sort(&self, iri: &str) -> String {
        for (prefix, ns) in &self.prefixes {
            if let Some(local) = iri.strip_prefix(ns.as_str()) {
                if !local.is_empty() && local.chars().all(is_pn_local_char) {
                    return format!("{prefix}:{local}");
                }
            }
        }
        iri.to_owned()
    }
}

/// Content-derived ordering keys for every blank/triple term in the graph.
///
/// The key for a blank is a canonical string built from the sorted
/// `(predicate-iri, object-key)` pairs of its properties; the key for a triple term
/// is built from its `s`/`p`/`o` component keys. Both recurse through nested
/// blank/triple objects so the key is a pure function of the term's subtree CONTENT —
/// independent of `TermId` interning order — which is what makes the render idempotent.
/// Recursion is bounded by a `seen` set so cyclic blank graphs terminate (a back-edge
/// to an in-progress node renders as a fixed `^` marker), and a depth budget caps
/// pathological chains; ties under the budget are harmless because they only affect
/// sort order between structurally indistinguishable subtrees.
struct ContentKeys {
    keys: HashMap<TermId, String>,
}

impl ContentKeys {
    const MAX_DEPTH: usize = 40;

    fn new(dataset: &RdfDataset, raw: &HashMap<TermId, BTreeMap<TermId, Vec<TermId>>>) -> Self {
        // `keys` doubles as the memoization cache: every acyclic blank/triple term
        // `compute_content_key` fully resolves is inserted, so a single traversal from
        // the top-level subjects/objects populates keys for ALL reachable nested
        // blank/triple terms (not just `q.s`/`q.o`) and never recomputes a shared
        // subtree. Cyclic / depth-capped subtrees are deliberately left out (see the
        // `cacheable` flag in `compute_content_key`).
        let mut keys: HashMap<TermId, String> = HashMap::new();
        for q in dataset.quads() {
            for term in [q.s, q.o] {
                if matches!(
                    dataset.resolve(term),
                    TermRef::Blank { .. } | TermRef::Triple { .. }
                ) && !keys.contains_key(&term)
                {
                    let mut seen = BTreeSet::new();
                    compute_content_key(dataset, raw, term, &mut seen, 0, &mut keys);
                }
            }
        }
        Self { keys }
    }

    /// The content key of a blank/triple term (empty for grounded terms, which never
    /// consult this map).
    fn key_for(&self, id: TermId) -> String {
        self.keys.get(&id).cloned().unwrap_or_default()
    }
}

/// Recursively fold a term into a canonical content string. Grounded terms map to
/// their lexical form; blank/triple terms descend into their subtree.
///
/// Returns `(key, cacheable)`. `cacheable` is `false` iff the subtree hit a back-edge
/// or the depth budget — those `^` markers are entry-point-RELATIVE, so an enclosing
/// key that embeds one must not be memoized (it would otherwise return an
/// interning-order-dependent value when the same node is later reached from a different
/// root, reintroducing the very non-determinism this fold exists to remove). Only fully
/// resolved acyclic subtrees are written to `cache`; cyclic / over-budget blank graphs
/// fall through to the `id.index()` tiebreak in [`ObjKey`], exactly as before this fix.
fn compute_content_key(
    dataset: &RdfDataset,
    raw: &HashMap<TermId, BTreeMap<TermId, Vec<TermId>>>,
    id: TermId,
    seen: &mut BTreeSet<TermId>,
    depth: usize,
    cache: &mut HashMap<TermId, String>,
) -> (String, bool) {
    // A memoized key is always a fully-resolved acyclic blank/triple key (leaf terms are
    // never cached), so reusing it is sound and cannot be a live back-edge: a node is
    // only inserted AFTER `seen.remove`, hence a cached id is never simultaneously in
    // `seen`.
    if let Some(k) = cache.get(&id) {
        return (k.clone(), true);
    }
    match dataset.resolve(id) {
        TermRef::Iri(iri) => (format!("I{iri}"), true),
        TermRef::Literal {
            lexical,
            datatype,
            language,
            ..
        } => {
            let dt = match dataset.resolve(datatype) {
                TermRef::Iri(iri) => iri,
                _ => "",
            };
            (
                format!("L{dt}\u{1}{}\u{1}{lexical}", language.unwrap_or("")),
                true,
            )
        }
        TermRef::Blank { .. } => {
            if depth >= ContentKeys::MAX_DEPTH || !seen.insert(id) {
                // Back-edge or budget exhausted: a stable marker keeps the fold finite,
                // but the enclosing key is NOT safe to memoize.
                return ("^".to_string(), false);
            }
            let mut cacheable = true;
            let mut parts: Vec<String> = Vec::new();
            if let Some(props) = raw.get(&id) {
                for (&pred, objs) in props {
                    let pk = match dataset.resolve(pred) {
                        TermRef::Iri(iri) => iri,
                        _ => "",
                    };
                    // Sort the per-predicate object keys so the fold is order-free.
                    let mut oks: Vec<String> = objs
                        .iter()
                        .map(|&o| {
                            let (k, c) =
                                compute_content_key(dataset, raw, o, seen, depth + 1, cache);
                            cacheable &= c;
                            k
                        })
                        .collect();
                    oks.sort();
                    parts.push(format!("{pk}\u{2}{}", oks.join("\u{3}")));
                }
            }
            parts.sort();
            seen.remove(&id);
            let key = format!("B[{}]", parts.join("\u{4}"));
            if cacheable {
                cache.insert(id, key.clone());
            }
            (key, cacheable)
        }
        TermRef::Triple { s, p, o } => {
            let (sk, cs) = compute_content_key(dataset, raw, s, seen, depth + 1, cache);
            let (pk, cp) = compute_content_key(dataset, raw, p, seen, depth + 1, cache);
            let (ok, co) = compute_content_key(dataset, raw, o, seen, depth + 1, cache);
            let cacheable = cs && cp && co;
            let key = format!("T<{sk}\u{1}{pk}\u{1}{ok}>");
            if cacheable {
                cache.insert(id, key.clone());
            }
            (key, cacheable)
        }
    }
}

/// An object term keyed for deterministic sorting, carrying its `TermId`.
#[derive(Clone)]
struct ObjKey {
    id: TermId,
    key: (u8, String),
}

impl ObjKey {
    fn new(dataset: &RdfDataset, id: TermId, content: &ContentKeys) -> Self {
        let key = match dataset.resolve(id) {
            TermRef::Iri(iri) => (0, iri.to_owned()),
            TermRef::Literal {
                lexical,
                datatype,
                language,
                ..
            } => {
                let dt = match dataset.resolve(datatype) {
                    TermRef::Iri(iri) => iri.to_owned(),
                    _ => String::new(),
                };
                (
                    1,
                    format!("{dt}\u{1}{}\u{1}{lexical}", language.unwrap_or("")),
                )
            }
            // Blanks/triples sort after grounded terms, ordered by a CONTENT-derived
            // key (the recursive structural signature of their subtree) so sibling
            // inline blocks order by what they say, idempotently under any interning
            // order — never by `id.index()`.
            TermRef::Blank { .. } => (2, content.key_for(id)),
            TermRef::Triple { .. } => (3, content.key_for(id)),
        };
        Self { id, key }
    }
}

impl PartialEq for ObjKey {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for ObjKey {}
impl PartialOrd for ObjKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for ObjKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key
            .cmp(&other.key)
            .then(self.id.index().cmp(&other.id.index()))
    }
}

/// A deterministic structural signature for each shared blank, used only to ORDER
/// their `_:bN` labels. A bounded refinement (two rounds): round 0 folds each
/// blank's grounded neighbourhood; round 1 folds neighbour signatures. Sufficient
/// for the non-symmetric blank graphs the authored ontology sources contain.
fn blank_signatures(
    dataset: &RdfDataset,
    by_subject: &HashMap<TermId, Props>,
    shared: &[TermId],
) -> HashMap<TermId, u64> {
    let ground = |id: TermId| -> u64 {
        match dataset.resolve(id) {
            TermRef::Iri(iri) => fnv(0xcbf2_9ce4_8422_2325, iri.as_bytes()),
            TermRef::Literal { lexical, .. } => fnv(0x1000_0001, lexical.as_bytes()),
            _ => 0, // blank/triple: not grounded
        }
    };
    let shared_set: BTreeSet<TermId> = shared.iter().copied().collect();
    let mut sig: HashMap<TermId, u64> = shared.iter().map(|&b| (b, 1)).collect();
    for round in 0..2 {
        let mut next = sig.clone();
        for &b in shared {
            let mut acc = round as u64 + 1;
            if let Some(props) = by_subject.get(&b) {
                for (pred, objs) in props {
                    let pg = ground(*pred);
                    for obj in objs {
                        let og = if shared_set.contains(&obj.id) {
                            sig.get(&obj.id).copied().unwrap_or(1)
                        } else {
                            ground(obj.id)
                        };
                        // Commutative fold across statements.
                        acc ^= pg.wrapping_mul(0x100_0000_01b3) ^ og.rotate_left(17);
                    }
                }
            }
            next.insert(b, acc);
        }
        sig = next;
    }
    sig
}

fn fnv(mut hash: u64, bytes: &[u8]) -> u64 {
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

// ── lexical helpers ──────────────────────────────────────────────────────────

fn is_pn_local_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '~')
}

fn escape_iri(iri: &str) -> String {
    let mut out = String::with_capacity(iri.len());
    for c in iri.chars() {
        match c {
            '<' | '>' | '"' | '{' | '}' | '|' | '^' | '`' | '\\' => {
                out.push_str(&format!("\\u{:04X}", c as u32));
            }
            c if (c as u32) <= 0x20 => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn quote(value: &str) -> String {
    if value.contains('\n') {
        let escaped = value
            .replace('\\', "\\\\")
            .replace("\"\"\"", "\\\"\\\"\\\"");
        format!("\"\"\"{escaped}\"\"\"")
    } else {
        let mut out = String::with_capacity(value.len() + 2);
        out.push('"');
        for c in value.chars() {
            match c {
                '"' => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\t' => out.push_str("\\t"),
                '\r' => out.push_str("\\r"),
                c => out.push(c),
            }
        }
        out.push('"');
        out
    }
}

fn is_turtle_integer(v: &str) -> bool {
    let s = v.strip_prefix(['+', '-']).unwrap_or(v);
    !s.is_empty()
        && s.bytes().all(|b| b.is_ascii_digit())
        && (s.len() == 1 || s.as_bytes()[0] != b'0')
}

fn is_turtle_decimal(v: &str) -> bool {
    let s = v.strip_prefix(['+', '-']).unwrap_or(v);
    match s.split_once('.') {
        Some((a, b)) => {
            !b.is_empty()
                && a.bytes().all(|c| c.is_ascii_digit())
                && b.bytes().all(|c| c.is_ascii_digit())
        }
        None => false,
    }
}

fn is_turtle_double(v: &str) -> bool {
    let lower = v.to_ascii_lowercase();
    lower.contains('e') && lower.parse::<f64>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefixes() -> Vec<(String, String)> {
        vec![
            ("rdf".into(), RDF.into()),
            (
                "rdfs".into(),
                "http://www.w3.org/2000/01/rdf-schema#".into(),
            ),
            ("owl".into(), "http://www.w3.org/2002/07/owl#".into()),
            ("xsd".into(), XSD.into()),
            ("ex".into(), "http://example.org/".into()),
        ]
    }

    fn norm(ttl: &str) -> String {
        canonical_turtle(ttl.as_bytes(), &prefixes()).expect("normalize")
    }

    fn iso(a: &str, b: &str) -> bool {
        let da = ingest(a.as_bytes()).unwrap();
        let db = ingest(b.as_bytes()).unwrap();
        crate::ir::datasets_isomorphic(&da, &db)
    }

    #[test]
    fn isomorphism_preserved_and_idempotent() {
        let src = r#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
            ex:A a owl:Class ;
                rdfs:label "A" ;
                rdfs:subClassOf [ a owl:Restriction ;
                    owl:onProperty ex:p ; owl:someValuesFrom ex:B ] .
        "#;
        let once = norm(src);
        assert!(iso(src, &once), "isomorphic to input:\n{once}");
        let twice = norm(&once);
        assert_eq!(once, twice, "idempotent");
        assert!(
            once.contains("rdfs:subClassOf [\n"),
            "inline bnode:\n{once}"
        );
        assert!(
            once.contains("        a owl:Restriction ;"),
            "a-first nested:\n{once}"
        );
    }

    #[test]
    fn rdf_collection_renders_as_parens() {
        let src = r#"
            @prefix ex: <http://example.org/> .
            @prefix owl: <http://www.w3.org/2002/07/owl#> .
            ex:U owl:unionOf ( ex:A ex:B ex:C ) .
        "#;
        let out = norm(src);
        assert!(out.contains("owl:unionOf ( ex:A ex:B ex:C )"), "{out}");
        assert!(iso(src, &out));
    }

    #[test]
    fn literals_use_native_syntax() {
        let src = r#"
            @prefix ex: <http://example.org/> .
            @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
            ex:s ex:i 42 ; ex:d "1.5"^^xsd:decimal ; ex:b true ;
                 ex:plain "hi" ; ex:typed "hi"^^xsd:string ; ex:lang "bonjour"@fr .
        "#;
        let out = norm(src);
        assert!(out.contains("ex:i 42 "), "{out}");
        assert!(out.contains("ex:d 1.5 "), "{out}");
        assert!(out.contains("ex:b true "), "{out}");
        assert!(out.contains("ex:plain \"hi\" "), "{out}");
        assert!(out.contains("ex:typed \"hi\" "), "{out}");
        assert!(out.contains("ex:lang \"bonjour\"@fr "), "{out}");
        assert!(iso(src, &out));
    }

    #[test]
    fn directional_literal_round_trips() {
        // RDF 1.2 base direction must survive normalize: render `@lang--dir` and stay
        // isomorphic to the input (oxigraph's Turtle parser round-trips the `--dir`
        // form at ingest, so the isomorphism gate holds).
        let src = r#"
            @prefix ex: <http://example.org/> .
            ex:s ex:rtl "مرحبا"@ar--rtl ;
                 ex:ltr "hello"@en--ltr .
        "#;
        let out = norm(src);
        assert!(
            out.contains("\"مرحبا\"@ar--rtl"),
            "rtl direction rendered:\n{out}"
        );
        assert!(
            out.contains("\"hello\"@en--ltr"),
            "ltr direction rendered:\n{out}"
        );
        assert!(iso(src, &out), "directional literal preserved:\n{out}");
        // Idempotent: re-normalizing the output is byte-identical.
        assert_eq!(out, norm(&out), "idempotent");
    }

    #[test]
    fn only_used_prefixes_in_header() {
        let src = "@prefix ex: <http://example.org/> .\nex:a ex:p ex:o .\n";
        let out = norm(src);
        assert!(out.starts_with("@prefix ex:"), "{out}");
        assert!(!out.contains("owl:"), "unused prefixes omitted:\n{out}");
    }

    #[test]
    fn shared_blank_gets_stable_label() {
        // A blank referenced by two subjects cannot inline; it gets a _:bN label,
        // and re-normalizing is idempotent.
        let src = r#"
            @prefix ex: <http://example.org/> .
            ex:A ex:p _:x .
            ex:B ex:q _:x .
            _:x ex:v "shared" .
        "#;
        let out = norm(src);
        assert!(out.contains("_:b0"), "shared blank labeled:\n{out}");
        assert_eq!(out, norm(&out), "idempotent with shared blank");
        assert!(iso(src, &out));
    }

    #[test]
    fn nested_sibling_blanks_order_by_deep_content() {
        // Two inline blank siblings under the SAME predicate that are identical except
        // for a value buried two levels down. Their order must be decided by that deep
        // content — NOT by blank-node interning order — so presenting the siblings in
        // either source order normalizes to byte-identical output. Before the
        // ContentKeys cache threaded nested terms through `compute_content_key`, the
        // inner `ex:child` blanks were absent from the key map, so `key_for` returned
        // "" for them and the sort fell back to `id.index()` (interning order),
        // breaking this property.
        let a = r#"
            @prefix ex: <http://example.org/> .
            ex:S ex:p
                [ ex:tag "same" ; ex:child [ ex:leaf "alpha" ] ] ,
                [ ex:tag "same" ; ex:child [ ex:leaf "beta" ] ] .
        "#;
        // Same graph, siblings written in the opposite source order.
        let b = r#"
            @prefix ex: <http://example.org/> .
            ex:S ex:p
                [ ex:tag "same" ; ex:child [ ex:leaf "beta" ] ] ,
                [ ex:tag "same" ; ex:child [ ex:leaf "alpha" ] ] .
        "#;
        let na = norm(a);
        let nb = norm(b);
        assert_eq!(
            na, nb,
            "source order must not affect output:\n{na}\n---\n{nb}"
        );
        assert_eq!(na, norm(&na), "idempotent");
        assert!(iso(a, &na), "isomorphic to input:\n{na}");
        // The deep value that breaks the tie must appear before its sibling's.
        let pos_alpha = na.find("alpha").expect("alpha present");
        let pos_beta = na.find("beta").expect("beta present");
        assert!(
            pos_alpha < pos_beta,
            "deep content orders the siblings:\n{na}"
        );
    }
}
