// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Deterministic Markdown → Typst renderer for the authored slice documents.
//!
//! The print projection inlines each slice's GUIDE (its grafted `docs.md` prose)
//! and its CHILD documents (`design/*.md`, …) directly into the PDF, before that
//! slice's generated term material. This module turns one authored
//! [`DocMarkdownDocument`] source into Typst markup:
//!
//! * ATX headings become real `#heading(level: …)` nodes (so the PDF outline is a
//!   total projection of the documents), each carrying a Typst `<label>` minted
//!   from the single [`SourceToPageMap`] page/anchor authority.
//! * GFM tables become real `#table(…)`, fenced code becomes `#raw(block: true, …)`,
//!   lists / block-quotes / paragraphs render as their Typst equivalents.
//! * Every inline `[text](target)` link is classified through
//!   [`SourceToPageMap::classify_doc_link`] — the SAME authority the HTML site and
//!   the mdbook use. An INTRA-CORPUS reference (another inlined document / heading
//!   of the same slice) becomes a resolvable Typst INTERNAL reference
//!   (`#link(<label>)`), so a click/anchor lands inside the PDF; an off-corpus or
//!   external reference is absolutized to the published site (a declared cross-link
//!   loss) — never a fabricated live internal link.
//!
//! Every interpolated fragment of authored text is emitted as the body of a Typst
//! STRING literal (`#"…"`) or a `#raw("…")` node via the crate's single
//! [`crate::render::escape_typ`] authority, so hostile document text can never
//! perturb or break compilation. The output is a pure function of the source + the
//! map, so it is byte-reproducible.

use gmeow_docs::DocLinkResolution;
use gmeow_docs::mdbook::PUBLISHED_SITE_BASE;
use gmeow_docs::source_map::SourceToPageMap;

use crate::render::escape_typ;

