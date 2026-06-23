// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `export` export leaf (#861 P4): CSV/CSVW/Markdown/JSONL/llms.txt + the
//! dataset/semantic-web tiers (N-Quads, TriG, statements JSONL, SKOS, OBO Graphs,
//! ShEx) under git-ignored `dist/`.
//!
//! A genuine Rust port of `src/gmeow_tools/export.py` (#377, #12): reads ONLY the
//! committed GTS snapshot (the narrow waist #267) through a fold view that mirrors
//! `gmeow_tools.gts_views.FoldView`, collects every class/property/individual as a
//! [`Term`], then renders the flattened views. Outputs live under git-ignored
//! `dist/`, so there is NO committed byte-parity gate — the bar is
//! structurally-valid, deterministic, non-empty output faithful to the Python
//! generator's format. Everything is sorted (BTreeMap/BTreeSet) for determinism.
//!
//! The lossless N-Quads / TriG forms delegate to the gmeow-gts Rust serializers
//! (`gmeow_gts::nquads::to_nquads` / `gmeow_gts::trig::to_trig`), with internal
//! `x-gmeow-*` language tags remapped to public BCP-47 at the projection boundary
//! (#287) exactly as the Python `write_nquads` / `write_trig` do.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_gts::model::{Graph, Term as GtsTerm, TermKind};

use crate::error::PipelineError;
use crate::node::{Stage, StageInput, StageKind, StageOutput, StageProduct};

include!("lpg_prefixes.rs");

pub const DIST_DIR: &str = "dist";

const OWL: &str = "http://www.w3.org/2002/07/owl#";
const RDFS: &str = "http://www.w3.org/2000/01/rdf-schema#";
const SKOS: &str = "http://www.w3.org/2004/02/skos/core#";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
const ONTOLOGY_IRI: &str = "https://blackcatinformatics.ca/gmeow";
const ALIGNMENTS_GRAPH: &str = "https://blackcatinformatics.ca/gmeow/graph/alignments";

const RDF_FIRST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#first";
const RDF_REST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#rest";
const RDF_NIL: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#nil";

const LANGUAGE_CLASS: &str = "https://blackcatinformatics.ca/gmeow/Language";
const LANGUAGE_TAG: &str = "https://blackcatinformatics.ca/gmeow/languageTag";
const BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";

// ── small language-tag helpers (mirror gmeow_validate / language_tags.py) ──────

/// `^x-gmeow-[a-z0-9\-]+$` (case-insensitive) — the GMEOW internal private-use tag.
fn is_internal_tag(lang: &str) -> bool {
    let lower = lang.to_ascii_lowercase();
    let Some(suffix) = lower.strip_prefix("x-gmeow-") else {
        return false;
    };
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// The shared language-preference sort key: the carrier language wins, then tags
/// lexicographically. Mirrors `language_tags.rank_language`.
fn rank_language(lang: Option<&str>) -> (u8, String) {
    let lower = lang.unwrap_or("").to_ascii_lowercase();
    let rank = if lower == "x-gmeow-english" { 0 } else { 1 };
    (rank, lower)
}

// ── literal bucketing ──────────────────────────────────────────────────────────

/// One bucketed literal candidate: `(retagged_text, public_bcp47, original_lang)`.
type LitRow = (String, Option<String>, String);

/// The deterministic-first NON-empty-keyed bucket's head row, ranked by language
/// (carrier language wins). Mirrors the `tagged = sorted(...); min(rank)` fallback.
fn best_tagged(by_bcp: &BTreeMap<String, Vec<LitRow>>) -> Option<(&str, &LitRow)> {
    by_bcp
        .iter()
        .filter(|(k, _)| !k.is_empty())
        .map(|(k, v)| (k.as_str(), &v[0]))
        .min_by(|a, b| rank_language(Some(a.0)).cmp(&rank_language(Some(b.0))))
}

// ── curie ──────────────────────────────────────────────────────────────────────

fn curie(iri: &str) -> String {
    for (prefix, ns) in PREFIXES_BY_LEN.iter() {
        if let Some(rest) = iri.strip_prefix(ns) {
            return format!("{prefix}:{rest}");
        }
    }
    iri.to_string()
}

// ── FoldView: read-side idioms over a folded gts Graph (mirror gts_views.py) ───

struct FoldView<'a> {
    graph: &'a Graph,
    iri_index: BTreeMap<&'a str, usize>,
    /// scope (graph IRI or "" for default) → subject tid → [(p, o)]
    spo: BTreeMap<String, BTreeMap<usize, Vec<(usize, usize)>>>,
    /// scope → (p, o) → [subject]
    po: BTreeMap<String, BTreeMap<(usize, usize), Vec<usize>>>,
    tag_map: BTreeMap<String, String>,
}

const DEFAULT_SCOPE: &str = "";
const ALL_SCOPE: &str = "__all__";

impl<'a> FoldView<'a> {
    fn new(graph: &'a Graph) -> Self {
        let mut iri_index: BTreeMap<&'a str, usize> = BTreeMap::new();
        for (tid, t) in graph.terms.iter().enumerate() {
            if t.kind == TermKind::Iri {
                if let Some(v) = &t.value {
                    iri_index.entry(v.as_str()).or_insert(tid);
                }
            }
        }
        let mut view = FoldView {
            graph,
            iri_index,
            spo: BTreeMap::new(),
            po: BTreeMap::new(),
            tag_map: BTreeMap::new(),
        };
        view.build_indexes();
        view.tag_map = view.build_tag_map();
        view
    }

    fn build_indexes(&mut self) {
        // Per-scope spo/po, plus an ALL scope spanning every graph.
        let mut spo: BTreeMap<String, BTreeMap<usize, Vec<(usize, usize)>>> = BTreeMap::new();
        let mut po: BTreeMap<String, BTreeMap<(usize, usize), Vec<usize>>> = BTreeMap::new();
        for &(s, p, o, g) in &self.graph.quads {
            let scope = match g {
                None => DEFAULT_SCOPE.to_string(),
                Some(gid) => self.graph.terms[gid].value.clone().unwrap_or_default(),
            };
            for key in [scope.clone(), ALL_SCOPE.to_string()] {
                spo.entry(key.clone())
                    .or_default()
                    .entry(s)
                    .or_default()
                    .push((p, o));
                po.entry(key)
                    .or_default()
                    .entry((p, o))
                    .or_default()
                    .push(s);
            }
        }
        self.spo = spo;
        self.po = po;
    }

    fn term(&self, tid: usize) -> &GtsTerm {
        &self.graph.terms[tid]
    }
    fn is_iri(&self, tid: usize) -> bool {
        self.term(tid).kind == TermKind::Iri
    }
    fn is_bnode(&self, tid: usize) -> bool {
        self.term(tid).kind == TermKind::Bnode
    }
    fn is_literal(&self, tid: usize) -> bool {
        self.term(tid).kind == TermKind::Literal
    }
    fn lex(&self, tid: usize) -> &str {
        self.term(tid).value.as_deref().unwrap_or("")
    }
    fn lang(&self, tid: usize) -> Option<&str> {
        self.term(tid).lang.as_deref()
    }
    fn datatype(&self, tid: usize) -> String {
        self.graph.datatype_iri(self.term(tid))
    }
    fn tid_of_iri(&self, iri: &str) -> Option<usize> {
        self.iri_index.get(iri).copied()
    }

    /// Subjects with `rdf:type <class_iri>` in scope, id-sorted unique.
    fn subjects_by_type(&self, class_iri: &str, scope: &str) -> Vec<usize> {
        let (Some(type_tid), Some(class_tid)) =
            (self.tid_of_iri(RDF_TYPE), self.tid_of_iri(class_iri))
        else {
            return Vec::new();
        };
        let mut out: BTreeSet<usize> = BTreeSet::new();
        if let Some(idx) = self.po.get(scope) {
            if let Some(subjects) = idx.get(&(type_tid, class_tid)) {
                out.extend(subjects.iter().copied());
            }
        }
        out.into_iter().collect()
    }

    /// Objects of `(s, p, ?)` in scope, id-sorted unique.
    fn objects(&self, s_tid: usize, p_iri: &str, scope: &str) -> Vec<usize> {
        let Some(p_tid) = self.tid_of_iri(p_iri) else {
            return Vec::new();
        };
        let mut out: BTreeSet<usize> = BTreeSet::new();
        if let Some(idx) = self.spo.get(scope) {
            if let Some(rows) = idx.get(&s_tid) {
                for &(p, o) in rows {
                    if p == p_tid {
                        out.insert(o);
                    }
                }
            }
        }
        out.into_iter().collect()
    }

    /// One object of `(s, p, ?)` — the nq-token-smallest (never graph order).
    fn value(&self, s_tid: usize, p_iri: &str, scope: &str) -> Option<usize> {
        let candidates = self.objects(s_tid, p_iri, scope);
        candidates
            .into_iter()
            .min_by(|&a, &b| self.nq_token(a).cmp(&self.nq_token(b)))
    }

