// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `llms.txt`-family SHAPE freeze: the model-facing surface that is NOT a
//! `generated/` path.
//!
//! `dist/llms.txt` and `dist/llms-full.txt` are bundle BLOBS, so nothing named `llms*`
//! exists under `generated/` and neither can be compared byte-for-byte against a
//! merge-base checkout. What CAN be frozen — and what actually carries the model-facing
//! contract — is the SOURCE that determines their shape: the shared skeleton emitter,
//! the section list and its order, the signature/notation conventions, the primer
//! heading, and the MCP consumer-index resource list.
//!
//! # What is frozen and what may grow
//!
//! `.goals` (MAXIMAL INFORMATION FLOW, ONTOLOGICAL USE) forbids suppressing terms to
//! green a gate, so the freeze is deliberately asymmetric:
//!
//! * the SHAPE is frozen — skeleton, section headings, section ordering, notation
//!   conventions, and the resource-list structure must be byte-identical to the
//!   merge-base;
//! * the CONTENT may grow — term entries follow the ontology, and the resource list may
//!   gain a resource for each `gmeow:` surface THIS CHANGE declares and the merge base
//!   did not, one resource per surface.
//!
//! Anything reworded, reordered or removed reds. That asymmetry is the whole point: a
//! surface that could shrink to pass a gate would make the gate an incentive to hide
//! information.
//!
//! # The permitted delta is DERIVED, never named
//!
//! A rule of the form "the list may gain one resource whose URI contains `medium`" is not
//! a gate: its pass condition is a string literal spelling the exact change its author was
//! making, so it cannot refuse that change and wrongly refuses every other. The permitted
//! delta here is instead read out of the repo's OWN data — the `gmeow:` terms the slice
//! `module.ttl` files declare on this branch, minus the ones they already declared at the
//! merge base ([`DeclaredSurfaces`]). A gained resource is legitimate exactly when its URI
//! names one of those newly-declared surfaces, so the gate's answer moves with the
//! ontology instead of with whoever last edited the gate.
//!
//! # Where this lives
//!
//! A test-support module rather than a `crates/pipeline` library module: nothing in the
//! shipped pipeline calls any of it. Its ONE consumer is
//! `crates/pipeline/tests/model_facing_invariance.rs`, which `#[path]`-includes this file
//! exactly as the medium negative controls in `support/medium_tamper.rs` are included.
//!
//! Everything below except [`declared_surfaces`] is a PURE function over source text, so
//! each clause has a reachable red arm — the gate's fixtures perturb the working text and
//! require a refusal.

#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use gmeow_pipeline::branch_base::{BaseFile, git_show_base};
use gmeow_pipeline::gmn_dialect::ModelFacingReport;

/// One frozen source item: the file it lives in, and the item within that file (the
/// whole file when [`FrozenItem::item`] is [`ItemRef::WholeFile`]).
#[derive(Debug, Clone, Copy)]
pub struct FrozenItem {
    /// Repo-relative path, forward slashes.
    pub path: &'static str,
    /// Where the SAME item lived at the merge base, when a change moved it between files.
    ///
    /// A freeze is about an item's bytes, not its address, so an item that moves house has to
    /// be looked up at its old address in the base and its new one on the branch; without this
    /// the gate reads a missing file and grades nothing, which is the one outcome a freeze may
    /// never have. `None` means the item did not move.
    pub base_path: Option<&'static str>,
    /// Which item carried this freeze at the merge base, when a change renamed it or split it
    /// out of its old home. `None` means the item kept its name.
    pub base_item: Option<ItemRef>,
    /// Which item of that file is frozen.
    pub item: ItemRef,
    /// Why this item carries the model-facing shape.
    pub why: &'static str,
}

/// Which item of a source file a freeze clause addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemRef {
    /// The entire file.
    WholeFile,
    /// A free function, by name.
    Function(&'static str),
    /// A `const` item, by name.
    Const(&'static str),
}

impl FrozenItem {
    /// Where this item is looked up at the MERGE BASE: its old address when a change moved
    /// it between files, its current one otherwise.
    #[must_use]
    pub fn base_lookup_path(self) -> &'static str {
        match self.base_path {
            Some(path) => path,
            None => self.path,
        }
    }

    /// Which item this freeze is looked up as at the MERGE BASE: its old name when a change
    /// renamed or relocated it, its current one otherwise.
    #[must_use]
    pub fn base_lookup_item(self) -> ItemRef {
        match self.base_item {
            Some(item) => item,
            None => self.item,
        }
    }
}

impl ItemRef {
    /// A human label for a failure message.
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Self::WholeFile => "<whole file>".to_string(),
            Self::Function(name) => format!("fn {name}"),
            Self::Const(name) => format!("const {name}"),
        }
    }
}

