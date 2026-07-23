// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared [llmstxt.org](https://llmstxt.org) skeleton emitter.
//!
//! Three surfaces emit an `llms.txt`-family document — the docs site index
//! ([`crate::render::llms_txt`]), the live MCP consumer index, and the flat
//! `dist/llms.txt` export. These were previously three independently-written
//! renderers that had silently diverged (`⊑` vs `subClassOf`, `→` vs `->`, a
//! three-line header vs a blockquote). This module is the ONE source of truth for
//! the format so they cannot diverge again: each surface builds a neutral list of
//! [`LlmsSection`]s from its own term model and hands them to [`render_index`].
//! The only thing that varies across surfaces is the per-bullet [`LlmsBullet::url`]
//! (present for a markdown link into a published site, `None` for a linkless
//! self-contained dump).

/// The canonical one-paragraph vocabulary summary — the `llms.txt` blockquote
/// body WITHOUT the leading `> `. The single source of truth shared by every
/// `llms.txt`-family surface (was previously duplicated in three renderers).
pub const GMEOW_SUMMARY: &str = "A reasoning-centric, RDF 1.2-native super-vocabulary grounded by its co-foundational language, mathematics, and logic slices; it unifies a person's or organization's digital existence (entities, contacts, email, trust/keys, time) and aligns it to schema.org, FOAF, PROV, the WOT schema, Wikidata, and more.";

/// The maximum number of characters of a bullet note in the link-INDEX form
/// (`llms.txt`). The COMPLETE form (`llms-full.txt`) inlines content and is bounded
/// by the token budget below, not this per-note cap. A fixed cap (no configurable
/// knob — project no-optionality doctrine).
pub const LLMS_NOTE_CAP: usize = 200;

/// The token budget for the complete inlined index (`llms-full.txt`): a fixed cap
/// (no configurable knob — project no-optionality doctrine) sized to fit a
/// large-context model. Callers emit whole term blocks in a deterministic total
/// order until the running [`estimate_tokens`] would exceed this budget, then
/// DISCLOSE the elided remainder (never silently drop it).
pub const LLMS_FULL_TOKEN_BUDGET: usize = 200_000;

/// The token budget for the GMN-1 teachability primer (`crate::gmn1_primer`): a fixed
/// ~500-token cap (no configurable knob — project no-optionality doctrine) sized to the
/// EPIC #1371 scenario-7 teachability contract ("a fresh model given ONLY the generated
/// ~500-token primer"). The primer emits its graph-derived rows (sigil table, operator
/// glyph table, repair-loop cards) in a deterministic total order until the running
/// [`estimate_tokens`] would exceed this budget, then DISCLOSES the elided remainder
/// (never silently drops it) — the same disclose-don't-truncate discipline the
/// [`LLMS_FULL_TOKEN_BUDGET`] elision uses.
pub const GMN1_PRIMER_TOKEN_BUDGET: usize = 500;

/// A deterministic, model-agnostic token estimate for `text`: one token per ~4
/// characters (the standard rough byte-pair ratio), rounded up. Dependency-free
/// and reproducible from source — no tokenizer model — so the budget elision
/// boundary is byte-stable across builds. Empty in → `0`.
pub fn estimate_tokens(text: &str) -> usize {
    text.chars().count().div_ceil(4)
}

/// One bullet in an `llms.txt` section.
pub struct LlmsBullet {
    /// The link / display text (a CURIE or a page title).
    pub text: String,
    /// The target URL — `Some` renders a markdown link `[text](url)`; `None`
    /// renders linkless `text` (the self-contained dist dump has no site to
    /// link into).
    pub url: Option<String>,
    /// A signature suffix appended directly after the text/link and before the
    /// `: note` — e.g. ` (⊑ Foo, Bar)` for a class or ` [A → B]` for a property.
    /// Empty when the item has no signature.
    pub signature: String,
    /// The trailing note (definition / summary), already collapsed to one line by
    /// the caller. Empty notes omit the `: ` separator entirely.
    pub note: String,
}

/// One `## `-headed section of an `llms.txt` document.
pub struct LlmsSection {
    /// The section heading text (rendered as `## {heading}`).
    pub heading: String,
    /// The bullets under the heading, in caller-determined (deterministic) order.
    pub bullets: Vec<LlmsBullet>,
}

/// Truncate a one-lined note to [`LLMS_NOTE_CAP`] characters on a `char`
/// boundary, appending `…` when truncated. Empty in → empty out.
pub fn cap_note(note: &str) -> String {
    if note.chars().count() <= LLMS_NOTE_CAP {
        return note.to_string();
    }
    let mut out: String = note.chars().take(LLMS_NOTE_CAP).collect();
    out.push('…');
    out
}

/// Render the llmstxt.org document header: `# {title}`, a blank line, the
/// `> {GMEOW_SUMMARY}` blockquote, a blank line, then each `prose` line followed
/// by a blank line. The trailing blank line means callers can append sections
/// directly.
pub fn llms_header(title: &str, prose: &[String]) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(title);
    out.push_str("\n\n");
    out.push_str("> ");
    out.push_str(GMEOW_SUMMARY);
    out.push_str("\n\n");
    for p in prose {
        out.push_str(p);
        out.push_str("\n\n");
    }
    out
}

