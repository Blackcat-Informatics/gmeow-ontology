// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Integrity gate for the complete grounding/math target-language carriers.

use std::collections::BTreeSet;
use std::path::PathBuf;

use gmeow_docs::i18n_compile::parse_po;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root canonicalizes")
}

fn entries(lang: &str) -> Vec<gmeow_docs::i18n_compile::PoEntry> {
    let path = repo_root()
        .join("slices/grounding/math/i18n")
        .join(format!("{lang}.po"));
    parse_po(
        &std::fs::read_to_string(path).expect("read math catalog"),
        true,
    )
    .expect("math catalog parses")
}

#[test]
fn math_catalogs_are_complete_parallel_and_not_hybrid_english() {
    let fr = entries("fr");
    let zh = entries("zh");
    assert_eq!(fr.len(), 1184, "fr-CA label+definition population");
    assert_eq!(zh.len(), 1184, "zh-Hans label+definition population");

    let fr_keys: BTreeSet<_> = fr.iter().map(|e| e.msgctxt.as_str()).collect();
    let zh_keys: BTreeSet<_> = zh.iter().map(|e| e.msgctxt.as_str()).collect();
    assert_eq!(fr_keys.len(), fr.len(), "fr-CA contexts are unique");
    assert_eq!(zh_keys.len(), zh.len(), "zh-Hans contexts are unique");
    assert_eq!(
        fr_keys, zh_keys,
        "the two catalogs cover the same identities"
    );

    for (locale, catalog) in [("fr", &fr), ("zh", &zh)] {
        for entry in catalog {
            assert!(
                !entry.msgstr.trim().is_empty(),
                "{} is translated",
                entry.msgctxt
            );
            assert_ne!(
                entry.msgstr, entry.msgid,
                "{} is not copied English",
                entry.msgctxt
            );
            let (term, predicate) = entry
                .msgctxt
                .split_once('|')
                .expect("context has term and predicate");
            let local = term.rsplit('/').next().expect("term local name");
            let anchor = format!("math:{local}");
            assert!(
                entry.msgstr.contains(&anchor),
                "{locale} {} retains its canonical CURIE anchor",
                entry.msgctxt
            );

            if predicate == "skos:definition" {
                if locale == "fr" {
                    assert!(entry.msgstr.starts_with("Le terme math:"));
                    let normalized = entry.msgstr.to_ascii_lowercase();
                    for english in [" the ", " and ", " whose ", " with ", " rather than "] {
                        assert!(
                            !normalized.contains(english),
                            "fr-CA {} contains hybrid English token {english:?}",
                            entry.msgctxt
                        );
                    }
                } else {
                    let han = entry
                        .msgstr
                        .chars()
                        .filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c))
                        .count();
                    assert!(
                        han >= 35,
                        "zh-Hans {} carries substantive Han prose",
                        entry.msgctxt
                    );
                }
            }
        }
    }
}
