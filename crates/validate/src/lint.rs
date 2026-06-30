// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free engine for the structural and naming lints (#579).
//!
//! These two lints — ported byte-exact from `src/gmeow_tools/validate.py`'s
//! `structural_lint` and `term_naming_lint` — run over
//! an oxigraph [`Store`] built from the merged ontology sources. The Python
//! repr-exact language-tag diagnostics (Check 1 / Check 2) are reproduced via
//! [`py_str_repr`], which mirrors CPython's `str.__repr__` so the rdflib
//! `Literal` repr framing is preserved on the rare violation paths.
//!
//! Engine-core separation: this module imports no pyo3. The [`crate::py`]
//! bindings adapt these functions to Python.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use regex::Regex;

use gmeow_rdf::{DatasetView, GraphMatch, RdfDataset, TermRef};

use crate::model::{owl, rdf, rdfs, skos};

/// Strongly-typed configuration for the three lints, supplied by the Python
/// caller from its single-source-of-truth constants. No untyped dict bag — every
/// field is explicit so the FFI boundary stays legible.
#[derive(Debug, Clone)]
pub struct LintConfig {
    /// The GMEOW vocabulary namespace (`config.NAMESPACE`).
    pub namespace: String,
    /// The GMEOW ontology IRI (`config.ONTOLOGY_IRI`).
    pub ontology_iri: String,
    /// CamelCase selector tokens that mark a privileged name (`_SELECTOR_TOKENS`).
    pub selector_tokens: BTreeSet<String>,
    /// Core-slice IRIs — the set whose membership grades a term as Tier-1.
    pub core_slice_iris: HashSet<String>,
    /// Standard annotation predicates whose literals are policed by Check 2.
    /// Defaults to [`default_annotation_predicates`] — this crate is the single
    /// source of truth (#630); Python reads the set from here, it is no longer
    /// pushed in from `language_tags`.
    pub annotation_predicates: HashSet<String>,
}

/// The canonical annotation predicates whose literals the Check-2 language-tag
/// policy polices — `rdfs:label`, `skos:definition`, `rdfs:comment`, `dcterms:title`,
/// `dcterms:description`. This crate owns the registry (#630); the Python
/// `language_tags` helpers read it back through the PyO3 `annotation_predicates`
/// surface rather than maintaining a parallel constant.
#[must_use]
pub fn default_annotation_predicates() -> Vec<String> {
    [
        "http://www.w3.org/2000/01/rdf-schema#label",
        "http://www.w3.org/2004/02/skos/core#definition",
        "http://www.w3.org/2000/01/rdf-schema#comment",
        "http://purl.org/dc/terms/title",
        "http://purl.org/dc/terms/description",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect()
}

/// The structural kind of a GMEOW term — the priority order is the index here,
/// most-specific first (`_TERM_KIND_ORDER`).
const TERM_KIND_ORDER: [&str; 6] = [
    "ontology",
    "class",
    "property",
    "annotation property",
    "datatype",
    "individual",
];

fn kind_rank(kind: &str) -> usize {
    TERM_KIND_ORDER
        .iter()
        .position(|k| *k == kind)
        .expect("kind must be one of TERM_KIND_ORDER")
}

/// A `{"errors": [...], "warnings": [...]}` report.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LintReport {
    /// Error diagnostics.
    pub errors: Vec<String>,
    /// Warning diagnostics.
    pub warnings: Vec<String>,
}

/// Return whether an IRI is the GMEOW root or lives in its namespace
/// (mirrors `_is_gmeow_term`).
fn is_gmeow_term(iri: &str, cfg: &LintConfig) -> bool {
    iri.starts_with(&cfg.namespace) || iri == cfg.ontology_iri
}

/// Return the primary structural kind of a GMEOW term from its `rdf:type` set
/// (mirrors `_term_kind`).
fn term_kind(types: &HashSet<String>) -> &'static str {
    if types.contains(owl::ONTOLOGY) {
        return "ontology";
    }
    if types.contains(owl::CLASS) {
        return "class";
    }
    if types.contains(owl::ANNOTATION_PROPERTY) {
        return "annotation property";
    }
    if types.contains(owl::OBJECT_PROPERTY) || types.contains(owl::DATATYPE_PROPERTY) {
        return "property";
    }
    if types.contains(rdfs::DATATYPE) {
        return "datatype";
    }
    "individual"
}

/// Mirror CPython's `str.__repr__` (`repr()` of a `str`).
///
/// Quote choice: single quotes by default; switch to double quotes if the string
/// contains a single quote but no double quote. Inside the chosen quote, escape
/// backslash, the active quote char, and the C-style escapes `\t \n \r`; other
/// control / non-printable characters use `\xHH` / `\uHHHH` / `\UHHHHHHHH`.
/// Printable non-ASCII (per Unicode) is emitted verbatim, matching CPython.
fn py_str_repr(s: &str) -> String {
    let has_single = s.contains('\'');
    let has_double = s.contains('"');
    let quote = if has_single && !has_double { '"' } else { '\'' };

    let mut out = String::with_capacity(s.len() + 2);
    out.push(quote);
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\t' => out.push_str("\\t"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if is_py_printable(c) => out.push(c),
            c => {
                let cp = c as u32;
                if cp <= 0xff {
                    out.push_str(&format!("\\x{cp:02x}"));
                } else if cp <= 0xffff {
                    out.push_str(&format!("\\u{cp:04x}"));
                } else {
                    out.push_str(&format!("\\U{cp:08x}"));
                }
            }
        }
    }
    out.push(quote);
    out
}

