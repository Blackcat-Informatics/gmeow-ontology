// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    let body = render_card_body(&sample(), CardDetail::Standard);
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
    let body = render_card_body(&card, CardDetail::Standard);
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
    let body = render_card_body(&card, CardDetail::Standard);
    assert!(
        !body.contains("- slice:"),
        "None slice must omit the line, not blank it"
    );
}

#[test]
fn render_card_prepends_h1_title() {
    let card = sample();
    let full = render_card("gmeow:Foo (⊑ Bar)", &card, CardDetail::Standard);
    assert!(full.starts_with("# gmeow:Foo (⊑ Bar)\n\n"));
    assert!(full.contains("- category: Class\n"));
    assert_eq!(
        full,
        format!(
            "# gmeow:Foo (⊑ Bar)\n\n{}",
            render_card_body(&card, CardDetail::Standard)
        )
    );
}

/// A card carrying every rich panel, for the tier-gating tests.
fn full_sample() -> Card {
    Card {
        entailments: vec![CardEntailment {
            rule: "subClassOf-transitivity".to_string(),
            conclusion: "Foo ⊑ Qux".to_string(),
            premises: vec!["Foo ⊑ Bar".to_string(), "Bar ⊑ Qux".to_string()],
        }],
        fixtures_do: vec![CardFixture {
            title: "Well-formed Foo".to_string(),
            body: "a valid foo shape".to_string(),
        }],
        fixtures_dont: vec![CardFixture {
            title: "Foo missing range".to_string(),
            body: "violates the range requirement".to_string(),
        }],
        diagnostics: vec![CardDiagnostic {
            code: "gmeow-range-missing".to_string(),
            note: "has 1 diagnostic finding(s)".to_string(),
        }],
        loss: vec![CardLoss {
            target: "owl".to_string(),
            preservation: "Weakened".to_string(),
        }],
        ..sample()
    }
}

#[test]
fn summary_is_definition_only_and_smaller_than_standard() {
    let card = full_sample();
    let summary = render_card_body(&card, CardDetail::Summary);
    // Definition present…
    assert!(summary.contains("A foo."));
    // …but NONE of the header / advisory / panel surface.
    assert!(!summary.contains("- category:"));
    assert!(!summary.contains("**Parents:**"));
    assert!(!summary.contains("## Entailments"));
    let standard = render_card_body(&card, CardDetail::Standard);
    assert!(summary.len() < standard.len());
}

#[test]
fn standard_carries_no_full_panels_but_full_does() {
    let card = full_sample();
    let standard = render_card_body(&card, CardDetail::Standard);
    // Standard is EXACTLY the compact card: no rich-panel headers.
    assert!(!standard.contains("## Entailments"));
    assert!(!standard.contains("## Do"));
    assert!(!standard.contains("## Don't"));
    assert!(!standard.contains("## Diagnostics"));
    assert!(!standard.contains("## Degrades under projection"));

    let full = render_card_body(&card, CardDetail::Full);
    // Full is a superset: the whole compact body PLUS every panel.
    assert!(full.starts_with(&standard));
    assert!(full.len() > standard.len());
    assert!(full.contains("## Entailments\n\n- **subClassOf-transitivity** ⊢ Foo ⊑ Qux\n"));
    assert!(full.contains("  - premises: Foo ⊑ Bar; Bar ⊑ Qux\n"));
    assert!(full.contains("## Do\n\n- **Well-formed Foo** — a valid foo shape\n"));
    assert!(full.contains("## Don't\n\n- **Foo missing range**"));
    assert!(full.contains("## Diagnostics\n\n- **gmeow-range-missing**"));
    assert!(full.contains("## Degrades under projection\n\n- owl — Weakened\n"));
}

#[test]
fn empty_panels_are_omitted_at_full_tier() {
    // A card with NO rich panels renders identically at Full and Standard —
    // honest empty sections are omitted, never fabricated.
    let card = sample();
    assert_eq!(
        render_card_body(&card, CardDetail::Full),
        render_card_body(&card, CardDetail::Standard)
    );
}