/// Render one authored markdown document as Typst markup, appended to `out`.
///
/// * `source` is the document's raw markdown.
/// * `slice_iri` / `source_path` locate the document in source space (so relative
///   links resolve against it through the map).
/// * `page` is the document's generated page path (the map's), used to mint the
///   document + heading labels and to match the map's page-scoped heading anchors.
/// * `base_level` offsets every heading: a guide/child rendered under a `= Slice:`
///   (level 1) chapter passes `base_level = 1`, so a source `#` H1 becomes a level-2
///   heading that sits under the slice in the outline.
pub fn render_document(
    out: &mut String,
    source: &str,
    slice_iri: &str,
    source_path: &str,
    page: &str,
    base_level: usize,
    map: &SourceToPageMap,
) {
    // A zero-size, invisible label anchor at the document's head, so a link to this
    // document (with no fragment) resolves to its start even when the first block is
    // not a heading.
    out.push_str(&format!("#metadata(none) <{}>\n\n", page_label(map, page)));

    let ctx = Ctx {
        slice_iri,
        source_path,
        map,
    };
    let lines: Vec<&str> = source.lines().collect();
    // The map's page-scoped heading anchors, in the SAME source order this walk
    // visits headings (both skip fenced code), so anchor N here is anchor N there.
    let anchors = map.heading_anchors(page);
    let mut anchor_idx = 0usize;

    let mut i = 0usize;
    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim_start();

        // Fenced code block → raw block (verbatim, never link/heading scanned).
        if let Some(marker) = fence_marker(trimmed) {
            let lang = fence_lang(trimmed, marker);
            let mut body = String::new();
            i += 1;
            while i < lines.len() && fence_close(lines[i].trim_start(), marker).is_none() {
                body.push_str(lines[i]);
                body.push('\n');
                i += 1;
            }
            if i < lines.len() {
                i += 1; // consume the closing fence
            }
            emit_raw_block(out, &body, lang);
            continue;
        }

        // Blank line → paragraph break.
        if trimmed.is_empty() {
            out.push('\n');
            i += 1;
            continue;
        }

        // ATX heading → real Typst heading with a page/anchor label.
        if let Some((level, text)) = atx_heading(trimmed) {
            let lvl = base_level + level as usize;
            let label = anchors
                .get(anchor_idx)
                .map(|a| anchor_label(map, page, &a.slug));
            anchor_idx += 1;
            out.push_str(&format!("#heading(level: {lvl})[{}]", inline(&text, &ctx)));
            if let Some(label) = label {
                out.push_str(&format!(" <{label}>"));
            }
            out.push_str("\n\n");
            i += 1;
            continue;
        }

        // GFM table (a row followed by a delimiter row) → real Typst table.
        if is_table_start(&lines, i) {
            let consumed = emit_table(out, &lines, i, &ctx);
            i += consumed;
            continue;
        }

        // Block-quote (one or more consecutive `>` lines) → a Typst quote block.
        if trimmed.starts_with('>') {
            let mut quoted: Vec<String> = Vec::new();
            while i < lines.len() && lines[i].trim_start().starts_with('>') {
                let q = lines[i]
                    .trim_start()
                    .strip_prefix('>')
                    .unwrap_or("")
                    .trim_start();
                quoted.push(q.to_string());
                i += 1;
            }
            let joined = quoted.join(" ");
            out.push_str(&format!(
                "#quote(block: true)[{}]\n\n",
                inline(&joined, &ctx)
            ));
            continue;
        }

        // List item (unordered `-`/`*`/`+` or ordered `N.`/`N)`) → a Typst list line,
        // preserving the source indent so nested lists stay nested.
        if let Some(item) = list_item(line) {
            out.push_str(&format!(
                "{}{} {}\n",
                item.indent,
                item.marker,
                inline(&item.content, &ctx)
            ));
            i += 1;
            continue;
        }

        // Otherwise: a paragraph — accumulate consecutive prose lines (soft-wrapped
        // into one paragraph) until a blank line or a block boundary.
        let mut para: Vec<&str> = Vec::new();
        while i < lines.len() {
            let t = lines[i].trim_start();
            if t.is_empty()
                || fence_marker(t).is_some()
                || atx_heading(t).is_some()
                || is_table_start(&lines, i)
                || t.starts_with('>')
                || list_item(lines[i]).is_some()
            {
                break;
            }
            para.push(lines[i]);
            i += 1;
        }
        let joined = para.iter().map(|l| l.trim()).collect::<Vec<_>>().join(" ");
        out.push_str(&inline(&joined, &ctx));
        out.push_str("\n\n");
    }
    // Guarantee a trailing separation so the next chapter never abuts this document.
    if !out.ends_with("\n\n") {
        out.push('\n');
    }
}

/// The source-space location of the document being rendered — threaded to the
/// inline link classifier.
struct Ctx<'a> {
    slice_iri: &'a str,
    source_path: &'a str,
    map: &'a SourceToPageMap,
}

// ── Labels ───────────────────────────────────────────────────────────────────

/// The document-level Typst label for a generated page — minted from the map's
/// INJECTIVE node slug, so it is globally unique across the whole PDF and safe as a
/// label (`[a-z0-9-]`). Falls back to a sanitized page path only if the page is
/// somehow unknown to the map (it never is for a rendered document).
fn page_label(map: &SourceToPageMap, page: &str) -> String {
    let ns = map
        .node_slug_of_page(page)
        .map(str::to_string)
        .unwrap_or_else(|| sanitize(page));
    format!("gdoc-{ns}")
}

/// The Typst label for a heading anchor `(page, slug)` — the document label plus the
/// page-scoped, disambiguated heading slug. Globally unique (page injective, slug
/// page-scoped unique).
fn anchor_label(map: &SourceToPageMap, page: &str, slug: &str) -> String {
    format!("{}--{}", page_label(map, page), sanitize(slug))
}

/// Reduce an arbitrary string to a Typst-label-safe `[a-z0-9-]` slug (a collapsing
/// fold; only used for the fallback page path and for anchor slugs, which are
/// already `[a-z0-9-]`).
fn sanitize(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_dash = false;
    for ch in s.chars() {
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
        "x".to_string()
    } else {
        trimmed.to_string()
    }
}