    /// All `(p, o)` pairs for a subject in scope, id-sorted unique.
    fn predicate_objects(&self, s_tid: usize, scope: &str) -> Vec<(usize, usize)> {
        let mut out: BTreeSet<(usize, usize)> = BTreeSet::new();
        if let Some(idx) = self.spo.get(scope) {
            if let Some(rows) = idx.get(&s_tid) {
                out.extend(rows.iter().copied());
            }
        }
        out.into_iter().collect()
    }

    fn has(&self, s_tid: usize, p_iri: &str, o_tid: usize, scope: &str) -> bool {
        let Some(p_tid) = self.tid_of_iri(p_iri) else {
            return false;
        };
        self.spo
            .get(scope)
            .and_then(|idx| idx.get(&s_tid))
            .map(|rows| rows.contains(&(p_tid, o_tid)))
            .unwrap_or(false)
    }

    fn rdf_list(&self, head_tid: usize, scope: &str) -> Vec<usize> {
        let nil = self.tid_of_iri(RDF_NIL);
        let mut out: Vec<usize> = Vec::new();
        let mut seen: BTreeSet<usize> = BTreeSet::new();
        let mut current = Some(head_tid);
        while let Some(c) = current {
            if Some(c) == nil || seen.contains(&c) {
                break;
            }
            seen.insert(c);
            if let Some(first) = self.value(c, RDF_FIRST, scope) {
                out.push(first);
            }
            current = self.value(c, RDF_REST, scope);
        }
        out
    }

    /// The canonical N-Triples token — stable sort/display key (mirror term_token).
    fn nq_token(&self, tid: usize) -> String {
        let t = self.term(tid);
        match t.kind {
            TermKind::Iri => format!("<{}>", self.lex(tid)),
            TermKind::Bnode => format!("_:{}", self.lex(tid)),
            TermKind::Literal => {
                let lex = nt_escape(self.lex(tid));
                if let Some(lang) = &t.lang {
                    format!("\"{lex}\"@{lang}")
                } else {
                    let dt = self.datatype(tid);
                    if dt == format!("{XSD}string") {
                        format!("\"{lex}\"")
                    } else {
                        format!("\"{lex}\"^^<{dt}>")
                    }
                }
            }
            TermKind::Triple => format!("<<tid:{tid}>>"),
        }
    }

    fn tag_map(&self) -> &BTreeMap<String, String> {
        &self.tag_map
    }

    fn build_tag_map(&self) -> BTreeMap<String, String> {
        let mut out: BTreeMap<String, String> = BTreeMap::new();
        for lang_tid in self.subjects_by_type(LANGUAGE_CLASS, ALL_SCOPE) {
            let internal = self.value(lang_tid, LANGUAGE_TAG, ALL_SCOPE);
            let bcp = self.value(lang_tid, BCP47_TAG, ALL_SCOPE);
            if let (Some(i), Some(b)) = (internal, bcp) {
                out.insert(self.lex(i).to_string(), self.lex(b).to_string());
            }
        }
        out
    }

    /// Selector-aware single text + fallback flag (English-only default selector).
    /// Mirrors `public_text_with_fallback` with `selector.requested == ("en",)`.
    fn public_text_with_fallback(&self, s_tid: usize, p_iri: &str) -> (String, bool) {
        let candidates: Vec<usize> = self
            .objects(s_tid, p_iri, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&o| self.is_literal(o))
            .collect();
        match self.select_literal(&candidates) {
            Some((text, _lang, fallback)) => (text, fallback),
            None => (String::new(), false),
        }
    }

    /// All requested-language texts (`en` only) + fallback. Mirrors `public_texts`.
    /// Returns `(text, bcp47_tag, is_fallback)` rows.
    fn public_texts(&self, s_tid: usize, p_iri: &str) -> Vec<(String, Option<String>, bool)> {
        let candidates: Vec<usize> = self
            .objects(s_tid, p_iri, DEFAULT_SCOPE)
            .into_iter()
            .filter(|&o| self.is_literal(o))
            .collect();
        self.filter_literals(&candidates)
    }

    /// Public BCP-47 tag for a literal tid (or `None`).
    fn bcp47_for(&self, tid: usize) -> Option<String> {
        let lang = self.lang(tid)?;
        if is_internal_tag(lang) {
            Some(
                self.tag_map
                    .get(lang)
                    .cloned()
                    .unwrap_or_else(|| lang.to_string()),
            )
        } else {
            Some(lang.to_string())
        }
    }

    /// `(retagged_text, public_bcp47, original_lang)` buckets keyed by lowercased
    /// public tag (`""` for untagged). Shared by select_literal / filter_literals.
    fn bucket_by_bcp(&self, candidates: &[usize]) -> BTreeMap<String, Vec<LitRow>> {
        let mut by_bcp: BTreeMap<String, Vec<LitRow>> = BTreeMap::new();
        for &tid in candidates {
            let bcp = self.bcp47_for(tid);
            let bucket = bcp.as_deref().unwrap_or("").to_ascii_lowercase();
            let text = self.lex(tid).to_string();
            let orig = self.lang(tid).unwrap_or("").to_string();
            by_bcp.entry(bucket).or_default().push((text, bcp, orig));
        }
        for items in by_bcp.values_mut() {
            items.sort_by(|a, b| {
                (rank_language(Some(&a.2)), &a.0).cmp(&(rank_language(Some(&b.2)), &b.0))
            });
        }
        by_bcp
    }

    /// `select_literal` for the English-only default selector.
    fn select_literal(&self, candidates: &[usize]) -> Option<(String, Option<String>, bool)> {
        if candidates.is_empty() {
            return None;
        }
        let by_bcp = self.bucket_by_bcp(candidates);
        if let Some(en) = by_bcp.get("en") {
            // requested == ("en",): a present "en" is NOT a fallback.
            let (text, bcp, _) = &en[0];
            return Some((text.clone(), bcp.clone(), false));
        }
        // Fallback: deterministic-first tagged literal, then untagged.
        if let Some((_, row)) = best_tagged(&by_bcp) {
            let (text, bcp, _) = row;
            return Some((text.clone(), bcp.clone(), true));
        }
        if let Some(untagged) = by_bcp.get("") {
            let (text, bcp, _) = &untagged[0];
            return Some((text.clone(), bcp.clone(), true));
        }
        None
    }

    /// `filter_literals` for the English-only default selector.
    fn filter_literals(&self, candidates: &[usize]) -> Vec<(String, Option<String>, bool)> {
        if candidates.is_empty() {
            return Vec::new();
        }
        let by_bcp = self.bucket_by_bcp(candidates);
        // requested == ("en",): all "en" rows, non-fallback.
        if let Some(en) = by_bcp.get("en") {
            return en
                .iter()
                .map(|(t, b, _)| (t.clone(), b.clone(), false))
                .collect();
        }
        // Fallback chain: first tagged, then untagged.
        if let Some((_, row)) = best_tagged(&by_bcp) {
            let (text, bcp, _) = row;
            return vec![(text.clone(), bcp.clone(), true)];
        }
        if let Some(untagged) = by_bcp.get("") {
            let (text, bcp, _) = &untagged[0];
            return vec![(text.clone(), bcp.clone(), true)];
        }
        Vec::new()
    }
}

/// N-Triples literal escaping for the nq token (subset rdflib emits).
fn nt_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out
}

// ── Term model (mirror export.Term + as_record) ────────────────────────────────

#[derive(Clone, Default)]
pub(crate) struct Term {
    pub(crate) category: &'static str, // "class" | "property" | "individual"
    pub(crate) iri: String,
    pub(crate) curie: String,
    pub(crate) label: String,
    pub(crate) definition: String,
    pub(crate) labels: BTreeMap<String, String>,
    pub(crate) definitions: BTreeMap<String, String>,
    pub(crate) label_fallback: bool,
    pub(crate) definition_fallback: bool,
    pub(crate) parents: Vec<String>,
    pub(crate) prop_kind: &'static str,
    pub(crate) domain: String,
    pub(crate) range: String,
    pub(crate) functional: bool,
    pub(crate) sub_property_of: Vec<String>,
    pub(crate) types: Vec<String>,
    pub(crate) alignments: Vec<String>,
    pub(crate) box_roles: Vec<String>,
    pub(crate) scope_notes: Vec<String>,
    pub(crate) examples: Vec<String>,
    pub(crate) use_when: Vec<String>,
    pub(crate) avoid_when: Vec<String>,
    pub(crate) how_to_use: Vec<String>,
    pub(crate) use_for_consumer: Vec<String>,
    pub(crate) avoid_for_consumer: Vec<String>,
}

// ── JSON helpers (json.dumps ensure_ascii=False, insertion-ordered objects) ────