/// Every source item whose bytes ARE the `llms.txt`-family shape.
///
/// The MCP `resources_result` is deliberately absent: its list may grow with the
/// vocabulary a change declares, so it is checked by [`check_resource_list`] against the
/// ontology-derived delta instead of frozen outright.
pub const FROZEN_LLMS_SHAPE: &[FrozenItem] = &[
    FrozenItem {
        path: "crates/docs-model/src/llms.rs",
        base_path: Some("crates/docs/src/llms.rs"),
        base_item: None,
        item: ItemRef::WholeFile,
        why: "the ONE llmstxt.org skeleton emitter — header, blockquote, bullet form, \
              note cap, token budgets, the standing Reference section and its page list. \
              Frozen whole because it carries no term content at all: every byte of it is \
              shape",
    },
    FrozenItem {
        path: "crates/bundle-view/src/export.rs",
        base_path: Some("crates/pipeline/src/stages/export.rs"),
        base_item: None,
        item: ItemRef::Function("llms_sections"),
        why: "the section HEADINGS (Classes / Properties / Individuals) and their order",
    },
    FrozenItem {
        path: "crates/bundle-view/src/export.rs",
        base_path: Some("crates/pipeline/src/stages/export.rs"),
        base_item: None,
        item: ItemRef::Function("llms_signature"),
        why: "the notation conventions — the `⊑` subclass and `→` domain/range spellings \
              a model reads off every bullet",
    },
    FrozenItem {
        path: "crates/bundle-view/src/export.rs",
        base_path: Some("crates/pipeline/src/stages/export.rs"),
        base_item: None,
        item: ItemRef::Function("llms_note"),
        why: "the bullet-note convention (definition, label fallback, the `[fallback: en]` \
              marker)",
    },
    FrozenItem {
        path: "crates/bundle-view/src/export.rs",
        base_path: Some("crates/pipeline/src/stages/export.rs"),
        base_item: None,
        item: ItemRef::Function("llms_prose"),
        why: "the shared prose line every index form carries under its header",
    },
    FrozenItem {
        path: "crates/bundle-view/src/export.rs",
        base_path: Some("crates/pipeline/src/stages/export.rs"),
        base_item: None,
        item: ItemRef::Function("write_llms_txt"),
        why: "the section ORDERING of the index form: term sections, then the standing \
              Reference section, then the GMN-1 primer section",
    },
    FrozenItem {
        path: "crates/docs-model/src/gmn1_primer.rs",
        base_path: Some("crates/docs/src/gmn1_primer.rs"),
        base_item: None,
        item: ItemRef::Const("PRIMER_HEADING"),
        why: "the primer's section heading — the anchor every surface's primer section is \
              found by",
    },
];

/// The MCP consumer-index item whose LIST may grow with the vocabulary a change declares.
pub const MCP_RESOURCE_LIST: FrozenItem = FrozenItem {
    path: "crates/mcp/src/lib.rs",
    base_path: Some("crates/pipeline/src/mcp.rs"),
    base_item: Some(ItemRef::Function("resources_result")),
    item: ItemRef::Function("builtin_resource_descriptors"),
    why: "the MCP consumer-index resource list: its structure is frozen, and its entries \
          may grow only to surface `gmeow:` vocabulary the change itself declares",
};

// ── Source-item extraction ───────────────────────────────────────────────────

/// The source text of one item of `text`, or `None` when the item is absent.
///
/// A hand-rolled brace/semicolon scanner rather than a parse: the gate needs a byte-exact
/// span of ONE named item out of a file that may be thousands of lines, and pulling a
/// syntax-tree dependency in for that would put a second (and looser) notion of "the same
/// item" beside the bytes the freeze is actually about.
#[must_use]
pub fn extract_item(text: &str, item: ItemRef) -> Option<String> {
    match item {
        ItemRef::WholeFile => Some(text.to_string()),
        ItemRef::Function(name) => extract_braced(text, &[format!("fn {name}(")]),
        ItemRef::Const(name) => extract_terminated(text, &format!("const {name}:")),
    }
}

/// Whether `bytes[index]` starts a token rather than continuing an identifier.
fn is_token_start(text: &str, index: usize) -> bool {
    text[..index]
        .chars()
        .next_back()
        .is_none_or(|c| !c.is_alphanumeric() && c != '_' && c != ':')
}

/// The span from the start of the line carrying the first occurrence of any `needle`
/// through the matching close brace of the item's body.
fn extract_braced(text: &str, needles: &[String]) -> Option<String> {
    let start = needles
        .iter()
        .filter_map(|needle| {
            text.match_indices(needle.as_str())
                .find(|(index, _)| is_token_start(text, *index))
                .map(|(index, _)| index)
        })
        .min()?;
    let line_start = text[..start].rfind('\n').map_or(0, |index| index + 1);
    let open = scan_to_open_brace(text, start)?;
    let close = match_brace(text, open)?;
    Some(text[line_start..=close].to_string())
}

/// The span from the start of the line carrying `needle` through the terminating `;`.
fn extract_terminated(text: &str, needle: &str) -> Option<String> {
    let start = text
        .match_indices(needle)
        .find(|(index, _)| is_token_start(text, *index))
        .map(|(index, _)| index)?;
    let line_start = text[..start].rfind('\n').map_or(0, |index| index + 1);
    let mut scanner = Scanner::new(text, start);
    while let Some((index, ch)) = scanner.next_code_char() {
        if ch == ';' {
            return Some(text[line_start..=index].to_string());
        }
    }
    None
}

/// The byte index of the first code `{` at or after `from`.
fn scan_to_open_brace(text: &str, from: usize) -> Option<usize> {
    let mut scanner = Scanner::new(text, from);
    while let Some((index, ch)) = scanner.next_code_char() {
        if ch == '{' {
            return Some(index);
        }
    }
    None
}