// ── Inline rendering ─────────────────────────────────────────────────────────

/// Render an inline markdown run to Typst markup: literal text as `#"…"` string
/// interpolations, inline code as `#raw("…")`, and `[text](target)` links as
/// resolved `#link(…)[…]`. Tokens are emitted with NO separating source whitespace,
/// so the rendered text is exactly the concatenation (spacing lives inside the
/// string literals). Markdown emphasis markers are carried literally (a deliberate,
/// safe fidelity floor of this lossy projection — the required carried content is
/// prose / headings / tables / code / links).
fn inline(text: &str, ctx: &Ctx) -> String {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len() + 8);
    let mut pending = String::new();
    let mut i = 0usize;

    let flush = |pending: &mut String, out: &mut String| {
        if !pending.is_empty() {
            out.push_str(&format!("#\"{}\"", escape_typ(pending)));
            pending.clear();
        }
    };

    while i < bytes.len() {
        // Inline code span: a run of N backticks closed by the next run of exactly N.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let close_from = i + run;
            if let Some(rel) = find_backtick_run(&bytes[close_from..], run) {
                let code = &text[close_from..close_from + rel];
                flush(&mut pending, &mut out);
                out.push_str(&format!("#raw(\"{}\")", escape_typ(code)));
                i = close_from + rel + run;
                continue;
            }
            // No closing run — the backticks are literal text.
            pending.push_str(&text[i..close_from]);
            i = close_from;
            continue;
        }

        // Inline link `[text](target)`.
        if bytes[i] == b'['
            && let Some((label, target, next)) = parse_link(text, i)
        {
            flush(&mut pending, &mut out);
            out.push_str(&render_link(label, target, ctx));
            i = next;
            continue;
        }

        // Ordinary character — accumulate. Advance by a whole UTF-8 char.
        let ch_len = utf8_len(bytes[i]);
        pending.push_str(&text[i..i + ch_len]);
        i += ch_len;
    }
    flush(&mut pending, &mut out);
    out
}

/// Parse a markdown inline link starting at `open` (`[`): returns
/// `(link_text, target, index_after_close_paren)` when the shape `[text](target)`
/// is present, else `None` (a bare `[` is literal text). Targets in authored slice
/// markdown never contain a raw `)`.
fn parse_link(text: &str, open: usize) -> Option<(&str, &str, usize)> {
    let bytes = text.as_bytes();
    // Find the matching `]` for `[` (no nested brackets in authored link text).
    let close_br_rel = text[open + 1..].find(']')?;
    let close_br = open + 1 + close_br_rel;
    // The `]` must be immediately followed by `(`.
    if bytes.get(close_br + 1) != Some(&b'(') {
        return None;
    }
    let paren_open = close_br + 2;
    let close_paren_rel = text[paren_open..].find(')')?;
    let close_paren = paren_open + close_paren_rel;
    let label = &text[open + 1..close_br];
    let target = &text[paren_open..close_paren];
    Some((label, target, close_paren + 1))
}

/// Render one classified link as a Typst `#link(…)[…]`. The body is the link text
/// rendered inline; the destination is a `<label>` internal reference for an
/// intra-corpus target, or a quoted absolute URL for an external / off-corpus one.
fn render_link(label_text: &str, target: &str, ctx: &Ctx) -> String {
    let body = {
        let inner = inline(label_text, ctx);
        if inner.is_empty() {
            format!("#\"{}\"", escape_typ(target))
        } else {
            inner
        }
    };
    match ctx
        .map
        .classify_doc_link(ctx.slice_iri, ctx.source_path, target)
    {
        DocLinkResolution::Corpus(loc) => {
            let label = match &loc.anchor {
                Some(slug) => anchor_label(ctx.map, &loc.page, slug),
                None => page_label(ctx.map, &loc.page),
            };
            format!("#link(<{label}>)[{body}]")
        }
        DocLinkResolution::External => {
            if target.is_empty() {
                body
            } else {
                format!("#link(\"{}\")[{body}]", escape_typ(target))
            }
        }
        DocLinkResolution::OffCorpus => {
            format!(
                "#link(\"{}\")[{body}]",
                escape_typ(&absolutize_offsite(target))
            )
        }
        DocLinkResolution::Dangling { target, anchor } => panic!(
            "print inliner: authored markdown `{}` in slice {} has a dangling internal link \
             {target:?} (anchor {anchor:?}) — fix the link in the slice's markdown",
            ctx.source_path, ctx.slice_iri
        ),
    }
}