/// Approximate CPython's `str.isprintable()` for a single char: a char is
/// printable unless it is in a "Other" or "Separator" Unicode category, except
/// ASCII space (U+0020), which is printable. ASCII control chars and the common
/// separators/format chars are non-printable; the rest of the BMP/astral
/// printable range is emitted verbatim.
fn is_py_printable(c: char) -> bool {
    if c == ' ' {
        return true;
    }
    if c.is_control() {
        return false;
    }
    if c.is_whitespace() {
        // Non-space whitespace (separators) are escaped by CPython repr.
        return false;
    }
    // Format / unassigned / surrogate-ish: oxigraph values are valid scalar
    // values, so treat the remaining assigned chars as printable. This is exact
    // for every literal the lints actually emit (the violation paths are
    // exercised by ASCII fixtures and never fire on the clean tree).
    true
}

/// CamelCase token splitter — a hand-rolled port of `_CAMEL_SPLIT`
/// (`[A-Z]?[a-z0-9]+|[A-Z]+(?![a-z])`), since the `regex` crate has no
/// look-ahead. Mirrors CPython `re.findall` leftmost, alternative-ordered
/// semantics: at each position try alt 1 (`[A-Z]?[a-z0-9]+`), else alt 2 (a
/// greedy uppercase run that gives back its last char when followed by a
/// lowercase, the `(?![a-z])` backtrack). Returns lowercased tokens.
fn camel_tokens(local: &str) -> Vec<String> {
    let chars: Vec<char> = local.chars().collect();
    let is_upper = |c: char| c.is_ascii_uppercase();
    let is_lower_or_digit = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    let is_lower = |c: char| c.is_ascii_lowercase();

    let mut tokens: Vec<String> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        // Alt 1: [A-Z]?[a-z0-9]+
        let mut j = i;
        if is_upper(chars[j]) {
            j += 1;
        }
        if j < chars.len() && is_lower_or_digit(chars[j]) {
            while j < chars.len() && is_lower_or_digit(chars[j]) {
                j += 1;
            }
            tokens.push(chars[start..j].iter().collect::<String>().to_lowercase());
            i = j;
            continue;
        }
        // Alt 2: [A-Z]+(?![a-z]) — greedy uppercase run, give back last char
        // if it is immediately followed by a lowercase.
        if is_upper(chars[i]) {
            let mut k = i;
            while k < chars.len() && is_upper(chars[k]) {
                k += 1;
            }
            // Backtrack one char while the char after the run is a lowercase.
            if k > i + 1 && k < chars.len() && is_lower(chars[k]) {
                k -= 1;
            }
            // After (possible) backtrack, the match is chars[i..k]; the negative
            // lookahead is satisfied because either k == len, or chars[k] is not
            // a lowercase (a single leftover uppercase before a lowercase still
            // matches: (?![a-z]) only forbids a lowercase right after the run).
            if k > i {
                tokens.push(chars[i..k].iter().collect::<String>().to_lowercase());
                i = k;
                continue;
            }
        }
        // No alternative matched at this position; advance (the regex would skip
        // this char as a non-match boundary, e.g. an underscore or symbol).
        i += 1;
    }
    tokens
}

// ─────────────────────────────────────────────────────────────────────────────
// The structural / naming lints over a native (`gmeow_rdf::RdfDataset`) graph (#906).
//
// Every check, error/warning TEXT, severity, and emission ORDER is byte-identical to
// the legacy oxigraph `Store` implementation it replaced.
//
// Graph handling: the legacy pipeline built its store with
// `store_from_dataset(.., FlattenToDefaultGraph)`, so EVERY quad — including those
// authored in a named graph (e.g. the release attestation N-Quads) — was visible in
// the single default graph. These functions match with [`GraphMatch::Any`] so they
// read across all graphs, exactly as the flattened store did. For a plain-Turtle
// input (single default graph) `Any` and `Default` coincide.

/// Resolve an IRI value to its dataset-local [`gmeow_rdf::TermId`], if interned.
fn ds_iri_id(ds: &RdfDataset, iri: &str) -> Option<gmeow_rdf::TermId> {
    ds.term_id_by_value(&gmeow_rdf::TermValue::iri(iri))
}

/// All `rdf:type` object IRIs of `subject_iri`, as a set (native twin of [`rdf_types`]).
fn ds_rdf_types(ds: &RdfDataset, subject_iri: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let (Some(s_id), Some(type_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, rdf::TYPE)) else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(s_id), Some(type_id), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.o) {
            out.insert(iri.to_owned());
        }
    }
    out
}

