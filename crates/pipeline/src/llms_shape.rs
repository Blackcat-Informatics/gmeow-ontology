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
//!   gain at most one medium resource, asserted as an EXACT enumerated delta.
//!
//! Anything reworded, reordered or removed reds. That asymmetry is the whole point: a
//! surface that could shrink to pass a gate would make the gate an incentive to hide
//! information.
//!
//! Everything here is a PURE function over source text so each clause has a reachable
//! red arm — the gate's fixtures perturb the working text and require a refusal.

use std::collections::BTreeSet;
use std::path::Path;

use crate::gmn_dialect::ModelFacingReport;

/// One frozen source item: the file it lives in, and the item within that file (the
/// whole file when [`FrozenItem::item`] is [`ItemRef::WholeFile`]).
#[derive(Debug, Clone, Copy)]
pub struct FrozenItem {
    /// Repo-relative path, forward slashes.
    pub path: &'static str,
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
/// The MCP `resources_result` is deliberately absent: its list may grow by one medium
/// resource, so it is checked by [`check_resource_list`] against an exact enumerated
/// delta instead of frozen outright.
pub const FROZEN_LLMS_SHAPE: &[FrozenItem] = &[
    FrozenItem {
        path: "crates/docs/src/llms.rs",
        item: ItemRef::WholeFile,
        why: "the ONE llmstxt.org skeleton emitter — header, blockquote, bullet form, \
              note cap, token budgets, the standing Reference section and its page list. \
              Frozen whole because it carries no term content at all: every byte of it is \
              shape",
    },
    FrozenItem {
        path: "crates/pipeline/src/stages/export.rs",
        item: ItemRef::Function("llms_sections"),
        why: "the section HEADINGS (Classes / Properties / Individuals) and their order",
    },
    FrozenItem {
        path: "crates/pipeline/src/stages/export.rs",
        item: ItemRef::Function("llms_signature"),
        why: "the notation conventions — the `⊑` subclass and `→` domain/range spellings \
              a model reads off every bullet",
    },
    FrozenItem {
        path: "crates/pipeline/src/stages/export.rs",
        item: ItemRef::Function("llms_note"),
        why: "the bullet-note convention (definition, label fallback, the `[fallback: en]` \
              marker)",
    },
    FrozenItem {
        path: "crates/pipeline/src/stages/export.rs",
        item: ItemRef::Function("llms_prose"),
        why: "the shared prose line every index form carries under its header",
    },
    FrozenItem {
        path: "crates/pipeline/src/stages/export.rs",
        item: ItemRef::Function("write_llms_txt"),
        why: "the section ORDERING of the index form: term sections, then the standing \
              Reference section, then the GMN-1 primer section",
    },
    FrozenItem {
        path: "crates/docs/src/gmn1_primer.rs",
        item: ItemRef::Const("PRIMER_HEADING"),
        why: "the primer's section heading — the anchor every surface's primer section is \
              found by",
    },
];

/// The MCP consumer-index item whose LIST may grow by at most one medium resource.
pub const MCP_RESOURCE_LIST: FrozenItem = FrozenItem {
    path: "crates/pipeline/src/mcp.rs",
    item: ItemRef::Function("resources_result"),
    why: "the MCP consumer-index resource list: its structure is frozen, its entries may \
          grow by at most one medium resource",
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
pub fn check_frozen_item(
    item: &FrozenItem,
    base_text: &str,
    work_text: &str,
    report: &mut ModelFacingReport,
) {
    let label = format!("{} :: {}", item.path, item.item.label());
    let Some(base) = extract_item(base_text, item.item) else {
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
    if base == work {
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
#[must_use]
pub fn resource_skeleton(body: &str) -> String {
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

/// The exact enumerated delta the MCP consumer-index resource list is allowed.
///
/// The working list must be the base list, or the base list with EXACTLY ONE additional
/// entry whose URI names the medium. Anything reworded, reordered or removed reds, and
/// so does a second addition — "at most one new medium resource, nothing else" is a
/// bound, not a direction.
///
/// Records a problem when the surrounding control flow moved, an existing entry moved or
/// changed, more than one entry was added, or an added entry is not a medium resource.
pub fn check_resource_list(base_body: &str, work_body: &str, report: &mut ModelFacingReport) {
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
    if added.len() > 1 {
        report.problem(format!(
            "the MCP consumer index gained {} resources ({:?}); the enumerated delta this \
             change is allowed is AT MOST ONE, and only a medium resource",
            added.len(),
            added.iter().map(|entry| &entry.uri).collect::<Vec<_>>()
        ));
    }
    for entry in added.iter().filter(|entry| !entry.uri.contains("medium")) {
        report.problem(format!(
            "the MCP consumer index gained resource {:?}, which is not a medium resource — \
             the enumerated delta is the new gmeow:Medium* surface and nothing else",
            entry.uri
        ));
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

// ── The merge-base comparand ─────────────────────────────────────────────────

/// The merge-base comparand, in the repo's tri-state discipline (the peer of
/// `resolve_base_ref` in `gmeow-dev-cli`'s slice-quality gate — restated here because
/// this crate is upstream of that binary and cannot depend on it).
#[derive(Debug, Clone)]
pub enum BaseRef {
    /// The resolved merge-base commit the frozen items are compared against.
    Resolved(String),
    /// `origin/main` genuinely does not exist in this checkout — the one case where "no
    /// prior committed state is reachable" is expected rather than broken. A LOUD skip.
    NoUpstream(String),
    /// `origin/main` exists but the comparand could not be obtained. HARD FAIL: the gate
    /// cannot perform the comparison it is defined to perform, and passing there would
    /// let a reworded surface through unseen.
    Unresolvable(String),
}

/// Resolve `git merge-base HEAD origin/main` locally (no network).
#[must_use]
pub fn resolve_base_ref(root: &Path) -> BaseRef {
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["rev-parse", "--verify", "--quiet", "origin/main"])
        .output()
    {
        Ok(out) if out.status.success() => {}
        Ok(_) => {
            return BaseRef::NoUpstream(
                "`origin/main` does not exist as a ref in this checkout (no upstream fetched)"
                    .to_owned(),
            );
        }
        Err(err) => return BaseRef::Unresolvable(format!("could not run git: {err}")),
    }
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["merge-base", "HEAD", "origin/main"])
        .output()
    {
        Ok(out) if out.status.success() => {
            let sha = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if sha.is_empty() {
                BaseRef::Unresolvable(
                    "`git merge-base HEAD origin/main` resolved no commit".to_owned(),
                )
            } else {
                BaseRef::Resolved(sha)
            }
        }
        Ok(out) => BaseRef::Unresolvable(format!(
            "`git merge-base HEAD origin/main` failed ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(err) => BaseRef::Unresolvable(format!("could not run git: {err}")),
    }
}

/// One file read at the merge base.
#[derive(Debug, Clone)]
pub enum BaseFile {
    /// The blob contents at the base commit.
    Contents(String),
    /// The path did not exist at the base — a brand-new file, whose shape cannot have
    /// moved because there was nothing to move.
    Absent,
    /// `git show` failed for any other reason — a HARD FAIL.
    Error(String),
}

/// Read `<base>:<rel>` via `git show` (local, no network).
#[must_use]
pub fn git_show_base(root: &Path, base: &str, rel: &str) -> BaseFile {
    let spec = format!("{base}:{rel}");
    match std::process::Command::new("git")
        .current_dir(root)
        .env("LC_ALL", "C")
        .args(["show", &spec])
        .output()
    {
        Ok(out) if out.status.success() => {
            BaseFile::Contents(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            if stderr.contains("does not exist in") || stderr.contains("exists on disk, but not in")
            {
                BaseFile::Absent
            } else {
                BaseFile::Error(format!(
                    "`git show {spec}` failed ({}): {}",
                    out.status,
                    stderr.trim()
                ))
            }
        }
        Err(err) => BaseFile::Error(format!("could not run `git show {spec}`: {err}")),
    }
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

    #[test]
    fn an_unchanged_resource_list_passes() {
        let body = resources_body(FIXTURE);
        let report = run(|r| check_resource_list(&body, &body, r));
        assert!(report.is_clean(), "an unchanged list passes: {report}");
    }

    #[test]
    fn exactly_one_new_medium_resource_is_the_allowed_delta() {
        let grown = FIXTURE.replace(
            r#"        resource("gmeow://ontology/okf-index","#,
            "        resource(\"gmeow://ontology/medium\", \"medium\", \"The medium axis.\", \
             \"application/json\"),\n        resource(\"gmeow://ontology/okf-index\",",
        );
        let report =
            run(|r| check_resource_list(&resources_body(FIXTURE), &resources_body(&grown), r));
        assert!(
            report.is_clean(),
            "one medium resource is the enumerated delta: {report}"
        );
    }

    /// The acceptance criterion's own red fixture: add a SECOND MCP resource.
    #[test]
    fn a_second_new_resource_reds_the_enumerated_delta() {
        let grown = FIXTURE.replace(
            r#"        resource("gmeow://ontology/okf-index","#,
            "        resource(\"gmeow://ontology/medium\", \"medium\", \"The medium axis.\", \
             \"application/json\"),\n        resource(\"gmeow://ontology/medium-effect\", \
             \"medium-effect\", \"The measurement.\", \"application/json\"),\n        \
             resource(\"gmeow://ontology/okf-index\",",
        );
        let report =
            run(|r| check_resource_list(&resources_body(FIXTURE), &resources_body(&grown), r));
        assert!(!report.is_clean(), "a second added resource must red");
        assert!(report.to_string().contains("AT MOST ONE"), "{report}");
    }

    #[test]
    fn a_non_medium_addition_reds() {
        let grown = FIXTURE.replace(
            r#"        resource("gmeow://ontology/okf-index","#,
            "        resource(\"gmeow://ontology/changelog\", \"changelog\", \"Whatever.\", \
             \"text/plain\"),\n        resource(\"gmeow://ontology/okf-index\",",
        );
        let report =
            run(|r| check_resource_list(&resources_body(FIXTURE), &resources_body(&grown), r));
        assert!(!report.is_clean(), "a non-medium addition must red");
        assert!(
            report.to_string().contains("not a medium resource"),
            "{report}"
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
        let report =
            run(|r| check_resource_list(&resources_body(FIXTURE), &resources_body(&reordered), r));
        assert!(!report.is_clean(), "a reordered list must red");
        assert!(
            report.to_string().contains("reordered or reworded"),
            "{report}"
        );
    }

    #[test]
    fn a_reworded_resource_description_reds() {
        let reworded = FIXTURE.replace("OKF manifest.", "OKF manifest envelope.");
        let report =
            run(|r| check_resource_list(&resources_body(FIXTURE), &resources_body(&reworded), r));
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
        let report =
            run(|r| check_resource_list(&resources_body(FIXTURE), &resources_body(&dropped), r));
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
        let report =
            run(|r| check_resource_list(&resources_body(FIXTURE), &resources_body(&changed), r));
        assert!(!report.is_clean(), "a changed mode guard must red");
        assert!(report.to_string().contains("STRUCTURE moved"), "{report}");
    }
}