/// Absolutize an off-corpus relative reference to the published documentation site —
/// the SAME base the HTML site and the mdbook externalize dropped surfaces to — so
/// the PDF carries an honest absolute link, never a live internal one. Leading `./`
/// / `../` segments are dropped.
fn absolutize_offsite(target: &str) -> String {
    let mut rest = target;
    loop {
        if let Some(r) = rest.strip_prefix("../") {
            rest = r;
        } else if let Some(r) = rest.strip_prefix("./") {
            rest = r;
        } else {
            break;
        }
    }
    format!("{PUBLISHED_SITE_BASE}{rest}")
}

// ── Blocks: code / tables ────────────────────────────────────────────────────

/// Emit a fenced code body as a Typst raw block, carrying the language when the
/// fence declared a simple identifier. Newlines are preserved literally inside the
/// Typst string; `\` / `"` are escaped and other control chars neutralized.
fn emit_raw_block(out: &mut String, body: &str, lang: Option<&str>) {
    // Trim exactly one trailing newline the collector added past the last code line.
    let body = body.strip_suffix('\n').unwrap_or(body);
    match lang {
        Some(lang) if !lang.is_empty() => out.push_str(&format!(
            "#raw(block: true, lang: \"{}\", \"{}\")\n\n",
            escape_typ(lang),
            escape_raw(body)
        )),
        _ => out.push_str(&format!("#raw(block: true, \"{}\")\n\n", escape_raw(body))),
    }
}

