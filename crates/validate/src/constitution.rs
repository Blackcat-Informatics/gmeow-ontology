// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Native constitution-as-code gate.
//!
//! Ports the full Python ``gmeow_tools.constitution`` checks to Rust:
//! enforcement coverage, principle/heading sync, cited artifact/symbol/target/CLI
//! existence, and supersession marker sync. The non-graph checks now live here
//! too, using pure text parsing helpers instead of Python introspection.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::OnceLock;

use gmeow_errors::{Finding, Severity};
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermId, TermRef, TermValue};
use regex::Regex;

use crate::model::rdf;

/// The governance meta namespace (`constitution.META`).
const META: &str = "https://blackcatinformatics.ca/gmeow/meta#";
/// The enforcement classes; `Practice` is the honor-system kind.
const ENFORCEMENT_KINDS: &[&str] = &["Lint", "TestSuite", "Shape", "Gate", "Practice"];
/// `rdfs:Class` — a node ALSO typed as this is a class declaration, not an
/// enforcement instance (mirrors the Python `(node, RDF.type, RDFS.Class)` skip).
const RDFS_CLASS: &str = "http://www.w3.org/2000/01/rdf-schema#Class";

/// Resolve an IRI value to its dataset-local [`TermId`], if interned.
#[inline]
fn iri_id(ds: &RdfDataset, iri: &str) -> Option<TermId> {
    ds.term_id_by_value(&TermValue::iri(iri))
}

/// Build a `<META><local>` IRI string.
fn meta_iri(local: &str) -> String {
    format!("{META}{local}")
}

static HEADING_RE: OnceLock<Regex> = OnceLock::new();
static PRINCIPLE_REF_RE: OnceLock<Regex> = OnceLock::new();
static MAKEFILE_TARGET_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_CLASS_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_DEF_RE: OnceLock<Regex> = OnceLock::new();
static PYTHON_ASSIGN_RE: OnceLock<Regex> = OnceLock::new();
static RUST_ENUM_RE: OnceLock<Regex> = OnceLock::new();
static RUST_COMMAND_NAME_RE: OnceLock<Regex> = OnceLock::new();
static RUST_VARIANT_RE: OnceLock<Regex> = OnceLock::new();
static RUST_ITEM_FN_RE: OnceLock<Regex> = OnceLock::new();
static RUST_ITEM_DECL_RE: OnceLock<Regex> = OnceLock::new();
static RUST_ITEM_MACRO_RE: OnceLock<Regex> = OnceLock::new();

/// One principle reconstructed from the manifest graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principle {
    pub iri: String,
    pub number: i64,
    pub title: String,
    pub enforced_by: Vec<String>,
    pub superseded_in_part_by: Vec<i64>,
    pub extends: Vec<i64>,
}

/// One enforcement mechanism declared in the manifest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Enforcement {
    pub iri: String,
    pub kind: String,
    pub artifacts: Vec<String>,
    pub symbols: Vec<String>,
    pub make_targets: Vec<String>,
    pub cli_commands: Vec<String>,
}

impl Enforcement {
    /// Local name used in diagnostic messages (IRI without the meta prefix).
    pub fn local_name(&self) -> &str {
        self.iri.strip_prefix(META).unwrap_or(&self.iri)
    }
}

/// Whether `node_id` is also declared an `rdfs:Class` (a class definition to skip).
fn is_rdfs_class(ds: &RdfDataset, node_id: TermId) -> bool {
    let (Some(type_id), Some(class_id)) = (iri_id(ds, rdf::TYPE), iri_id(ds, RDFS_CLASS)) else {
        return false;
    };
    ds.quads_for_pattern(
        Some(node_id),
        Some(type_id),
        Some(class_id),
        GraphMatch::Any,
    )
    .next()
    .is_some()
}

/// Collect all string-object values for `subject predicate_local`.
fn strings_for(ds: &RdfDataset, subject_id: TermId, predicate_local: &str) -> Vec<String> {
    let Some(predicate_id) = iri_id(ds, &meta_iri(predicate_local)) else {
        return Vec::new();
    };
    let mut values: Vec<String> = ds
        .quads_for_pattern(Some(subject_id), Some(predicate_id), None, GraphMatch::Any)
        .filter_map(|q| literal_string(ds.resolve(q.o)))
        .collect();
    values.sort();
    values
}

/// Resolve `subject predicate ?obj` where `?obj` is a Principle node to its
/// heading number.
fn principle_numbers(
    ds: &RdfDataset,
    subject_id: TermId,
    predicate_id: TermId,
    iri_to_number: &BTreeMap<String, i64>,
) -> Vec<i64> {
    let mut numbers: Vec<i64> = ds
        .quads_for_pattern(Some(subject_id), Some(predicate_id), None, GraphMatch::Any)
        .filter_map(|q| match ds.resolve(q.o) {
            TermRef::Iri(n) => iri_to_number.get(n).copied(),
            _ => None,
        })
        .collect();
    numbers.sort();
    numbers.dedup();
    numbers
}

/// Collect the declared enforcement instances keyed by full IRI.
pub fn collect_enforcements(ds: &RdfDataset) -> BTreeMap<String, Enforcement> {
    let mut enforcements = BTreeMap::new();
    let Some(type_id) = iri_id(ds, rdf::TYPE) else {
        return enforcements;
    };
    for kind in ENFORCEMENT_KINDS {
        let Some(kind_id) = iri_id(ds, &meta_iri(kind)) else {
            continue;
        };
        for q in ds.quads_for_pattern(None, Some(type_id), Some(kind_id), GraphMatch::Any) {
            if let TermRef::Iri(node) = ds.resolve(q.s) {
                if is_rdfs_class(ds, q.s) {
                    continue;
                }
                let iri = node.to_string();
                let enforcement = Enforcement {
                    iri: iri.clone(),
                    kind: (*kind).to_string(),
                    artifacts: strings_for(ds, q.s, "artifact"),
                    symbols: strings_for(ds, q.s, "symbol"),
                    make_targets: strings_for(ds, q.s, "makeTarget"),
                    cli_commands: strings_for(ds, q.s, "cliCommand"),
                };
                enforcements.insert(iri, enforcement);
            }
        }
    }
    enforcements
}

/// Collect the principles (number, title, enforced_by edges, relations).
pub fn collect_principles(ds: &RdfDataset) -> Vec<Principle> {
    let Some(type_id) = iri_id(ds, rdf::TYPE) else {
        return Vec::new();
    };
    let (number_p, title_p, enforced_p) = (
        iri_id(ds, &meta_iri("number")),
        iri_id(ds, &meta_iri("title")),
        iri_id(ds, &meta_iri("enforcedBy")),
    );

    let mut principles: Vec<Principle> = Vec::new();
    let mut iri_to_number: BTreeMap<String, i64> = BTreeMap::new();

    let Some(principle_type_id) = iri_id(ds, &meta_iri("Principle")) else {
        return Vec::new();
    };
    for q in ds.quads_for_pattern(
        None,
        Some(type_id),
        Some(principle_type_id),
        GraphMatch::Any,
    ) {
        let TermRef::Iri(node) = ds.resolve(q.s) else {
            continue;
        };
        let iri = node.to_string();
        let node_id = q.s;
        let number = number_p
            .and_then(|p_id| {
                ds.quads_for_pattern(Some(node_id), Some(p_id), None, GraphMatch::Any)
                    .find_map(|qq| literal_i64(ds.resolve(qq.o)))
            })
            .unwrap_or(-1);
        let title = title_p
            .and_then(|p_id| {
                ds.quads_for_pattern(Some(node_id), Some(p_id), None, GraphMatch::Any)
                    .find_map(|qq| literal_string(ds.resolve(qq.o)))
            })
            .unwrap_or_default();
        let mut enforced_by: Vec<String> = enforced_p
            .map(|p_id| {
                ds.quads_for_pattern(Some(node_id), Some(p_id), None, GraphMatch::Any)
                    .filter_map(|qq| match ds.resolve(qq.o) {
                        TermRef::Iri(n) => Some(n.to_string()),
                        _ => None,
                    })
                    .collect()
            })
            .unwrap_or_default();
        enforced_by.sort();

        iri_to_number.insert(iri.clone(), number);
        principles.push(Principle {
            iri,
            number,
            title,
            enforced_by,
            superseded_in_part_by: Vec::new(),
            extends: Vec::new(),
        });
    }

    let superseded_p = iri_id(ds, &meta_iri("supersededInPartBy"));
    let extends_p = iri_id(ds, &meta_iri("extends"));
    for principle in &mut principles {
        let Some(node_id) = iri_id(ds, &principle.iri) else {
            continue;
        };
        if let Some(p_id) = superseded_p {
            principle.superseded_in_part_by = principle_numbers(ds, node_id, p_id, &iri_to_number);
        }
        if let Some(p_id) = extends_p {
            principle.extends = principle_numbers(ds, node_id, p_id, &iri_to_number);
        }
    }

    principles.sort_by_key(|p| p.number);
    principles
}

fn literal_i64(term: TermRef<'_>) -> Option<i64> {
    match term {
        TermRef::Literal { lexical, .. } => lexical.parse().ok(),
        _ => None,
    }
}

fn literal_string(term: TermRef<'_>) -> Option<String> {
    match term {
        TermRef::Literal { lexical, .. } => Some(lexical.to_string()),
        _ => None,
    }
}

/// ``## N. Title`` headings of CONSTITUTION.md, as ``{number: title}``.
pub fn constitution_headings(md_text: &str) -> BTreeMap<i64, String> {
    let re = HEADING_RE
        .get_or_init(|| Regex::new(r"(?m)^## (\d+)\. (.+?)\s*$").expect("valid heading regex"));
    re.captures_iter(md_text)
        .filter_map(|cap| {
            let number: i64 = cap[1].parse().ok()?;
            Some((number, cap[2].to_string()))
        })
        .collect()
}

/// Map each principle's heading number to the target numbers named in `marker`.
///
/// A relation is read from a bold marker line inside that principle's section;
/// the `from` number is the enclosing ``## N. Title`` heading, the targets are
/// every ``Principle N`` on the marker line.
pub fn markdown_relations(md_text: &str, marker: &str) -> BTreeMap<i64, BTreeSet<i64>> {
    let heading_re = HEADING_RE
        .get_or_init(|| Regex::new(r"(?m)^## (\d+)\. (.+?)\s*$").expect("valid heading regex"));
    let principle_ref = PRINCIPLE_REF_RE
        .get_or_init(|| Regex::new(r"Principle (\d+)").expect("valid principle ref regex"));

    let headings: Vec<(i64, usize, usize)> = heading_re
        .captures_iter(md_text)
        .filter_map(|cap| {
            let number: i64 = cap[1].parse().ok()?;
            let m = cap.get(0).expect("heading match");
            Some((number, m.start(), m.end()))
        })
        .collect();

    let mut relations: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
    for (idx, (number, _start, end)) in headings.iter().enumerate() {
        let section_end = headings.get(idx + 1).map(|h| h.1).unwrap_or(md_text.len());
        let section = &md_text[*end..section_end];
        for line in section.lines() {
            if line.trim_start().starts_with(marker) {
                let targets: BTreeSet<i64> = principle_ref
                    .find_iter(line)
                    .filter_map(|m| {
                        m.as_str()
                            .strip_prefix("Principle ")
                            .and_then(|n| n.parse().ok())
                    })
                    .collect();
                if !targets.is_empty() {
                    relations.entry(*number).or_default().extend(targets);
                }
            }
        }
    }
    relations
}