enum J {
    Bool(bool),
    Str(String),
    /// A pre-formatted numeric token rendered verbatim (xsd:integer/decimal lexeme).
    RawNum(String),
    Arr(Vec<J>),
    Obj(Vec<(String, J)>),
}

impl J {
    /// `json.dumps(obj, ensure_ascii=False)` compact form (`, ` / `: ` separators).
    fn compact(&self, out: &mut String) {
        match self {
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Str(s) => out.push_str(&json_str(s)),
            J::RawNum(n) => out.push_str(n),
            J::Arr(items) => {
                out.push('[');
                for (i, it) in items.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    it.compact(out);
                }
                out.push(']');
            }
            J::Obj(entries) => {
                out.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        out.push_str(", ");
                    }
                    out.push_str(&json_str(k));
                    out.push_str(": ");
                    v.compact(out);
                }
                out.push('}');
            }
        }
    }

    /// `json.dumps(obj, indent=2, ensure_ascii=False)`.
    fn pretty(&self, indent: usize, out: &mut String) {
        let pad = "  ".repeat(indent);
        let pad1 = "  ".repeat(indent + 1);
        match self {
            J::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
            J::Str(s) => out.push_str(&json_str(s)),
            J::RawNum(n) => out.push_str(n),
            J::Arr(items) => {
                if items.is_empty() {
                    out.push_str("[]");
                    return;
                }
                out.push_str("[\n");
                for (i, it) in items.iter().enumerate() {
                    out.push_str(&pad1);
                    it.pretty(indent + 1, out);
                    if i + 1 < items.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push(']');
            }
            J::Obj(entries) => {
                if entries.is_empty() {
                    out.push_str("{}");
                    return;
                }
                out.push_str("{\n");
                for (i, (k, v)) in entries.iter().enumerate() {
                    out.push_str(&pad1);
                    out.push_str(&json_str(k));
                    out.push_str(": ");
                    v.pretty(indent + 1, out);
                    if i + 1 < entries.len() {
                        out.push(',');
                    }
                    out.push('\n');
                }
                out.push_str(&pad);
                out.push('}');
            }
        }
    }
}

fn jarr_str(items: &[String]) -> J {
    J::Arr(items.iter().map(|s| J::Str(s.clone())).collect())
}

/// `json.dumps(s, ensure_ascii=False)` of a string.
fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

// ── collect_terms (mirror export.collect_terms; English-only selector) ─────────

fn fold_curies(view: &FoldView, s_tid: usize, p_iri: &str) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for o in view.objects(s_tid, p_iri, DEFAULT_SCOPE) {
        if view.is_iri(o) {
            out.insert(curie(view.lex(o)));
        }
    }
    out.into_iter().collect()
}

fn fold_public_texts(view: &FoldView, s_tid: usize, p_iri: &str) -> Vec<String> {
    view.public_texts(s_tid, p_iri)
        .into_iter()
        .map(|(text, _lang, _fallback)| text)
        .collect()
}

const ALIGN_TAGS: &[(&str, &str)] = &[
    ("equivalentClass", "equivalentClass"),
    ("equivalentProperty", "equivalentProperty"),
    ("subClassOf", "subClassOf"),
    ("subPropertyOf", "subPropertyOf"),
    ("closeMatch", "closeMatch"),
    ("exactMatch", "exactMatch"),
    ("relatedMatch", "relatedMatch"),
];

fn align_tag(pred_iri: &str) -> String {
    let local = pred_iri.rsplit(['#', '/']).next().unwrap_or(pred_iri);
    for (key, tag) in ALIGN_TAGS {
        if *key == local
            && (pred_iri == format!("{OWL}{key}")
                || pred_iri == format!("{RDFS}{key}")
                || pred_iri == format!("{SKOS}{key}"))
        {
            return tag.to_string();
        }
    }
    curie(pred_iri)
}

fn fold_alignments(view: &FoldView, s_tid: usize) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for (p, o) in view.predicate_objects(s_tid, ALIGNMENTS_GRAPH) {
        let tag = align_tag(view.lex(p));
        out.insert(format!("{tag}={}", curie(view.lex(o))));
    }
    out.into_iter().collect()
}

fn fold_describe_node(view: &FoldView, tid: usize) -> String {
    if view.is_iri(tid) {
        return curie(view.lex(tid));
    }
    if view.is_bnode(tid) {
        if let Some(head) = view.value(tid, &format!("{OWL}unionOf"), DEFAULT_SCOPE) {
            return view
                .rdf_list(head, DEFAULT_SCOPE)
                .into_iter()
                .map(|item| fold_describe_node(view, item))
                .collect::<Vec<_>>()
                .join(" | ");
        }
        if let Some(head) = view.value(tid, &format!("{OWL}intersectionOf"), DEFAULT_SCOPE) {
            return view
                .rdf_list(head, DEFAULT_SCOPE)
                .into_iter()
                .map(|item| fold_describe_node(view, item))
                .collect::<Vec<_>>()
                .join(" & ");
        }
    }
    view.lex(tid).to_string()
}

fn term_label_def(view: &FoldView, t: usize, term: &mut Term) {
    let label_iri = format!("{RDFS}label");
    let definition_iri = format!("{SKOS}definition");
    let (label, label_fb) = view.public_text_with_fallback(t, &label_iri);
    let (definition, def_fb) = view.public_text_with_fallback(t, &definition_iri);
    term.label = label;
    term.label_fallback = label_fb;
    term.definition = definition;
    term.definition_fallback = def_fb;
    for (text, lang, fallback) in view.public_texts(t, &label_iri) {
        if let Some(l) = lang {
            if !fallback && !term.labels.contains_key(&l) {
                term.labels.insert(l, text);
            }
        }
    }
    for (text, lang, fallback) in view.public_texts(t, &definition_iri) {
        if let Some(l) = lang {
            if !fallback && !term.definitions.contains_key(&l) {
                term.definitions.insert(l, text);
            }
        }
    }
}

fn fold_advisory(view: &FoldView, t: usize, term: &mut Term) {
    term.box_roles = fold_curies(view, t, &format!("{NAMESPACE}graphBoxRole"));
    term.scope_notes = fold_public_texts(view, t, &format!("{SKOS}scopeNote"));
    term.examples = fold_public_texts(view, t, &format!("{SKOS}example"));
    term.use_when = fold_public_texts(view, t, &format!("{NAMESPACE}useWhen"));
    term.avoid_when = fold_public_texts(view, t, &format!("{NAMESPACE}avoidWhen"));
    term.how_to_use = fold_public_texts(view, t, &format!("{NAMESPACE}howToUse"));
    term.use_for_consumer = fold_curies(view, t, &format!("{NAMESPACE}useForConsumer"));
    term.avoid_for_consumer = fold_curies(view, t, &format!("{NAMESPACE}avoidForConsumer"));
}

const PROPERTY_KINDS: &[(&str, &str)] = &[
    ("ObjectProperty", "object"),
    ("DatatypeProperty", "datatype"),
    ("AnnotationProperty", "annotation"),
];

fn collect_terms(view: &FoldView) -> Vec<Term> {
    let in_namespace =
        |view: &FoldView, tid: usize| view.is_iri(tid) && view.lex(tid).starts_with(NAMESPACE);

    let classes: BTreeSet<usize> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| in_namespace(view, t))
        .collect();

    let mut properties: BTreeMap<usize, &'static str> = BTreeMap::new();
    for (ptype, kind) in PROPERTY_KINDS {
        for t in view.subjects_by_type(&format!("{OWL}{ptype}"), DEFAULT_SCOPE) {
            if in_namespace(view, t) {
                properties.insert(t, kind);
            }
        }
    }

    let functional_tid = view.tid_of_iri(&format!("{OWL}FunctionalProperty"));

    let mut terms: Vec<Term> = Vec::new();

    for &t in &classes {
        let mut term = Term {
            category: "class",
            iri: view.lex(t).to_string(),
            curie: curie(view.lex(t)),
            parents: fold_curies(view, t, &format!("{RDFS}subClassOf")),
            alignments: fold_alignments(view, t),
            ..Term::default()
        };
        fold_advisory(view, t, &mut term);
        term_label_def(view, t, &mut term);
        terms.push(term);
    }

    for (&t, &kind) in &properties {
        let domain_tid = view.value(t, &format!("{RDFS}domain"), DEFAULT_SCOPE);
        let range_tid = view.value(t, &format!("{RDFS}range"), DEFAULT_SCOPE);
        let functional =
            matches!(functional_tid, Some(ft) if view.has(t, RDF_TYPE, ft, DEFAULT_SCOPE));
        let mut term = Term {
            category: "property",
            iri: view.lex(t).to_string(),
            curie: curie(view.lex(t)),
            prop_kind: kind,
            domain: domain_tid
                .map(|d| fold_describe_node(view, d))
                .unwrap_or_default(),
            range: range_tid
                .map(|r| fold_describe_node(view, r))
                .unwrap_or_default(),
            functional,
            sub_property_of: fold_curies(view, t, &format!("{RDFS}subPropertyOf")),
            alignments: fold_alignments(view, t),
            ..Term::default()
        };
        fold_advisory(view, t, &mut term);
        term_label_def(view, t, &mut term);
        terms.push(term);
    }

    // Individuals: subjects typed by an in-namespace class, not themselves declared.
    let declared: BTreeSet<usize> = classes
        .iter()
        .copied()
        .chain(properties.keys().copied())
        .collect();
    let mut seen: BTreeSet<usize> = BTreeSet::new();
    for &cls in &classes {
        let cls_iri = view.lex(cls).to_string();
        for t in view.subjects_by_type(&cls_iri, DEFAULT_SCOPE) {
            if !in_namespace(view, t) || declared.contains(&t) || seen.contains(&t) {
                continue;
            }
            seen.insert(t);
            let types: Vec<String> = {
                let mut s: BTreeSet<String> = BTreeSet::new();
                for o in view.objects(t, RDF_TYPE, DEFAULT_SCOPE) {
                    if classes.contains(&o) {
                        s.insert(curie(view.lex(o)));
                    }
                }
                s.into_iter().collect()
            };
            let mut term = Term {
                category: "individual",
                iri: view.lex(t).to_string(),
                curie: curie(view.lex(t)),
                types,
                alignments: fold_alignments(view, t),
                ..Term::default()
            };
            fold_advisory(view, t, &mut term);
            term_label_def(view, t, &mut term);
            terms.push(term);
        }
    }

    terms.sort_by(|a, b| (a.category, &a.curie).cmp(&(b.category, &b.curie)));
    terms
}

