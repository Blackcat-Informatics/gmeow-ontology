// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! PyO3-free engine for the structural and naming lints.
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

use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef};

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
    /// source of truth; Python reads the set from here, it is no longer
    /// pushed in from `language_tags`.
    pub annotation_predicates: HashSet<String>,
}

/// The canonical annotation predicates whose literals the Check-2 language-tag
/// policy polices — `rdfs:label`, `skos:definition`, `rdfs:comment`, `dcterms:title`,
/// `dcterms:description`. This crate owns the registry; the Python
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
// The structural / naming lints over a native (`purrdf::RdfDataset`) graph.
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

/// Resolve an IRI value to its dataset-local [`purrdf::TermId`], if interned.
fn ds_iri_id(ds: &RdfDataset, iri: &str) -> Option<purrdf::TermId> {
    ds.term_id_by_value(&purrdf::TermValue::iri(iri))
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

/// Native twin of [`structural_lint`] over a frozen [`RdfDataset`].
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
                "{kind} {term} is missing gmeow:useWhen (Tier-1 depth)"
            ));
        }
        let has_how_to_use = ds_has_predicate(ds, term, &how_to_use);
        if !has_how_to_use {
            report.warnings.push(format!(
                "{kind} {term} is missing gmeow:howToUse (Tier-1 depth)"
            ));
        } else if !ds_has_predicate(ds, term, skos::EXAMPLE) {
            report.warnings.push(format!(
                "{kind} {term} has gmeow:howToUse but no skos:example (Tier-1 depth)"
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

    // lang: meaning-stratum native gates (charter primary gates): compositional-
    // lowering preservation, co-resident-reading non-collapse, and the whole-bundle
    // one-way lang:->logic: bridge acyclicity.
    check_lang_meaning_invariants(ds, cfg, &mut report);

    // math: measure-and-dimension reasoned gate — dimensional homogeneity computed
    // from the exact-rational (ℚ⁷) exponent vectors, not asserted data.
    check_math_dimension_invariants(ds, &mut report);

    report
}

/// Namespace roots for the `lang:`/`logic:` meaning-stratum invariants.
const LANG_NS: &str = "https://blackcatinformatics.ca/lang/";
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

fn lang_iri(term: &str) -> String {
    format!("{LANG_NS}{term}")
}

fn logic_iri(term: &str) -> String {
    format!("{LOGIC_NS}{term}")
}

/// Subjects carrying an explicit `rdf:type` of `type_iri`.
fn ds_subjects_of_type(ds: &RdfDataset, type_iri: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(type_id), Some(t_id)) = (ds_iri_id(ds, rdf::TYPE), ds_iri_id(ds, type_iri)) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(type_id), Some(t_id), GraphMatch::Any) {
        if let TermRef::Iri(s) = ds.resolve(q.s) {
            out.push(s.to_owned());
        }
    }
    // `GraphMatch::Any` visits every graph, so a typing triple repeated across the
    // default graph and named graphs (or across imported fixtures) yields the same
    // subject more than once. Collapse them so downstream gates lint — and report —
    // each subject exactly once.
    out.sort();
    out.dedup();
    out
}

/// The `lang:` meaning-stratum invariants the charter designates as native
/// Rust-validator primary gates (realized here rather than in SHACL), plus the
/// whole-bundle one-way-bridge acyclicity. Runs over the merged dataset, so the
/// invariants hold bundle-wide, not merely per fixture.
fn check_lang_meaning_invariants(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    check_undeclared_lowering(ds, report);
    check_silent_disambiguation(ds, cfg, report);
    check_one_way_bridge(ds, report);
}

/// `lang:UndeclaredLoweringStage` — every `lang:Denotation` whose kind bridges
/// into `logic:` declares a `logic:preservationKind`. Being a lowering is derived
/// from the kind, never an optional flag, so the gate cannot fail open.
fn check_undeclared_lowering(ds: &RdfDataset, report: &mut LintReport) {
    let bridge_kinds = [
        lang_iri("denotesLogicFormula"),
        lang_iri("denotesLogicTerm"),
        lang_iri("denotesLogicType"),
        lang_iri("denotesQuery"),
    ];
    let denotation_kind = lang_iri("denotationKind");
    let preservation_kind = logic_iri("preservationKind");
    for subj in ds_subjects_of_type(ds, &lang_iri("Denotation")) {
        let kinds = ds_object_iris(ds, &subj, &denotation_kind);
        let bridges = kinds.iter().any(|k| bridge_kinds.iter().any(|b| b == k));
        if bridges && !ds_has_predicate(ds, &subj, &preservation_kind) {
            report.errors.push(format!(
                "lang:UndeclaredLoweringStage: denotation {subj} bridges into logic: \
                 (lang:denotationKind) but declares no logic:preservationKind"
            ));
        }
    }
}

