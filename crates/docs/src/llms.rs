// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The shared [llmstxt.org](https://llmstxt.org) skeleton emitter (#1027).
//!
//! Three surfaces emit an `llms.txt`-family document — the docs site index
//! ([`crate::render::llms_txt`]), the live MCP consumer index, and the flat
//! `dist/llms.txt` export. Before #1027 these were three independently-written
//! renderers that had silently diverged (`⊑` vs `subClassOf`, `→` vs `->`, a
//! three-line header vs a blockquote). This module is the ONE source of truth for
//! the format so they cannot diverge again: each surface builds a neutral list of
//! [`LlmsSection`]s from its own term model and hands them to [`render_index`].
//! The only thing that varies across surfaces is the per-bullet [`LlmsBullet::url`]
//! (present for a markdown link into a published site, `None` for a linkless
//! self-contained dump).

/// The canonical one-paragraph vocabulary summary — the `llms.txt` blockquote
/// body WITHOUT the leading `> `. The single source of truth shared by every
/// `llms.txt`-family surface (was duplicated in three renderers before #1027).
pub const GMEOW_SUMMARY: &str = "A reasoning-centric, OWL 2 DL, gUFO-grounded super-vocabulary that unifies a person's or organization's digital existence (entities, contacts, email, trust/keys, time) and aligns it to schema.org, FOAF, PROV, the WOT schema, Wikidata, and more.";

/// The maximum number of characters of a bullet note in the link-INDEX form
/// (`llms.txt`). The COMPLETE form (`llms-full.txt`) inlines content and does not
/// truncate. A fixed cap (no configurable knob — project no-optionality doctrine).
pub const LLMS_NOTE_CAP: usize = 200;

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

/// Render a complete llmstxt.org index document: the header (with the canonical
/// summary blockquote) followed by each section's `## ` heading and bullets.
pub fn render_index(title: &str, prose: &[String], sections: &[LlmsSection]) -> String {
    let mut out = llms_header(title, prose);
    for section in sections {
        out.push_str("## ");
        out.push_str(&section.heading);
        out.push_str("\n\n");
        for bullet in &section.bullets {
            out.push_str(&render_bullet(bullet));
            out.push('\n');
        }
        out.push('\n');
    }
    out
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