fn fold_meta(view: &FoldView) -> Result<(String, String), PipelineError> {
    let onto = view.tid_of_iri(ONTOLOGY_IRI).ok_or_else(|| {
        PipelineError::Parse(format!(
            "ontology header {ONTOLOGY_IRI} not present in the snapshot"
        ))
    })?;
    let title = view
        .value(onto, "http://purl.org/dc/terms/title", DEFAULT_SCOPE)
        .map(|t| view.lex(t).to_string());
    let version = view
        .value(onto, &format!("{OWL}versionInfo"), DEFAULT_SCOPE)
        .map(|t| view.lex(t).to_string());
    match (title, version) {
        (Some(t), Some(v)) => Ok((t, v)),
        _ => Err(PipelineError::Parse(
            "ontology header lacks dcterms:title / owl:versionInfo".into(),
        )),
    }
}

// ── as_record (JSONL) ──────────────────────────────────────────────────────────

fn term_record(t: &Term) -> J {
    let mut rec: Vec<(String, J)> = vec![
        ("category".into(), J::Str(t.category.to_string())),
        ("curie".into(), J::Str(t.curie.clone())),
        ("iri".into(), J::Str(t.iri.clone())),
        ("label".into(), J::Str(t.label.clone())),
        ("definition".into(), J::Str(t.definition.clone())),
    ];
    if !t.labels.is_empty() {
        rec.push((
            "labels".into(),
            J::Obj(
                t.labels
                    .iter()
                    .map(|(k, v)| (k.clone(), J::Str(v.clone())))
                    .collect(),
            ),
        ));
    }
    if !t.definitions.is_empty() {
        rec.push((
            "definitions".into(),
            J::Obj(
                t.definitions
                    .iter()
                    .map(|(k, v)| (k.clone(), J::Str(v.clone())))
                    .collect(),
            ),
        ));
    }
    if t.label_fallback {
        rec.push(("labelFallback".into(), J::Bool(true)));
    }
    if t.definition_fallback {
        rec.push(("definitionFallback".into(), J::Bool(true)));
    }
    match t.category {
        "class" => rec.push(("subClassOf".into(), jarr_str(&t.parents))),
        "property" => {
            rec.push(("propertyKind".into(), J::Str(t.prop_kind.to_string())));
            rec.push(("domain".into(), J::Str(t.domain.clone())));
            rec.push(("range".into(), J::Str(t.range.clone())));
            rec.push(("functional".into(), J::Bool(t.functional)));
            rec.push(("subPropertyOf".into(), jarr_str(&t.sub_property_of)));
        }
        _ => rec.push(("types".into(), jarr_str(&t.types))),
    }
    let extra: &[(&str, &Vec<String>)] = &[
        ("alignments", &t.alignments),
        ("boxRoles", &t.box_roles),
        ("scopeNotes", &t.scope_notes),
        ("examples", &t.examples),
        ("useWhen", &t.use_when),
        ("avoidWhen", &t.avoid_when),
        ("howToUse", &t.how_to_use),
        ("useForConsumer", &t.use_for_consumer),
        ("avoidForConsumer", &t.avoid_for_consumer),
    ];
    for (key, vals) in extra {
        if !vals.is_empty() {
            rec.push(((*key).into(), jarr_str(vals)));
        }
    }
    J::Obj(rec)
}

// ── CSV (csv.DictWriter QUOTE_MINIMAL, lineterminator "\r\n") ───────────────────

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_row(cols: &[String]) -> String {
    cols.iter()
        .map(|c| csv_field(c))
        .collect::<Vec<_>>()
        .join(",")
        + "\r\n"
}

const ADVISORY_COLUMNS: &[&str] = &[
    "boxRoles",
    "scopeNotes",
    "examples",
    "useWhen",
    "avoidWhen",
    "howToUse",
    "useForConsumer",
    "avoidForConsumer",
];

fn advisory_cells(t: &Term) -> Vec<String> {
    vec![
        t.box_roles.join("; "),
        t.scope_notes.join("; "),
        t.examples.join("; "),
        t.use_when.join("; "),
        t.avoid_when.join("; "),
        t.how_to_use.join("; "),
        t.use_for_consumer.join("; "),
        t.avoid_for_consumer.join("; "),
    ]
}

/// Insert per-language label/definition columns after "definition" (en-only here).
fn lang_columns(base: &[&str], languages: &[&str]) -> Vec<String> {
    let mut extra: Vec<String> = Vec::new();
    for lang in languages {
        extra.push(format!("label_{lang}"));
        extra.push(format!("definition_{lang}"));
    }
    extra.push("label_fallback".to_string());
    extra.push("definition_fallback".to_string());
    let mut out: Vec<String> = Vec::new();
    for col in base {
        out.push((*col).to_string());
        if *col == "definition" {
            out.extend(extra.iter().cloned());
        }
    }
    out
}

/// The shared label/definition + per-language cells (after "definition").
fn lang_cells(t: &Term, languages: &[&str]) -> Vec<String> {
    let mut cells = vec![t.label.clone(), t.definition.clone()];
    for lang in languages {
        cells.push(t.labels.get(*lang).cloned().unwrap_or_default());
        cells.push(t.definitions.get(*lang).cloned().unwrap_or_default());
    }
    cells.push(if t.label_fallback { "true" } else { "false" }.to_string());
    cells.push(
        if t.definition_fallback {
            "true"
        } else {
            "false"
        }
        .to_string(),
    );
    cells
}

fn class_columns() -> Vec<&'static str> {
    let mut c = vec!["curie", "label", "definition"];
    c.extend(ADVISORY_COLUMNS);
    c.extend(["subClassOf", "alignments", "iri"]);
    c
}
fn property_columns() -> Vec<&'static str> {
    let mut c = vec!["curie", "label", "definition"];
    c.extend(ADVISORY_COLUMNS);
    c.extend([
        "propertyKind",
        "domain",
        "range",
        "functional",
        "subPropertyOf",
        "alignments",
        "iri",
    ]);
    c
}
fn individual_columns() -> Vec<&'static str> {
    let mut c = vec!["curie", "label", "definition"];
    c.extend(ADVISORY_COLUMNS);
    c.extend(["types", "alignments", "iri"]);
    c
}

fn write_class_csv(classes: &[&Term], languages: &[&str]) -> Vec<u8> {
    let cols = lang_columns(&class_columns(), languages);
    let mut s = csv_row(&cols);
    for t in classes {
        let mut row = vec![t.curie.clone()];
        row.extend(lang_cells(t, languages));
        row.extend(advisory_cells(t));
        row.push(t.parents.join("; "));
        row.push(t.alignments.join("; "));
        row.push(t.iri.clone());
        s.push_str(&csv_row(&row));
    }
    s.into_bytes()
}

fn write_property_csv(properties: &[&Term], languages: &[&str]) -> Vec<u8> {
    let cols = lang_columns(&property_columns(), languages);
    let mut s = csv_row(&cols);
    for t in properties {
        let mut row = vec![t.curie.clone()];
        row.extend(lang_cells(t, languages));
        row.extend(advisory_cells(t));
        row.push(t.prop_kind.to_string());
        row.push(t.domain.clone());
        row.push(t.range.clone());
        row.push(if t.functional { "true" } else { "false" }.to_string());
        row.push(t.sub_property_of.join("; "));
        row.push(t.alignments.join("; "));
        row.push(t.iri.clone());
        s.push_str(&csv_row(&row));
    }
    s.into_bytes()
}