/// `lang:SilentDisambiguation` — an interpretation act that resolves to a single
/// reading among two or more co-resident readings must be backed by a vantage-held
/// `gmeow:Observation` (through `lang:aboutReading`, with `gmeow:vantage`);
/// otherwise it has silently collapsed the ambiguity.
fn check_silent_disambiguation(ds: &RdfDataset, cfg: &LintConfig, report: &mut LintReport) {
    let produced = lang_iri("producedReading");
    let resolved = lang_iri("resolvedReading");
    let about_reading = lang_iri("aboutReading");
    let vantage = format!("{}vantage", cfg.namespace);
    for act in ds_subjects_of_type(ds, &lang_iri("InterpretationAct")) {
        let readings = ds_object_iris(ds, &act, &produced);
        if readings.len() < 2 {
            continue;
        }
        for chosen in ds_object_iris(ds, &act, &resolved) {
            if !reading_claim_is_grounded(ds, &about_reading, &chosen, &vantage) {
                report.errors.push(format!(
                    "lang:SilentDisambiguation: interpretation act {act} resolves to reading \
                     {chosen} among {} co-resident readings with no vantage-held observation \
                     grounding the choice",
                    readings.len()
                ));
            }
        }
    }
}

/// Whether some subject names `chosen` through `lang:aboutReading` and carries a
/// `gmeow:vantage` — a grounded reading-correctness claim.
fn reading_claim_is_grounded(
    ds: &RdfDataset,
    about_reading: &str,
    chosen: &str,
    vantage: &str,
) -> bool {
    let (Some(p_id), Some(o_id)) = (ds_iri_id(ds, about_reading), ds_iri_id(ds, chosen)) else {
        return false;
    };
    for q in ds.quads_for_pattern(None, Some(p_id), Some(o_id), GraphMatch::Any) {
        if let TermRef::Iri(obs) = ds.resolve(q.s) {
            if ds_has_predicate(ds, obs, vantage) {
                return true;
            }
        }
    }
    false
}

/// One-way bridge acyclicity (Principle 19): no `logic:`-namespaced subject carries
/// a `lang:`-namespaced predicate. The bridge runs `lang:` -> `logic:` through
/// `lang:denotationTarget` and never reverses.
fn check_one_way_bridge(ds: &RdfDataset, report: &mut LintReport) {
    for q in ds.quads_for_pattern(None, None, None, GraphMatch::Any) {
        let (TermRef::Iri(s), TermRef::Iri(p)) = (ds.resolve(q.s), ds.resolve(q.p)) else {
            continue;
        };
        if s.starts_with(LOGIC_NS) && p.starts_with(LANG_NS) {
            report.errors.push(format!(
                "lang: one-way bridge violated: logic: subject {s} carries lang: predicate {p} \
                 (Principle 19: the lang:->logic: bridge never reverses)"
            ));
        }
    }
}

/// Namespace root for the `math:` measure-and-dimension invariants.
const MATH_NS: &str = "https://blackcatinformatics.ca/math/";

fn math_iri(term: &str) -> String {
    format!("{MATH_NS}{term}")
}

/// The seven SI base dimensions, in canonical ℚ⁷ index order. A dimension vector
/// is a length-7 array of exact rationals over these generators.
const BASE_DIMENSIONS: [&str; 7] = [
    "lengthDimension",
    "massDimension",
    "timeDimension",
    "electricCurrentDimension",
    "temperatureDimension",
    "amountOfSubstanceDimension",
    "luminousIntensityDimension",
];

/// Position of a base-dimension IRI in the canonical ℚ⁷ order, if it is one of the
/// seven SI generators.
fn base_dimension_index(iri: &str) -> Option<usize> {
    BASE_DIMENSIONS.iter().position(|b| iri == math_iri(b))
}

