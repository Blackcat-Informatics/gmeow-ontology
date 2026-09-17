// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