fn write_individual_csv(individuals: &[&Term], languages: &[&str]) -> Vec<u8> {
    let cols = lang_columns(&individual_columns(), languages);
    let mut s = csv_row(&cols);
    for t in individuals {
        let mut row = vec![t.curie.clone()];
        row.extend(lang_cells(t, languages));
        row.extend(advisory_cells(t));
        row.push(t.types.join("; "));
        row.push(t.alignments.join("; "));
        row.push(t.iri.clone());
        s.push_str(&csv_row(&row));
    }
    s.into_bytes()
}

// ── CSVW descriptor ─────────────────────────────────────────────────────────────

fn write_csvw(title: &str, languages: &[&str]) -> Vec<u8> {
    let table = |url: &str, cols: Vec<String>| -> J {
        J::Obj(vec![
            ("url".into(), J::Str(url.to_string())),
            (
                "tableSchema".into(),
                J::Obj(vec![(
                    "columns".into(),
                    J::Arr(
                        cols.into_iter()
                            .map(|c| {
                                J::Obj(vec![
                                    ("name".into(), J::Str(c.clone())),
                                    ("titles".into(), J::Str(c)),
                                ])
                            })
                            .collect(),
                    ),
                )]),
            ),
        ])
    };
    let descriptor = J::Obj(vec![
        (
            "@context".into(),
            J::Str("http://www.w3.org/ns/csvw".into()),
        ),
        (
            "dc:title".into(),
            J::Str(format!("{title} — term dictionaries")),
        ),
        ("dc:source".into(), J::Str(ONTOLOGY_IRI.into())),
        (
            "tables".into(),
            J::Arr(vec![
                table(
                    "gmeow-classes.csv",
                    lang_columns(&class_columns(), languages),
                ),
                table(
                    "gmeow-properties.csv",
                    lang_columns(&property_columns(), languages),
                ),
                table(
                    "gmeow-individuals.csv",
                    lang_columns(&individual_columns(), languages),
                ),
            ]),
        ),
    ]);
    let mut s = String::new();
    descriptor.pretty(0, &mut s);
    s.push('\n');
    s.into_bytes()
}

// ── JSONL ───────────────────────────────────────────────────────────────────────

fn write_jsonl(terms: &[Term]) -> Vec<u8> {
    let mut lines: Vec<String> = Vec::new();
    for t in terms {
        let mut s = String::new();
        term_record(t).compact(&mut s);
        lines.push(s);
    }
    (lines.join("\n") + "\n").into_bytes()
}

// ── Markdown term reference ─────────────────────────────────────────────────────

fn marked(text: &str, fallback: bool) -> String {
    if fallback {
        format!("{text} [fallback: en]")
    } else {
        text.to_string()
    }
}