/// The byte index of the `}` matching the `{` at `open`.
fn match_brace(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut scanner = Scanner::new(text, open);
    while let Some((index, ch)) = scanner.next_code_char() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// A byte index of the `)` matching the `(` at `open`.
fn match_paren(text: &str, open: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut scanner = Scanner::new(text, open);
    while let Some((index, ch)) = scanner.next_code_char() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

/// A forward scanner over Rust source that yields only CODE characters — string, char
/// and comment bodies are skipped, so a `{` inside a doc comment or a `"…}…"` literal
/// cannot unbalance a span. Indices are absolute into the original text.
struct Scanner<'a> {
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
}

impl<'a> Scanner<'a> {
    /// A scanner positioned at byte `from` of `text`.
    fn new(text: &'a str, from: usize) -> Self {
        let mut chars = text.char_indices().peekable();
        while chars.peek().is_some_and(|(index, _)| *index < from) {
            chars.next();
        }
        Self { chars }
    }

    fn next_code_char(&mut self) -> Option<(usize, char)> {
        loop {
            let (index, ch) = self.chars.next()?;
            match ch {
                '"' => self.skip_string(),
                '\'' => self.skip_char_literal(),
                '/' => match self.chars.peek().map(|(_, c)| *c) {
                    Some('/') => self.skip_line_comment(),
                    Some('*') => self.skip_block_comment(),
                    _ => return Some((index, ch)),
                },
                _ => return Some((index, ch)),
            }
        }
    }

    fn skip_string(&mut self) {
        while let Some((_, ch)) = self.chars.next() {
            match ch {
                '\\' => {
                    self.chars.next();
                }
                '"' => return,
                _ => {}
            }
        }
    }

    /// A `'` in Rust opens a char literal OR a lifetime. A lifetime carries no closing
    /// quote, so the scan gives up at the first newline or after more characters than a
    /// char literal can hold rather than swallowing the rest of the file.
    fn skip_char_literal(&mut self) {
        let mut consumed = 0usize;
        while let Some((_, ch)) = self.chars.peek().copied() {
            if ch == '\\' {
                self.chars.next();
                self.chars.next();
                consumed += 2;
                continue;
            }
            if ch == '\'' {
                self.chars.next();
                return;
            }
            if consumed > 8 || ch == '\n' {
                return;
            }
            self.chars.next();
            consumed += 1;
        }
    }

    fn skip_line_comment(&mut self) {
        for (_, ch) in self.chars.by_ref() {
            if ch == '\n' {
                return;
            }
        }
    }

    fn skip_block_comment(&mut self) {
        self.chars.next();
        let mut previous = ' ';
        for (_, ch) in self.chars.by_ref() {
            if previous == '*' && ch == '/' {
                return;
            }
            previous = ch;
        }
    }
}

// ── Freeze comparison ────────────────────────────────────────────────────────

/// Compare one frozen item's base and working text, recording a problem when the item is
/// absent from either side or its bytes moved.
/// `text` with CRATE ADDRESSES collapsed — the rustdoc `crate::…` link target inside a doc
/// comment, and the `gmeow_…::` crate segment of a fully-qualified path in code.
///
/// The freeze is over the emitter's SHAPE: the skeleton, the section headings and their order,
/// the bullet form, the note cap, the token budgets. WHICH crate a type or link resolves
/// through is not shape — it is an address, and moving an emitter between crates forces the
/// address to change (`gmeow_docs::llms::LlmsSection` becomes `gmeow_docs_model::llms::LlmsSection`
/// when the emitter moves to break a dependency chain, and a `crate::…` link must be re-pointed
/// or rustdoc cannot resolve it). Collapsing addresses on BOTH sides is the same normalization
/// the resource entries already apply for a rustfmt re-wrap.
///
/// Everything else stays byte-exact: a reworded heading, a reordered section, a changed cap or
/// budget still reds, because none of those is an address.
fn collapse_gmeow_crate_segments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find("gmeow_") {
        let starts_segment = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let ident_end = tail
            .find(|c: char| !(c.is_alphanumeric() || c == '_'))
            .unwrap_or(tail.len());
        if starts_segment && tail[ident_end..].starts_with("::") {
            out.push_str("<crate>");
            rest = &tail[ident_end..];
        } else {
            out.push_str(&tail[..ident_end]);
            rest = &tail[ident_end..];
        }
    }
    out.push_str(rest);
    out
}

fn shape_text(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    // `crate::<segments>` — the rustdoc intra-doc link form.
    while let Some(at) = rest.find("crate::") {
        // Only when `crate` starts a path segment, so `gmeow_docs_model::llms` is left to the
        // crate-segment rule below rather than being half-eaten here.
        let starts_segment = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        out.push_str(&rest[..at]);
        if !starts_segment {
            out.push_str("crate::");
            rest = &rest[at + "crate::".len()..];
            continue;
        }
        out.push_str("<crate>::");
        rest = &rest[at + "crate::".len()..];
        let end = rest
            .find(|c: char| !(c.is_alphanumeric() || c == '_' || c == ':'))
            .unwrap_or(rest.len());
        rest = &rest[end..];
    }
    out.push_str(rest);
    // `gmeow_<ident>::` — the crate segment of a fully-qualified path.
    collapse_gmeow_crate_segments(&out)
}

pub fn check_frozen_item(
    item: &FrozenItem,
    base_text: &str,
    work_text: &str,
    report: &mut ModelFacingReport,
) {
    let label = format!("{} :: {}", item.path, item.item.label());
    let Some(base) = extract_item(base_text, item.base_lookup_item()) else {
        report.problem(format!(
            "{label}: absent at the merge base — the freeze has no comparand"
        ));
        return;
    };
    let Some(work) = extract_item(work_text, item.item) else {
        report.problem(format!(
            "{label}: REMOVED on this branch. {}. The llms shape is frozen: removing a \
             surface is a model-facing change, not a simplification",
            item.why
        ));
        return;
    };
    if shape_text(&base) == shape_text(&work) {
        return;
    }
    report.problem(format!(
        "{label}: the llms-family SHAPE moved. {}. Section headers, section ordering and \
         notation conventions are frozen byte-for-byte against the merge base for this \
         change — term ENTRIES may grow, the shape may not. Reworded, reordered or removed \
         all count.\n--- base ---\n{base}\n--- working ---\n{work}",
        item.why
    ));
}