/// Makefile target names of the form ``name:``.
pub fn makefile_targets(makefile_text: &str) -> BTreeSet<String> {
    let re = MAKEFILE_TARGET_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9_-]*):").expect("valid make target regex")
    });
    makefile_text
        .lines()
        .filter_map(|line| re.captures(line).map(|cap| cap[1].to_string()))
        .collect()
}

/// Top-level `def`, `class`, assignment, and annotated-assignment names.
pub fn python_top_level_names(py_text: &str) -> BTreeSet<String> {
    let class_re = PYTHON_CLASS_RE.get_or_init(|| {
        Regex::new(r"^class\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("valid class regex")
    });
    let def_re = PYTHON_DEF_RE.get_or_init(|| {
        Regex::new(r"^(?:async\s+)?def\s+([A-Za-z_][A-Za-z0-9_]*)\b").expect("valid def regex")
    });
    let assign_re = PYTHON_ASSIGN_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)\s*(?::[^=]*|=)").expect("valid assign regex")
    });

    let keywords: BTreeSet<&str> = [
        "if", "elif", "else", "for", "while", "try", "except", "finally", "with", "return",
        "raise", "assert", "import", "from", "pass", "break", "continue", "global", "nonlocal",
        "del", "yield", "async", "await", "class", "def", "lambda", "as", "or", "and", "is",
        "True", "False", "None", "match", "case",
    ]
    .iter()
    .copied()
    .collect();

    let mut names = BTreeSet::new();
    for line in py_text.lines() {
        // Only collect names from zero-indented lines; nested definitions and
        // assignments inside classes/functions must not be treated as top-level.
        if line.starts_with(' ') || line.starts_with('\t') {
            continue;
        }
        if let Some(cap) = class_re.captures(line) {
            names.insert(cap[1].to_string());
            continue;
        }
        if let Some(cap) = def_re.captures(line) {
            names.insert(cap[1].to_string());
            continue;
        }
        if let Some(cap) = assign_re.captures(line) {
            let name = &cap[1];
            if !keywords.contains(name) {
                names.insert(name.to_string());
            }
        }
    }
    names
}

/// Replace the content of every line comment, block comment (nestable),
/// string literal, raw-string literal, and char literal with spaces, preserving
/// the surrounding code and line structure. Delimiters are all ASCII, so a raw
/// byte scan never splits a multi-byte UTF-8 code point. This is the strictness
/// pillar of [`rust_item_names`]: a symbol that appears only in a doc-comment
/// (`[`foo`]`), a string (`"foo"`), or a char/lifetime must not be mistaken for
/// a real item definition.
fn strip_rust_comments_and_strings(src: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Code,
        LineComment,
        BlockComment(u32),
        Str,
        RawStr(usize),
    }
    let b = src.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(b.len());
    let mut state = State::Code;
    let mut i = 0;
    while i < b.len() {
        match state {
            State::Code => {
                if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    state = State::LineComment;
                } else if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    state = State::BlockComment(1);
                } else if b[i] == b'r' && {
                    // r"…" or r#…#"…"#, but only when it is not part of a
                    // longer identifier (e.g. `render`): the char before `r`
                    // must not be an identifier char.
                    let prev_ident =
                        i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
                    !prev_ident && {
                        let mut j = i + 1;
                        while j < b.len() && b[j] == b'#' {
                            j += 1;
                        }
                        j < b.len() && b[j] == b'"'
                    }
                } {
                    let mut j = i + 1;
                    let mut hashes = 0usize;
                    while j < b.len() && b[j] == b'#' {
                        hashes += 1;
                        j += 1;
                    }
                    // j is at the opening quote; blank r, hashes, and the quote.
                    out.resize(out.len() + (j - i + 1), b' ');
                    i = j + 1;
                    state = State::RawStr(hashes);
                } else if b[i] == b'"' {
                    out.push(b' ');
                    i += 1;
                    state = State::Str;
                } else if b[i] == b'\'' {
                    // Distinguish a char literal (`'x'`, `'\n'`, `'\''`) from a
                    // lifetime / label (`'a`, `'static`). A char literal has a
                    // closing `'` within a few bytes; a lifetime is `'` followed
                    // by an identifier and NO closing quote.
                    if is_char_literal(&b[i..]) {
                        let len = char_literal_len(&b[i..]);
                        out.resize(out.len() + len, b' ');
                        i += len;
                    } else {
                        out.push(b'\'');
                        i += 1;
                    }
                } else {
                    out.push(b[i]);
                    i += 1;
                }
            }
            State::LineComment => {
                if b[i] == b'\n' {
                    out.push(b'\n');
                    state = State::Code;
                } else {
                    out.push(b' ');
                }
                i += 1;
            }
            State::BlockComment(depth) => {
                if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    state = State::BlockComment(depth + 1);
                } else if b[i] == b'*' && i + 1 < b.len() && b[i + 1] == b'/' {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                    state = if depth == 1 {
                        State::Code
                    } else {
                        State::BlockComment(depth - 1)
                    };
                } else {
                    out.push(if b[i] == b'\n' { b'\n' } else { b' ' });
                    i += 1;
                }
            }
            State::Str => {
                if b[i] == b'\\' && i + 1 < b.len() {
                    out.push(b' ');
                    out.push(b' ');
                    i += 2;
                } else if b[i] == b'"' {
                    out.push(b' ');
                    i += 1;
                    state = State::Code;
                } else {
                    out.push(if b[i] == b'\n' { b'\n' } else { b' ' });
                    i += 1;
                }
            }
            State::RawStr(hashes) => {
                if b[i] == b'"' {
                    // Closing quote iff followed by exactly `hashes` `#`.
                    let mut j = i + 1;
                    let mut seen = 0usize;
                    while seen < hashes && j < b.len() && b[j] == b'#' {
                        seen += 1;
                        j += 1;
                    }
                    if seen == hashes {
                        out.resize(out.len() + (j - i), b' ');
                        i = j;
                        state = State::Code;
                    } else {
                        out.push(b' ');
                        i += 1;
                    }
                } else {
                    out.push(if b[i] == b'\n' { b'\n' } else { b' ' });
                    i += 1;
                }
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|_| src.to_string())
}

/// Whether `bytes` (starting at a `'`) opens a char literal rather than a
/// lifetime/label. A char literal is `'\...'` (escape) or `'x'` (any single
/// code unit then a closing `'`); a lifetime is `'ident` with no near close.
fn is_char_literal(bytes: &[u8]) -> bool {
    char_literal_len(bytes) > 0
}

/// Byte length of the char literal starting at `bytes[0] == '\''`, or 0 if this
/// `'` begins a lifetime/label instead.
fn char_literal_len(bytes: &[u8]) -> usize {
    if bytes.first() != Some(&b'\'') {
        return 0;
    }
    if bytes.len() >= 2 && bytes[1] == b'\\' {
        // Escaped: '\n' '\'' '\\' '\x41' '\u{1F}' … — find the closing quote.
        let mut j = 2;
        while j < bytes.len() && bytes[j] != b'\'' && bytes[j] != b'\n' {
            j += 1;
        }
        if j < bytes.len() && bytes[j] == b'\'' {
            return j + 1;
        }
        return 0;
    }
    // Unescaped: a single code point then a closing quote. Accept one leading
    // byte plus any UTF-8 continuation bytes, then require `'`.
    let mut j = 1;
    if j < bytes.len() && bytes[j] != b'\'' && bytes[j] != b'\n' {
        j += 1;
        while j < bytes.len() && (bytes[j] & 0xC0) == 0x80 {
            j += 1;
        }
    }
    if j < bytes.len() && bytes[j] == b'\'' {
        j + 1
    } else {
        0
    }
}

/// Names of all Rust item *definitions* in `rust_text`, at any nesting depth:
/// free / associated / trait `fn`s (including `#[test]` fns nested in
/// `mod tests`), `struct`/`enum`/`union`/`trait`/`type`/`const`/`static`, and
/// `macro_rules!`. Occurrences in comments, string literals, or call sites do
/// NOT count — the source is comment/string-stripped first (see
/// [`strip_rust_comments_and_strings`]) and only the identifier immediately
/// following an item-introducer keyword is collected. This is what makes a
/// cited `meta:symbol` prove a real `.rs` definition rather than any textual
/// mention (the previous `text.contains` accepted the latter).
pub fn rust_item_names(rust_text: &str) -> BTreeSet<String> {
    let fn_re = RUST_ITEM_FN_RE.get_or_init(|| {
        Regex::new(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)").expect("valid rust fn regex")
    });
    let decl_re = RUST_ITEM_DECL_RE.get_or_init(|| {
        Regex::new(
            r"\b(?:struct|enum|union|trait|type|const|static)\s+(?:mut\s+)?([A-Za-z_][A-Za-z0-9_]*)",
        )
        .expect("valid rust item regex")
    });
    let macro_re = RUST_ITEM_MACRO_RE.get_or_init(|| {
        Regex::new(r"\bmacro_rules!\s+([A-Za-z_][A-Za-z0-9_]*)").expect("valid rust macro regex")
    });

    let stripped = strip_rust_comments_and_strings(rust_text);
    let mut names = BTreeSet::new();
    for cap in fn_re.captures_iter(&stripped) {
        names.insert(cap[1].to_string());
    }
    for cap in decl_re.captures_iter(&stripped) {
        // `const fn foo` / `static fn` — the decl regex captures the `fn`
        // keyword; the real name is picked up by `fn_re`, so drop `fn` here.
        if &cap[1] != "fn" {
            names.insert(cap[1].to_string());
        }
    }
    for cap in macro_re.captures_iter(&stripped) {
        names.insert(cap[1].to_string());
    }
    names
}

