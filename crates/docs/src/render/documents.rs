// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Link, heading, and anchor rewrites for authored Markdown documents.

use gmeow_errors::{Diag, Result};

use crate::source_map::{DocLinkResolution, LinkResolution, SourceToPageMap, fence_open};

use super::{find_backtick_run, rel};

// ── First-class Markdown document rewrite (graft + child page + llms corpus) ──

/// Re-emit an authored slice Markdown document (`docs.md` or a child `*.md`) with
/// every internal link resolved through the single [`SourceToPageMap`] authority
/// and, when `demote` is set, every ATX heading demoted one level.
///
/// The body is processed line by line so every Markdown construct is preserved
/// verbatim except the two deliberate rewrites. Fenced code blocks (```` ``` ````
/// / `~~~`) and inline code spans are passed through untouched, so a `#` or a
/// `](…)` inside a code sample is never mistaken for a heading or a link.
///
/// * `from_slice` / `from_path` locate the document in source space (the resolver
///   joins relative links against `from_path`).
/// * `page_dir` is the site directory of the page the body is rendered ONTO (the
///   slice page for a graft, the child page for a child render, `""` for the
///   root-level llms corpus) — relative hrefs are computed from it.
/// * `demote` selects the heading transform (see [`HeadingDemotion`]).
/// * `inject_anchors` — when set, emit an explicit `<a id="{slug}"></a>` before
///   each heading using the map's page-scoped, text-derived slugs (matched in
///   source order). pulldown-cmark emits no heading `id`, so these anchors are what
///   the rewritten `#slug` / `…index.md#slug` cross-links resolve to on the HTML
///   page. Off for the plain-text `llms-full` corpus (no HTML anchors there).
pub(super) fn rewrite_doc_body(
    source: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
    demote: HeadingDemotion,
    inject_anchors: bool,
) -> Result<String> {
    // The page-scoped heading anchors, in source order, for the id injection. Empty
    // when not injecting or when the document has no known page.
    let anchors: &[crate::source_map::HeadingAnchor] = if inject_anchors {
        map.page_of(from_slice, from_path)
            .map(|page| map.heading_anchors(page))
            .unwrap_or(&[])
    } else {
        &[]
    };
    let mut anchor_idx = 0usize;

    let mut out = String::with_capacity(source.len() + 64);
    let mut in_fence = false;
    let mut fence_marker = "";
    for line in source.split_inclusive('\n') {
        let trimmed = line.trim_start();
        // Toggle fenced-code state; fenced lines pass through verbatim.
        if let Some(marker) = fence_open(trimmed) {
            if in_fence {
                if trimmed.starts_with(fence_marker) {
                    in_fence = false;
                    fence_marker = "";
                }
            } else {
                in_fence = true;
                fence_marker = marker;
            }
            out.push_str(line);
            continue;
        }
        if in_fence {
            // Fenced code is emitted verbatim and its links are NOT rewritten (a
            // `](x.md)` in a code sample is not a document link). The one exception
            // is the `CorpusFloor` flatten, which additionally demotes any leading
            // `#`-run so an inlined Turtle/shell comment (`# note`) never surfaces
            // as a spurious H1/section in the flat `llms-full` corpus — harmless,
            // since `#`-to-end-of-line comment syntaxes stay comments at any depth.
            if demote == HeadingDemotion::CorpusFloor {
                out.push_str(&demote_heading_line(line, demote, from_slice, from_path)?);
            } else {
                out.push_str(line);
            }
            continue;
        }
        // Before an ATX heading (in source order), emit its explicit HTML anchor so
        // the resolved `#slug` cross-links have a matching `id` on the page.
        if inject_anchors && atx_level(line).is_some() {
            if let Some(anchor) = anchors.get(anchor_idx) {
                out.push_str(&format!("<a id=\"{}\"></a>\n\n", anchor.slug));
            }
            anchor_idx += 1;
        }
        // Heading demotion is applied to the raw line before link rewriting; the
        // added `#`s never affect link scanning.
        let demoted = demote_heading_line(line, demote, from_slice, from_path)?;
        rewrite_doc_line(&demoted, map, from_slice, from_path, page_dir, &mut out)?;
    }
    Ok(out)
}