// ── The MCP resource list ────────────────────────────────────────────────────

/// One `resource(uri, name, description, mime)` entry of the MCP consumer index, with
/// whitespace normalized so a rustfmt re-wrap is not read as a content change.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ResourceEntry {
    /// The `gmeow://…` URI (the entry's identity).
    pub uri: String,
    /// The whole call's normalized argument text (the entry's content).
    pub normalized: String,
}

/// Every `resource(...)` entry of `body`, in source order.
#[must_use]
pub fn resource_entries(body: &str) -> Vec<ResourceEntry> {
    let mut out: Vec<ResourceEntry> = Vec::new();
    let mut from = 0usize;
    while let Some(offset) = body[from..].find("resource(") {
        let start = from + offset;
        if !is_token_start(body, start) {
            from = start + "resource(".len();
            continue;
        }
        let open = start + "resource".len();
        let Some(close) = match_paren(body, open) else {
            break;
        };
        let inner = &body[open + 1..close];
        let normalized = collapse(inner);
        let uri = first_string_literal(inner).unwrap_or_default();
        out.push(ResourceEntry { uri, normalized });
        from = close + 1;
    }
    out
}

/// `body` with every `resource(...)` call elided and all whitespace collapsed — the
/// CONTROL FLOW around the list (the `vec![`, the dev-tools mode guard, the JSON
/// envelope), which is frozen even though the list itself may grow.
///
/// Private: the structure clause is [`check_resource_list`]'s, not a caller's — the gate
/// asks whether the control flow moved, never for the skeleton itself.
#[must_use]
fn resource_skeleton(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut from = 0usize;
    while let Some(offset) = body[from..].find("resource(") {
        let start = from + offset;
        if !is_token_start(body, start) {
            out.push_str(&body[from..start + "resource(".len()]);
            from = start + "resource(".len();
            continue;
        }
        let open = start + "resource".len();
        let Some(close) = match_paren(body, open) else {
            break;
        };
        out.push_str(&body[from..start]);
        from = close + 1;
    }
    out.push_str(&body[from..]);
    // Commas go BEFORE the whitespace collapse: each elided entry leaves its own `,` and
    // its own newline behind, so collapsing first would make the skeleton a function of
    // how MANY entries the list has — exactly the thing the list is allowed to change.
    collapse(&out.replace(',', ""))
}