/// Convert a clap `Subcommand` variant identifier to the subcommand name clap
/// derives by default (`rename_all = "kebab-case"`, matching `heck`): word
/// boundaries fall at lower/digit→upper transitions and at the tail of an
/// acronym run (upper→upper-then-lower), with each word lowercased and joined by
/// `-`. E.g. `SliceQuality`→`slice-quality`, `Mcp`→`mcp`, `I18n`→`i18n`.
fn variant_to_kebab(ident: &str) -> String {
    let chars: Vec<char> = ident.chars().collect();
    let mut words: Vec<String> = Vec::new();
    let mut cur = String::new();
    for i in 0..chars.len() {
        let c = chars[i];
        if !cur.is_empty() {
            let prev = chars[i - 1];
            let boundary = (c.is_uppercase() && (prev.is_lowercase() || prev.is_ascii_digit()))
                || (c.is_uppercase()
                    && prev.is_uppercase()
                    && i + 1 < chars.len()
                    && chars[i + 1].is_lowercase());
            if boundary {
                words.push(std::mem::take(&mut cur));
            }
        }
        cur.push(c);
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    words
        .into_iter()
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join("-")
}

/// Every subcommand declared by the clap `#[derive(Subcommand)]` enums in a Rust
/// CLI crate's `lib.rs`. Each `enum <X>Commands { … }` block (the top-level
/// `Commands` enum plus every nested sub-app enum) is scanned; a variant's name
/// is its explicit `#[command(name = "…")]` override when present, otherwise the
/// clap-default kebab-case rendering of the variant identifier. Sub-app group
/// names (e.g. `box-roles`, `logic`, `i18n`) surface automatically as the
/// top-level `Commands` variants that carry the nested enums.
pub fn cli_command_names_from_rust(rust_text: &str) -> BTreeSet<String> {
    let enum_re = RUST_ENUM_RE
        .get_or_init(|| Regex::new(r"\benum\s+\w*Commands\b").expect("valid enum regex"));
    let name_re = RUST_COMMAND_NAME_RE.get_or_init(|| {
        Regex::new(r#"#\[\s*command\([^)]*\bname\s*=\s*"([^"]*)""#).expect("valid name regex")
    });
    let variant_re = RUST_VARIANT_RE
        .get_or_init(|| Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)\b").expect("valid variant regex"));

    let mut names = BTreeSet::new();
    let mut in_enum = false;
    // Nesting depth of `{`/`(`/`[` *inside* the enum body: 0 means directly at
    // the enum's top level, where variant identifiers and their `#[command(…)]`
    // attributes live; anything deeper is variant field/attribute detail.
    let mut body_depth: i32 = 0;
    let mut pending_override: Option<String> = None;

    for raw_line in rust_text.lines() {
        let line = raw_line.trim();

        if !in_enum {
            if enum_re.is_match(line) && line.contains('{') {
                in_enum = true;
                body_depth = 0;
                pending_override = None;
            }
            continue;
        }

        // At the enum's top level, recognise variant declarations and pending
        // `#[command(name = "…")]` overrides before updating the brace depth.
        if body_depth == 0 {
            if line.starts_with('#') {
                if let Some(cap) = name_re.captures(line) {
                    pending_override = Some(cap[1].to_string());
                }
            } else if !line.starts_with("//") && !line.is_empty() && line != "}" {
                if let Some(cap) = variant_re.captures(line) {
                    let ident = &cap[1];
                    let name = pending_override
                        .take()
                        .unwrap_or_else(|| variant_to_kebab(ident));
                    names.insert(name);
                }
                pending_override = None;
            }
        }

        for ch in line.chars() {
            match ch {
                '{' | '(' | '[' => body_depth += 1,
                '}' | ')' | ']' => {
                    body_depth -= 1;
                    if body_depth < 0 {
                        in_enum = false;
                        pending_override = None;
                        break;
                    }
                }
                _ => {}
            }
        }
    }
    names
}

/// Every command registered on the public (`gmeow`) or repository-maintenance
/// (`gmeow-dev`) Rust clap CLI, read from the clap `Subcommand` enums in each
/// crate's `lib.rs`.
pub fn cli_surface_command_names(root: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    for rel in [
        "crates/gmeow-cli/src/lib.rs",
        "crates/gmeow-dev-cli/src/lib.rs",
    ] {
        let text = fs::read_to_string(root.join(rel)).unwrap_or_default();
        names.extend(cli_command_names_from_rust(&text));
    }
    names
}

/// Run the enforcement-coverage check over the parsed manifest dataset.
pub fn check_enforcement_coverage(ds: &RdfDataset) -> Vec<Finding> {
    let enforcements = collect_enforcements(ds);
    let principles = collect_principles(ds);

    let mut findings = Vec::new();
    let mut cited: BTreeSet<String> = BTreeSet::new();

    for principle in &principles {
        let mut any_known = false;
        let mut has_non_practice = false;
        for e in &principle.enforced_by {
            match enforcements.get(e) {
                Some(enforcement) => {
                    any_known = true;
                    has_non_practice |= enforcement.kind != "Practice";
                    cited.insert(e.clone());
                }
                None => findings.push(error(
                    "undeclared-enforcement",
                    format!(
                        "principle {} cites undeclared enforcement {e}",
                        principle.number
                    ),
                )),
            }
        }
        if !any_known {
            findings.push(error(
                "principle-unenforced",
                format!(
                    "principle {} ({}) has zero registered enforcement",
                    principle.number,
                    py_repr(&principle.title)
                ),
            ));
        } else if !has_non_practice {
            findings.push(
                Finding::new(
                    Severity::Warning,
                    crate::codes::CONSTITUTION_HONOR_SYSTEM,
                    format!(
                        "principle {} ({}) is enforced only by review practice (honor system)",
                        principle.number,
                        py_repr(&principle.title)
                    ),
                )
                .with_tool("constitution"),
            );
        }
    }

    for orphan in enforcements.keys() {
        if !cited.contains(orphan) {
            findings.push(error(
                crate::codes::CONSTITUTION_ORPHANED_ENFORCEMENT
                    .strip_prefix(crate::codes::CONSTITUTION_FAMILY)
                    .expect(
                        "CONSTITUTION_ORPHANED_ENFORCEMENT carries the constitution. family prefix",
                    ),
                format!("orphaned enforcement {orphan} maps to no principle — why does it exist?"),
            ));
        }
    }

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// Manifest principles and CONSTITUTION.md headings must agree exactly.
pub fn check_principle_sync(
    principles: &[Principle],
    headings: &BTreeMap<i64, String>,
) -> Vec<Finding> {
    let declared: BTreeMap<i64, &Principle> = principles.iter().map(|p| (p.number, p)).collect();
    let mut findings = Vec::new();

    for number in headings.keys() {
        if !declared.contains_key(number) {
            findings.push(error(
                "missing-manifest-entry",
                format!(
                    "principle {number} ({}) has no manifest entry in governance/constitution.ttl",
                    py_repr(&headings[number])
                ),
            ));
        }
    }

    for number in declared.keys() {
        if !headings.contains_key(number) {
            let principle = declared[number];
            findings.push(error(
                "absent-from-constitution",
                format!(
                    "manifest declares principle {number} ({}) absent from CONSTITUTION.md",
                    py_repr(&principle.title)
                ),
            ));
        }
    }

    for number in declared.keys() {
        if let Some(md_title) = headings.get(number) {
            let principle = declared[number];
            if principle.title != *md_title {
                findings.push(error(
                    "title-drift",
                    format!(
                        "principle {number} title drift: manifest says {}, CONSTITUTION.md says {}",
                        py_repr(&principle.title),
                        py_repr(md_title)
                    ),
                ));
            }
        }
    }

    findings
}

/// Whether `symbol` is a real definition in any cited artifact. A `.py`
/// artifact resolves the symbol as a top-level name; a `.rs` artifact resolves
/// it as a Rust *item* definition (fn/method/struct/enum/const/…, at any nesting
/// depth), NOT a mere textual occurrence; any other artifact (`.ttl`, `.yaml`,
/// `.md`, `.json`, …) keeps the verbatim substring match, since its "symbols"
/// are ontology terms or config keys with no Rust/Python item structure.
fn symbol_defined(
    symbol: &str,
    artifacts: &[String],
    root: &Path,
    caches: &mut SymbolCaches,
) -> bool {
    for artifact in artifacts {
        let path = root.join(artifact);
        if !path.is_file() {
            continue;
        }
        match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
            "py" => {
                let names = caches.py.entry(artifact.clone()).or_insert_with(|| {
                    fs::read_to_string(&path)
                        .map(|text| python_top_level_names(&text))
                        .unwrap_or_default()
                });
                if names.contains(symbol) {
                    return true;
                }
            }
            "rs" => {
                let names = caches.rust.entry(artifact.clone()).or_insert_with(|| {
                    fs::read_to_string(&path)
                        .map(|text| rust_item_names(&text))
                        .unwrap_or_default()
                });
                if names.contains(symbol) {
                    return true;
                }
            }
            _ => {
                let text = caches
                    .text
                    .entry(artifact.clone())
                    .or_insert_with(|| fs::read_to_string(&path).unwrap_or_default());
                if text.contains(symbol) {
                    return true;
                }
            }
        }
    }
    false
}

/// Per-artifact resolution caches shared across a single `check_references`
/// pass so each cited file is read and parsed at most once.
#[derive(Default)]
struct SymbolCaches {
    py: BTreeMap<String, BTreeSet<String>>,
    rust: BTreeMap<String, BTreeSet<String>>,
    text: BTreeMap<String, String>,
}

/// Every cited artifact / symbol / make target / CLI command must exist.
pub fn check_references(enforcements: &BTreeMap<String, Enforcement>, root: &Path) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut caches = SymbolCaches::default();

    let makefile_text = fs::read_to_string(root.join("Makefile")).unwrap_or_default();
    let make_targets = makefile_targets(&makefile_text);

    let cli_commands = cli_surface_command_names(root);

    for enforcement in enforcements.values() {
        let name = enforcement.local_name();
        for artifact in &enforcement.artifacts {
            if !root.join(artifact).exists() {
                findings.push(error(
                    "stale-artifact",
                    format!(
                        "{name}: cited artifact {} does not exist",
                        py_repr(artifact)
                    ),
                ));
            }
        }
        for symbol in &enforcement.symbols {
            if !symbol_defined(symbol, &enforcement.artifacts, root, &mut caches) {
                findings.push(error(
                    "stale-symbol",
                    format!(
                        "{name}: symbol {} not found in any cited artifact",
                        py_repr(symbol)
                    ),
                ));
            }
        }
        for target in &enforcement.make_targets {
            if !make_targets.contains(target) {
                findings.push(error(
                    "stale-make-target",
                    format!("{name}: Makefile target {} does not exist", py_repr(target)),
                ));
            }
        }
        for command in &enforcement.cli_commands {
            if !cli_commands.contains(command) {
                findings.push(error(
                    "stale-cli-command",
                    format!(
                        "{name}: CLI command {} is not registered on gmeow or gmeow-dev",
                        py_repr(command)
                    ),
                ));
            }
        }
    }

    findings
}

// ─────────────────────────────────────────────────────────────────────────
// makeTarget → symbol execution binding, and gate-lane membership.
//
// A cited `meta:makeTarget` must (a) exist (checked above), (b) be reachable
// from a gate-aggregate lane, and (c) — when the enforcement also cites Rust
// `fn`/test symbols — have a static call-name path from the target's entrypoint
// to each cited symbol. This proves the citation is not merely a name that
// resolves in isolation but one the target actually *runs*, closing the hollow
// "cited-but-not-run" gate. The reachability is NAME-BASED and workspace-scoped:
// it establishes that a static call-name path EXISTS from the target's
// entrypoint to the symbol — not that the symbol is dynamically executed
// (macro/trait/fn-pointer indirection is invisible, and same-named fns are not
// disambiguated by module path). That leniency is deliberate and only ever
// makes the check MORE permissive, never falsely accusatory.

static MAKE_ASSIGN_RE: OnceLock<Regex> = OnceLock::new();
static MAKE_VAR_REF_RE: OnceLock<Regex> = OnceLock::new();
static MAKE_TARGET_HEAD_RE: OnceLock<Regex> = OnceLock::new();
static RUST_CALL_RE: OnceLock<Regex> = OnceLock::new();
static RUST_TEST_ATTR_RE: OnceLock<Regex> = OnceLock::new();
static CARGO_PKG_NAME_RE: OnceLock<Regex> = OnceLock::new();
static DISPATCH_ARM_RE: OnceLock<Regex> = OnceLock::new();
static XTASK_TARGET_RE: OnceLock<Regex> = OnceLock::new();

/// A Makefile target's prerequisites and its recipe command lines (each a
/// whitespace token list with simple `$(VAR)` references already expanded).
#[derive(Debug, Default, Clone)]
struct TargetRecipe {
    prereqs: Vec<String>,
    commands: Vec<Vec<String>>,
}

/// Single-line Makefile variable assignments (`NAME := v`, `NAME ?= v`,
/// `NAME = v`), first definition winning (matching make's `?=`/simple-var use
/// here well enough for the fixed vars the recipes reference).
fn makefile_variables(text: &str) -> BTreeMap<String, String> {
    let re = MAKE_ASSIGN_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z_][A-Za-z0-9_]*)\s*(?::=|\?=|=)\s*(.*)$")
            .expect("valid make assign regex")
    });
    let mut vars: BTreeMap<String, String> = BTreeMap::new();
    // `$(MAKE)` is a make built-in; recipes invoke sub-makes through it.
    vars.insert("MAKE".to_string(), "make".to_string());
    // Join `\`-continued assignment lines (e.g. a multi-line `CHECK_TARGETS`)
    // into one logical line before matching, so every listed value is seen.
    // A continuation line of an assignment may itself be tab-indented (make
    // strips leading whitespace), so a tab prefix only means "recipe line" when
    // we are NOT mid-continuation.
    let mut logical = String::new();
    for raw in text.lines() {
        if logical.is_empty() && raw.starts_with('\t') {
            // A recipe line that starts no assignment — skip it.
            continue;
        }
        let piece = if logical.is_empty() {
            raw
        } else {
            raw.trim_start()
        };
        if let Some(cont) = piece.strip_suffix('\\') {
            logical.push_str(cont);
            logical.push(' ');
            continue;
        }
        logical.push_str(piece);
        if let Some(cap) = re.captures(&logical) {
            vars.entry(cap[1].to_string())
                .or_insert_with(|| cap[2].trim().to_string());
        }
        logical.clear();
    }
    vars
}

