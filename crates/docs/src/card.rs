// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ONE canonical term-card renderer (#1027, §19 one-path).
//!
//! A GMEOW term card is a compact, link-free, prompt-ready Markdown block: a
//! metadata header, the definition, and every usage-advisory field. Before this
//! module two divergent renderers emitted "the same card" — the docs-site
//! `term_body` (`crates/docs/src/render.rs`) and the folded-snapshot
//! `term_card_lines` (`crates/pipeline/src/stages/export.rs`) — with different
//! conventions (bold vs italic labels, `; ` vs `, ` delimiters, backticks or
//! not). That violated §19 (one-path) and P15 (maximal information flow): the
//! MCP card claimed to be "the live twin" of the site card while rendering it
//! differently.
//!
//! This module is the single source of truth. Both crates build a neutral
//! plain-data [`Card`] (values are PRE-RESOLVED display strings — the caller
//! does local-name/CURIE resolution) and call [`render_card_body`] /
//! [`render_card`]. The canonical convention is the SITE card's: **bold**
//! labels, `; ` value delimiters, NO per-item backticks, and a metadata header.
//!
//! Pure / std-only so it has no I/O or graph dependency.

/// The full UNION of fields both the docs-site `DocTerm` and the folded `Term`
/// can carry for one term. Every value is a pre-resolved DISPLAY string (the
/// caller resolves IRIs to local names / CURIEs); the renderer never touches
/// IRIs. Empty `Vec`s / empty `String`s / `None`s are omitted from the output.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Card {
    /// The vocabulary category, singular and human-cased (`"Class"`,
    /// `"Property"`, `"Individual"`, `"Datatype"`, `"Term"`).
    pub category: String,
    /// The full term IRI.
    pub iri: String,
    /// `rdfs:label`, omitted from the header when it equals the title/CURIE.
    pub label: Option<String>,
    /// The defining slice as a display string (the owning module's local name).
    /// Both sources carry it: the docs side from `DocTerm::owner_slice`, the
    /// folded/MCP side from the documentation graph's `gmeow:docOwnerSlice`.
    /// `None` only when the source genuinely has no slice for this term, and is
    /// rendered as no `slice:` header line — NEVER a blank value.
    pub slice: Option<String>,
    /// `gmeow:graphBoxRole` four-boxes role display names (e.g. `boxTBox`).
    pub box_roles: Vec<String>,
    /// `skos:definition` (falling back to `rdfs:comment`), one-lined.
    pub definition: Option<String>,
    /// `rdfs:subClassOf` / `rdfs:subPropertyOf` parent display names.
    pub parents: Vec<String>,
    /// `rdfs:domain` display names.
    pub domain: Vec<String>,
    /// `rdfs:range` display names.
    pub range: Vec<String>,
    /// `gmeow:useWhen` prose.
    pub use_when: Vec<String>,
    /// `gmeow:avoidWhen` prose.
    pub avoid_when: Vec<String>,
    /// `gmeow:howToUse` prose.
    pub how_to_use: Vec<String>,
    /// `skos:scopeNote` prose.
    pub scope_notes: Vec<String>,
    /// `skos:example` prose.
    pub examples: Vec<String>,
    /// `logic:*` stereotype CURIEs.
    pub logic_stereotypes: Vec<String>,
    /// Related-term display names (`skos:related` ∪ `gmeow:pairsWith` ∪
    /// `rdfs:seeAlso`).
    pub related_terms: Vec<String>,
    /// `gmeow:useForConsumer` profile display names.
    pub use_for_consumer: Vec<String>,
    /// `gmeow:avoidForConsumer` profile display names.
    pub avoid_for_consumer: Vec<String>,
    /// Alignment facets, each a pre-formatted `predicate=object` display string.
    pub aligns: Vec<String>,
}