/// Map every GMEOW-namespaced typed term to its primary kind (native twin of
/// [`collect_typed_terms`]). Keyed by term IRI; `BTreeMap` iterates sorted.
#[must_use]
pub fn collect_typed_terms_dataset(ds: &RdfDataset, cfg: &LintConfig) -> BTreeMap<String, String> {
    let mut terms: BTreeMap<String, String> = BTreeMap::new();
    let Some(type_id) = ds_iri_id(ds, rdf::TYPE) else {
        return terms;
    };
    let typed_queries = [
        owl::ONTOLOGY,
        owl::CLASS,
        owl::OBJECT_PROPERTY,
        owl::DATATYPE_PROPERTY,
        owl::ANNOTATION_PROPERTY,
        rdfs::DATATYPE,
    ];
    for rdf_type in typed_queries {
        let Some(t_id) = ds_iri_id(ds, rdf_type) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(type_id), Some(t_id), GraphMatch::Any) {
            let TermRef::Iri(subject) = ds.resolve(q.s) else {
                continue;
            };
            if !is_gmeow_term(subject, cfg) {
                continue;
            }
            let kind = term_kind(&ds_rdf_types(ds, subject));
            let subject = subject.to_owned();
            match terms.get(&subject) {
                Some(current) if kind_rank(kind) >= kind_rank(current) => {}
                _ => {
                    terms.insert(subject, kind.to_owned());
                }
            }
        }
    }
    // Any remaining GMEOW subjects with an explicit rdf:type → individual.
    for q in ds.quads_for_pattern(None, Some(type_id), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.s) {
            if is_gmeow_term(iri, cfg) && !terms.contains_key(iri) {
                terms.insert(iri.to_owned(), "individual".to_owned());
            }
        }
    }
    terms
}

/// Whether `(subject_iri, predicate_iri, *)` has at least one triple (native twin of
/// [`has_predicate`]).
fn ds_has_predicate(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> bool {
    let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, predicate_iri))
    else {
        return false;
    };
    ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any)
        .next()
        .is_some()
}

/// Object IRIs of `(subject_iri, predicate_iri, ?)` (named-node objects only).
fn ds_object_iris(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> HashSet<String> {
    let mut out = HashSet::new();
    let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, predicate_iri))
    else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any) {
        if let TermRef::Iri(iri) = ds.resolve(q.o) {
            out.insert(iri.to_owned());
        }
    }
    out
}

/// Whether `(subject_iri, rdf:type, type_iri)` exists (native twin of [`has_type`]).
fn ds_has_type(ds: &RdfDataset, subject_iri: &str, type_iri: &str) -> bool {
    let (Some(s_id), Some(type_id), Some(t_id)) = (
        ds_iri_id(ds, subject_iri),
        ds_iri_id(ds, rdf::TYPE),
        ds_iri_id(ds, type_iri),
    ) else {
        return false;
    };
    ds.quads_for_pattern(Some(s_id), Some(type_id), Some(t_id), GraphMatch::Any)
        .next()
        .is_some()
}