/// An exact rational, always stored reduced with a positive denominator so that
/// derived `PartialEq`/`Eq` is value equality — dimensions are equal exactly when
/// their exponent vectors are equal (the derived `math:commensurableWith`), and
/// exact rationals (not `xsd:decimal`) keep that equality precise for fractional
/// dimensions such as T^(-1/2).
#[derive(Clone, Copy, PartialEq, Eq)]
struct Rat {
    num: i128,
    den: i128,
}

fn gcd_u128(a: u128, b: u128) -> u128 {
    if b == 0 {
        a
    } else {
        gcd_u128(b, a % b)
    }
}

impl Rat {
    fn zero() -> Self {
        Rat { num: 0, den: 1 }
    }

    /// Reduce `num/den` to canonical form (positive denominator, gcd 1). `None` on a
    /// zero denominator or on i128 overflow of the reduction.
    fn new(num: i128, den: i128) -> Option<Self> {
        if den == 0 {
            return None;
        }
        let g = gcd_u128(num.unsigned_abs(), den.unsigned_abs()).max(1) as i128;
        let mut n = num / g;
        let mut d = den / g;
        if d < 0 {
            n = n.checked_neg()?;
            d = d.checked_neg()?;
        }
        Some(Rat { num: n, den: d })
    }

    /// Exact rational addition, `None` on i128 overflow.
    fn add(self, other: Rat) -> Option<Rat> {
        let num = self
            .num
            .checked_mul(other.den)?
            .checked_add(other.num.checked_mul(self.den)?)?;
        let den = self.den.checked_mul(other.den)?;
        Rat::new(num, den)
    }
}

type DimVector = [Rat; 7];

fn zero_vector() -> DimVector {
    [Rat::zero(); 7]
}

/// Componentwise exact-rational vector sum — the group operation of the dimension
/// vector space (a product of dimensions adds their exponent vectors). `None` on
/// overflow.
fn add_vectors(a: &DimVector, b: &DimVector) -> Option<DimVector> {
    let mut out = zero_vector();
    for i in 0..7 {
        out[i] = a[i].add(b[i])?;
    }
    Some(out)
}

/// Literal lexical values of `(subject, predicate, ?)` (literals only).
fn ds_object_literals(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (Some(s_id), Some(p_id)) = (ds_iri_id(ds, subject_iri), ds_iri_id(ds, predicate_iri))
    else {
        return out;
    };
    for q in ds.quads_for_pattern(Some(s_id), Some(p_id), None, GraphMatch::Any) {
        if let TermRef::Literal { lexical, .. } = ds.resolve(q.o) {
            out.push(lexical.to_owned());
        }
    }
    out
}

/// Object IRIs of `(subject, predicate, ?)`, sorted for deterministic iteration.
fn ds_object_iris_sorted(ds: &RdfDataset, subject_iri: &str, predicate_iri: &str) -> Vec<String> {
    let mut v: Vec<String> = ds_object_iris(ds, subject_iri, predicate_iri)
        .into_iter()
        .collect();
    v.sort();
    v
}

/// Canonical base-dimension symbols, in the same order as [`BASE_DIMENSIONS`]. Used
/// to render a dimension vector to its human-readable string for the drift check.
const BASE_SYMBOLS: [&str; 7] = ["L", "M", "T", "I", "\u{0398}", "N", "J"];

/// Render a dimension vector to its canonical `math:dimensionVector` string, e.g.
/// `"L·T-1"` for velocity or `"1"` for a dimensionless quantity. Exponent 1 is
/// elided; a non-unit denominator prints as `num/den`. This is the single source the
/// authored string must match — the string is a computed projection, not an
/// independent hand-authored fact.
fn render_dimension_vector(v: &DimVector) -> String {
    let mut parts: Vec<String> = Vec::new();
    for i in 0..7 {
        let r = v[i];
        if r.num == 0 {
            continue;
        }
        let mut s = BASE_SYMBOLS[i].to_string();
        if !(r.num == 1 && r.den == 1) {
            if r.den == 1 {
                s.push_str(&r.num.to_string());
            } else {
                s.push_str(&format!("{}/{}", r.num, r.den));
            }
        }
        parts.push(s);
    }
    if parts.is_empty() {
        "1".to_string()
    } else {
        parts.join("\u{00b7}")
    }
}