/// Render a single bullet line (no trailing newline). The markdown-link form when
/// a URL is present, the linkless form otherwise; the `: note` is omitted when the
/// note is empty.
fn render_bullet(b: &LlmsBullet) -> String {
    let head = match &b.url {
        Some(url) => format!("- [{}]({}){}", b.text, url, b.signature),
        None => format!("- {}{}", b.text, b.signature),
    };
    if b.note.is_empty() {
        head
    } else {
        format!("{head}: {}", b.note)
    }
}

/// Render a single `## `-headed section (heading + bullets + a trailing blank
/// line) — the per-section unit [`render_index`] emits for each entry in its
/// `sections` list. Exposed so a caller that builds its own header/body by hand
/// (e.g. the complete/`llms-full.txt` forms, which inline term blocks directly
/// rather than going through [`LlmsSection`] end to end) can still append a
/// caller-built section (such as the shared [`standing_reference_section`])
/// through the ONE bullet-rendering path.
pub fn render_section(section: &LlmsSection) -> String {
    let mut out = String::new();
    out.push_str("## ");
    out.push_str(&section.heading);
    out.push_str("\n\n");
    for bullet in &section.bullets {
        out.push_str(&render_bullet(bullet));
        out.push('\n');
    }
    out.push('\n');
    out
}

/// Render a complete llmstxt.org index document: the header (with the canonical
/// summary blockquote) followed by each section's `## ` heading and bullets.
pub fn render_index(title: &str, prose: &[String], sections: &[LlmsSection]) -> String {
    let mut out = llms_header(title, prose);
    for section in sections {
        out.push_str(&render_section(section));
    }
    out
}

/// The standing documentation pages every `llms.txt`-family **Reference**
/// section names, in canonical order. The docs-site index (`render::llms_txt`)
/// links each into its published page; the MCP/consumer surfaces (which are
/// not always tied to a published site) name them as plain-text bullets via
/// [`standing_reference_section`]. Shared by both so they cannot silently
/// diverge — the docs-site llms-full form and the MCP surface had previously
/// drifted (the expansion landed only on the site).
pub const STANDING_REFERENCE_PAGES: &[&str] = &[
    "Competency questions",
    "Conformance fixtures",
    "Notation grammars",
    "Glossary",
    "Build pipeline",
];

/// The one-line description of the offline snippet corpus — the flattened
/// prompt-ready per-term cards written by `gmeow-dev sync --mode update --outputs docs
/// snippets`. Shared verbatim by every `llms.txt`-family surface (docs site +
/// MCP/consumer) so they cannot drift.
pub const SNIPPETS_CORPUS_NOTE: &str = "`gmeow-dev sync --mode update --outputs docs` writes one prompt-ready Markdown card per term to `terms/<slug>.md` — the offline, agent-ingestible projection of these docs.";

/// Build the linkless **Reference** [`LlmsSection`] every MCP/consumer
/// `llms.txt`-family surface must carry: [`STANDING_REFERENCE_PAGES`] as
/// plain-text bullets, plus the offline-snippet-corpus row carrying
/// [`SNIPPETS_CORPUS_NOTE`]. `Build pipeline` is included unconditionally — the
/// MCP/consumer surfaces render from the whole-repo fold graph, which always
/// carries a discovered pipeline (no bare-model case to gate on). The docs-site
/// renderer instead builds its own linked bullets from the same page-name list
/// (it DOES gate `Build pipeline` on a bare model — see `render::llms_txt`).
pub fn standing_reference_section() -> LlmsSection {
    let mut bullets: Vec<LlmsBullet> = STANDING_REFERENCE_PAGES
        .iter()
        .map(|title| LlmsBullet {
            text: (*title).to_string(),
            url: None,
            signature: String::new(),
            note: String::new(),
        })
        .collect();
    bullets.push(LlmsBullet {
        text: "Offline snippet corpus".to_string(),
        url: None,
        signature: String::new(),
        note: SNIPPETS_CORPUS_NOTE.to_string(),
    });
    LlmsSection {
        heading: "Reference".to_string(),
        bullets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_note_truncates_on_char_boundary() {
        let long = "x".repeat(LLMS_NOTE_CAP + 10);
        let capped = cap_note(&long);
        assert_eq!(capped.chars().count(), LLMS_NOTE_CAP + 1); // +1 for the ellipsis
        assert!(capped.ends_with('…'));
        assert_eq!(cap_note("short"), "short");
    }

    #[test]
    fn render_index_emits_one_h1_blockquote_and_sections() {
        let doc = render_index(
            "Title",
            &["Prose line.".to_string()],
            &[LlmsSection {
                heading: "Terms".to_string(),
                bullets: vec![
                    LlmsBullet {
                        text: "gmeow:Foo".to_string(),
                        url: Some("terms/foo/index.html".to_string()),
                        signature: " (⊑ Bar)".to_string(),
                        note: "A foo.".to_string(),
                    },
                    LlmsBullet {
                        text: "gmeow:Bare".to_string(),
                        url: None,
                        signature: String::new(),
                        note: String::new(),
                    },
                ],
            }],
        );
        assert_eq!(doc.lines().filter(|l| l.starts_with("# ")).count(), 1);
        assert!(doc.contains(&format!("> {GMEOW_SUMMARY}")));
        assert!(doc.contains("## Terms"));
        assert!(doc.contains("- [gmeow:Foo](terms/foo/index.html) (⊑ Bar): A foo."));
        assert!(doc.contains("- gmeow:Bare\n"));
    }
}