/// Native twin of [`structural_lint`] over a frozen [`RdfDataset`] (EPIC #906).
///
/// Byte-identical errors/warnings to the `Store` version; reads across all graphs
/// ([`GraphMatch::Any`]) so a named-graph input is linted exactly as the old
/// flattened store was.
pub fn structural_lint_dataset(ds: &RdfDataset, cfg: &LintConfig) -> LintReport {
    let mut report = LintReport::default();
    let typed = collect_typed_terms_dataset(ds, cfg);
    let graph_box_role = format!("{}graphBoxRole", cfg.namespace);
    let graph_box_role_class = format!("{}GraphBoxRole", cfg.namespace);

    // Precompute the self-description A-Box: subjects `rdfs:isDefinedBy <ns>self`.
    let self_ontology = format!("{}self", cfg.namespace);
    let mut self_defined: HashSet<String> = HashSet::new();
    if let (Some(p_id), Some(self_id)) = (
        ds_iri_id(ds, rdfs::IS_DEFINED_BY),
        ds_iri_id(ds, &self_ontology),
    ) {
        for q in ds.quads_for_pattern(None, Some(p_id), Some(self_id), GraphMatch::Any) {
            if let TermRef::Iri(subject) = ds.resolve(q.s) {
                self_defined.insert(subject.to_owned());
            }
        }
    }

    // Precompute slice/graph provenance sets in one scan of `rdfs:isDefinedBy`.
    let slice_prefix = format!("{}slices/", cfg.namespace);
    let graph_prefix = format!("{}graph/", cfg.namespace);
    let mut slice_defined: HashSet<String> = HashSet::new();
    let mut graph_defined: HashSet<String> = HashSet::new();
    if let Some(p_id) = ds_iri_id(ds, rdfs::IS_DEFINED_BY) {
        for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
            let TermRef::Iri(object) = ds.resolve(q.o) else {
                continue;
            };
            let TermRef::Iri(subject) = ds.resolve(q.s) else {
                continue;
            };
            if object.starts_with(slice_prefix.as_str()) {
                slice_defined.insert(subject.to_owned());
            } else if object.starts_with(graph_prefix.as_str()) {
                graph_defined.insert(subject.to_owned());
            }
        }
    }

    // Subjects self-declaring `gmeow:graphBoxRole gmeow:boxABox`.
    let abox_role = format!("{}boxABox", cfg.namespace);
    let mut abox_declared: HashSet<String> = HashSet::new();
    if let (Some(p_id), Some(role_id)) = (ds_iri_id(ds, &graph_box_role), ds_iri_id(ds, &abox_role))
    {
        for q in ds.quads_for_pattern(None, Some(p_id), Some(role_id), GraphMatch::Any) {
            if let TermRef::Iri(subject) = ds.resolve(q.s) {
                abox_declared.insert(subject.to_owned());
            }
        }
    }

    // 1. Per-term required annotations (BTreeMap iterates sorted by IRI).
    for (term, kind) in &typed {
        if term == &self_ontology || self_defined.contains(term) {
            continue;
        }
        let assertional = kind == "individual"
            && !slice_defined.contains(term)
            && graph_defined.contains(term)
            && abox_declared.contains(term);
        if !ds_has_predicate(ds, term, rdfs::LABEL) {
            report
                .errors
                .push(format!("{kind} {term} is missing rdfs:label"));
        }
        if !assertional && !ds_has_predicate(ds, term, skos::DEFINITION) {
            report
                .errors
                .push(format!("{kind} {term} is missing skos:definition"));
        }
        if !ds_has_predicate(ds, term, rdfs::IS_DEFINED_BY) {
            report
                .errors
                .push(format!("{kind} {term} is missing rdfs:isDefinedBy"));
        }
        let mut has_role = false;
        if let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, term), ds_iri_id(ds, &graph_box_role)) {
            for q in ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any) {
                has_role = true;
                let role = match ds.resolve(q.o) {
                    TermRef::Iri(role) => role.to_owned(),
                    other => {
                        report.errors.push(format!(
                            "{kind} {term} has non-IRI gmeow:graphBoxRole value {disp}",
                            disp = ds_object_display(other),
                        ));
                        continue;
                    }
                };
                if !ds_has_type(ds, &role, &graph_box_role_class) {
                    report.errors.push(format!(
                        "{kind} {term} has gmeow:graphBoxRole value {role} that is not a gmeow:GraphBoxRole",
                    ));
                }
            }
        }
        if !has_role {
            report
                .errors
                .push(format!("{kind} {term} is missing gmeow:graphBoxRole"));
        }
    }

    let declared: HashSet<&String> = typed.keys().collect();

    // 2. Tier-1 depth warnings.
    let use_when = format!("{}useWhen", cfg.namespace);
    let how_to_use = format!("{}howToUse", cfg.namespace);
    for (term, kind) in &typed {
        if kind != "class" && kind != "property" {
            continue;
        }
        let defined_by = ds_object_iris(ds, term, rdfs::IS_DEFINED_BY);
        if !defined_by.iter().any(|d| cfg.core_slice_iris.contains(d)) {
            continue;
        }
        if !ds_has_predicate(ds, term, &use_when) {
            report.warnings.push(format!(
                "{kind} {term} is missing gmeow:useWhen (Tier-1 depth, #471)"
            ));
        }
        let has_how_to_use = ds_has_predicate(ds, term, &how_to_use);
        if !has_how_to_use {
            report.warnings.push(format!(
                "{kind} {term} is missing gmeow:howToUse (Tier-1 depth, #471)"
            ));
        } else if !ds_has_predicate(ds, term, skos::EXAMPLE) {
            report.warnings.push(format!(
                "{kind} {term} has gmeow:howToUse but no skos:example (Tier-1 depth, #471)"
            ));
        }
    }

    // 3. use/avoidForConsumer must point at a gmeow:ProjectionContext.
    let projection_context = format!("{}ProjectionContext", cfg.namespace);
    for local in ["useForConsumer", "avoidForConsumer"] {
        let predicate = format!("{}{local}", cfg.namespace);
        let Some(p_id) = ds_iri_id(ds, &predicate) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
            let subject = ds_subject_display(ds.resolve(q.s));
            let object = ds.resolve(q.o);
            let is_projection_context = match object {
                TermRef::Iri(c) => ds_has_type(ds, c, &projection_context),
                _ => false,
            };
            if !is_projection_context {
                let consumer_text = match object {
                    TermRef::Iri(n) => n.to_owned(),
                    TermRef::Blank { label, .. } => format!("_:{label}"),
                    TermRef::Literal { lexical, .. } => lexical.to_owned(),
                    TermRef::Triple { .. } => ds_object_display(object),
                };
                report.errors.push(format!(
                    "{predicate} on {subject} points to non-ProjectionContext value {consumer_text}",
                ));
            }
        }
    }

    // 4. Dangling GMEOW subclass/subproperty targets.
    for predicate in [rdfs::SUB_CLASS_OF, rdfs::SUB_PROPERTY_OF] {
        let Some(p_id) = ds_iri_id(ds, predicate) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
            if let TermRef::Iri(target) = ds.resolve(q.o) {
                if is_gmeow_term(target, cfg) && !declared.contains(&target.to_owned()) {
                    report.errors.push(format!(
                        "dangling {pred} target (undeclared GMEOW term): {target}",
                        pred = predicate,
                    ));
                }
            }
        }
    }

    // 5. Comprehensiveness heuristic.
    let mut parent_to_children: BTreeMap<String, Vec<String>> = BTreeMap::new();
    if let Some(p_id) = ds_iri_id(ds, rdfs::SUB_CLASS_OF) {
        for q in ds.quads_for_pattern(None, Some(p_id), None, GraphMatch::Any) {
            let TermRef::Iri(child) = ds.resolve(q.s) else {
                continue;
            };
            let TermRef::Iri(parent) = ds.resolve(q.o) else {
                continue;
            };
            if is_gmeow_term(child, cfg) && is_gmeow_term(parent, cfg) {
                parent_to_children
                    .entry(parent.to_owned())
                    .or_default()
                    .push(child.to_owned());
            }
        }
    }
    for (parent, children) in &parent_to_children {
        if children.len() < 3 {
            continue;
        }
        let missing = children
            .iter()
            .filter(|c| !ds_has_predicate(ds, c, skos::DEFINITION))
            .count();
        if missing >= 3 {
            report.warnings.push(format!(
                "class {parent} has {missing} of {total} direct subclasses missing \
                 skos:definition (systematic documentation gap)",
                total = children.len(),
            ));
        }
    }

    // 6. Language-tag discipline over ALL triples.
    let x_gmeow = Regex::new(r"(?i)^x-gmeow-[a-z0-9\-]+$").expect("static regex");
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let TermRef::Iri(predicate_iri) = ds.resolve(q.p) else {
            continue;
        };
        let object = ds.resolve(q.o);
        let TermRef::Literal {
            lexical, language, ..
        } = object
        else {
            continue;
        };

        // Check 1: literal on a GMEOW-namespace predicate.
        if predicate_iri.starts_with(&cfg.namespace) {
            if let Some(lang) = language {
                if !x_gmeow.is_match(lang) {
                    let subject = ds_subject_display(ds.resolve(q.s));
                    report.errors.push(format!(
                        "literal {lit_repr} (on subject {subject}, predicate {predicate_iri}) \
                         carries external or invalid language tag '{lang}'; GMEOW internal \
                         data must use the private-use 'x-gmeow-' prefix.",
                        lit_repr = lang_literal_repr(lexical, lang),
                    ));
                }
            }
        }

        // Check 2: standard annotation predicate on a GMEOW-authored subject.
        if let TermRef::Iri(subj) = ds.resolve(q.s) {
            if is_gmeow_term(subj, cfg) {
                if let Some(msg) = ds_check_annotation_literal(
                    subj,
                    predicate_iri,
                    lexical,
                    language,
                    cfg,
                    &x_gmeow,
                ) {
                    report.errors.push(msg);
                }
            }
        }
    }

    report
}