/// The exact-rational ℚ⁷ exponent vector of a dimension IRI. A base dimension is a
/// unit basis vector; `math:dimensionless` (or any `math:Dimensionless`) is zero; a
/// `math:DerivedDimension` sums `power * e_base` over its `math:baseDimensionExponent`
/// cells. Pure: returns `None` for a dimension whose structure is ill-formed (a
/// non-base exponent target, a missing/non-integer/zero-denominator power, or
/// arithmetic overflow) or whose kind cannot be computed, so an unrelated node never
/// yields a false positive. The ill-formed structural cases are surfaced explicitly,
/// not swallowed here — a non-base exponent target by the SHACL `DimensionExponentShape`
/// (`sh:class`), and a zero denominator by both that shape (`sh:not sh:hasValue 0`) and
/// the native zero-denominator scan in [`check_math_dimension_invariants`] — so a `None`
/// from a genuinely malformed cell means "already reported elsewhere", never "silently
/// dropped".
fn dimension_vector(ds: &RdfDataset, dim_iri: &str) -> Option<DimVector> {
    if let Some(i) = base_dimension_index(dim_iri) {
        let mut v = zero_vector();
        v[i] = Rat::new(1, 1)?;
        return Some(v);
    }
    if dim_iri == math_iri("dimensionless") || ds_has_type(ds, dim_iri, &math_iri("Dimensionless"))
    {
        return Some(zero_vector());
    }
    if !ds_has_type(ds, dim_iri, &math_iri("DerivedDimension")) {
        return None;
    }
    let mut v = zero_vector();
    for cell in ds_object_iris_sorted(ds, dim_iri, &math_iri("baseDimensionExponent")) {
        let base = ds_object_iris_sorted(ds, &cell, &math_iri("exponentOfDimension"))
            .into_iter()
            .next()?;
        let bi = base_dimension_index(&base)?;
        let num = ds_object_literals(ds, &cell, &math_iri("exponentNumerator"))
            .into_iter()
            .find_map(|l| l.parse::<i128>().ok())?;
        let den = ds_object_literals(ds, &cell, &math_iri("exponentDenominator"))
            .into_iter()
            .find_map(|l| l.parse::<i128>().ok())?;
        v[bi] = v[bi].add(Rat::new(num, den)?)?;
    }
    Some(v)
}

/// The single dimension IRI a dimensioned node carries through `math:hasDimension`
/// (lexically least if several — the shape forbids more than one).
fn node_dimension_iri(ds: &RdfDataset, node_iri: &str) -> Option<String> {
    ds_object_iris_sorted(ds, node_iri, &math_iri("hasDimension"))
        .into_iter()
        .next()
}