/// Escape a multi-line code body for a Typst string literal, PRESERVING newlines
/// and tabs literally (Typst string literals may span lines), escaping only `\` and
/// `"`, and neutralizing other control characters as `\u{…}`.
fn escape_raw(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' | '\t' => out.push(c),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Whether a GFM table starts at line `i` (a `|`-bearing header row immediately
/// followed by a delimiter row of `|`, `-`, `:`, spaces with at least one `-`).
fn is_table_start(lines: &[&str], i: usize) -> bool {
    lines[i].contains('|') && lines.get(i + 1).is_some_and(|l| is_delimiter_row(l))
}

/// A GFM table delimiter row: only `|`, `-`, `:`, and spaces, at least one `-`, and
/// at least one `|`.
fn is_delimiter_row(line: &str) -> bool {
    let t = line.trim();
    if t.is_empty() || !t.contains('|') {
        return false;
    }
    let mut has_dash = false;
    for c in t.chars() {
        match c {
            '-' => has_dash = true,
            '|' | ':' | ' ' => {}
            _ => return false,
        }
    }
    has_dash
}

/// Emit a GFM table as a Typst `#table(…)`; returns the number of source lines
/// consumed (header + delimiter + body rows). Cells are rendered inline, so links
/// inside a table cell resolve exactly like body links.
fn emit_table(out: &mut String, lines: &[&str], start: usize, ctx: &Ctx) -> usize {
    let header = row_cells(lines[start]);
    let cols = header.len().max(1);
    // Body rows: everything after the delimiter row that still looks like a row.
    let mut end = start + 2;
    while end < lines.len() {
        let t = lines[end].trim();
        if t.is_empty() || !t.contains('|') {
            break;
        }
        end += 1;
    }

    out.push_str(&format!("#table(\n  columns: {cols},\n"));
    out.push_str("  table.header(");
    for k in 0..cols {
        let cell = header.get(k).map(String::as_str).unwrap_or("");
        out.push_str(&format!("[{}], ", inline(cell, ctx)));
    }
    out.push_str("),\n");
    for row in &lines[start + 2..end] {
        let cells = row_cells(row);
        for k in 0..cols {
            let cell = cells.get(k).map(String::as_str).unwrap_or("");
            out.push_str(&format!("  [{}],\n", inline(cell, ctx)));
        }
    }
    out.push_str(")\n\n");
    end - start
}

/// The interior cells of a GFM table row, trimmed, dropping the optional leading /
/// trailing pipe. Splits on unescaped `|`.
fn row_cells(line: &str) -> Vec<String> {
    let t = line.trim();
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = inner.strip_suffix('|').unwrap_or(inner);
    let mut cells: Vec<String> = Vec::new();
    let mut cur = String::new();
    let mut escaped = false;
    for c in inner.chars() {
        if escaped {
            cur.push(c);
            escaped = false;
        } else if c == '\\' {
            cur.push(c);
            escaped = true;
        } else if c == '|' {
            cells.push(cur.trim().to_string());
            cur.clear();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

// ── Lists ────────────────────────────────────────────────────────────────────

/// A parsed list item: its Typst marker (`-` unordered / `+` ordered), the source
/// indent (preserved so nested lists stay nested), and the item content.
struct ListItem {
    marker: &'static str,
    indent: String,
    content: String,
}

/// Parse a list item from a raw line, else `None`. Recognizes unordered `-`/`*`/`+`
/// and ordered `N.`/`N)` markers.
fn list_item(line: &str) -> Option<ListItem> {
    let indent_len = line.len() - line.trim_start().len();
    let indent = line[..indent_len].replace('\t', "  ");
    let rest = &line[indent_len..];

    // Unordered.
    for m in ["- ", "* ", "+ "] {
        if let Some(content) = rest.strip_prefix(m) {
            return Some(ListItem {
                marker: "-",
                indent,
                content: content.to_string(),
            });
        }
    }
    // Ordered: a digit run then `.` or `)` then a space.
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 {
        let after = &rest[digits..];
        if let Some(content) = after
            .strip_prefix(". ")
            .or_else(|| after.strip_prefix(") "))
        {
            return Some(ListItem {
                marker: "+",
                indent,
                content: content.to_string(),
            });
        }
    }
    None
}

// ── Markdown lexical helpers (mirror the map's heading detector) ──────────────

/// The fence marker (` ``` ` / `~~~`) a trimmed line OPENS a fenced code block with
/// (three or more of one marker char), else `None`.
fn fence_marker(trimmed: &str) -> Option<&'static str> {
    if trimmed.starts_with("```") {
        Some("```")
    } else if trimmed.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Whether a trimmed line CLOSES a fenced block opened with `marker`.
fn fence_close(trimmed: &str, marker: &str) -> Option<()> {
    trimmed.starts_with(marker).then_some(())
}

/// The (simple-identifier) language token after an opening fence, when present.
fn fence_lang<'a>(trimmed: &'a str, marker: &str) -> Option<&'a str> {
    let info = trimmed.strip_prefix(marker)?.trim();
    if !info.is_empty()
        && info
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '+')
    {
        Some(info)
    } else {
        None
    }
}

/// Parse an ATX heading line into `(level, text)` — mirrors the map's detector so
/// the heading order aligns with the map's page-scoped anchor order.
fn atx_heading(trimmed: &str) -> Option<(u8, String)> {
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.is_empty() && !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim().to_string();
    if text.is_empty() {
        return None;
    }
    Some((hashes as u8, text))
}

/// The byte offset of the next run of EXACTLY `run` backticks in `bytes`, else
/// `None` (used to close an inline code span).
fn find_backtick_run(bytes: &[u8], run: usize) -> Option<usize> {
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'`' {
            let here = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            if here == run {
                return Some(i);
            }
            i += here;
        } else {
            i += 1;
        }
    }
    None
}

/// The UTF-8 byte length of the char whose leading byte is `b`.
fn utf8_len(b: u8) -> usize {
    match b {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        _ => 4,
    }
}