/// The heading transform [`rewrite_doc_body`] applies to a re-emitted document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum HeadingDemotion {
    /// Keep every heading level (a child page: its authored H1 is the page H1).
    Keep,
    /// Demote one level (H1→H2 … H5→H6) so a grafted `docs.md` sits under the
    /// generated slice H1. A source H6 is the H6-overflow guard: H6→H7 is illegal,
    /// so it is a hard failure naming the document.
    GraftOne,
    /// Floor headings two levels down, clamped at H6, so the inlined `llms-full`
    /// document corpus sits below both the single `# ` document H1 and the
    /// `## Documents` section header (every heading becomes H3+, never a bare `## `
    /// that the flat llmstxt structure would read as a new section). Clamps rather
    /// than overflowing — the corpus is a lossy flattened surface, not the graft.
    CorpusFloor,
}

/// Apply the [`HeadingDemotion`] transform to one line, preserving leading
/// whitespace and the trailing newline. Non-heading lines are returned unchanged.
fn demote_heading_line(
    line: &str,
    demote: HeadingDemotion,
    from_slice: &str,
    from_path: &str,
) -> Result<String> {
    if demote == HeadingDemotion::Keep {
        return Ok(line.to_string());
    }
    let indent_len = line.len() - line.trim_start().len();
    let (indent, rest) = line.split_at(indent_len);
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return Ok(line.to_string());
    }
    let after = &rest[hashes..];
    // A heading needs whitespace (or end of line) after the hash run; otherwise
    // it is not an ATX heading (e.g. `#hashtag`) and must not be demoted.
    if !after.is_empty() && !after.starts_with([' ', '\t', '\n']) {
        return Ok(line.to_string());
    }
    let extra = match demote {
        HeadingDemotion::Keep => unreachable!("handled above"),
        HeadingDemotion::GraftOne => {
            if hashes == 6 {
                return Err(Diag::of_kind(crate::render_error::HeadingOverflow {
                    detail: format!(
                        "markdown source `{from_path}` in slice {from_slice} carries a level-6 heading \
                     that cannot be demoted (H6→H7 is illegal): {:?}",
                        line.trim_end()
                    ),
                }));
            }
            1
        }
        // Floor at H6: every heading lands at level+2 but never past 6.
        HeadingDemotion::CorpusFloor => (6 - hashes).min(2),
    };
    let prefix = "#".repeat(hashes + extra);
    Ok(format!("{indent}{prefix}{after}"))
}

/// Rewrite the link targets on a single (non-fenced) source line, appending the
/// result to `out`. Scans for inline-link / image destinations `](target)`,
/// skipping inline code spans so a `](…)` inside backticks is left verbatim.
fn rewrite_doc_line(
    line: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
    out: &mut String,
) -> Result<()> {
    let bytes = line.as_bytes();
    let mut i = 0usize;
    let mut span_start = 0usize;
    while i < bytes.len() {
        // Skip an inline code span: a backtick run of length N is closed by the
        // next run of exactly N backticks (CommonMark). Everything between is
        // literal and never rewritten.
        if bytes[i] == b'`' {
            let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
            let close_from = i + run;
            if let Some(rel_close) = find_backtick_run(&bytes[close_from..], run) {
                i = close_from + rel_close + run;
                continue;
            }
            // No closing run on this line: the rest is not a code span, keep scanning.
            i += run;
            continue;
        }
        // An inline link/image destination opens at `](`.
        if bytes[i] == b']' && i + 1 < bytes.len() && bytes[i + 1] == b'(' {
            let open = i + 2;
            // Destinations in authored slice markdown never contain a raw `)`.
            if let Some(rel_close) = line[open..].find(')') {
                let close = open + rel_close;
                out.push_str(&line[span_start..open]);
                out.push_str(&rewrite_doc_target(
                    &line[open..close],
                    map,
                    from_slice,
                    from_path,
                    page_dir,
                )?);
                span_start = close;
                i = close;
                continue;
            }
        }
        i += 1;
    }
    out.push_str(&line[span_start..]);
    Ok(())
}