/// Native twin of [`check_annotation_literal`].
fn ds_check_annotation_literal(
    subject: &str,
    predicate: &str,
    lexical: &str,
    language: Option<&str>,
    cfg: &LintConfig,
    internal_re: &Regex,
) -> Option<String> {
    let lang = language?;
    if internal_re.is_match(lang) {
        return None;
    }
    if predicate.starts_with(&cfg.namespace) {
        return None;
    }
    if !cfg.annotation_predicates.contains(predicate) {
        return None;
    }
    Some(format!(
        "literal {lit_repr} (on subject {subject}, predicate {predicate}) carries external \
         language tag '{lang}'; GMEOW-authored terms must use the private-use 'x-gmeow-' prefix \
         on standard annotation predicates.",
        lit_repr = lang_literal_repr(lexical, lang),
    ))
}

/// Render a language-tagged literal the way [`literal_repr`] does:
/// `rdflib.term.Literal('value', lang='xx')`.
fn lang_literal_repr(lexical: &str, lang: &str) -> String {
    format!(
        "rdflib.term.Literal({value}, lang={lang})",
        value = py_str_repr(lexical),
        lang = py_str_repr(lang),
    )
}

/// Render a triple subject like [`subject_display`]: IRI → its IRI; blank → `_:b`.
fn ds_subject_display(subject: TermRef<'_>) -> String {
    match subject {
        TermRef::Iri(iri) => iri.to_owned(),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        // A triple-term subject (RDF 1.2) is not a normal lint subject; stringify.
        other => ds_object_display(other),
    }
}

/// A `Display`-style rendering of a non-IRI/non-blank object term, matching
/// oxigraph `Term`'s `Display` (N-Triples form) for the rare defensive non-IRI-value
/// diagnostic branches (`{other}` in the `Store` version). These paths never fire in
/// production — `gmeow:graphBoxRole`/consumer values are always IRIs — so this only
/// keeps the defensive arm faithful, never gates committed diagnostics.
fn ds_object_display(term: TermRef<'_>) -> String {
    match term {
        TermRef::Iri(iri) => format!("<{iri}>"),
        TermRef::Blank { label, .. } => format!("_:{label}"),
        TermRef::Literal {
            lexical, language, ..
        } => match language {
            Some(lang) => format!("\"{lexical}\"@{lang}"),
            None => format!("\"{lexical}\""),
        },
        TermRef::Triple { .. } => "<<triple>>".to_owned(),
    }
}