/// All whitespace runs collapsed to one space, trimmed.
fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The content of the first `"…"` literal in `text` (concatenating adjacent
/// continuation literals is unnecessary — a resource URI is one literal).
fn first_string_literal(text: &str) -> Option<String> {
    let open = text.find('"')?;
    let mut out = String::new();
    let mut escaped = false;
    for ch in text[open + 1..].chars() {
        if escaped {
            out.push(ch);
            escaped = false;
            continue;
        }
        match ch {
            '\\' => escaped = true,
            '"' => return Some(out),
            _ => out.push(ch),
        }
    }
    None
}

/// The ONTOLOGY-DERIVED delta the MCP consumer-index resource list is allowed.
///
/// The working list must be the base list, or the base list plus entries that each
/// surface a distinct `gmeow:` term this change declares and the merge base did not
/// ([`DeclaredSurfaces`]). Anything reworded, reordered or removed reds, and so does an
/// addition with no newly-declared surface behind it.
///
/// The bound on HOW MANY entries may appear is therefore the ontology's, not a number
/// written here: the list can grow by at most one entry per surface the change adds, and
/// by none at all when it adds no vocabulary.
///
/// Records a problem when the surrounding control flow moved, an existing entry moved or
/// changed, an added entry names no newly-declared surface, or two added entries claim the
/// same one.
pub fn check_resource_list(
    base_body: &str,
    work_body: &str,
    surfaces: &DeclaredSurfaces,
    report: &mut ModelFacingReport,
) {
    let base_skeleton = resource_skeleton(base_body);
    let work_skeleton = resource_skeleton(work_body);
    if base_skeleton != work_skeleton {
        report.problem(format!(
            "the MCP consumer-index resource-list STRUCTURE moved (the control flow around \
             the entries, not the entries themselves).\n--- base ---\n{base_skeleton}\n--- \
             working ---\n{work_skeleton}"
        ));
    }

    let base = resource_entries(base_body);
    let work = resource_entries(work_body);
    let base_uris: Vec<&str> = base.iter().map(|entry| entry.uri.as_str()).collect();
    let work_uris: Vec<&str> = work.iter().map(|entry| entry.uri.as_str()).collect();

    let added: Vec<&ResourceEntry> = work
        .iter()
        .filter(|entry| !base_uris.contains(&entry.uri.as_str()))
        .collect();
    let removed: BTreeSet<&str> = base_uris
        .iter()
        .copied()
        .filter(|uri| !work_uris.contains(uri))
        .collect();

    if !removed.is_empty() {
        report.problem(format!(
            "the MCP consumer index DROPPED resource(s) {removed:?} — .goals forbids \
             suppressing a surface to green a gate, so a removal is always a model-facing \
             regression"
        ));
    }

    // The PERMITTED DELTA, derived from the repo's own data rather than named here. Each
    // added entry must surface a `gmeow:` term this change declares and the merge base did
    // not, and no two entries may claim the same one — so the list grows by at most one
    // resource per surface the ontology gained, and not at all when it gained none.
    if !added.is_empty() && surfaces.working.is_empty() {
        report.problem(
            "the declared-surface derivation read ZERO gmeow: terms out of the slice modules, \
             so the permitted delta would be decided by looking at nothing — the resource-list \
             delta cannot be graded"
                .to_owned(),
        );
    } else {
        let mut claimed: BTreeMap<&str, &str> = BTreeMap::new();
        for entry in &added {
            match surfaces.resolve(&entry.uri) {
                SurfaceMatch::Undeclared => report.problem(format!(
                    "the MCP consumer index gained resource {:?}, which names NO gmeow: term \
                     any slice module declares. The permitted delta is DERIVED from the \
                     ontology — a resource may appear only to surface vocabulary the change \
                     itself declares — so an addition with nothing declared behind it is a \
                     bare model-facing change",
                    entry.uri
                )),
                SurfaceMatch::Preexisting(local) => report.problem(format!(
                    "the MCP consumer index gained resource {:?}, which surfaces gmeow:{local} \
                     — a term the merge base ALREADY declared. The permitted delta is the \
                     vocabulary THIS change adds; exposing long-standing vocabulary through a \
                     new consumer resource is a model-facing change on its own",
                    entry.uri
                )),
                SurfaceMatch::New(local) => {
                    if let Some(prior) = claimed.insert(local, entry.uri.as_str()) {
                        report.problem(format!(
                            "the MCP consumer index gained BOTH {prior:?} and {:?} for the one \
                             newly-declared surface gmeow:{local} — the delta is one resource \
                             per surface the change adds, so a second entry claiming the same \
                             surface has no declaration behind it",
                            entry.uri
                        ));
                    }
                }
            }
        }
    }

    // Order and content of everything that already existed: the shared prefix must be
    // unchanged entry-for-entry, so a reorder or a reworded description reds even though
    // the URI set is unchanged.
    let carried_base: Vec<&ResourceEntry> = base.iter().collect();
    let carried_work: Vec<&ResourceEntry> = work
        .iter()
        .filter(|entry| base_uris.contains(&entry.uri.as_str()))
        .collect();
    if carried_base.len() != carried_work.len() {
        report.problem(format!(
            "the MCP consumer index carries {} of its {} base resources — a duplicated or \
             lost entry",
            carried_work.len(),
            carried_base.len()
        ));
        return;
    }
    for (base_entry, work_entry) in carried_base.iter().zip(carried_work.iter()) {
        if base_entry != work_entry {
            report.problem(format!(
                "the MCP consumer index resource list was reordered or reworded at \
                 {:?}:\n--- base ---\n{}\n--- working ---\n{}",
                base_entry.uri, base_entry.normalized, work_entry.normalized
            ));
        }
    }
}

// ── The declared surface, read out of the ontology ───────────────────────────

/// The `gmeow:` vocabulary this branch DECLARES, and the same set at the merge base.
///
/// This is the gate's comparand for "what surface did this change actually add?" — the
/// repo's own slice modules, read the same way the vocabulary-ownership gate
/// (`crates/docs/tests/vocabulary_ownership.rs`) reads them: every `gmeow:` subject
/// carrying an `rdf:type` assertion in a `slices/**/module.ttl` or in the root ontology is
/// a declared term. Built by [`declared_surfaces`]; consumed by [`check_resource_list`].
#[derive(Debug, Clone, Default)]
pub struct DeclaredSurfaces {
    /// Local names of every `gmeow:` term declared on this branch.
    pub working: BTreeSet<String>,
    /// Local names of every `gmeow:` term declared at the merge base.
    pub base: BTreeSet<String>,
}

/// What surface a consumer-resource URI names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceMatch<'a> {
    /// A `gmeow:` term this change declares and the merge base did not.
    New(&'a str),
    /// A `gmeow:` term the merge base already declared.
    Preexisting(&'a str),
    /// No declared `gmeow:` term at all.
    Undeclared,
}

impl DeclaredSurfaces {
    /// Every local name this change declares that the merge base did not.
    #[must_use]
    pub fn newly_declared(&self) -> BTreeSet<&str> {
        self.working
            .iter()
            .map(String::as_str)
            .filter(|local| !self.base.contains(*local))
            .collect()
    }

    /// The declared surface a `gmeow://…/<segment>` resource URI names.
    ///
    /// The correspondence is the resource's terminal path segment against a term's local
    /// name, compared on their letters and digits alone: a URI segment is kebab-cased and
    /// lowercase (`medium`, `medium-effect`) where a term is `UpperCamel` (`Medium`), and
    /// neither spelling is a fact about the other, so matching on the shared skeleton is
    /// the only correspondence that does not smuggle a naming convention into the gate.
    #[must_use]
    pub fn resolve<'a>(&'a self, uri: &str) -> SurfaceMatch<'a> {
        let segment = uri.rsplit('/').next().unwrap_or(uri);
        let wanted = skeleton(segment);
        if wanted.is_empty() {
            return SurfaceMatch::Undeclared;
        }
        // The base set is searched too, so an addition that surfaces LONG-STANDING
        // vocabulary is reported as exactly that rather than as "undeclared" — two
        // different defects with two different fixes.
        let found = self
            .working
            .iter()
            .chain(self.base.iter())
            .find(|local| skeleton(local) == wanted);
        match found {
            None => SurfaceMatch::Undeclared,
            Some(local) if self.base.contains(local) => SurfaceMatch::Preexisting(local),
            Some(local) => SurfaceMatch::New(local),
        }
    }
}