/// The ATX heading level (1–6) of a non-fenced line, or `None` when it is not an
/// ATX heading. Mirrors the `source_map` heading detector so this walk's heading
/// order aligns with the map's page-scoped anchor order.
fn atx_level(line: &str) -> Option<u8> {
    let rest = line.trim_start();
    let hashes = rest.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let after = &rest[hashes..];
    if after.is_empty() || after.starts_with([' ', '\t', '\n']) {
        Some(hashes as u8)
    } else {
        None
    }
}

/// Rewrite a single authored link/image destination (the text between `](` and
/// `)`), routing every internal reference through the single [`SourceToPageMap`]
/// authority.
///
/// * Absolute / scheme-qualified / `mailto:` / site-absolute targets are external
///   and pass through verbatim.
/// * A pure `#fragment` addresses a heading in THIS document; a missing anchor is
///   a hard failure (a broken same-document link must not render silently).
/// * A relative `*.md` reference that stays WITHIN the slice is resolved to its
///   generated page (relative href, with any `#anchor`); a dangling within-slice
///   reference is a hard failure the slice's markdown must fix.
/// * A reference that escapes the slice root (another slice, a repo `docs/` file)
///   or targets a non-markdown asset is off-corpus and is absolutized to the
///   published site, so the rendered page never dangles.
fn rewrite_doc_target(
    target: &str,
    map: &SourceToPageMap,
    from_slice: &str,
    from_path: &str,
    page_dir: &str,
) -> Result<String> {
    if target.is_empty() {
        return Ok(target.to_string());
    }
    // The path portion (before any `#fragment`) classifies the target; the resolver
    // re-splits and validates the fragment itself.
    let path_part = target.split_once('#').map_or(target, |(p, _)| p);

    // A pure fragment: a heading in this same document.
    if path_part.is_empty() {
        return match map.resolve_link(from_slice, from_path, target) {
            LinkResolution::Resolved(loc) => Ok(match loc.anchor {
                Some(anchor) => format!("#{anchor}"),
                // A bare `#` with no anchor — leave as authored.
                None => target.to_string(),
            }),
            LinkResolution::Dangling { .. } => {
                Err(Diag::of_kind(crate::render_error::DanglingLink {
                    detail: format!(
                        "markdown source `{from_path}` in slice {from_slice} has a dangling same-document \
                 anchor link `{target}` (no such heading)"
                    ),
                }))
            }
        };
    }

    // Everything with a path portion is CLASSIFIED by the single authority
    // (`SourceToPageMap::classify_doc_link`) — the site renderer never re-derives
    // "internal vs. off-corpus vs. external"; it only formats the classification for
    // its own (site-relative) output.
    match map.classify_doc_link(from_slice, from_path, target) {
        DocLinkResolution::External => Ok(target.to_string()),
        DocLinkResolution::OffCorpus => Ok(absolutize_offsite(target)),
        DocLinkResolution::Corpus(loc) => {
            let target_dir = loc.page.strip_suffix('/').unwrap_or(&loc.page);
            let href = rel(page_dir, target_dir);
            Ok(match loc.anchor {
                Some(anchor) => format!("{href}index.md#{anchor}"),
                None => format!("{href}index.md"),
            })
        }
        DocLinkResolution::Dangling { .. } => {
            Err(Diag::of_kind(crate::render_error::DanglingLink {
                detail: format!(
                    "markdown source `{from_path}` in slice {from_slice} has a dangling internal document \
             link `{target}` — fix the link in the slice's markdown (it names no document in the \
             slice)"
                ),
            }))
        }
    }
}

/// Absolutize an off-corpus relative reference to the published documentation site,
/// so a rendered page never carries a dangling relative link into a document this
/// render does not emit. Leading `./` / `../` segments are dropped and the
/// remainder is appended to [`crate::mdbook::PUBLISHED_SITE_BASE`] — the SAME
/// published-site base the mdbook renderer externalizes dropped surfaces to.
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
    format!("{}{rest}", crate::mdbook::PUBLISHED_SITE_BASE)
}