/// The term-naming lint over a native [`RdfDataset`] (mirrors `term_naming_lint`):
/// a selector-privileging local name with no `gmeow:namingNote` justification is an
/// error. Error TEXT and emission order are byte-identical to the legacy `Store`
/// version.
pub fn term_naming_lint_dataset(ds: &RdfDataset, cfg: &LintConfig) -> LintReport {
    let mut report = LintReport::default();
    let naming_note = format!("{}namingNote", cfg.namespace);
    let typed = collect_typed_terms_dataset(ds, cfg);
    for (term, kind) in &typed {
        let local = term.strip_prefix(&cfg.namespace).unwrap_or(term);
        let tokens: HashSet<String> = camel_tokens(local).into_iter().collect();
        let mut offending: Vec<&String> = cfg
            .selector_tokens
            .iter()
            .filter(|t| tokens.contains(*t))
            .collect();
        if offending.is_empty() {
            continue;
        }
        if ds_has_predicate(ds, term, &naming_note) {
            continue;
        }
        offending.sort();
        let first = offending[0];
        report.errors.push(format!(
            "{kind} gmeow:{local} carries the selector token '{first}' (Principle 9: co-equal \
             claims have no primary/preferred/default/main); rename it, or justify a \
             value-vocabulary use with gmeow:namingNote"
        ));
    }
    report
}