/// Render the card BODY (metadata header + definition + every advisory field, NO
/// heading) — the shared core of the per-term `card.md` and the inlined
/// `llms-full.txt` block. Deterministic field order; a field is emitted only
/// when non-empty.
pub fn render_card_body(card: &Card) -> String {
    let mut out = String::new();

    // ── Metadata header ──────────────────────────────────────────────────────
    out.push_str(&format!(
        "- category: {}\n- iri: {}\n",
        card.category, card.iri
    ));
    if let Some(slice) = &card.slice {
        out.push_str(&format!("- slice: {slice}\n"));
    }
    if let Some(label) = &card.label {
        out.push_str(&format!("- label: {label}\n"));
    }
    if let Some(box_role) = card.box_roles.first() {
        out.push_str(&format!("- box: {box_role}\n"));
    }
    out.push('\n');

    // ── Definition ───────────────────────────────────────────────────────────
    if let Some(def) = &card.definition {
        if !def.is_empty() {
            out.push_str(def);
            out.push_str("\n\n");
        }
    }

    // ── Advisory fields (one canonical convention: **bold**, `; ` delimiter) ──
    let mut field = |label: &str, values: &[String]| {
        if !values.is_empty() {
            out.push_str(&format!("**{label}:** {}\n\n", values.join("; ")));
        }
    };
    field("Parents", &card.parents);
    field("Domain", &card.domain);
    field("Range", &card.range);
    field("Box roles", &card.box_roles);
    field("Use when", &card.use_when);
    field("Avoid when", &card.avoid_when);
    field("How to use", &card.how_to_use);
    field("Use for consumers", &card.use_for_consumer);
    field("Avoid for consumers", &card.avoid_for_consumer);
    field("Scope notes", &card.scope_notes);
    field("Examples", &card.examples);
    field("Logic", &card.logic_stereotypes);
    field("Related", &card.related_terms);
    field("Aligns", &card.aligns);

    out
}

/// Render a complete, standalone card: `# {title}\n\n` followed by
/// [`render_card_body`]. Used for the per-term `card.md` file and the live MCP
/// `gmeow_doc_card`.
pub fn render_card(title: &str, card: &Card) -> String {
    format!("# {title}\n\n{}", render_card_body(card))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Card {
        Card {
            category: "Class".to_string(),
            iri: "https://blackcatinformatics.ca/gmeow/Foo".to_string(),
            label: Some("Foo".to_string()),
            slice: Some("demo".to_string()),
            box_roles: vec!["boxTBox".to_string()],
            definition: Some("A foo.".to_string()),
            parents: vec!["Bar".to_string(), "Baz".to_string()],
            use_when: vec!["When you have a foo.".to_string()],
            use_for_consumer: vec!["gmeow:profileMemory".to_string()],
            aligns: vec!["exactMatch=ex:Foo".to_string()],
            ..Default::default()
        }
    }

    #[test]
    fn body_emits_metadata_header_then_definition_then_advisories() {
        let body = render_card_body(&sample());
        // Header lines, in canonical order.
        assert!(body.starts_with(
            "- category: Class\n- iri: https://blackcatinformatics.ca/gmeow/Foo\n- slice: demo\n- label: Foo\n- box: boxTBox\n\n"
        ));
        // Definition.
        assert!(body.contains("\nA foo.\n\n"));
        // Bold labels + `; ` delimiter, NO backticks.
        assert!(body.contains("**Parents:** Bar; Baz\n\n"));
        assert!(body.contains("**Use when:** When you have a foo.\n\n"));
        assert!(body.contains("**Use for consumers:** gmeow:profileMemory\n\n"));
        assert!(body.contains("**Aligns:** exactMatch=ex:Foo\n\n"));
        assert!(
            !body.contains('`'),
            "canonical card uses no per-item backticks"
        );
        // Labels are bold (`**`), never the folded side's single-asterisk italic
        // (`*Use when:* …`). A bold `**Use when:** ` is fine; assert no italic
        // label survives — i.e. no `\n*Use when:* ` (single asterisk after a newline).
        assert!(
            !body.contains("\n*Use when:* "),
            "labels must be bold (**), never single-italic"
        );
    }

    #[test]
    fn empty_fields_are_omitted() {
        let card = Card {
            category: "Individual".to_string(),
            iri: "https://blackcatinformatics.ca/gmeow/Bar".to_string(),
            ..Default::default()
        };
        let body = render_card_body(&card);
        assert_eq!(
            body,
            "- category: Individual\n- iri: https://blackcatinformatics.ca/gmeow/Bar\n\n"
        );
        // No slice / label / box header line when those are absent.
        assert!(!body.contains("- slice:"));
        assert!(!body.contains("- label:"));
        assert!(!body.contains("- box:"));
    }

    #[test]
    fn slice_none_never_emits_blank_slice() {
        let card = Card {
            category: "Class".to_string(),
            iri: "x".to_string(),
            slice: None,
            ..Default::default()
        };
        let body = render_card_body(&card);
        assert!(
            !body.contains("- slice:"),
            "None slice must omit the line, not blank it"
        );
    }

    #[test]
    fn render_card_prepends_h1_title() {
        let card = sample();
        let full = render_card("gmeow:Foo (⊑ Bar)", &card);
        assert!(full.starts_with("# gmeow:Foo (⊑ Bar)\n\n"));
        assert!(full.contains("- category: Class\n"));
        assert_eq!(
            full,
            format!("# gmeow:Foo (⊑ Bar)\n\n{}", render_card_body(&card))
        );
    }
}