/// Expand `$(VAR)` references (bounded depth; unknown vars and make functions
/// like `$(if …)` — which contain spaces and so never match — are left as-is
/// and simply become opaque tokens).
fn expand_make_vars(s: &str, vars: &BTreeMap<String, String>) -> String {
    let re = MAKE_VAR_REF_RE.get_or_init(|| {
        Regex::new(r"\$\(([A-Za-z_][A-Za-z0-9_]*)\)").expect("valid var ref regex")
    });
    let mut cur = s.to_string();
    for _ in 0..8 {
        let mut changed = false;
        cur = re
            .replace_all(&cur, |cap: &regex::Captures<'_>| match vars.get(&cap[1]) {
                Some(v) => {
                    changed = true;
                    v.clone()
                }
                None => cap[0].to_string(),
            })
            .into_owned();
        if !changed {
            break;
        }
    }
    cur
}

/// Parse the Makefile into `target → (prereqs, recipe command token lists)`.
/// Recipe line continuations (`\` at EOL) are joined; leading `@`/`-` recipe
/// prefixes are stripped; `$(VAR)` references are expanded.
fn makefile_recipes(text: &str) -> BTreeMap<String, TargetRecipe> {
    let head_re = MAKE_TARGET_HEAD_RE.get_or_init(|| {
        // A target header: `name:` (or `name: prereqs`), but NOT an assignment
        // (`name :=`/`name ?=`) — require the char after `:` to not be `=`.
        Regex::new(r"^([A-Za-z][A-Za-z0-9_-]*)\s*:(?:[^=].*|$)").expect("valid target head regex")
    });
    let vars = makefile_variables(text);
    let mut recipes: BTreeMap<String, TargetRecipe> = BTreeMap::new();
    let mut current: Option<String> = None;
    let mut pending = String::new();

    let flush_pending = |current: &Option<String>,
                         pending: &mut String,
                         recipes: &mut BTreeMap<String, TargetRecipe>| {
        if pending.is_empty() {
            return;
        }
        if let Some(t) = current {
            let expanded = expand_make_vars(pending, &vars);
            let toks: Vec<String> = expanded.split_whitespace().map(str::to_string).collect();
            if !toks.is_empty() {
                recipes.entry(t.clone()).or_default().commands.push(toks);
            }
        }
        pending.clear();
    };

    for raw in text.lines() {
        if let Some(stripped) = raw.strip_prefix('\t') {
            // Recipe line for the current target.
            let mut line = stripped.trim_start();
            while let Some(rest) = line.strip_prefix('@').or_else(|| line.strip_prefix('-')) {
                line = rest.trim_start();
            }
            pending.push(' ');
            if let Some(cont) = line.strip_suffix('\\') {
                pending.push_str(cont);
            } else {
                pending.push_str(line);
                flush_pending(&current, &mut pending, &mut recipes);
            }
            continue;
        }
        // Non-recipe line ends any pending continuation.
        flush_pending(&current, &mut pending, &mut recipes);
        if raw.starts_with('#') || raw.trim().is_empty() {
            continue;
        }
        if let Some(cap) = head_re.captures(raw) {
            let name = cap[1].to_string();
            // Prerequisites: everything after the first `:` up to a `#` comment.
            let after = raw.split_once(':').map(|x| x.1).unwrap_or("");
            let after = after.split('#').next().unwrap_or("");
            let prereqs: Vec<String> = expand_make_vars(after, &vars)
                .split_whitespace()
                .map(str::to_string)
                .collect();
            recipes.entry(name.clone()).or_default().prereqs = prereqs;
            current = Some(name);
        } else {
            current = None;
        }
    }
    flush_pending(&current, &mut pending, &mut recipes);
    recipes
}

/// Sub-make targets named by `make <t> …` within a command's tokens (skipping
/// flags and `VAR=value` assignments).
fn submake_targets(cmd: &[String]) -> Vec<String> {
    if cmd.first().map(String::as_str) != Some("make") {
        return Vec::new();
    }
    cmd[1..]
        .iter()
        .filter(|t| !t.starts_with('-') && !t.contains('='))
        .cloned()
        .collect()
}

/// Make targets declared by the `cargo xtask check` DAG. The xtask is the
/// canonical aggregate scheduler, so static Make reachability must cross that
/// delegation boundary instead of treating it as an opaque command.
fn xtask_check_targets(root: &Path) -> BTreeSet<String> {
    let Ok(text) = fs::read_to_string(root.join("crates/xtask/src/main.rs")) else {
        return BTreeSet::new();
    };
    let Some(start) = text.find("const CHECK_DAG:") else {
        return BTreeSet::new();
    };
    let body = &text[start..];
    let body = body.find("];").map_or(body, |end| &body[..end]);
    let re = XTASK_TARGET_RE.get_or_init(|| {
        Regex::new(r#"\btarget\s*:\s*\"([^\"]+)\""#).expect("valid xtask target regex")
    });
    re.captures_iter(body)
        .map(|capture| capture[1].to_string())
        .collect()
}

fn invokes_xtask_check(cmd: &[String]) -> bool {
    cmd.windows(3)
        .any(|window| window == ["cargo", "xtask", "check"])
}

/// Transitive closure of targets reached by running `root`: itself, its
/// prerequisites, every `make <t>` sub-invocation, and the Make targets
/// delegated to the canonical `cargo xtask check` DAG, cycle-guarded.
fn reached_targets(
    root: &str,
    recipes: &BTreeMap<String, TargetRecipe>,
    xtask_targets: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![root.to_string()];
    while let Some(t) = stack.pop() {
        if !seen.insert(t.clone()) {
            continue;
        }
        if let Some(r) = recipes.get(&t) {
            for p in &r.prereqs {
                if !seen.contains(p) {
                    stack.push(p.clone());
                }
            }
            for cmd in &r.commands {
                for sub in submake_targets(cmd) {
                    if !seen.contains(&sub) {
                        stack.push(sub);
                    }
                }
                if invokes_xtask_check(cmd) {
                    for delegated in xtask_targets {
                        if !seen.contains(delegated) {
                            stack.push(delegated.clone());
                        }
                    }
                }
            }
        }
    }
    seen
}

/// What a leaf recipe command executes, for symbol-reachability purposes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ExecKind {
    /// `cargo nextest`/`cargo test` over a package set (`None` = whole
    /// workspace minus `excludes`).
    Nextest {
        packages: Option<BTreeSet<String>>,
        excludes: BTreeSet<String>,
    },
    /// `gmeow-dev`/`gmeow` `<subcommand>` (kebab name).
    Subcommand(String),
    /// Anything else — binds nothing (a doctest lane, clippy, a shell script,
    /// or an unrecognised `cargo run` bin).
    Opaque,
}