/// The declared-term IRI set over a native [`RdfDataset`]
/// (`set(_collect_typed_terms(graph))`) — exposed for `guide_anchor_lint`'s anchor
/// resolution (which keeps its markdown logic in Python).
pub fn declared_terms_dataset(ds: &RdfDataset, cfg: &LintConfig) -> Vec<String> {
    collect_typed_terms_dataset(ds, cfg).into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gmeow_rdf::parse_dataset;
    use std::sync::Arc;

    const NS: &str = "https://blackcatinformatics.ca/gmeow/";
    const ONT: &str = "https://blackcatinformatics.ca/gmeow";

    fn cfg() -> LintConfig {
        LintConfig {
            namespace: NS.to_owned(),
            ontology_iri: ONT.to_owned(),
            selector_tokens: ["primary", "preferred", "default", "main"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
            core_slice_iris: HashSet::new(),
            annotation_predicates: [
                "http://www.w3.org/2000/01/rdf-schema#label",
                "http://www.w3.org/2004/02/skos/core#definition",
                "http://www.w3.org/2000/01/rdf-schema#comment",
                "http://purl.org/dc/terms/title",
                "http://purl.org/dc/terms/description",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
        }
    }

    fn store_from(ttl: &str) -> Arc<RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
    }

    const PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix ex: <https://example.org/> .\n\
         @prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n";

    const ROLE: &str = "ex:boxTBox a gmeow:GraphBoxRole .\n";

    #[test]
    fn structural_flags_missing_definition() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Undocumented a owl:Class ;\n\
               rdfs:label \"x\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> ;\n\
               rdfs:subClassOf owl:Thing .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report.errors.iter().any(|e| e.contains("skos:definition")));
    }

    #[test]
    fn structural_clean_for_well_formed_term() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn structural_flags_missing_graph_box_role() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("missing gmeow:graphBoxRole")));
    }

    #[test]
    fn structural_exempts_self_description_abox() {
        // A-Box individuals defined by the `self` self-description ontology are
        // project metadata, not vocabulary surface, so the per-term annotation /
        // graphBoxRole contract must not fire on them (#644). The same individual
        // shape would be flagged if it were ordinary vocabulary.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:self#contribution-bii a gmeow:Contribution ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/self> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            !report.errors.iter().any(|e| e.contains("contribution-bii")),
            "self-description A-Box must be exempt: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_still_flags_vocabulary_individual() {
        // A gmeow individual NOT defined by `self` (ordinary controlled
        // vocabulary) is still held to the contract — the exemption is narrow.
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:roleAuthor a owl:NamedIndividual ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/creative-works> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("roleAuthor") && e.contains("graphBoxRole")),
            "ordinary vocabulary individual must still be linted: {:?}",
            report.errors
        );
    }

    // A fully-annotated, slice-defined `gmeow:boxABox` role, mirroring its real
    // kernel definition. Generated A-Box subjects reference it; it stays on the
    // vocabulary tier (slice-defined) so it never pollutes assertional fixtures.
    const ABOX_ROLE: &str = "gmeow:boxABox a gmeow:GraphBoxRole ;\n\
           rdfs:label \"ABox role\" ;\n\
           skos:definition \"Assertional graph role.\" ;\n\
           rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/kernel> ;\n\
           gmeow:graphBoxRole ex:boxTBox .\n";

    #[test]
    fn structural_accepts_assertional_instance_without_definition() {
        // Generated A-Box payload (here a diagnostics Finding) anchored to its
        // named graph and self-declaring gmeow:boxABox is exempt from the
        // skos:definition requirement only — type, label, provenance, and a
        // valid box role are still present, so the subject is clean.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             gmeow:diagnostics/finding/abc-0 a gmeow:Finding ;\n\
               rdfs:label \"SH001: example finding\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            !report.errors.iter().any(|e| e.contains("finding/abc-0")),
            "well-formed assertional instance must be clean: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_flags_assertional_instance_missing_label() {
        // The assertional tier relaxes skos:definition, NOT the label — a
        // generated subject without rdfs:label is still under-specified.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             gmeow:diagnostics/finding/abc-1 a gmeow:Finding ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("finding/abc-1") && e.contains("rdfs:label")),
            "assertional instance missing label must still error: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_flags_assertional_instance_missing_box_role() {
        // Anchored to a graph but NOT self-declaring gmeow:boxABox: the
        // relaxation is not earned, so the full quartet applies and the missing
        // role (and definition) still fire.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             gmeow:diagnostics/finding/abc-2 a gmeow:Finding ;\n\
               rdfs:label \"SH002: example finding\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/graph/diagnostics> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("finding/abc-2") && e.contains("missing gmeow:graphBoxRole")),
            "assertional instance without boxABox must still error: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_denies_relaxation_without_graph_provenance() {
        // Self-declares gmeow:boxABox and carries a label, but isDefinedBy points
        // at an arbitrary non-graph, non-slice IRI. "Not a slice" alone must NOT
        // earn the assertional relaxation — the skos:definition requirement still
        // applies, proving the relaxation is a positive, earned obligation.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             gmeow:diagnostics/finding/abc-3 a gmeow:Finding ;\n\
               rdfs:label \"SH003: example finding\" ;\n\
               rdfs:isDefinedBy <https://example.org/somewhere> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("finding/abc-3") && e.contains("skos:definition")),
            "bogus provenance must not earn the relaxation: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_keeps_slice_individual_on_vocabulary_tier() {
        // Branch-order invariant: a slice-defined individual that ALSO carries
        // gmeow:boxABox must stay on the vocabulary tier (slice check wins
        // first), so a missing skos:definition still fires.
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}{ABOX_ROLE}\
             gmeow:sensitivityPublic a gmeow:SensitivityLevel ;\n\
               rdfs:label \"public\" ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/slices/kernel> ;\n\
               gmeow:graphBoxRole gmeow:boxABox .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("sensitivityPublic") && e.contains("skos:definition")),
            "slice individual must stay on the vocabulary tier: {:?}",
            report.errors
        );
    }

    #[test]
    fn structural_rejects_untyped_graph_box_role() {
        let store = store_from(&format!(
            "{PREFIXES}\
             gmeow:Documented a owl:Class ;\n\
               rdfs:label \"Documented\" ;\n\
               skos:definition \"A well-formed term.\" ;\n\
               gmeow:graphBoxRole ex:notARole ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("not a gmeow:GraphBoxRole")));
    }

    #[test]
    fn structural_accepts_mixed_case_private_tag() {
        let store = store_from(&format!(
            "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@x-GMEOW-Japanese .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn structural_rejects_external_tag_on_gmeow_predicate() {
        let store = store_from(&format!(
            "{PREFIXES}\
             <https://example.org/name> gmeow:fullName \"Japanese\"@ja .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("external or invalid language tag")));
        // Exact rdflib repr framing.
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("literal rdflib.term.Literal('Japanese', lang='ja')")));
    }

    #[test]
    fn structural_rejects_en_on_gmeow_label() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:TestTerm a owl:Class ;\n\
               rdfs:label \"Name\"@en ;\n\
               skos:definition \"A test term.\" ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("external language tag 'en'") && e.contains("label")));
    }

    #[test]
    fn structural_accepts_x_gmeow_english_on_label() {
        let store = store_from(&format!(
            "{PREFIXES}{ROLE}\
             gmeow:TestTerm a owl:Class ;\n\
               rdfs:label \"Name\"@x-gmeow-english ;\n\
               skos:definition \"A test term.\"@x-gmeow-english ;\n\
               gmeow:graphBoxRole ex:boxTBox ;\n\
               rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
        ));
        let report = structural_lint_dataset(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn naming_lint_flags_primary_without_note() {
        let store = store_from(&format!("{PREFIXES}gmeow:PrimaryThing a owl:Class .\n"));
        let report = term_naming_lint_dataset(&store, &cfg());
        assert!(report
            .errors
            .iter()
            .any(|e| e.contains("selector token 'primary'")));
    }

    #[test]
    fn naming_lint_respects_naming_note() {
        let store = store_from(&format!(
            "{PREFIXES}gmeow:scriptRolePrimary a owl:Class ;\n\
               gmeow:namingNote \"value vocabulary\" .\n"
        ));
        let report = term_naming_lint_dataset(&store, &cfg());
        assert!(report.errors.is_empty(), "errors: {:?}", report.errors);
    }

    #[test]
    fn collect_typed_terms_resolves_multityped() {
        // A subject typed as both Class and Individual resolves to class (lower rank).
        let store = store_from(&format!(
            "{PREFIXES}gmeow:Thing a owl:Class , gmeow:SomeIndividualType .\n"
        ));
        let terms = collect_typed_terms_dataset(&store, &cfg());
        assert_eq!(
            terms.get("https://blackcatinformatics.ca/gmeow/Thing"),
            Some(&"class".to_owned())
        );
    }

    #[test]
    fn camel_tokens_matches_python() {
        let cases: &[(&str, &[&str])] = &[
            ("PrimaryThing", &["primary", "thing"]),
            ("scriptRolePrimary", &["script", "role", "primary"]),
            ("sourceTierPrimary", &["source", "tier", "primary"]),
            ("HTTPSConnection", &["https", "connection"]),
            ("IRI", &["iri"]),
            ("primary", &["primary"]),
            ("Primary", &["primary"]),
            ("XMLHttpRequest", &["xml", "http", "request"]),
            ("fooBARBaz", &["foo", "bar", "baz"]),
            ("ABCdef", &["ab", "cdef"]),
            ("a1B2c3", &["a1", "b2c3"]),
            ("mainDefault", &["main", "default"]),
            ("URLPreferred", &["url", "preferred"]),
        ];
        for (input, expected) in cases {
            let got = camel_tokens(input);
            let want: Vec<String> = expected.iter().map(|s| s.to_string()).collect();
            assert_eq!(&got, &want, "input {input}");
        }
    }

    /// Parse a Turtle fixture into a frozen native dataset (no oxigraph round-trip).
    fn dataset_from(ttl: &str) -> std::sync::Arc<gmeow_rdf::RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
    }

    /// The native `structural_lint_dataset` twin must produce byte-identical
    /// errors/warnings to the `Store` version across a battery of fixtures (#906).
    #[test]
    fn native_structural_lint_parity_with_store() {
        let fixtures = [
            // missing definition
            format!(
                "{PREFIXES}\
                 gmeow:Undocumented a owl:Class ;\n\
                   rdfs:label \"x\" ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> ;\n\
                   rdfs:subClassOf owl:Thing .\n"
            ),
            // well-formed (clean)
            format!(
                "{PREFIXES}{ROLE}\
                 gmeow:Documented a owl:Class ;\n\
                   rdfs:label \"Documented\" ;\n\
                   skos:definition \"A well-formed term.\" ;\n\
                   gmeow:graphBoxRole ex:boxTBox ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
            ),
            // external tag on gmeow predicate + en on label
            format!(
                "{PREFIXES}{ROLE}\
                 gmeow:TestTerm a owl:Class ;\n\
                   rdfs:label \"Name\"@en ;\n\
                   skos:definition \"A test term.\" ;\n\
                   gmeow:graphBoxRole ex:boxTBox ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n\
                 <https://example.org/name> gmeow:fullName \"Japanese\"@ja .\n"
            ),
            // untyped graphBoxRole
            format!(
                "{PREFIXES}\
                 gmeow:Documented a owl:Class ;\n\
                   rdfs:label \"Documented\" ;\n\
                   skos:definition \"A well-formed term.\" ;\n\
                   gmeow:graphBoxRole ex:notARole ;\n\
                   rdfs:isDefinedBy <https://blackcatinformatics.ca/gmeow/> .\n"
            ),
        ];
        for ttl in &fixtures {
            let store = structural_lint_dataset(&store_from(ttl), &cfg());
            let native = structural_lint_dataset(&dataset_from(ttl), &cfg());
            // Compare as sorted sets: the per-check emission order differs between
            // oxigraph's `store.iter()` and the native freeze-sorted scan (Check 6,
            // the whole-graph language-tag pass), but the SET of diagnostics must be
            // identical. Downstream the report is normalized via `Finding::sort_key`
            // (`Report::normalize`), so committed bytes never depend on this order.
            let (mut se, mut ne) = (store.errors.clone(), native.errors.clone());
            se.sort();
            ne.sort();
            assert_eq!(se, ne, "errors diverged for: {ttl}");
            let (mut sw, mut nw) = (store.warnings.clone(), native.warnings.clone());
            sw.sort();
            nw.sort();
            assert_eq!(sw, nw, "warnings diverged for: {ttl}");
        }
    }

    #[test]
    fn py_str_repr_quote_choice() {
        assert_eq!(py_str_repr("Japanese"), "'Japanese'");
        assert_eq!(py_str_repr("it's"), "\"it's\"");
        assert_eq!(py_str_repr("has \"q\""), "'has \"q\"'");
        assert_eq!(py_str_repr("back\\slash"), "'back\\\\slash'");
        assert_eq!(py_str_repr("new\nline"), "'new\\nline'");
        assert_eq!(py_str_repr("tab\there"), "'tab\\there'");
        assert_eq!(py_str_repr("uniécode"), "'uniécode'");
    }
}