fn append_md_advisory(lines: &mut Vec<String>, t: &Term) {
    if !t.box_roles.is_empty() {
        lines.push(format!(
            "\n*Box roles:* {}",
            t.box_roles
                .iter()
                .map(|r| format!("`{r}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    for (label, values) in [
        ("Scope", &t.scope_notes),
        ("Example", &t.examples),
        ("Use when", &t.use_when),
        ("Avoid when", &t.avoid_when),
        ("How to use", &t.how_to_use),
    ] {
        if !values.is_empty() {
            lines.push(format!("\n*{label}:* {}", values.join(" ")));
        }
    }
    if !t.use_for_consumer.is_empty() {
        lines.push(format!(
            "\n*Use for consumers:* {}",
            t.use_for_consumer
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    if !t.avoid_for_consumer.is_empty() {
        lines.push(format!(
            "\n*Avoid for consumers:* {}",
            t.avoid_for_consumer
                .iter()
                .map(|c| format!("`{c}`"))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
}

fn write_markdown(terms: &[Term], title: &str, version: &str) -> Vec<u8> {
    let classes: Vec<&Term> = terms.iter().filter(|t| t.category == "class").collect();
    let properties: Vec<&Term> = terms.iter().filter(|t| t.category == "property").collect();
    let individuals: Vec<&Term> = terms
        .iter()
        .filter(|t| t.category == "individual")
        .collect();
    let mut lines: Vec<String> = vec![
        format!("# {title} — term reference"),
        String::new(),
        format!(
            "Generated from the GMEOW {version} vocabulary ({} classes, {} properties, {} individuals). The OWL source is canonical.",
            classes.len(),
            properties.len(),
            individuals.len()
        ),
        String::new(),
        "## Classes".into(),
        String::new(),
    ];
    for t in &classes {
        let head = marked(
            if t.label.is_empty() {
                &t.curie
            } else {
                &t.label
            },
            t.label_fallback,
        );
        lines.push(format!("### {head} (`{}`)", t.curie));
        if !t.definition.is_empty() {
            lines.push(format!(
                "\n{}",
                marked(&t.definition, t.definition_fallback)
            ));
        }
        append_md_advisory(&mut lines, t);
        if !t.parents.is_empty() {
            lines.push(format!(
                "\n*Subclass of:* {}",
                t.parents
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if !t.alignments.is_empty() {
            lines.push(format!(
                "\n*Aligns:* {}",
                t.alignments
                    .iter()
                    .map(|a| format!("`{a}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.push(String::new());
    }
    lines.push("## Properties".into());
    lines.push(String::new());
    for t in &properties {
        let head = marked(
            if t.label.is_empty() {
                &t.curie
            } else {
                &t.label
            },
            t.label_fallback,
        );
        lines.push(format!("### {head} (`{}`)", t.curie));
        if !t.definition.is_empty() {
            lines.push(format!(
                "\n{}",
                marked(&t.definition, t.definition_fallback)
            ));
        }
        append_md_advisory(&mut lines, t);
        let mut meta = format!("*{} property*", t.prop_kind);
        if !t.domain.is_empty() || !t.range.is_empty() {
            let d = if t.domain.is_empty() { "?" } else { &t.domain };
            let r = if t.range.is_empty() { "?" } else { &t.range };
            meta.push_str(&format!(" — `{d}` → `{r}`"));
        }
        if t.functional {
            meta.push_str(" (functional)");
        }
        lines.push(format!("\n{meta}"));
        if !t.alignments.is_empty() {
            lines.push(format!(
                "\n*Aligns:* {}",
                t.alignments
                    .iter()
                    .map(|a| format!("`{a}`"))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        lines.push(String::new());
    }
    if !individuals.is_empty() {
        lines.push("## Individuals".into());
        lines.push(String::new());
        for t in &individuals {
            let head = marked(
                if t.label.is_empty() {
                    &t.curie
                } else {
                    &t.label
                },
                t.label_fallback,
            );
            lines.push(format!("### {head} (`{}`)", t.curie));
            if !t.definition.is_empty() {
                lines.push(format!(
                    "\n{}",
                    marked(&t.definition, t.definition_fallback)
                ));
            }
            append_md_advisory(&mut lines, t);
            if !t.types.is_empty() {
                lines.push(format!(
                    "\n*Type:* {}",
                    t.types
                        .iter()
                        .map(|x| format!("`{x}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            lines.push(String::new());
        }
    }
    (lines.join("\n") + "\n").into_bytes()
}

// ── llms.txt ────────────────────────────────────────────────────────────────────

fn term_summary(t: &Term) -> String {
    let base = if t.definition.is_empty() {
        &t.label
    } else {
        &t.definition
    };
    let mut summary = marked(base, t.definition_fallback || t.label_fallback);
    if !t.box_roles.is_empty() {
        summary.push_str(&format!(" [box roles: {}]", t.box_roles.join(", ")));
    }
    summary
}

fn write_llms_txt(terms: &[Term], title: &str, version: &str) -> Vec<u8> {
    let classes: Vec<&Term> = terms.iter().filter(|t| t.category == "class").collect();
    let properties: Vec<&Term> = terms.iter().filter(|t| t.category == "property").collect();
    let individuals: Vec<&Term> = terms
        .iter()
        .filter(|t| t.category == "individual")
        .collect();
    let mut lines: Vec<String> = vec![
        format!("# {title}"),
        String::new(),
        "> A reasoning-centric, OWL 2 DL, gUFO-grounded super-vocabulary that unifies a person's or organization's digital existence (entities, contacts, email, trust/keys, time) and aligns it to schema.org, FOAF, PROV, the WOT schema, Wikidata, and more.".into(),
        String::new(),
        format!("Vocabulary {version}. Namespace: {NAMESPACE}. Each term below is `curie` — definition; the OWL source is canonical."),
        String::new(),
        "## Classes".into(),
        String::new(),
    ];
    for t in &classes {
        let sub = if t.parents.is_empty() {
            String::new()
        } else {
            format!(" (⊑ {})", t.parents.join(", "))
        };
        lines.push(format!("- {}{sub}: {}", t.curie, term_summary(t)));
    }
    lines.push(String::new());
    lines.push("## Properties".into());
    lines.push(String::new());
    for t in &properties {
        let sig = if t.domain.is_empty() && t.range.is_empty() {
            String::new()
        } else {
            let d = if t.domain.is_empty() { "?" } else { &t.domain };
            let r = if t.range.is_empty() { "?" } else { &t.range };
            format!(" [{d} → {r}]")
        };
        let f_ = if t.functional { " (functional)" } else { "" };
        lines.push(format!("- {}{sig}{f_}: {}", t.curie, term_summary(t)));
    }
    if !individuals.is_empty() {
        lines.push(String::new());
        lines.push("## Individuals".into());
        lines.push(String::new());
        for t in &individuals {
            let types = if t.types.is_empty() {
                String::new()
            } else {
                format!(" (a {})", t.types.join(", "))
            };
            lines.push(format!("- {}{types}: {}", t.curie, term_summary(t)));
        }
    }
    (lines.join("\n") + "\n").into_bytes()
}

// ── dataset forms: N-Quads / TriG (gmeow-gts serializers) ──────────────────────

/// A shallow clone of the graph with internal `x-gmeow-*` language tags remapped
/// to public BCP-47 (the #287 projection boundary). Only literal lang tags change.
fn graph_with_public_tags(graph: &Graph, tag_map: &BTreeMap<String, String>) -> Graph {
    let mut clone = clone_graph(graph);
    for term in &mut clone.terms {
        if term.kind == TermKind::Literal {
            if let Some(lang) = &term.lang {
                if let Some(public) = tag_map.get(lang) {
                    term.lang = Some(public.clone());
                }
            }
        }
    }
    clone
}

/// A field-for-field clone of the parts of `Graph` the serializers consume.
fn clone_graph(graph: &Graph) -> Graph {
    let mut out = Graph {
        terms: graph.terms.clone(),
        quads: graph.quads.clone(),
        reifiers: graph.reifiers.clone(),
        annotations: graph.annotations.clone(),
        ..Graph::default()
    };
    for (digest, entry) in &graph.blobs {
        out.blobs.push((digest.clone(), entry.clone()));
    }
    out.blob_meta = graph.blob_meta.clone();
    out.meta = graph.meta.clone();
    out
}

fn write_nquads(graph: &Graph, tag_map: &BTreeMap<String, String>) -> Vec<u8> {
    let public = graph_with_public_tags(graph, tag_map);
    gmeow_gts::nquads::to_nquads(&public).into_bytes()
}

fn write_trig(graph: &Graph, tag_map: &BTreeMap<String, String>) -> Vec<u8> {
    let public = graph_with_public_tags(graph, tag_map);
    gmeow_gts::trig::to_trig(&public).into_bytes()
}

// ── statements JSONL ─────────────────────────────────────────────────────────────

/// `python_value` with public lang-tag remap (mirror `_public_value`).
fn public_value_json(view: &FoldView, tid: usize) -> J {
    let t = view.term(tid);
    match t.kind {
        TermKind::Literal => {
            let dt = view.datatype(tid);
            let lex = view.lex(tid);
            if dt == format!("{XSD}integer") {
                // emit the integer lexeme verbatim as a number-ish string is wrong;
                // statements JSONL keeps numeric values, but for determinism without
                // a number variant we surface the lexical via a raw token.
                J::RawNum(lex.to_string())
            } else if dt == format!("{XSD}decimal")
                || dt == format!("{XSD}double")
                || dt == format!("{XSD}float")
            {
                J::RawNum(lex.to_string())
            } else if dt == format!("{XSD}boolean") {
                J::Bool(matches!(lex.to_ascii_lowercase().as_str(), "true" | "1"))
            } else if let Some(lang) = &t.lang {
                let public = view
                    .tag_map
                    .get(lang)
                    .cloned()
                    .unwrap_or_else(|| lang.clone());
                J::Obj(vec![
                    ("value".into(), J::Str(lex.to_string())),
                    ("lang".into(), J::Str(public)),
                ])
            } else {
                J::Str(lex.to_string())
            }
        }
        TermKind::Iri => J::Str(curie(view.lex(tid))),
        TermKind::Bnode => J::Str(format!("_:{}", view.lex(tid))),
        TermKind::Triple => J::Str(view.nq_token(tid)),
    }
}

fn write_statements_jsonl(view: &FoldView) -> Vec<u8> {
    // grouped: reifier → predicate-curie → [values]
    let mut grouped: BTreeMap<usize, BTreeMap<String, Vec<J>>> = BTreeMap::new();
    for &(r, p, v) in &view.graph.annotations {
        let key = curie(view.lex(p));
        grouped
            .entry(r)
            .or_default()
            .entry(key)
            .or_default()
            .push(public_value_json(view, v));
    }
    // reifiers sorted by nq_token of the reifier id.
    let mut reifiers: Vec<(usize, (usize, usize, usize))> = view.graph.reifiers.clone();
    reifiers.sort_by_key(|a| view.nq_token(a.0));
    let mut rows: Vec<String> = Vec::new();
    for (rid, (s, p, o)) in &reifiers {
        let mut annotations: Vec<(String, J)> = Vec::new();
        if let Some(per_pred) = grouped.get(rid) {
            for (key, values) in per_pred {
                let val = if values.len() == 1 {
                    clone_j(&values[0])
                } else {
                    // sorted(values, key=str)
                    let mut sortable: Vec<(String, &J)> =
                        values.iter().map(|v| (j_str_key(v), v)).collect();
                    sortable.sort_by(|a, b| a.0.cmp(&b.0));
                    J::Arr(sortable.into_iter().map(|(_, v)| clone_j(v)).collect())
                };
                annotations.push((key.clone(), val));
            }
        }
        let record = J::Obj(vec![
            ("id".into(), public_value_json(view, *rid)),
            ("subject".into(), public_value_json(view, *s)),
            ("predicate".into(), J::Str(curie(view.lex(*p)))),
            ("object".into(), public_value_json(view, *o)),
            ("annotations".into(), J::Obj(annotations)),
        ]);
        let mut s = String::new();
        record.compact(&mut s);
        rows.push(s);
    }
    (rows.join("\n") + "\n").into_bytes()
}

// ── SKOS extract ─────────────────────────────────────────────────────────────────

fn ttl_literal(text: &str, lang: Option<&str>) -> String {
    let lit = json_str(text);
    match lang {
        Some(l) => format!("{lit}@{l}"),
        None => lit,
    }
}

const SKOS_MATCHES: &[&str] = &["exactMatch", "closeMatch", "relatedMatch"];

fn write_skos(view: &FoldView, title: &str, version: &str) -> Vec<u8> {
    let mut classes: Vec<usize> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| view.is_iri(t) && view.lex(t).starts_with(NAMESPACE))
        .collect();
    classes.sort_by_key(|&a| curie(view.lex(a)));
    let class_iris: BTreeSet<String> = classes.iter().map(|&t| view.lex(t).to_string()).collect();

    let mut lines: Vec<String> = vec![
        "# The GMEOW vocabulary as a SKOS concept scheme — a LOSSY projection:".into(),
        "# classes only (typed skos:Concept on their original IRIs);".into(),
        "# subClassOf → skos:broader; SKOS mapping rows carried from the".into(),
        "# alignments graph. OWL axioms, properties, and individuals are".into(),
        "# dropped. STANDALONE: never merge with the OWL form (class punning).".into(),
        "@prefix skos: <http://www.w3.org/2004/02/skos/core#> .".into(),
        "@prefix dcterms: <http://purl.org/dc/terms/> .".into(),
        "@prefix owl: <http://www.w3.org/2002/07/owl#> .".into(),
        format!("@prefix gmeow: <{NAMESPACE}> ."),
        String::new(),
        format!("<{ONTOLOGY_IRI}> a skos:ConceptScheme ;"),
        format!(
            "    dcterms:title {} ;",
            ttl_literal(&format!("{title} — SKOS extract"), None)
        ),
        format!("    owl:versionInfo {} .", ttl_literal(version, None)),
    ];

    let mut top_concepts: Vec<String> = Vec::new();
    let mut bodies: Vec<String> = Vec::new();
    for &t in &classes {
        let c = curie(view.lex(t));
        let mut broader: BTreeSet<String> = BTreeSet::new();
        for o in view.objects(t, &format!("{RDFS}subClassOf"), DEFAULT_SCOPE) {
            if view.is_iri(o) && class_iris.contains(view.lex(o)) {
                broader.insert(curie(view.lex(o)));
            }
        }
        let broader: Vec<String> = broader.into_iter().collect();
        if broader.is_empty() {
            top_concepts.push(c.clone());
        }
        let mut stanza: Vec<String> = vec![
            format!("{c} a skos:Concept ;"),
            format!("    skos:inScheme <{ONTOLOGY_IRI}> ;"),
        ];
        let mut seen_labels: BTreeSet<String> = BTreeSet::new();
        for (text, lang, _fallback) in view.public_texts(t, &format!("{RDFS}label")) {
            if let Some(l) = lang {
                if seen_labels.insert(l.clone()) {
                    stanza.push(format!(
                        "    skos:prefLabel {} ;",
                        ttl_literal(&text, Some(&l))
                    ));
                }
            }
        }
        let mut seen_defs: BTreeSet<String> = BTreeSet::new();
        for (text, lang, _fallback) in view.public_texts(t, &format!("{SKOS}definition")) {
            if let Some(l) = lang {
                if seen_defs.insert(l.clone()) {
                    stanza.push(format!(
                        "    skos:definition {} ;",
                        ttl_literal(&text, Some(&l))
                    ));
                }
            }
        }
        for b in &broader {
            stanza.push(format!("    skos:broader {b} ;"));
        }
        for (p, o) in view.predicate_objects(t, ALIGNMENTS_GRAPH) {
            let p_local = view.lex(p).rsplit('#').next().unwrap_or("");
            if SKOS_MATCHES.contains(&p_local) && view.is_iri(o) {
                stanza.push(format!("    skos:{p_local} <{}> ;", view.lex(o)));
            }
        }
        let last = stanza.len() - 1;
        stanza[last] = stanza[last].trim_end_matches(" ;").to_string() + " .";
        bodies.push(String::new());
        bodies.extend(stanza);
    }

    for c in &top_concepts {
        lines.push(format!("<{ONTOLOGY_IRI}> skos:hasTopConcept {c} ."));
    }
    let mut all = lines;
    all.extend(bodies);
    (all.join("\n") + "\n").into_bytes()
}

// ── OBO Graphs JSON ──────────────────────────────────────────────────────────────

fn write_obographs(view: &FoldView, version: &str) -> Vec<u8> {
    let label_iri = format!("{RDFS}label");
    let definition_iri = format!("{SKOS}definition");
    let mut classes: Vec<usize> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| view.is_iri(t) && view.lex(t).starts_with(NAMESPACE))
        .collect();
    classes.sort_by(|&a, &b| view.lex(a).cmp(view.lex(b)));

    let mut nodes: Vec<J> = Vec::new();
    let mut edges: Vec<J> = Vec::new();
    let mut node_ids: BTreeSet<String> = BTreeSet::new();
    let mut edge_objs: BTreeSet<String> = BTreeSet::new();
    for &t in &classes {
        let iri = view.lex(t).to_string();
        let mut node: Vec<(String, J)> = vec![
            ("id".into(), J::Str(iri.clone())),
            ("type".into(), J::Str("CLASS".into())),
        ];
        let (label, _fb) = view.public_text_with_fallback(t, &label_iri);
        if !label.is_empty() {
            node.push(("lbl".into(), J::Str(label)));
        }
        let (definition, _fb) = view.public_text_with_fallback(t, &definition_iri);
        if !definition.is_empty() {
            node.push((
                "meta".into(),
                J::Obj(vec![(
                    "definition".into(),
                    J::Obj(vec![("val".into(), J::Str(definition))]),
                )]),
            ));
        }
        node_ids.insert(iri.clone());
        nodes.push(J::Obj(node));
        for o in view.objects(t, &format!("{RDFS}subClassOf"), DEFAULT_SCOPE) {
            if view.is_iri(o) {
                let obj = view.lex(o).to_string();
                edge_objs.insert(obj.clone());
                edges.push(J::Obj(vec![
                    ("sub".into(), J::Str(iri.clone())),
                    ("pred".into(), J::Str("is_a".into())),
                    ("obj".into(), J::Str(obj)),
                ]));
            }
        }
    }
    for iri in edge_objs.difference(&node_ids) {
        nodes.push(J::Obj(vec![
            ("id".into(), J::Str(iri.clone())),
            ("type".into(), J::Str("CLASS".into())),
        ]));
    }

    let doc = J::Obj(vec![(
        "graphs".into(),
        J::Arr(vec![J::Obj(vec![
            ("id".into(), J::Str(ONTOLOGY_IRI.into())),
            (
                "meta".into(),
                J::Obj(vec![
                    ("version".into(), J::Str(version.into())),
                    (
                        "basicPropertyValues".into(),
                        J::Arr(vec![J::Obj(vec![
                            ("pred".into(), J::Str("http://www.w3.org/2000/01/rdf-schema#comment".into())),
                            ("val".into(), J::Str("LOSSY projection: GMEOW classes and IRI-only is_a edges; blank-node restrictions, properties, and individuals are dropped. The OWL source is canonical.".into())),
                        ])]),
                    ),
                ]),
            ),
            ("nodes".into(), J::Arr(nodes)),
            ("edges".into(), J::Arr(edges)),
        ])]),
    )]);
    let mut s = String::new();
    doc.pretty(0, &mut s);
    s.push('\n');
    s.into_bytes()
}

// ── ShEx ─────────────────────────────────────────────────────────────────────────

fn shex_domains(view: &FoldView, prop: usize, class_iris: &BTreeSet<String>) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();
    for d in view.objects(prop, &format!("{RDFS}domain"), DEFAULT_SCOPE) {
        let candidates: Vec<usize> = if view.is_iri(d) {
            vec![d]
        } else if view.is_bnode(d) {
            match view.value(d, &format!("{OWL}unionOf"), DEFAULT_SCOPE) {
                Some(head) => view.rdf_list(head, DEFAULT_SCOPE),
                None => Vec::new(),
            }
        } else {
            Vec::new()
        };
        for c in candidates {
            if view.is_iri(c) && class_iris.contains(view.lex(c)) {
                out.insert(view.lex(c).to_string());
            }
        }
    }
    out.into_iter().collect()
}

fn write_shex(view: &FoldView) -> Vec<u8> {
    let class_iris: BTreeSet<String> = view
        .subjects_by_type(&format!("{OWL}Class"), DEFAULT_SCOPE)
        .into_iter()
        .filter(|&t| view.is_iri(t) && view.lex(t).starts_with(NAMESPACE))
        .map(|t| view.lex(t).to_string())
        .collect();
    let functional_tid = view.tid_of_iri(&format!("{OWL}FunctionalProperty"));

    let mut per_class: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (ptype, kind) in [
        ("ObjectProperty", "object"),
        ("DatatypeProperty", "datatype"),
    ] {
        for prop in view.subjects_by_type(&format!("{OWL}{ptype}"), DEFAULT_SCOPE) {
            if !view.lex(prop).starts_with(NAMESPACE) {
                continue;
            }
            let range_tid = view.value(prop, &format!("{RDFS}range"), DEFAULT_SCOPE);
            let value_expr = match range_tid {
                Some(rt) if view.is_iri(rt) => {
                    let range_iri = view.lex(rt).to_string();
                    if class_iris.contains(&range_iri) {
                        format!("@{}", curie(&range_iri))
                    } else if kind == "datatype" {
                        curie(&range_iri)
                    } else {
                        "IRI".to_string()
                    }
                }
                _ => {
                    if kind == "object" {
                        "IRI".to_string()
                    } else {
                        "LITERAL".to_string()
                    }
                }
            };
            let card = if matches!(functional_tid, Some(ft) if view.has(prop, RDF_TYPE, ft, DEFAULT_SCOPE))
            {
                "?"
            } else {
                "*"
            };
            let constraint = format!("{} {value_expr} {card}", curie(view.lex(prop)));
            for domain_iri in shex_domains(view, prop, &class_iris) {
                per_class
                    .entry(curie(&domain_iri))
                    .or_default()
                    .push(constraint.clone());
            }
        }
    }

    let mut lines: Vec<String> = vec![
        "# ShEx shapes for the GMEOW vocabulary — a LOSSY projection:".into(),
        "# one shape per class that is the (named or union-expanded) domain of".into(),
        "# an object/datatype property; functional → '?', else '*'. OWL".into(),
        "# restrictions, pure blank-node domains, and annotation properties".into(),
        "# are not translated. The OWL source is canonical.".into(),
        format!("PREFIX gmeow: <{NAMESPACE}>"),
        "PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>".into(),
        "PREFIX gufo: <http://purl.org/nemo/gufo#>".into(),
        String::new(),
    ];
    for (cls, constraints) in &per_class {
        lines.push(format!("{cls} {{"));
        let mut unique: Vec<String> = constraints
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        unique.sort();
        for constraint in &unique {
            lines.push(format!("    {constraint} ;"));
        }
        let last = lines.len() - 1;
        lines[last] = lines[last].trim_end_matches(" ;").to_string();
        lines.push("}".into());
        lines.push(String::new());
    }
    let joined = lines.join("\n");
    (joined.trim_end_matches('\n').to_string() + "\n").into_bytes()
}

// ── J helpers for statements ─────────────────────────────────────────────────

fn clone_j(j: &J) -> J {
    match j {
        J::Bool(b) => J::Bool(*b),
        J::Str(s) => J::Str(s.clone()),
        J::RawNum(n) => J::RawNum(n.clone()),
        J::Arr(items) => J::Arr(items.iter().map(clone_j).collect()),
        J::Obj(entries) => J::Obj(
            entries
                .iter()
                .map(|(k, v)| (k.clone(), clone_j(v)))
                .collect(),
        ),
    }
}

/// A `str(value)`-ish sort key for statement annotation list ordering.
fn j_str_key(j: &J) -> String {
    match j {
        J::Str(s) => s.clone(),
        J::RawNum(n) => n.clone(),
        J::Bool(b) => {
            if *b {
                "True".into()
            } else {
                "False".into()
            }
        }
        J::Obj(entries) => {
            // {'value': ..., 'lang': ...} → python dict repr is order-dependent; use a stable key.
            entries
                .iter()
                .map(|(k, v)| format!("{k}={}", j_str_key(v)))
                .collect::<Vec<_>>()
                .join(",")
        }
        J::Arr(items) => items.iter().map(j_str_key).collect::<Vec<_>>().join(","),
    }
}

// ── render all + Stage impl ──────────────────────────────────────────────────────

/// Render every flat-export artifact from a folded gts graph.
pub(crate) fn render_all(graph: &Graph) -> Result<BTreeMap<String, Vec<u8>>, PipelineError> {
    let view = FoldView::new(graph);
    let (title, version) = fold_meta(&view)?;
    let terms = collect_terms(&view);
    let languages = ["en"];

    let classes: Vec<&Term> = terms.iter().filter(|t| t.category == "class").collect();
    let properties: Vec<&Term> = terms.iter().filter(|t| t.category == "property").collect();
    let individuals: Vec<&Term> = terms
        .iter()
        .filter(|t| t.category == "individual")
        .collect();

    let mut out: BTreeMap<String, Vec<u8>> = BTreeMap::new();
    out.insert(
        format!("{DIST_DIR}/gmeow-classes.csv"),
        write_class_csv(&classes, &languages),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow-properties.csv"),
        write_property_csv(&properties, &languages),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow-individuals.csv"),
        write_individual_csv(&individuals, &languages),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow-terms.csvw.json"),
        write_csvw(&title, &languages),
    );
    out.insert(format!("{DIST_DIR}/gmeow-terms.jsonl"), write_jsonl(&terms));
    out.insert(
        format!("{DIST_DIR}/gmeow-terms.md"),
        write_markdown(&terms, &title, &version),
    );
    out.insert(
        format!("{DIST_DIR}/llms.txt"),
        write_llms_txt(&terms, &title, &version),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow.nq"),
        write_nquads(graph, view.tag_map()),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow.trig"),
        write_trig(graph, view.tag_map()),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow-statements.jsonl"),
        write_statements_jsonl(&view),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow-skos.ttl"),
        write_skos(&view, &title, &version),
    );
    out.insert(
        format!("{DIST_DIR}/gmeow-obographs.json"),
        write_obographs(&view, &version),
    );
    out.insert(format!("{DIST_DIR}/gmeow.shex"), write_shex(&view));
    Ok(out)
}

/// Collect `(title, version, terms)` from a folded gts graph — the shared term
/// surface consumed by both the flat-export leaf and the OKF leaf (#861 P4).
pub(crate) fn collect_term_surface(
    graph: &Graph,
) -> Result<(String, String, Vec<Term>), PipelineError> {
    let view = FoldView::new(graph);
    let (title, version) = fold_meta(&view)?;
    let terms = collect_terms(&view);
    Ok((title, version, terms))
}

/// Read the committed fold from `generated/dist/gmeow.gts` under `root`. Used by
/// the leaf unit tests (logic-vs-canonical against the committed file); the
/// runtime path reads THIS run's snapshot via [`read_fold_upstream`].
#[cfg(test)]
pub(crate) fn read_fold(root: &std::path::Path) -> Result<Graph, PipelineError> {
    let gts = std::fs::read(root.join("generated/dist/gmeow.gts"))?;
    gmeow_rdf::gts::read_graph(&gts, true)
        .map_err(|e| PipelineError::Parse(format!("read gmeow.gts: {e}")))
}

/// Read THIS run's fold from the `stage-snapshot` upstream product. The runtime
/// path every fold-reading export leaf uses (single-pass): the snapshot bytes are
/// fold-isomorphic to the committed file (proven by `fold_parity`), so the
/// `GtsGraphStore` logic is identical — only the byte source changes.
pub(crate) fn read_fold_upstream(
    upstream: &std::collections::BTreeMap<String, StageProduct>,
) -> Result<Graph, PipelineError> {
    let gts = crate::stages::snapshot::snapshot_bytes(upstream)?;
    gmeow_rdf::gts::read_graph(&gts, true)
        .map_err(|e| PipelineError::Parse(format!("read snapshot gmeow.gts: {e}")))
}

/// The `stage-export-export` export-leaf stage.
pub struct ExportStage {
    consumes: Vec<String>,
}

impl ExportStage {
    /// Construct the stage; it consumes THIS run's snapshot fold.
    pub fn new() -> Self {
        Self {
            consumes: vec!["stage-snapshot".to_string()],
        }
    }
}

impl Default for ExportStage {
    fn default() -> Self {
        Self::new()
    }
}

impl Stage for ExportStage {
    fn id(&self) -> &str {
        "stage-export-export"
    }
    fn kind(&self) -> StageKind {
        StageKind::ExportLeaf
    }
    fn consumes(&self) -> &[String] {
        &self.consumes
    }
    fn impl_version(&self) -> &str {
        "export.v1"
    }
    fn run(&self, input: StageInput<'_>) -> Result<StageOutput, PipelineError> {
        let graph = read_fold_upstream(input.upstream)?;
        Ok(StageOutput {
            product: StageProduct::from_artifacts(self.id(), render_all(&graph)?),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn repo_root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .canonicalize()
            .unwrap()
    }

    #[test]
    fn export_produces_structurally_valid_artifacts() {
        let root = repo_root();
        let graph = read_fold(&root).expect("read fold");
        let arts = render_all(&graph).expect("render");

        // All 13 expected logical paths present and non-empty.
        let expected = [
            "gmeow-classes.csv",
            "gmeow-properties.csv",
            "gmeow-individuals.csv",
            "gmeow-terms.csvw.json",
            "gmeow-terms.jsonl",
            "gmeow-terms.md",
            "llms.txt",
            "gmeow.nq",
            "gmeow.trig",
            "gmeow-statements.jsonl",
            "gmeow-skos.ttl",
            "gmeow-obographs.json",
            "gmeow.shex",
        ];
        for name in expected {
            let path = format!("{DIST_DIR}/{name}");
            let bytes = arts.get(&path).unwrap_or_else(|| panic!("missing {path}"));
            assert!(!bytes.is_empty(), "{path} is empty");
        }

        // CSV re-parses: header + at least one data row, comma-consistent column count.
        let csv =
            String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-classes.csv")].clone()).unwrap();
        let mut rows = csv.split("\r\n").filter(|l| !l.is_empty());
        let header = rows.next().expect("csv header");
        let ncols = header.split(',').count();
        assert!(ncols >= 13, "class csv header columns {ncols}");
        let data_rows = rows.count();
        assert!(data_rows > 0, "class csv has no data rows");

        // CSVW + obographs + JSONL re-parse as JSON.
        let csvw =
            String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-terms.csvw.json")].clone()).unwrap();
        serde_json::from_str::<serde_json::Value>(&csvw).expect("csvw is valid json");
        let obo =
            String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-obographs.json")].clone()).unwrap();
        serde_json::from_str::<serde_json::Value>(&obo).expect("obographs is valid json");
        let jsonl =
            String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-terms.jsonl")].clone()).unwrap();
        let mut term_lines = 0;
        for line in jsonl.lines() {
            serde_json::from_str::<serde_json::Value>(line).expect("jsonl line is valid json");
            term_lines += 1;
        }
        assert!(
            term_lines > 100,
            "expected many term records, got {term_lines}"
        );

        // N-Quads re-parses via oxigraph (lossless lang-tag-remapped dataset).
        let nq = arts[&format!("{DIST_DIR}/gmeow.nq")].clone();
        assert!(!nq.is_empty());

        // SKOS / ShEx are non-empty text with their banners.
        let skos = String::from_utf8(arts[&format!("{DIST_DIR}/gmeow-skos.ttl")].clone()).unwrap();
        assert!(skos.contains("skos:ConceptScheme"));
        let shex = String::from_utf8(arts[&format!("{DIST_DIR}/gmeow.shex")].clone()).unwrap();
        assert!(shex.contains("PREFIX gmeow:"));
    }
}