#[test]
fn projected_json_tiers_are_strictly_nested() {
    let card = full_sample();
    let summary = serde_json::to_string(&card.projected(CardDetail::Summary)).unwrap();
    let standard = serde_json::to_string(&card.projected(CardDetail::Standard)).unwrap();
    let full = serde_json::to_string(&card.projected(CardDetail::Full)).unwrap();
    // Standard JSON MUST NOT carry any full-tier rich key.
    assert!(!standard.contains("entailments"));
    assert!(!standard.contains("fixtures_do"));
    assert!(!standard.contains("diagnostics"));
    // Summary carries identity + definition but no advisory field.
    assert!(summary.contains("\"definition\":\"A foo.\""));
    assert!(!summary.contains("use_when"));
    // Full carries the rich panels.
    assert!(full.contains("\"entailments\""));
    assert!(full.contains("\"loss\""));
    // Byte-stable across two serializations.
    assert_eq!(
        full,
        serde_json::to_string(&card.projected(CardDetail::Full)).unwrap()
    );
    // Monotone by size.
    assert!(summary.len() <= standard.len());
    assert!(standard.len() < full.len());
}

#[test]
fn python_model_path_and_snippet_route_through_the_emitter() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/lifecycle";
    let term = "https://blackcatinformatics.ca/gmeow/Foo";
    assert_eq!(
        python_model_path(slice, term),
        "gmeow_models.lifecycle.Foo",
        "the dotted path is gmeow_models.<slice>.<Class>"
    );
    let snippet = python_model_snippet(slice, term, "gmeow:Foo");
    assert_eq!(
        snippet,
        "from gmeow_models.lifecycle import Foo\n\
             obj = Foo.model_validate({\"@type\": \"gmeow:Foo\"})"
    );
}

#[test]
fn class_card_carries_python_model_link_and_snippet() {
    let slice = "https://blackcatinformatics.ca/gmeow/slices/lifecycle";
    let term = "https://blackcatinformatics.ca/gmeow/Foo";
    let card = Card {
        python_model: Some(python_model_path(slice, term)),
        python_snippet: Some(python_model_snippet(slice, term, "gmeow:Foo")),
        ..sample()
    };

    // Standard body renders the explicit link + the fenced snippet.
    let standard = render_card_body(&card, CardDetail::Standard);
    assert!(standard.contains("**Python model:** `gmeow_models.lifecycle.Foo`\n\n"));
    assert!(
        standard.contains("```python\nfrom gmeow_models.lifecycle import Foo\n"),
        "the compact card carries the fenced Pydantic snippet"
    );
    // Full carries it too (it is a superset of Standard).
    let full = render_card_body(&card, CardDetail::Full);
    assert!(full.contains("**Python model:** `gmeow_models.lifecycle.Foo`"));

    // Summary drops the model surface entirely.
    let summary = render_card_body(&card, CardDetail::Summary);
    assert!(!summary.contains("Python model"));

    // JSON: Standard carries both fields; Summary drops them.
    let standard_json = serde_json::to_string(&card.projected(CardDetail::Standard)).unwrap();
    assert!(standard_json.contains("\"python_model\":\"gmeow_models.lifecycle.Foo\""));
    assert!(standard_json.contains("\"python_snippet\":"));
    let summary_json = serde_json::to_string(&card.projected(CardDetail::Summary)).unwrap();
    assert!(!summary_json.contains("python_model"));
    assert!(!summary_json.contains("python_snippet"));

    // A non-class card carries neither field, so no Python section renders.
    let plain = Card {
        category: "Property".to_string(),
        ..sample()
    };
    assert!(!render_card_body(&plain, CardDetail::Standard).contains("Python model"));
}

#[test]
fn toon_scalar_str_escapes_carriage_return_and_backslash() {
    // A lone `\r` and a `\` must both force quoting AND be escaped —
    // otherwise TOON's line-oriented, indentation-based format would
    // either emit a raw CR inside a bare token or leave a backslash
    // ambiguous with the escape sequences that follow it.
    assert_eq!(
        toon_scalar_str("line1\rline2\\tail"),
        "\"line1\\rline2\\\\tail\""
    );
}