/// Classify one already-var-expanded, tokenised leaf command.
fn classify_command(cmd: &[String]) -> ExecKind {
    let has = |w: &str| cmd.iter().any(|t| t == w);
    if has("cargo") && (has("nextest") || has("test")) {
        if has("--doc") {
            return ExecKind::Opaque; // doctests run doc examples, not cited fns.
        }
        let mut packages: BTreeSet<String> = BTreeSet::new();
        let mut excludes: BTreeSet<String> = BTreeSet::new();
        let mut it = cmd.iter();
        let mut workspace = false;
        while let Some(t) = it.next() {
            match t.as_str() {
                "-p" | "--package" => {
                    if let Some(p) = it.next() {
                        packages.insert(p.clone());
                    }
                }
                "--workspace" => workspace = true,
                "--exclude" => {
                    if let Some(p) = it.next() {
                        excludes.insert(p.clone());
                    }
                }
                other => {
                    if let Some(p) = other.strip_prefix("--package=") {
                        packages.insert(p.to_string());
                    } else if let Some(p) = other.strip_prefix("--exclude=") {
                        excludes.insert(p.to_string());
                    }
                }
            }
        }
        return ExecKind::Nextest {
            packages: if workspace || packages.is_empty() {
                None
            } else {
                Some(packages)
            },
            excludes,
        };
    }
    if has("cargo") && has("run") {
        // `cargo run … -p <cli-crate> -- <sub> …` is a CLI subcommand;
        // any other `cargo run` bin is opaque for our purposes.
        let pkg = cmd
            .iter()
            .position(|t| t == "-p" || t == "--package")
            .and_then(|i| cmd.get(i + 1))
            .map(String::as_str);
        let is_cli = matches!(pkg, Some("gmeow-dev-cli") | Some("gmeow-cli"));
        if is_cli
            && let Some(sub) = cmd
                .iter()
                .position(|t| t == "--")
                .and_then(|dd| cmd.get(dd + 1))
        {
            return ExecKind::Subcommand(sub.clone());
        }
        return ExecKind::Opaque;
    }
    ExecKind::Opaque
}

/// One Rust `fn` definition found in the workspace: its owning package, whether
/// it carries a `#[test]`/`#[bench]` attribute, and the set of identifiers it
/// calls (approximate — any `ident(` in its body).
#[derive(Debug, Default, Clone)]
struct FnDef {
    package: String,
    is_test: bool,
    callees: BTreeSet<String>,
}

/// Workspace `fn` index: name → every definition of that name (overloads across
/// impls/modules/crates are all kept — reachability is intentionally lenient).
type FnIndex = BTreeMap<String, Vec<FnDef>>;

/// Recursively collect `*.rs` paths under `dir`, skipping any `target` tree.
fn collect_rs_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|n| n.to_str()) == Some("target") {
                continue;
            }
            collect_rs_files(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            out.push(path);
        }
    }
}