/// A name reduced to its ASCII alphanumerics, lowercased — the shape `medium`,
/// `Medium` and `medium-` all share, and that `medium-effect` and `MediumEffect` share
/// with each other but not with `Medium`.
fn skeleton(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

/// Whether a repo-relative path is a `gmeow:` DECLARATION surface — a slice `module.ttl`
/// or the root ontology.
fn is_declaration_file(rel: &str) -> bool {
    rel == ROOT_ONTOLOGY
        || (rel.starts_with("slices/") && rel.rsplit('/').next() == Some("module.ttl"))
}

/// The root ontology document, which declares vocabulary no slice module does.
const ROOT_ONTOLOGY: &str = "ontology/gmeow.ttl";

/// Read the declared `gmeow:` surface off BOTH sides of the comparison: the working tree,
/// and the merge base via `git show`.
///
/// The base side is enumerated from the base tree itself rather than from the working
/// file list, so a module this change DELETES still contributes its base declarations —
/// otherwise redeclaring a deleted module's term elsewhere would read as new vocabulary.
///
/// # Panics
/// When git cannot list the base tree, when a base declaration file cannot be read for any
/// reason other than being absent, or when a working declaration file is unreadable. Each
/// is a comparand the gate is defined to have and does not, never a reason to pass.
#[must_use]
pub fn declared_surfaces(root: &Path, base: &str) -> DeclaredSurfaces {
    let mut out = DeclaredSurfaces::default();

    for rel in working_declaration_files(root) {
        let text = std::fs::read_to_string(root.join(&rel))
            .unwrap_or_else(|err| panic!("{rel}: declaration surface is unreadable: {err}"));
        collect_declared(&rel, &text, &mut out.working);
    }

    let listed = match gmeow_pipeline::branch_base::git_ls_tree(root, base, &["slices", "ontology"])
    {
        gmeow_pipeline::branch_base::BaseTree::Paths(paths) => paths,
        gmeow_pipeline::branch_base::BaseTree::Error(why) => panic!(
            "the resource-list delta is DERIVED from the ontology at the merge base, so a base \
             tree that cannot be listed is unfinished work rather than a pass: {why}"
        ),
    };
    for rel in listed.into_iter().filter(|rel| is_declaration_file(rel)) {
        match git_show_base(root, base, &rel) {
            BaseFile::Contents(text) => collect_declared(&rel, &text, &mut out.base),
            // `git ls-tree` just reported it, so absence here is a race, not a fact.
            BaseFile::Absent => panic!("{rel}: listed at {base} but `git show` reports it absent"),
            BaseFile::Error(why) => panic!("{rel}: unreadable at the merge base: {why}"),
        }
    }
    out
}

/// Every `slices/**/module.ttl` plus the root ontology in the working tree, repo-relative
/// with forward slashes, sorted.
fn working_declaration_files(root: &Path) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if root.join(ROOT_ONTOLOGY).is_file() {
        out.push(ROOT_ONTOLOGY.to_owned());
    }
    let mut stack = vec![root.join("slices")];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_symlink() {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.file_name().is_some_and(|name| name == "module.ttl") {
                out.push(
                    path.strip_prefix(root)
                        .unwrap_or(&path)
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out.sort();
    out
}

/// Fold one Turtle document's `gmeow:` TBox declarations into `out`.
///
/// A declaration is a `gmeow:` subject carrying an `rdf:type` assertion whose local part
/// has no further `/` — the same reading `crates/docs/tests/vocabulary_ownership.rs` uses,
/// which keeps slice IRIs and named-graph IRIs out of the vocabulary set.
///
/// # Panics
/// On a document that does not parse. A declaration surface that cannot be read is a
/// comparand the gate does not have; skipping it would silently shrink the derived delta.
fn collect_declared(rel: &str, text: &str, out: &mut BTreeSet<String>) {
    use purrdf::slice::rdf_query::{Dataset, GraphSel, Subject};

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

    let dataset = Dataset::parse_turtle(text.as_bytes(), rel)
        .unwrap_or_else(|err| panic!("{rel}: declaration surface does not parse: {err}"));
    dataset.graph(GraphSel::Any).for_each_quad(|s, p, _o, _g| {
        if p != RDF_TYPE {
            return;
        }
        if let Subject::Named(iri) = &s
            && let Some(local) = iri
                .strip_prefix(gmeow_ns::GMEOW_NS)
                .filter(|local| !local.contains('/'))
        {
            out.insert(local.to_owned());
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"
/// A doc comment with a stray { brace and a "quote.
pub fn llms_sections(terms: &[Term]) -> Vec<Section> {
    [section("Classes"), section("Properties")].into_iter().collect()
}

pub const PRIMER_HEADING: &str = "GMN-1 emission primer";

fn resources_result(&self) -> Value {
    let mut resources = vec![
        resource("gmeow://ontology/llms.txt", "llms.txt", "Standard index.", "text/plain"),
        resource("gmeow://ontology/okf-index", "okf-index", "OKF manifest.", "application/json"),
    ];
    if self.mode.includes_dev_tools() {
        resources.push(resource("gmeow://ontology/constitution", "constitution", "The Constitution.", "text/markdown"));
    }
    json!({ "resources": resources })
}
"#;

    #[test]
    fn a_function_span_is_extracted_whole_and_brace_safe() {
        let extracted =
            extract_item(FIXTURE, ItemRef::Function("llms_sections")).expect("the fn is found");
        assert!(
            extracted.starts_with("pub fn llms_sections("),
            "{extracted}"
        );
        assert!(extracted.ends_with('}'), "{extracted}");
        assert!(extracted.contains("section(\"Properties\")"), "{extracted}");
        // The doc comment above (with its unbalanced `{` and its stray quote) is NOT part
        // of the span, and did not unbalance it either.
        assert!(!extracted.contains("stray"), "{extracted}");
    }

    #[test]
    fn a_const_span_stops_at_its_terminator() {
        let extracted =
            extract_item(FIXTURE, ItemRef::Const("PRIMER_HEADING")).expect("the const is found");
        assert_eq!(
            extracted,
            "pub const PRIMER_HEADING: &str = \"GMN-1 emission primer\";"
        );
    }

    #[test]
    fn a_missing_item_is_an_absence_rather_than_a_panic() {
        assert!(extract_item(FIXTURE, ItemRef::Function("not_here")).is_none());
        assert!(extract_item(FIXTURE, ItemRef::Const("NOT_HERE")).is_none());
    }

    #[test]
    fn an_unchanged_item_passes_the_freeze() {
        let item = FrozenItem {
            path: "fixture.rs",
            base_path: None,
            base_item: None,
            item: ItemRef::Function("llms_sections"),
            why: "fixture",
        };
        let report = run(|r| check_frozen_item(&item, FIXTURE, FIXTURE, r));
        assert!(
            report.is_clean(),
            "identical text is frozen-clean: {report}"
        );
    }

    /// The acceptance criterion's own red fixture: reorder a section header.
    #[test]
    fn reordering_a_section_header_reds_the_freeze() {
        let item = FrozenItem {
            path: "fixture.rs",
            base_path: None,
            base_item: None,
            item: ItemRef::Function("llms_sections"),
            why: "fixture",
        };
        let reordered = FIXTURE.replace(
            "[section(\"Classes\"), section(\"Properties\")]",
            "[section(\"Properties\"), section(\"Classes\")]",
        );
        let report = run(|r| check_frozen_item(&item, FIXTURE, &reordered, r));
        assert!(!report.is_clean(), "a reordered section list must red");
        assert!(report.to_string().contains("SHAPE moved"), "{report}");
    }

    #[test]
    fn removing_a_frozen_item_reds_the_freeze() {
        let item = FrozenItem {
            path: "fixture.rs",
            base_path: None,
            base_item: None,
            item: ItemRef::Const("PRIMER_HEADING"),
            why: "fixture",
        };
        let removed = FIXTURE.replace("pub const PRIMER_HEADING", "pub const OTHER_HEADING");
        let report = run(|r| check_frozen_item(&item, FIXTURE, &removed, r));
        assert!(!report.is_clean(), "a removed item must red");
        assert!(report.to_string().contains("REMOVED"), "{report}");
    }

    /// Run one check and return its report.
    fn run(check: impl FnOnce(&mut ModelFacingReport)) -> ModelFacingReport {
        let mut report = ModelFacingReport::default();
        check(&mut report);
        report
    }

    fn resources_body(text: &str) -> String {
        extract_item(text, ItemRef::Function("resources_result")).expect("the fn is found")
    }

    #[test]
    fn the_resource_list_reads_back_in_source_order() {
        let entries = resource_entries(&resources_body(FIXTURE));
        assert_eq!(
            entries.iter().map(|e| e.uri.as_str()).collect::<Vec<_>>(),
            vec![
                "gmeow://ontology/llms.txt",
                "gmeow://ontology/okf-index",
                "gmeow://ontology/constitution",
            ]
        );
    }

    /// A synthetic declared surface: this change declares `Medium` and `MediumEnvelope`,
    /// while `Finding` predates it. Synthetic on purpose — these clauses are pure
    /// functions of the two lists and the declared set, so pinning them against the live
    /// repo would make them a report on today's ontology instead of on the rule.
    fn surfaces() -> DeclaredSurfaces {
        DeclaredSurfaces {
            working: ["Medium", "MediumEnvelope", "Finding"]
                .into_iter()
                .map(str::to_owned)
                .collect(),
            base: ["Finding"].into_iter().map(str::to_owned).collect(),
        }
    }

    /// One `resource(...)` entry named `slug`, spliced in front of the okf-index entry.
    fn grown_with(slugs: &[&str]) -> String {
        let mut spliced = String::new();
        for slug in slugs {
            spliced.push_str(&format!(
                "        resource(\"gmeow://ontology/{slug}\", \"{slug}\", \"A fixture.\", \
                 \"application/json\"),\n"
            ));
        }
        spliced.push_str(r#"        resource("gmeow://ontology/okf-index","#);
        let grown = FIXTURE.replace(
            r#"        resource("gmeow://ontology/okf-index","#,
            &spliced,
        );
        assert_ne!(grown, FIXTURE, "the fixture must actually perturb the list");
        resources_body(&grown)
    }

    #[test]
    fn an_unchanged_resource_list_passes() {
        let body = resources_body(FIXTURE);
        let report = run(|r| check_resource_list(&body, &body, &surfaces(), r));
        assert!(report.is_clean(), "an unchanged list passes: {report}");
    }

    #[test]
    fn a_resource_for_a_newly_declared_surface_is_the_permitted_delta() {
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &grown_with(&["medium"]),
                &surfaces(),
                r,
            )
        });
        assert!(
            report.is_clean(),
            "a resource surfacing the newly-declared gmeow:Medium is licensed by the \
             ontology: {report}"
        );
    }

    /// The bound on how many entries may appear is the ONTOLOGY's, not a number in the
    /// gate: two additions pass when the change declares two surfaces for them.
    #[test]
    fn two_resources_for_two_newly_declared_surfaces_both_pass() {
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &grown_with(&["medium", "medium-envelope"]),
                &surfaces(),
                r,
            )
        });
        assert!(
            report.is_clean(),
            "gmeow:Medium and gmeow:MediumEnvelope are both newly declared, so both \
             resources are licensed: {report}"
        );
    }

    /// The acceptance criterion's own red fixture, re-aimed at the derived rule: a SECOND
    /// resource claiming a surface the first already accounts for.
    #[test]
    fn a_second_resource_for_the_same_surface_reds() {
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &grown_with(&["medium", "me-dium"]),
                &surfaces(),
                r,
            )
        });
        assert!(
            !report.is_clean(),
            "two resources for one newly-declared surface must red"
        );
        assert!(
            report.to_string().contains("one resource per surface"),
            "{report}"
        );
    }

    /// The arm the retired `uri.contains("medium")` rule could not have: a name the
    /// ontology says nothing about.
    #[test]
    fn a_resource_with_no_declared_surface_reds() {
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &grown_with(&["changelog"]),
                &surfaces(),
                r,
            )
        });
        assert!(!report.is_clean(), "an undeclared addition must red");
        assert!(
            report.to_string().contains("names NO gmeow: term"),
            "{report}"
        );
    }

    /// A resource surfacing LONG-STANDING vocabulary: declared, but not by this change,
    /// so nothing licenses the addition.
    #[test]
    fn a_resource_for_preexisting_vocabulary_reds() {
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &grown_with(&["finding"]),
                &surfaces(),
                r,
            )
        });
        assert!(
            !report.is_clean(),
            "surfacing vocabulary the base already declared must red"
        );
        assert!(report.to_string().contains("ALREADY declared"), "{report}");
    }

    /// The derivation itself must not be able to license by looking at nothing: with an
    /// empty declared set, an addition reds as an ungradeable delta rather than passing.
    #[test]
    fn an_empty_declared_set_reds_rather_than_licensing() {
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &grown_with(&["medium"]),
                &DeclaredSurfaces::default(),
                r,
            )
        });
        assert!(!report.is_clean(), "a vacuous derivation must red");
        assert!(report.to_string().contains("ZERO gmeow: terms"), "{report}");
    }

    /// The correspondence is on letters and digits alone, in BOTH directions: a kebab-case
    /// URI segment resolves the `UpperCamel` term it names, and a different term does not.
    #[test]
    fn a_resource_slug_resolves_the_term_it_names() {
        let surfaces = surfaces();
        assert_eq!(
            surfaces.resolve("gmeow://ontology/medium-envelope"),
            SurfaceMatch::New("MediumEnvelope")
        );
        assert_eq!(
            surfaces.resolve("gmeow://ontology/finding"),
            SurfaceMatch::Preexisting("Finding")
        );
        assert_eq!(
            surfaces.resolve("gmeow://ontology/medium-effect"),
            SurfaceMatch::Undeclared,
            "a near-miss must not resolve — `Medium` is a different term from `MediumEffect`"
        );
        assert_eq!(
            surfaces.newly_declared(),
            ["Medium", "MediumEnvelope"].into_iter().collect()
        );
    }

    #[test]
    fn a_reordered_resource_list_reds() {
        let reordered = FIXTURE
            .replace(
                r#"        resource("gmeow://ontology/llms.txt", "llms.txt", "Standard index.", "text/plain"),
        resource("gmeow://ontology/okf-index", "okf-index", "OKF manifest.", "application/json"),"#,
                r#"        resource("gmeow://ontology/okf-index", "okf-index", "OKF manifest.", "application/json"),
        resource("gmeow://ontology/llms.txt", "llms.txt", "Standard index.", "text/plain"),"#,
            );
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &resources_body(&reordered),
                &surfaces(),
                r,
            )
        });
        assert!(!report.is_clean(), "a reordered list must red");
        assert!(
            report.to_string().contains("reordered or reworded"),
            "{report}"
        );
    }

    #[test]
    fn a_reworded_resource_description_reds() {
        let reworded = FIXTURE.replace("OKF manifest.", "OKF manifest envelope.");
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &resources_body(&reworded),
                &surfaces(),
                r,
            )
        });
        assert!(!report.is_clean(), "a reworded description must red");
        assert!(
            report.to_string().contains("reordered or reworded"),
            "{report}"
        );
    }

    #[test]
    fn a_dropped_resource_reds() {
        let dropped = FIXTURE.replace(
            r#"        resource("gmeow://ontology/okf-index", "okf-index", "OKF manifest.", "application/json"),
"#,
            "",
        );
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &resources_body(&dropped),
                &surfaces(),
                r,
            )
        });
        assert!(!report.is_clean(), "a dropped resource must red");
        assert!(report.to_string().contains("DROPPED"), "{report}");
    }

    /// The list may grow, but the control flow AROUND it may not: a mode guard that
    /// changed would move which consumers see which resources.
    #[test]
    fn a_changed_mode_guard_reds_the_structure() {
        let changed = FIXTURE.replace(
            "if self.mode.includes_dev_tools() {",
            "if self.mode.includes_consumer_tools() {",
        );
        let report = run(|r| {
            check_resource_list(
                &resources_body(FIXTURE),
                &resources_body(&changed),
                &surfaces(),
                r,
            )
        });
        assert!(!report.is_clean(), "a changed mode guard must red");
        assert!(report.to_string().contains("STRUCTURE moved"), "{report}");
    }
}