/// The `math:` measure-and-dimension reasoned gate. Dimensional homogeneity is
/// computed from the exact-rational exponent vectors of the ℚ-vector space of
/// dimensions — a reasoned check, not asserted data. Runs over the merged dataset
/// (`GraphMatch::Any`), so it holds bundle-wide.
fn check_math_dimension_invariants(ds: &RdfDataset, report: &mut LintReport) {
    // dimensionVector drift: an authored string must match the canonical render of the
    // structured exponents (the string is a projection, never a divergent second
    // source). Only SHACL cannot express this, so it lives here.
    if let Some(dv_pid) = ds_iri_id(ds, &math_iri("dimensionVector")) {
        let mut flagged: HashSet<String> = HashSet::new();
        let mut rows: Vec<(String, String)> = Vec::new();
        for q in ds.quads_for_pattern(None, Some(dv_pid), None, GraphMatch::Any) {
            let (TermRef::Iri(subj), TermRef::Literal { lexical, .. }) =
                (ds.resolve(q.s), ds.resolve(q.o))
            else {
                continue;
            };
            rows.push((subj.to_owned(), lexical.to_owned()));
        }
        rows.sort();
        for (subj, lexical) in rows {
            let Some(vec) = dimension_vector(ds, &subj) else {
                continue;
            };
            let canonical = render_dimension_vector(&vec);
            if canonical != lexical && flagged.insert(subj.clone()) {
                report.errors.push(format!(
                    "math:MalformedDimension: dimension {subj} declares math:dimensionVector \
                     \"{lexical}\" but its structured exponents render to \"{canonical}\" — the \
                     string is a computed projection, not an independent source"
                ));
            }
        }
    }

    // Zero-denominator exponent: an exact-rational power needs a non-zero denominator.
    // `dimension_vector` returns `None` on such a cell (Rat::new rejects a zero
    // denominator), which would let the cell be silently skipped by the homogeneity /
    // composition loops below; surface it here as math:MalformedDimension so a malformed
    // power hard-fails rather than fails open. The SHACL DimensionExponentShape forbids
    // it too (sh:not sh:hasValue 0) — this native scan is the whole-bundle twin, genuine
    // defense in depth rather than a false claim of one.
    {
        let mut cells = ds_subjects_of_type(ds, &math_iri("DimensionExponent"));
        cells.sort();
        for cell in cells {
            let has_zero_denominator =
                ds_object_literals(ds, &cell, &math_iri("exponentDenominator"))
                    .into_iter()
                    .any(|l| l.trim().parse::<i128>() == Ok(0));
            if has_zero_denominator {
                report.errors.push(format!(
                    "math:MalformedDimension: dimension-exponent cell {cell} declares \
                     math:exponentDenominator 0 — an exact-rational power needs a non-zero \
                     denominator; the cell is ill-formed"
                ));
            }
        }
    }

    // Homogeneity: every operand of a math:DimensionalExpression shares one dimension.
    for expr in ds_subjects_of_type(ds, &math_iri("DimensionalExpression")) {
        let operands = ds_object_iris_sorted(ds, &expr, &math_iri("homogeneousOperand"));
        let mut seen: Vec<(DimVector, String)> = Vec::new();
        let mut undimensioned: Vec<String> = Vec::new();
        for operand in operands {
            let Some(dim_iri) = node_dimension_iri(ds, &operand) else {
                // No math:hasDimension at all: an undimensioned operand cannot be shown
                // homogeneous with a dimensioned one. Do not fail open — collect it.
                // (A malformed dimension — hasDimension present but structurally broken —
                // is reported by the zero-denominator scan / SHACL, so `dimension_vector`
                // returning None below is a deliberate skip, not a fail-open.)
                undimensioned.push(operand);
                continue;
            };
            let Some(vec) = dimension_vector(ds, &dim_iri) else {
                continue;
            };
            if !seen.iter().any(|(v, _)| *v == vec) {
                seen.push((vec, dim_iri));
            }
        }
        if !undimensioned.is_empty() {
            report.errors.push(format!(
                "math:DimensionalInhomogeneity: dimensional expression {expr} combines \
                 undimensioned operand(s) [{}] — every math:homogeneousOperand must carry a \
                 math:hasDimension to be shown homogeneous",
                undimensioned.join(", ")
            ));
        }
        if seen.len() >= 2 {
            let mut dims: Vec<String> = seen.into_iter().map(|(_, d)| d).collect();
            dims.sort();
            report.errors.push(format!(
                "math:DimensionalInhomogeneity: dimensional expression {expr} combines operands \
                 of differing dimensions [{}]",
                dims.join(", ")
            ));
        }
    }

    // Integral parameter composition: dim(result) == dim(integrand) + dim(measure).
    for integral in ds_subjects_of_type(ds, &math_iri("Integral")) {
        let Some(result_dim) = node_dimension_iri(ds, &integral) else {
            continue;
        };
        let integrand = ds_object_iris_sorted(ds, &integral, &math_iri("integrand"))
            .into_iter()
            .next();
        let measure = ds_object_iris_sorted(ds, &integral, &math_iri("withRespectTo"))
            .into_iter()
            .next();
        let (Some(integrand), Some(measure)) = (integrand, measure) else {
            // Missing integrand/measure is math:IncompleteIntegral (SHACL IntegralShape).
            continue;
        };
        let (Some(idim), Some(mdim)) = (
            node_dimension_iri(ds, &integrand),
            node_dimension_iri(ds, &measure),
        ) else {
            // The integral declares a result dimension but its integrand or measure carries
            // none, so the composition cannot be checked. Do not fail open — an integral
            // engaged in dimensional bookkeeping must dimension the parts it composes.
            report.errors.push(format!(
                "math:DimensionalInhomogeneity: integral {integral} declares result dimension \
                 {result_dim} but its integrand ({integrand}) or measure ({measure}) carries no \
                 math:hasDimension, so the composition cannot be verified"
            ));
            continue;
        };
        let (Some(rv), Some(iv), Some(mv)) = (
            dimension_vector(ds, &result_dim),
            dimension_vector(ds, &idim),
            dimension_vector(ds, &mdim),
        ) else {
            continue;
        };
        let Some(composed) = add_vectors(&iv, &mv) else {
            continue;
        };
        if rv != composed {
            report.errors.push(format!(
                "math:DimensionalInhomogeneity: integral {integral} declares result dimension \
                 {result_dim} but its integrand ({idim}) and measure ({mdim}) compose to a \
                 different dimension"
            ));
        }
    }
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
    use purrdf::parse_dataset;
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
        // graphBoxRole contract must not fire on them. The same individual
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
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-0> a gmeow:Finding ;\n\
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
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-1> a gmeow:Finding ;\n\
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
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-2> a gmeow:Finding ;\n\
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
             <https://blackcatinformatics.ca/gmeow/diagnostics/finding/abc-3> a gmeow:Finding ;\n\
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
    fn dataset_from(ttl: &str) -> std::sync::Arc<purrdf::RdfDataset> {
        parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
    }

    /// The native `structural_lint_dataset` twin must produce byte-identical
    /// errors/warnings to the `Store` version across a battery of fixtures.
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

    // --- lang: meaning-stratum native gates ---------------------------------- #

    const LANG_PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix lang: <https://blackcatinformatics.ca/lang/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix ex: <https://example.org/> .\n";

    #[test]
    fn undeclared_lowering_flags_bridge_denotation_without_preservation() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationKind lang:denotesLogicFormula ;\n\
               lang:denotedForm ex:f ;\n\
               lang:denotationTarget ex:formula ;\n\
               lang:denotationContext ex:ctx .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("lang:UndeclaredLoweringStage")),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn undeclared_lowering_clean_when_preservation_declared() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationKind lang:denotesLogicFormula ;\n\
               lang:denotedForm ex:f ;\n\
               lang:denotationTarget ex:formula ;\n\
               lang:denotationContext ex:ctx ;\n\
               logic:preservationKind logic:ExactPreservation .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("lang:UndeclaredLoweringStage")),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn silent_disambiguation_flags_ungrounded_resolution() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 ;\n\
               lang:resolvedReading ex:r1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("lang:SilentDisambiguation")),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn silent_disambiguation_clean_when_resolution_is_vantage_held() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:act a lang:InterpretationAct ;\n\
               lang:producedReading ex:r1 , ex:r2 ;\n\
               lang:resolvedReading ex:r1 .\n\
             ex:obs lang:aboutReading ex:r1 ;\n\
               gmeow:vantage ex:annotator .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("lang:SilentDisambiguation")),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn one_way_bridge_flags_logic_subject_with_lang_predicate() {
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             logic:someObject lang:denotedForm ex:f .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("one-way bridge violated")),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn one_way_bridge_clean_for_lang_to_logic_target() {
        // The lawful direction: a lang: denotation targeting a logic: object.
        let ds = dataset_from(&format!(
            "{LANG_PREFIXES}\
             ex:d1 a lang:Denotation ;\n\
               lang:denotationTarget logic:someFormula .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("one-way bridge violated")),
            "errors: {:?}",
            report.errors
        );
    }

    // --- math: measure-and-dimension reasoned gate --------------------------- #

    const MATH_PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix math: <https://blackcatinformatics.ca/math/> .\n\
         @prefix ex: <https://example.org/> .\n";

    /// A quantity of pure time (dimension T), used across the homogeneity tests.
    const TIME_QUANTITIES: &str =
        "ex:t1 a math:Quantity ; math:hasDimension math:timeDimension .\n\
         ex:t2 a math:Quantity ; math:hasDimension math:timeDimension .\n\
         ex:len a math:Quantity ; math:hasDimension math:lengthDimension .\n";

    fn has_inhomogeneity(report: &LintReport) -> bool {
        report
            .errors
            .iter()
            .any(|e| e.contains("math:DimensionalInhomogeneity"))
    }

    #[test]
    fn homogeneous_expression_is_clean() {
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{TIME_QUANTITIES}\
             ex:sum a math:DimensionalExpression ;\n\
               math:homogeneousOperand ex:t1 , ex:t2 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(!has_inhomogeneity(&report), "errors: {:?}", report.errors);
    }

    #[test]
    fn inhomogeneous_expression_is_flagged() {
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{TIME_QUANTITIES}\
             ex:bad a math:DimensionalExpression ;\n\
               math:homogeneousOperand ex:t1 , ex:len .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(has_inhomogeneity(&report), "errors: {:?}", report.errors);
    }

    /// The integrand/measure parameter slots compose to the integral's result
    /// dimension — proving the gate is wired across a *parameter*, not just an
    /// addition. Energy = ∫ (energy-density) d(volume): M·L⁻¹·T⁻² times L³ = M·L²·T⁻².
    const ENERGY_INTEGRAL: &str = "\
         ex:energyDim a math:DerivedDimension ;\n\
           math:baseDimensionExponent ex:mE1 , ex:lE2 , ex:tEm2 .\n\
         ex:mE1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;\n\
           math:exponentNumerator 1 ; math:exponentDenominator 1 .\n\
         ex:lE2 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
           math:exponentNumerator 2 ; math:exponentDenominator 1 .\n\
         ex:tEm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
           math:exponentNumerator -2 ; math:exponentDenominator 1 .\n\
         ex:densityDim a math:DerivedDimension ;\n\
           math:baseDimensionExponent ex:mD1 , ex:lDm1 , ex:tDm2 .\n\
         ex:mD1 a math:DimensionExponent ; math:exponentOfDimension math:massDimension ;\n\
           math:exponentNumerator 1 ; math:exponentDenominator 1 .\n\
         ex:lDm1 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
           math:exponentNumerator -1 ; math:exponentDenominator 1 .\n\
         ex:tDm2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
           math:exponentNumerator -2 ; math:exponentDenominator 1 .\n\
         ex:volumeDim a math:DerivedDimension ; math:baseDimensionExponent ex:lV3 .\n\
         ex:lV3 a math:DimensionExponent ; math:exponentOfDimension math:lengthDimension ;\n\
           math:exponentNumerator 3 ; math:exponentDenominator 1 .\n\
         ex:density a math:MeasurableFunction ; math:hasDimension ex:densityDim .\n\
         ex:vol a math:Measure ; math:hasDimension ex:volumeDim .\n";

    #[test]
    fn integral_with_composed_parameter_dimensions_is_clean() {
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{ENERGY_INTEGRAL}\
             ex:energy a math:Integral ;\n\
               math:integrand ex:density ;\n\
               math:withRespectTo ex:vol ;\n\
               math:hasDimension ex:energyDim .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(!has_inhomogeneity(&report), "errors: {:?}", report.errors);
    }

    #[test]
    fn integral_with_mismatched_result_dimension_is_flagged() {
        // Declare the result as time (T) instead of energy — the parameter slots do
        // not compose to it.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{ENERGY_INTEGRAL}\
             ex:energy a math:Integral ;\n\
               math:integrand ex:density ;\n\
               math:withRespectTo ex:vol ;\n\
               math:hasDimension math:timeDimension .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(has_inhomogeneity(&report), "errors: {:?}", report.errors);
    }

    #[test]
    fn dimension_vector_string_drift_is_flagged() {
        // Structured exponents render to "T-1"; the authored string says "L" — drift.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:freqDim a math:DerivedDimension ;\n\
               math:dimensionVector \"L\" ;\n\
               math:baseDimensionExponent ex:tm1 .\n\
             ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("math:MalformedDimension") && e.contains("dimensionVector")),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn dimension_vector_string_matching_render_is_clean() {
        // Structured exponents render to "T-1"; the authored string matches.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:freqDim a math:DerivedDimension ;\n\
               math:dimensionVector \"T-1\" ;\n\
               math:baseDimensionExponent ex:tm1 .\n\
             ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 1 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("math:MalformedDimension")),
            "errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn fractional_dimensions_are_exact_and_distinct() {
        // √Hz is T^(-1/2); it is homogeneous with itself and distinct from T^(-1).
        let clean = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:asdDim a math:DerivedDimension ; math:baseDimensionExponent ex:th .\n\
             ex:th a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n\
             ex:asdDim2 a math:DerivedDimension ; math:baseDimensionExponent ex:th2 .\n\
             ex:th2 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n\
             ex:q1 a math:Quantity ; math:hasDimension ex:asdDim .\n\
             ex:q2 a math:Quantity ; math:hasDimension ex:asdDim2 .\n\
             ex:ok a math:DimensionalExpression ; math:homogeneousOperand ex:q1 , ex:q2 .\n"
        ));
        assert!(
            !has_inhomogeneity(&structural_lint_dataset(&clean, &cfg())),
            "T^(-1/2) must be homogeneous with itself"
        );
        // Now mix T^(-1/2) with T^(-1): distinct, so inhomogeneous.
        let mixed = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:asdDim a math:DerivedDimension ; math:baseDimensionExponent ex:th .\n\
             ex:th a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n\
             ex:freqDim a math:DerivedDimension ; math:baseDimensionExponent ex:tm1 .\n\
             ex:tm1 a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 1 .\n\
             ex:q1 a math:Quantity ; math:hasDimension ex:asdDim .\n\
             ex:q3 a math:Quantity ; math:hasDimension ex:freqDim .\n\
             ex:bad a math:DimensionalExpression ; math:homogeneousOperand ex:q1 , ex:q3 .\n"
        ));
        assert!(
            has_inhomogeneity(&structural_lint_dataset(&mixed, &cfg())),
            "T^(-1/2) and T^(-1) must be inhomogeneous"
        );
    }

    #[test]
    fn zero_denominator_exponent_is_flagged() {
        // An exact-rational power with denominator 0 is ill-formed: dimension_vector
        // returns None on it, so the homogeneity/composition loops would skip it
        // silently. The native scan must surface it as math:MalformedDimension rather
        // than fail open.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:badDim a math:DerivedDimension ; math:baseDimensionExponent ex:zc .\n\
             ex:zc a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 0 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            report
                .errors
                .iter()
                .any(|e| e.contains("math:MalformedDimension")
                    && e.contains("exponentDenominator 0")),
            "a zero-denominator exponent cell must raise math:MalformedDimension; errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn nonzero_denominator_exponent_is_clean() {
        // The well-formed twin: a legitimate 1/2 power must NOT trip the zero-denominator
        // scan (guards against an over-broad match on the denominator literal).
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}\
             ex:okDim a math:DerivedDimension ; math:baseDimensionExponent ex:hc .\n\
             ex:hc a math:DimensionExponent ; math:exponentOfDimension math:timeDimension ;\n\
               math:exponentNumerator -1 ; math:exponentDenominator 2 .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            !report
                .errors
                .iter()
                .any(|e| e.contains("math:MalformedDimension")),
            "a non-zero denominator must not raise math:MalformedDimension; errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn undimensioned_operand_is_flagged() {
        // An operand carrying no math:hasDimension must not let the expression fail open:
        // mixing a dimensioned operand with an undimensioned one is not homogeneous.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{TIME_QUANTITIES}\
             ex:mystery a math:Quantity .\n\
             ex:bad a math:DimensionalExpression ;\n\
               math:homogeneousOperand ex:t1 , ex:mystery .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            has_inhomogeneity(&report)
                && report
                    .errors
                    .iter()
                    .any(|e| e.contains("undimensioned operand")),
            "an undimensioned operand must raise math:DimensionalInhomogeneity; errors: {:?}",
            report.errors
        );
    }

    #[test]
    fn integral_with_undimensioned_part_is_flagged() {
        // The integral declares a result dimension but its measure carries none — the
        // composition cannot be verified, so it must not fail open.
        let ds = dataset_from(&format!(
            "{MATH_PREFIXES}{ENERGY_INTEGRAL}\
             ex:vol2 a math:Measure .\n\
             ex:energy a math:Integral ;\n\
               math:integrand ex:density ;\n\
               math:withRespectTo ex:vol2 ;\n\
               math:hasDimension ex:energyDim .\n"
        ));
        let report = structural_lint_dataset(&ds, &cfg());
        assert!(
            has_inhomogeneity(&report) && report.errors.iter().any(|e| e.contains("carries no")),
            "an integral with an undimensioned measure must raise math:DimensionalInhomogeneity; \
             errors: {:?}",
            report.errors
        );
    }
}