/// Every workspace package under `crates/*`, mapped `package-name → crate dir`.
fn workspace_packages(root: &Path) -> BTreeMap<String, std::path::PathBuf> {
    let name_re = CARGO_PKG_NAME_RE.get_or_init(|| {
        Regex::new(r#"(?m)^\s*name\s*=\s*"([^"]+)""#).expect("valid pkg name regex")
    });
    let mut pkgs = BTreeMap::new();
    let crates = root.join("crates");
    let Ok(entries) = fs::read_dir(&crates) else {
        return pkgs;
    };
    for entry in entries.flatten() {
        let dir = entry.path();
        let manifest = dir.join("Cargo.toml");
        if let Ok(text) = fs::read_to_string(&manifest)
            && let Some(cap) = name_re.captures(&text)
        {
            pkgs.insert(cap[1].to_string(), dir);
        }
    }
    pkgs
}

/// Parse all `fn` definitions in one `.rs` file into `index`, tagging each with
/// `package`, whether it is a test, and its called-identifier set. Operates on
/// the comment/string-stripped source so call sites in comments/strings do not
/// pollute the call graph.
fn index_rust_file(text: &str, package: &str, index: &mut FnIndex) {
    let fn_re = RUST_ITEM_FN_RE.get_or_init(|| {
        Regex::new(r"\bfn\s+([A-Za-z_][A-Za-z0-9_]*)").expect("valid rust fn regex")
    });
    let call_re = RUST_CALL_RE.get_or_init(|| {
        Regex::new(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*\(").expect("valid rust call regex")
    });
    let test_re = RUST_TEST_ATTR_RE.get_or_init(|| {
        Regex::new(r"#\s*\[\s*(?:[A-Za-z_][A-Za-z0-9_]*\s*::\s*)*(?:test|bench)\b")
            .expect("valid rust test-attr regex")
    });
    const KEYWORDS: &[&str] = &[
        "if", "while", "for", "match", "return", "fn", "let", "as", "in", "loop", "move",
    ];

    let stripped = strip_rust_comments_and_strings(text);
    let b = stripped.as_bytes();
    for cap in fn_re.captures_iter(&stripped) {
        let name = cap[1].to_string();
        let name_m = cap.get(1).expect("fn name group");
        let fn_kw_start = cap.get(0).expect("fn match").start();

        // Signature scan from just after the name: balance ()/[] and stop at the
        // first depth-0 `{` (body) or `;` (bodiless decl).
        let mut i = name_m.end();
        let mut depth: i32 = 0;
        let mut body_start: Option<usize> = None;
        while i < b.len() {
            match b[i] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b'{' if depth <= 0 => {
                    body_start = Some(i);
                    break;
                }
                b';' if depth <= 0 => break,
                _ => {}
            }
            i += 1;
        }

        let mut def = FnDef {
            package: package.to_string(),
            is_test: false,
            callees: BTreeSet::new(),
        };
        // Attributes bind to this fn iff they sit between the previous item
        // terminator (`;`/`{`/`}`) and the `fn` keyword.
        let attr_from = stripped[..fn_kw_start]
            .rfind([';', '{', '}'])
            .map(|p| p + 1)
            .unwrap_or(0);
        if test_re.is_match(&stripped[attr_from..fn_kw_start]) {
            def.is_test = true;
        }
        if let Some(bs) = body_start {
            // Balance the body braces to find its end.
            let mut d: i32 = 0;
            let mut j = bs;
            let mut end = bs;
            while j < b.len() {
                match b[j] {
                    b'{' => d += 1,
                    b'}' => {
                        d -= 1;
                        if d == 0 {
                            end = j;
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            for c in call_re.captures_iter(&stripped[bs..end.max(bs)]) {
                let callee = &c[1];
                if !KEYWORDS.contains(&callee) {
                    def.callees.insert(callee.to_string());
                }
            }
        }
        index.entry(name).or_default().push(def);
    }
}

/// Build the workspace `fn` index once (all `crates/*` packages).
fn build_fn_index(root: &Path) -> FnIndex {
    let mut index: FnIndex = BTreeMap::new();
    for (pkg, dir) in workspace_packages(root) {
        let mut files = Vec::new();
        collect_rs_files(&dir, &mut files);
        for f in files {
            if let Ok(text) = fs::read_to_string(&f) {
                index_rust_file(&text, &pkg, &mut index);
            }
        }
    }
    index
}

/// Names reachable from `roots` by following called-identifier edges through
/// workspace `fn` definitions (a callee is followed only if it names a defined
/// workspace fn — the strictness lever). Returns the set of all reached names,
/// which includes the roots themselves.
fn reachable_names(roots: &BTreeSet<String>, index: &FnIndex) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<String> = roots.iter().cloned().collect();
    while let Some(name) = stack.pop() {
        if !seen.insert(name.clone()) {
            continue;
        }
        if let Some(defs) = index.get(&name) {
            for def in defs {
                for callee in &def.callees {
                    if !seen.contains(callee) && index.contains_key(callee) {
                        stack.push(callee.clone());
                    }
                }
            }
        }
    }
    seen
}

/// Map each CLI subcommand (kebab name) to its handler fn identifier, read from
/// the `match cli.command { Commands::Variant … => path::handler(…) }` dispatch
/// in each CLI crate's `lib.rs`.
fn cli_subcommand_handlers(root: &Path) -> BTreeMap<String, String> {
    let arm_re = DISPATCH_ARM_RE.get_or_init(|| {
        Regex::new(
            r"Commands::([A-Za-z_][A-Za-z0-9_]*)(?:\s*\{[^}]*\}|\s*\([^)]*\))?\s*=>\s*(?:\{[\s\S]*?)?(?:[A-Za-z_][A-Za-z0-9_]*::)*([A-Za-z_][A-Za-z0-9_]*)\s*\(",
        )
        .expect("valid dispatch-arm regex")
    });
    let mut map = BTreeMap::new();
    for rel in [
        "crates/gmeow-cli/src/lib.rs",
        "crates/gmeow-dev-cli/src/lib.rs",
    ] {
        let Ok(text) = fs::read_to_string(root.join(rel)) else {
            continue;
        };
        let stripped = strip_rust_comments_and_strings(&text);
        for cap in arm_re.captures_iter(&stripped) {
            let variant = &cap[1];
            let handler = cap[2].to_string();
            map.entry(variant_to_kebab(variant)).or_insert(handler);
        }
    }
    map
}

/// Names of every documented top-level workflow target — a header line
/// `name: … ## help`. In this Makefile the `## ` help annotation marks the
/// public, directly-invocable workflow surface ("Make … names the workflows");
/// an undocumented target is an internal helper. A gate may cite a documented
/// workflow directly; citing an undocumented/dead target is what
/// `off-lane-target` forbids.
fn documented_workflow_targets(text: &str) -> BTreeSet<String> {
    let head_re = MAKE_TARGET_HEAD_RE.get_or_init(|| {
        Regex::new(r"^([A-Za-z][A-Za-z0-9_-]*)\s*:(?:[^=].*|$)").expect("valid target head regex")
    });
    text.lines()
        .filter(|line| !line.starts_with('\t') && line.contains("## "))
        .filter_map(|line| head_re.captures(line).map(|c| c[1].to_string()))
        .collect()
}

/// The gate-aggregate lanes a cited `meta:makeTarget` must be reachable from:
/// the local `check` lane and its `CHECK_TARGETS`, the explicit test / release /
/// maintainer entrypoints, and every documented (`## `) top-level workflow
/// target. A cited target reachable from none of these is dangling
/// (`off-lane-target`).
fn gate_lane_targets(
    text: &str,
    recipes: &BTreeMap<String, TargetRecipe>,
    vars: &BTreeMap<String, String>,
    xtask_targets: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut roots: BTreeSet<String> = ["check", "rust-test", "full-release", "verify-release"]
        .into_iter()
        .map(str::to_string)
        .collect();
    if let Some(v) = vars.get("CHECK_TARGETS") {
        for t in expand_make_vars(v, vars).split_whitespace() {
            roots.insert(t.to_string());
        }
    }
    for name in recipes.keys() {
        if name.starts_with("maint-") {
            roots.insert(name.clone());
        }
    }
    roots.extend(documented_workflow_targets(text));
    let mut lane: BTreeSet<String> = BTreeSet::new();
    for r in &roots {
        lane.extend(reached_targets(r, recipes, xtask_targets));
    }
    lane
}

/// Names reachable by *running* a make target: the union of the reachable-name
/// closures of every entrypoint of every leaf command in the target's
/// transitive `reached_targets` set.
fn names_reached_by_target(
    target: &str,
    recipes: &BTreeMap<String, TargetRecipe>,
    handlers: &BTreeMap<String, String>,
    index: &FnIndex,
    test_names_by_pkg: &BTreeMap<String, BTreeSet<String>>,
    xtask_targets: &BTreeSet<String>,
) -> BTreeSet<String> {
    let mut roots: BTreeSet<String> = BTreeSet::new();
    for t in reached_targets(target, recipes, xtask_targets) {
        let Some(recipe) = recipes.get(&t) else {
            continue;
        };
        for cmd in &recipe.commands {
            match classify_command(cmd) {
                ExecKind::Nextest { packages, excludes } => match packages {
                    None => {
                        for (pkg, names) in test_names_by_pkg {
                            if !excludes.contains(pkg) {
                                roots.extend(names.iter().cloned());
                            }
                        }
                    }
                    Some(pkgs) => {
                        for pkg in &pkgs {
                            if let Some(names) = test_names_by_pkg.get(pkg) {
                                roots.extend(names.iter().cloned());
                            }
                        }
                    }
                },
                ExecKind::Subcommand(sub) => {
                    if let Some(handler) = handlers.get(&sub) {
                        roots.insert(handler.clone());
                    }
                }
                ExecKind::Opaque => {}
            }
        }
    }
    reachable_names(&roots, index)
}

/// makeTarget → symbol execution binding and gate-lane membership, over every
/// enforcement that cites make targets.
fn check_target_bindings(
    enforcements: &BTreeMap<String, Enforcement>,
    root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    let makefile_text = fs::read_to_string(root.join("Makefile")).unwrap_or_default();
    if makefile_text.is_empty() {
        return findings;
    }
    let recipes = makefile_recipes(&makefile_text);
    let vars = makefile_variables(&makefile_text);
    let xtask_targets = xtask_check_targets(root);
    let lane = gate_lane_targets(&makefile_text, &recipes, &vars, &xtask_targets);

    // ── gate-lane membership (H3) ──────────────────────────────────────────
    for enforcement in enforcements.values() {
        let name = enforcement.local_name();
        for target in &enforcement.make_targets {
            // Only adjudicate targets that actually exist (a missing target is
            // already reported as `stale-make-target`).
            if recipes.contains_key(target) && !lane.contains(target) {
                findings.push(error(
                    "off-lane-target",
                    format!(
                        "{name}: Makefile target {} exists but is reachable from no gate lane (check / xtask DAG / rust-test / release / maint-*)",
                        py_repr(target)
                    ),
                ));
            }
        }
    }

    // ── makeTarget → symbol execution binding (H2) ─────────────────────────
    // Only build the (expensive) workspace fn index when a node co-cites both
    // an existing make target and Rust fn symbols.
    let needs_binding = enforcements
        .values()
        .any(|e| !e.symbols.is_empty() && e.make_targets.iter().any(|t| recipes.contains_key(t)));
    if !needs_binding {
        return findings;
    }
    let index = build_fn_index(root);
    let handlers = cli_subcommand_handlers(root);
    let mut test_names_by_pkg: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for (fname, defs) in &index {
        for def in defs {
            if def.is_test {
                test_names_by_pkg
                    .entry(def.package.clone())
                    .or_default()
                    .insert(fname.clone());
            }
        }
    }

    // Memoise per-target reachable-name closures across enforcements.
    let mut reach_cache: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for enforcement in enforcements.values() {
        let name = enforcement.local_name();
        let cited_targets: Vec<&String> = enforcement
            .make_targets
            .iter()
            .filter(|t| recipes.contains_key(*t))
            .collect();
        if cited_targets.is_empty() {
            continue;
        }
        for symbol in &enforcement.symbols {
            // Binding only applies to Rust fn/test symbols (the only thing a
            // make target can be said to *execute*). A symbol with no fn
            // definition anywhere in the workspace is a type/const/ontology
            // term — its existence is covered by the symbol check, and it is
            // not "run", so it is out of binding scope.
            if !index.contains_key(symbol) {
                continue;
            }
            let bound = cited_targets.iter().any(|target| {
                let reachable = reach_cache.entry((*target).clone()).or_insert_with(|| {
                    names_reached_by_target(
                        target,
                        &recipes,
                        &handlers,
                        &index,
                        &test_names_by_pkg,
                        &xtask_targets,
                    )
                });
                reachable.contains(symbol)
            });
            if !bound {
                let targets: Vec<&str> = cited_targets.iter().map(|s| s.as_str()).collect();
                findings.push(error(
                    "unbound-symbol",
                    format!(
                        "{name}: symbol {} has no static call path from any cited makeTarget ({}) — cited but not run",
                        py_repr(symbol),
                        targets.join(", ")
                    ),
                ));
            }
        }
    }

    findings
}

fn format_list(set: &BTreeSet<i64>) -> String {
    if set.is_empty() {
        "∅".to_string()
    } else {
        format!("{:?}", set.iter().collect::<Vec<_>>())
    }
}

/// Python-style ``repr`` for strings: single-quoted, escaping ``\`` and ``'``.
fn py_repr(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for ch in s.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '\'' => out.push_str("\\'"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('\'');
    out
}

fn compare_relation(
    prop: &str,
    md_relations: &BTreeMap<i64, BTreeSet<i64>>,
    ttl_relations: &BTreeMap<i64, BTreeSet<i64>>,
) -> Vec<Finding> {
    let mut findings = Vec::new();
    for number in md_relations
        .keys()
        .chain(ttl_relations.keys())
        .copied()
        .collect::<BTreeSet<_>>()
    {
        let md = md_relations.get(&number).cloned().unwrap_or_default();
        let ttl = ttl_relations.get(&number).cloned().unwrap_or_default();
        if md != ttl {
            findings.push(error(
                "relation-drift",
                format!(
                    "principle {number} meta:{prop} drift: CONSTITUTION.md marker names {}, governance/constitution.ttl names {}",
                    format_list(&md),
                    format_list(&ttl)
                ),
            ));
        }
    }
    findings
}

/// The bold supersession markers in CONSTITUTION.md must match the TTL relations.
pub fn check_supersession(md_text: &str, principles: &[Principle]) -> Vec<Finding> {
    let mut findings = Vec::new();

    let md_superseded = markdown_relations(md_text, "**Superseded in part by Principle");
    let ttl_superseded: BTreeMap<i64, BTreeSet<i64>> = principles
        .iter()
        .map(|p| {
            (
                p.number,
                p.superseded_in_part_by
                    .iter()
                    .copied()
                    .collect::<BTreeSet<_>>(),
            )
        })
        .filter(|(_, set)| !set.is_empty())
        .collect();
    findings.extend(compare_relation(
        "supersededInPartBy",
        &md_superseded,
        &ttl_superseded,
    ));

    let md_extends = markdown_relations(md_text, "**Extends Principle");
    let ttl_extends: BTreeMap<i64, BTreeSet<i64>> = principles
        .iter()
        .map(|p| (p.number, p.extends.iter().copied().collect::<BTreeSet<_>>()))
        .filter(|(_, set)| !set.is_empty())
        .collect();
    findings.extend(compare_relation("extends", &md_extends, &ttl_extends));

    findings
}

fn load_dataset_from_ttl(ttl: &str) -> gmeow_errors::Result<std::sync::Arc<RdfDataset>> {
    purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).map_err(|e| {
        gmeow_errors::Diag::of_kind(crate::error::Parse {
            detail: e.to_string(),
        })
    })
}

/// Run every constitution-as-code check into one granular finding list.
pub fn constitution_full_report(
    manifest_path: &Path,
    constitution_path: &Path,
    root: &Path,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    let ttl = match fs::read_to_string(manifest_path) {
        Ok(text) => text,
        Err(e) => {
            findings.push(error(
                "manifest-unreadable",
                format!("{}: cannot read: {e}", manifest_path.display()),
            ));
            return findings;
        }
    };

    let dataset = match load_dataset_from_ttl(&ttl) {
        Ok(ds) => ds,
        Err(e) => {
            findings.push(error(
                "manifest-parse",
                format!("{}: does not parse: {e}", manifest_path.display()),
            ));
            return findings;
        }
    };

    let md_text = match fs::read_to_string(constitution_path) {
        Ok(text) => text,
        Err(e) => {
            findings.push(error(
                "constitution-unreadable",
                format!("{}: cannot read: {e}", constitution_path.display()),
            ));
            return findings;
        }
    };

    let enforcements = collect_enforcements(&dataset);
    let principles = collect_principles(&dataset);
    let headings = constitution_headings(&md_text);

    findings.extend(check_enforcement_coverage(&dataset));
    findings.extend(check_principle_sync(&principles, &headings));
    findings.extend(check_references(&enforcements, root));
    findings.extend(check_target_bindings(&enforcements, root));
    findings.extend(check_supersession(&md_text, &principles));

    findings.sort_by(|a, b| (&a.code, &a.message).cmp(&(&b.code, &b.message)));
    findings
}

/// Build one `constitution.<code>` error finding.
fn error(code: &str, message: String) -> Finding {
    Finding::new(
        Severity::Error,
        format!("{}{code}", crate::codes::CONSTITUTION_FAMILY),
        message,
    )
    .with_tool("constitution")
}

#[cfg(test)]
mod tests {
    use super::*;

    // cargo-mutants (T9) surfaced surviving mutants in `literal_i64` /
    // `literal_string` — the helpers had no direct coverage, so replacing their
    // body with `None`/`Some(0)`/deleting the match arm went undetected. These
    // tests pin both the literal path and the non-literal fallthrough, killing
    // that mutant cluster.
    /// Resolve the object term of the single triple `<s> <p> obj` in a tiny dataset,
    /// where `obj` is the given Turtle object syntax.
    fn object_term_ref(ds: &RdfDataset) -> TermRef<'_> {
        let q = ds
            .quads_for_pattern(None, None, None, GraphMatch::Any)
            .next()
            .expect("one triple");
        ds.resolve(q.o)
    }

    #[test]
    fn literal_i64_parses_only_integer_literals() {
        let lit = store_from("<https://e/s> <https://e/p> \"42\" .");
        assert_eq!(literal_i64(object_term_ref(&lit)), Some(42));
        let neg = store_from("<https://e/s> <https://e/p> \"-7\" .");
        assert_eq!(literal_i64(object_term_ref(&neg)), Some(-7));
        let bad = store_from("<https://e/s> <https://e/p> \"notanint\" .");
        assert_eq!(literal_i64(object_term_ref(&bad)), None);
        let iri = store_from("<https://e/s> <https://e/p> <https://e/x> .");
        assert_eq!(literal_i64(object_term_ref(&iri)), None);
    }

    #[test]
    fn literal_string_extracts_only_literal_lexical_values() {
        let lit = store_from("<https://e/s> <https://e/p> \"hello\" .");
        assert_eq!(
            literal_string(object_term_ref(&lit)),
            Some("hello".to_string())
        );
        let iri = store_from("<https://e/s> <https://e/p> <https://e/x> .");
        assert_eq!(literal_string(object_term_ref(&iri)), None);
    }

    fn store_from(ttl: &str) -> std::sync::Arc<RdfDataset> {
        purrdf::parse_dataset(ttl.as_bytes(), "text/turtle", None).unwrap()
    }

    const PREFIX: &str = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
         @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n";

    #[test]
    fn unenforced_principle_is_an_error() {
        let store = store_from(&format!(
            "{PREFIX}meta:P1 a meta:Principle ; meta:number 1 ; meta:title \"Solo\" .\n"
        ));
        let msgs: Vec<String> = check_enforcement_coverage(&store)
            .into_iter()
            .map(|f| f.message)
            .collect();
        assert!(
            msgs.iter()
                .any(|m| m.contains("zero registered enforcement"))
        );
    }

    #[test]
    fn practice_only_principle_warns_and_orphan_errors() {
        let store = store_from(&format!(
            "{PREFIX}\
             meta:P1 a meta:Principle ; meta:number 1 ; meta:title \"Honor\" ; meta:enforcedBy meta:rev .\n\
             meta:rev a meta:Practice .\n\
             meta:gate-orphan a meta:Gate .\n"
        ));
        let findings = check_enforcement_coverage(&store);
        assert!(
            findings
                .iter()
                .any(|f| f.severity == Severity::Warning && f.message.contains("review practice"))
        );
        assert!(
            findings
                .iter()
                .any(|f| f.code == "constitution.orphaned-enforcement"
                    && f.message.contains("gate-orphan"))
        );
    }

    // ------------------------------------------------------------------
    // Pure helper unit tests
    // ------------------------------------------------------------------

    #[test]
    fn constitution_headings_extracts_numbered_sections() {
        let md = "# Preamble\n\n## 1. First\nbody\n## 2. Second thing\n";
        let got = constitution_headings(md);
        let mut expected = BTreeMap::new();
        expected.insert(1, "First".to_string());
        expected.insert(2, "Second thing".to_string());
        assert_eq!(got, expected);
    }

    #[test]
    fn markdown_relations_read_marker_lines() {
        let md = "## 1. A\n\n**Superseded in part by Principle 2:** ok.\n\n## 2. B\nno marker.\n";
        let got = markdown_relations(md, "**Superseded in part by Principle");
        let mut expected: BTreeMap<i64, BTreeSet<i64>> = BTreeMap::new();
        expected.insert(1, [2].into_iter().collect());
        assert_eq!(got, expected);
    }

    #[test]
    fn makefile_targets_skips_pattern_rules() {
        let mk = "all:\n\t@echo ok\n\n%.o: %.c\n\tcc $< -o $@\n\ncheck:\n";
        let got = makefile_targets(mk);
        assert!(got.contains("all"));
        assert!(got.contains("check"));
        assert!(!got.contains("%.o"));
    }

    #[test]
    fn python_top_level_names_finds_definitions_and_assignments() {
        let py = "class Foo:\n    pass\n\ndef bar():\n    pass\n\nasync def baz():\n    pass\n\nX: int = 1\nY = 2\n";
        let got = python_top_level_names(py);
        assert!(got.contains("Foo"));
        assert!(got.contains("bar"));
        assert!(got.contains("baz"));
        assert!(got.contains("X"));
        assert!(got.contains("Y"));
        assert!(!got.contains("pass"));
    }

    #[test]
    fn python_top_level_names_ignores_nested_symbols() {
        let py = r#"
def outer():
    def inner():
        pass
    class NestedClass:
        pass
    nested_var = 1
class TopClass:
    def method(self):
        pass
top_level = 42
"#;
        let got = python_top_level_names(py);
        assert!(got.contains("outer"));
        assert!(got.contains("TopClass"));
        assert!(got.contains("top_level"));
        assert!(!got.contains("inner"), "nested def should not be collected");
        assert!(
            !got.contains("NestedClass"),
            "nested class should not be collected"
        );
        assert!(
            !got.contains("nested_var"),
            "nested assignment should not be collected"
        );
        assert!(
            !got.contains("method"),
            "method inside class should not be collected"
        );
    }

    #[test]
    fn rust_item_names_finds_all_forms_and_excludes_noise() {
        let rust = r###"
/// Doc link to [`ghost_doc`] must not count as a definition.
pub fn real_fn() {}
const REAL_CONST: u32 = 1;
static mut REAL_STATIC: u32 = 2;
struct RealStruct;
enum RealEnum { A }
trait RealTrait { type RealAssoc; fn real_trait_method(&self); }
macro_rules! real_macro { () => {}; }
impl RealStruct {
    pub fn real_method(&self) {
        // a call site, not a definition:
        ghost_call();
        let _ghost_str = "ghost_string_name";
        let _c = '"'; // a quote inside a char literal must not open a string
    }
}
mod tests {
    #[test]
    fn real_nested_test() {}
}
"###;
        let got = rust_item_names(rust);
        for want in [
            "real_fn",
            "REAL_CONST",
            "REAL_STATIC",
            "RealStruct",
            "RealEnum",
            "RealTrait",
            "RealAssoc",
            "real_trait_method",
            "real_macro",
            "real_method",
            "real_nested_test",
        ] {
            assert!(got.contains(want), "missing item {want}: {got:?}");
        }
        for ghost in ["ghost_doc", "ghost_call", "ghost_string_name", "fn"] {
            assert!(!got.contains(ghost), "ghost {ghost} leaked: {got:?}");
        }
    }

    #[test]
    fn strip_rust_comments_and_strings_blanks_noise_keeps_code() {
        let src = "fn a() {}\n// fn commented\nlet s = \"fn instr\";\n/* fn block */ fn b() {}\n";
        let stripped = strip_rust_comments_and_strings(src);
        // Real definitions survive; commented / in-string `fn NAME` are gone.
        let names = rust_item_names(src);
        assert!(names.contains("a") && names.contains("b"), "{names:?}");
        assert!(
            !names.contains("commented") && !names.contains("instr"),
            "{names:?}"
        );
        // Line structure preserved (newline count unchanged).
        assert_eq!(src.matches('\n').count(), stripped.matches('\n').count());
    }

    #[test]
    fn variant_to_kebab_matches_clap_default_rename() {
        assert_eq!(variant_to_kebab("Version"), "version");
        assert_eq!(variant_to_kebab("SliceQuality"), "slice-quality");
        assert_eq!(
            variant_to_kebab("VerifyReleaseBundle"),
            "verify-release-bundle"
        );
        assert_eq!(variant_to_kebab("Mcp"), "mcp");
        assert_eq!(variant_to_kebab("BoxRoles"), "box-roles");
        assert_eq!(variant_to_kebab("I18n"), "i18n");
        assert_eq!(variant_to_kebab("ExportCsv"), "export-csv");
    }

    #[test]
    fn cli_command_names_from_rust_reads_variants_and_forms() {
        let rust = "\
            #[derive(Debug, Subcommand)]\n\
            pub enum Commands {\n\
            \x20   /// bare variant.\n\
            \x20   Version,\n\
            \x20   /// tuple variant.\n\
            \x20   Info(InfoArgs),\n\
            \x20   /// struct variant with a field carrying its own attr.\n\
            \x20   Sync {\n\
            \x20       #[arg(long = \"mode\")]\n\
            \x20       mode: String,\n\
            \x20   },\n\
            }\n";
        let got = cli_command_names_from_rust(rust);
        assert!(got.contains("version"));
        assert!(got.contains("info"));
        assert!(got.contains("sync"));
        // A field attribute inside a variant body must not leak as a command.
        assert!(!got.contains("mode"));
    }

    #[test]
    fn cli_command_names_from_rust_honors_command_name_override() {
        let rust = "\
            #[derive(Debug, Subcommand)]\n\
            pub enum Commands {\n\
            \x20   /// override wins over the kebab of the identifier.\n\
            \x20   #[command(name = \"sync-now\")]\n\
            \x20   SyncNow,\n\
            \x20   /// a non-name command attr must not become an override.\n\
            \x20   #[command(disable_help_flag = true)]\n\
            \x20   Gts {\n\
            \x20       #[arg(trailing_var_arg = true)]\n\
            \x20       args: Vec<String>,\n\
            \x20   },\n\
            }\n";
        let got = cli_command_names_from_rust(rust);
        assert!(got.contains("sync-now"));
        assert!(!got.contains("sync_now"));
        assert!(got.contains("gts"));
    }

    #[test]
    fn cli_command_names_from_rust_includes_subapp_groups_and_nested() {
        let rust = "\
            #[derive(Debug, Subcommand)]\n\
            pub enum Commands {\n\
            \x20   /// group carrier variant.\n\
            \x20   Logic {\n\
            \x20       #[command(subcommand)]\n\
            \x20       command: LogicCommands,\n\
            \x20   },\n\
            }\n\
            #[derive(Debug, Subcommand)]\n\
            pub enum LogicCommands {\n\
            \x20   /// backward goal resolution.\n\
            \x20   Query,\n\
            \x20   /// compile pipeline.\n\
            \x20   Compile,\n\
            }\n";
        let got = cli_command_names_from_rust(rust);
        // Sub-app group name surfaces from the top-level carrier variant.
        assert!(got.contains("logic"));
        // Nested sub-app subcommands surface from the nested enum.
        assert!(got.contains("query"));
        assert!(got.contains("compile"));
    }

    #[test]
    fn cli_surface_command_names_reads_both_rust_bins() {
        let tmp = tempfile::tempdir().unwrap();
        let public = tmp.path().join("crates/gmeow-cli/src");
        let dev = tmp.path().join("crates/gmeow-dev-cli/src");
        fs::create_dir_all(&public).unwrap();
        fs::create_dir_all(&dev).unwrap();
        fs::write(
            public.join("lib.rs"),
            "#[derive(Subcommand)]\npub enum Commands {\n    #[command(name = \"verify-release-bundle\")]\n    VerifyReleaseBundle,\n}\n",
        )
        .unwrap();
        fs::write(
            dev.join("lib.rs"),
            "#[derive(Subcommand)]\npub enum Commands {\n    #[command(name = \"release-bundle\")]\n    ReleaseBundle,\n}\n",
        )
        .unwrap();

        let got = cli_surface_command_names(tmp.path());
        assert!(got.contains("verify-release-bundle"));
        assert!(got.contains("release-bundle"));
    }

    // ------------------------------------------------------------------
    // Integration tests over temp directories
    // ------------------------------------------------------------------

    fn write_pair(
        tmp: &tempfile::TempDir,
        manifest_ttl: &str,
        constitution_md: &str,
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let prefixes = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";
        let manifest = tmp.path().join("constitution.ttl");
        fs::write(&manifest, format!("{prefixes}{manifest_ttl}")).unwrap();
        let constitution = tmp.path().join("CONSTITUTION.md");
        fs::write(&constitution, constitution_md).unwrap();
        (manifest, constitution)
    }

    #[test]
    fn zero_enforcement_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("zero registered enforcement"))
        );
    }

    #[test]
    fn practice_only_principle_warns_not_errors() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:practice-x a meta:Practice ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:practice-x .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(
            findings.iter().any(|f| f.severity == Severity::Warning
                && f.message.contains("only by review practice"))
        );
        assert!(
            !findings
                .iter()
                .any(|f| f.message.contains("zero registered enforcement"))
        );
    }

    #[test]
    fn stale_artifact_reference_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:gate-x a meta:Gate ; meta:artifact \"no/such/file.py\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("'no/such/file.py' does not exist"))
        );
    }

    #[test]
    fn stale_symbol_make_target_and_cli_command_are_errors() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let py_dir = tmp.path().join("src/gmeow_tools");
        fs::create_dir_all(&py_dir).unwrap();
        fs::write(py_dir.join("validate.py"), "def real_function(): pass\n").unwrap();

        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:gate-x a meta:Gate ;\n\
             meta:artifact \"src/gmeow_tools/validate.py\" ;\n\
             meta:symbol \"no_such_function\" ;\n\
             meta:makeTarget \"no-such-target\" ;\n\
             meta:cliCommand \"no-such-command\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
        assert!(text.contains("'no_such_function' not found"), "{text}");
        assert!(text.contains("Makefile target 'no-such-target'"), "{text}");
        assert!(text.contains("CLI command 'no-such-command'"), "{text}");
    }

    #[test]
    fn stale_rust_symbol_only_in_comment_or_string_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let rs_dir = tmp.path().join("crates/x/src");
        fs::create_dir_all(&rs_dir).unwrap();
        // `ghost_symbol` appears only in a comment, a string, and a call site —
        // never as an item definition. `real_item` is a genuine impl method.
        fs::write(
            rs_dir.join("lib.rs"),
            "// ghost_symbol is only named here\n\
             pub struct S;\n\
             impl S { pub fn real_item(&self) { let _ = \"ghost_symbol\"; ghost_symbol(); } }\n",
        )
        .unwrap();

        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:gate-ghost a meta:Gate ;\n\
             meta:artifact \"crates/x/src/lib.rs\" ;\n\
             meta:symbol \"ghost_symbol\" .\n\
             meta:gate-real a meta:Gate ;\n\
             meta:artifact \"crates/x/src/lib.rs\" ;\n\
             meta:symbol \"real_item\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-ghost, meta:gate-real .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
        // The comment/string/call-site-only symbol is now rejected …
        assert!(text.contains("'ghost_symbol' not found"), "{text}");
        // … while the genuine impl-method definition resolves (no finding).
        assert!(!text.contains("'real_item' not found"), "{text}");
    }

    /// Build a temp repo with one workspace crate `foo` containing `real_test`
    /// (a `#[test]` calling `reached_fn`) and `lonely` (reached by nothing).
    fn write_binding_repo(tmp: &tempfile::TempDir, makefile: &str, manifest_ttl: &str) {
        fs::write(tmp.path().join("Makefile"), makefile).unwrap();
        let foo = tmp.path().join("crates/foo/src");
        fs::create_dir_all(&foo).unwrap();
        fs::write(
            tmp.path().join("crates/foo/Cargo.toml"),
            "[package]\nname = \"foo\"\nversion = \"0.0.0\"\n",
        )
        .unwrap();
        fs::write(
            foo.join("lib.rs"),
            "pub fn reached_fn() {}\n\
             pub fn lonely() {}\n\
             #[cfg(test)]\nmod t {\n    use super::*;\n    #[test]\n    fn real_test() { reached_fn(); }\n}\n",
        )
        .unwrap();
        let prefixes = "@prefix meta: <https://blackcatinformatics.ca/gmeow/meta#> .\n\
             @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n";
        fs::write(
            tmp.path().join("constitution.ttl"),
            format!("{prefixes}{manifest_ttl}"),
        )
        .unwrap();
        fs::write(
            tmp.path().join("CONSTITUTION.md"),
            "## 1. Be good\n\nprose\n",
        )
        .unwrap();
    }

    #[test]
    fn unbound_symbol_fires_when_target_cannot_run_it() {
        let tmp = tempfile::tempdir().unwrap();
        // `runit` runs the workspace test `real_test` (which reaches `reached_fn`
        // but NOT `lonely`). Gate `reach` cites a symbol on that call path;
        // gate `unreach` cites `lonely`, which nothing the target runs reaches.
        write_binding_repo(
            &tmp,
            "runit: ## workflow\n\tcargo nextest run -p foo\n",
            "meta:reach a meta:Gate ;\n\
             meta:artifact \"crates/foo/src/lib.rs\" ; meta:symbol \"reached_fn\" ; meta:makeTarget \"runit\" .\n\
             meta:unreach a meta:Gate ;\n\
             meta:artifact \"crates/foo/src/lib.rs\" ; meta:symbol \"lonely\" ; meta:makeTarget \"runit\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:reach, meta:unreach .\n",
        );
        let findings = constitution_full_report(
            &tmp.path().join("constitution.ttl"),
            &tmp.path().join("CONSTITUTION.md"),
            tmp.path(),
        );
        let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
        // `lonely` is cited but no cited target runs it → fires.
        assert!(
            text.contains("unreach: symbol 'lonely' has no static call path"),
            "{text}"
        );
        // `reached_fn` is on the test's call path → bound, no finding.
        assert!(!text.contains("reach: symbol 'reached_fn'"), "{text}");
    }

    #[test]
    fn xtask_check_binds_tests_run_by_its_declared_make_targets() {
        let tmp = tempfile::tempdir().unwrap();
        write_binding_repo(
            &tmp,
            "check: ## workflow\n\tcargo xtask check\nrust-gate:\n\tcargo nextest run -p foo\n",
            "meta:reach a meta:Gate ;\n\
             meta:artifact \"crates/foo/src/lib.rs\" ; meta:symbol \"reached_fn\" ; meta:makeTarget \"check\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:reach .\n",
        );
        let xtask = tmp.path().join("crates/xtask/src");
        fs::create_dir_all(&xtask).unwrap();
        fs::write(
            xtask.join("main.rs"),
            "const CHECK_DAG: &[Task] = &[Task { name: \"tests\", target: \"rust-gate\", dependencies: &[] }];\n",
        )
        .unwrap();

        let findings = constitution_full_report(
            &tmp.path().join("constitution.ttl"),
            &tmp.path().join("CONSTITUTION.md"),
            tmp.path(),
        );
        let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
        assert!(!text.contains("unbound-symbol"), "{text}");
        assert!(!text.contains("reach: symbol 'reached_fn'"), "{text}");
    }

    #[test]
    fn off_lane_target_fires_for_undocumented_unreachable_target() {
        let tmp = tempfile::tempdir().unwrap();
        // `orphan-target` is undocumented and reachable from no gate lane;
        // `runit` is a documented workflow, so it stays on-lane.
        write_binding_repo(
            &tmp,
            "orphan-target:\n\techo hi\nrunit: ## workflow\n\tcargo nextest run -p foo\n",
            "meta:dead a meta:Gate ; meta:makeTarget \"orphan-target\" .\n\
             meta:live a meta:Gate ; meta:makeTarget \"runit\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:dead, meta:live .\n",
        );
        let findings = constitution_full_report(
            &tmp.path().join("constitution.ttl"),
            &tmp.path().join("CONSTITUTION.md"),
            tmp.path(),
        );
        let text: String = findings.iter().map(|f| f.message.clone() + "\n").collect();
        assert!(
            text.contains(
                "dead: Makefile target 'orphan-target' exists but is reachable from no gate lane"
            ),
            "{text}"
        );
        assert!(!text.contains("live: Makefile target 'runit'"), "{text}");
    }

    #[test]
    fn orphaned_enforcement_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:gate-used a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:gate-orphan a meta:Lint ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-used .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(findings.iter().any(|f| {
            f.message.contains("orphaned enforcement") && f.message.contains("gate-orphan")
        }));
    }

    #[test]
    fn title_drift_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be excellent\" ;\n\
             meta:enforcedBy meta:gate-x .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(findings.iter().any(|f| f.message.contains("title drift")));
    }

    #[test]
    fn undeclared_enforcement_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (manifest, constitution) = write_pair(
            &tmp,
            "meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:nonexistent-gate .\n",
            "## 1. Be good\n\nprose\n",
        );
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(
            findings
                .iter()
                .any(|f| f.message.contains("undeclared enforcement"))
        );
    }

    #[test]
    fn supersession_matching_pair_passes() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x ; meta:supersededInPartBy meta:Principle1 .\n";
        let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\n**Superseded in part by Principle 1:** because reasons.\n";
        let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(
            !findings
                .iter()
                .any(|f| f.message.contains("supersededInPartBy drift"))
        );
    }

    #[test]
    fn supersession_markdown_only_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x .\n";
        let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\n**Superseded in part by Principle 1:** because reasons.\n";
        let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(findings.iter().any(|f| {
            f.message
                .contains("principle 2 meta:supersededInPartBy drift")
        }));
    }

    #[test]
    fn supersession_ttl_only_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x ; meta:supersededInPartBy meta:Principle1 .\n";
        let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\nno marker here.\n";
        let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(findings.iter().any(|f| {
            f.message
                .contains("principle 2 meta:supersededInPartBy drift")
        }));
    }

    #[test]
    fn extends_matching_pair_passes() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("Makefile"), "all:\n").unwrap();
        let manifest_ttl = "meta:gate-x a meta:Gate ; meta:artifact \"Makefile\" .\n\
             meta:Principle1 a meta:Principle ; meta:number 1 ; meta:title \"Be good\" ;\n\
             meta:enforcedBy meta:gate-x .\n\
             meta:Principle2 a meta:Principle ; meta:number 2 ; meta:title \"Be great\" ;\n\
             meta:enforcedBy meta:gate-x ; meta:extends meta:Principle1 .\n";
        let md = "## 1. Be good\n\nprose\n\n## 2. Be great\n\n**Extends Principle 1.**\n";
        let (manifest, constitution) = write_pair(&tmp, manifest_ttl, md);
        let findings = constitution_full_report(&manifest, &constitution, tmp.path());
        assert!(!findings.iter().any(|f| f.message.contains("extends drift")));
    }

    #[test]
    fn real_repo_constitution_passes() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap();
        let manifest = root.join("governance").join("constitution.ttl");
        let constitution = root.join("CONSTITUTION.md");
        let findings = constitution_full_report(&manifest, &constitution, root);
        let errors: Vec<_> = findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .map(|f| f.message.clone())
            .collect();
        assert!(errors.is_empty(), "{:#?}", errors);
    }
}
