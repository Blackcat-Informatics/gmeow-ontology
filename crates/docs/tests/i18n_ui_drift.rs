// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! UI-string single-source-of-truth drift guard.
//!
//! `UI_TEMPLATES` is the ONE source of the documentation UI-chrome strings: the
//! renderer reads it (via `ui_default`/`ui_string`) and the native `.pot`
//! extraction (`i18n_extract`) reads the very same table directly in Rust — there
//! is no second copy to diverge from. These tests pin that contract:
//!
//! 1. the table itself stays well-formed (sorted, unique, non-empty, the expected
//!    key count) — so every consumer sees a clean single source;
//! 2. per-language override catalogs cannot silently drift — a translated key that
//!    is not in `UI_TEMPLATES` (a phantom key) or an override for an undeclared
//!    language is caught, not silently ignored at render time;
//! 3. overrides are language-scoped and English is NEVER overridden (its strings
//!    always come straight from the single source).

use std::collections::BTreeSet;

use serde_json::json;

use gmeow_docs::i18n::{ENGLISH, UI_TEMPLATES};
use gmeow_docs::{UiCatalog, ui_string};

mod common;

/// The set of canonical UI-chrome keys (the single source).
fn template_keys() -> BTreeSet<&'static str> {
    UI_TEMPLATES.iter().map(|(k, _)| *k).collect()
}

/// Build a `UiCatalog` from `(lang, key, value)` triples without touching the
/// filesystem — `UiCatalog` round-trips through its flat serde DTO.
fn catalog_from(entries: &[(&str, &str, &str)]) -> UiCatalog {
    let overrides: Vec<_> = entries
        .iter()
        .map(|(lang, key, value)| json!({ "lang": lang, "key": key, "value": value }))
        .collect();
    serde_json::from_value(json!({ "overrides": overrides }))
        .expect("synthetic UiCatalog must deserialize")
}

/// Enumerate the `(lang, key)` overrides a catalog carries, via its serde shape
/// (the `overrides` field is private to the i18n module).
fn override_pairs(catalog: &UiCatalog) -> Vec<(String, String)> {
    let value = serde_json::to_value(catalog).expect("UiCatalog must serialize");
    value["overrides"]
        .as_array()
        .expect("overrides must be an array")
        .iter()
        .map(|entry| {
            (
                entry["lang"].as_str().unwrap().to_string(),
                entry["key"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn ui_templates_is_a_well_formed_single_source() {
    // Both the renderer and the native `.pot` extraction read THIS table; if it
    // is clean, the single source is clean for every consumer.
    assert_eq!(UI_TEMPLATES.len(), 219, "UI-template key count changed");

    let mut keys: Vec<&str> = UI_TEMPLATES.iter().map(|(k, _)| *k).collect();
    let unsorted = keys.clone();
    keys.sort_unstable();
    assert_eq!(
        unsorted, keys,
        "UI_TEMPLATES must stay sorted by key (the table documents this for determinism)"
    );

    let unique: BTreeSet<&str> = keys.iter().copied().collect();
    assert_eq!(unique.len(), keys.len(), "UI_TEMPLATES has a duplicate key");

    for (key, value) in UI_TEMPLATES {
        assert!(
            !value.trim().is_empty(),
            "UI key `{key}` has an empty value"
        );
    }
}

#[test]
fn phantom_override_key_is_detected() {
    // A catalog that translates a real key (`nav_home`) plus one that no longer
    // exists upstream (`nav_phantom`). The drift check must flag exactly the
    // phantom — proving the live guard below is not vacuous.
    let catalog = catalog_from(&[
        ("fr", "nav_home", "Accueil"),
        ("fr", "nav_phantom", "Fantôme"),
    ]);
    let keys = template_keys();
    let phantoms: Vec<String> = override_pairs(&catalog)
        .into_iter()
        .filter(|(_, key)| !keys.contains(key.as_str()))
        .map(|(_, key)| key)
        .collect();
    assert_eq!(phantoms, vec!["nav_phantom".to_string()]);

    // The legitimate override still resolves through the single source.
    assert_eq!(ui_string("nav_home", "fr", &catalog), "Accueil");
}

#[test]
fn overrides_are_language_scoped_and_english_is_never_overridden() {
    let catalog = catalog_from(&[
        ("fr", "nav_home", "Accueil"),
        // An override deliberately keyed on English must be ignored: English
        // strings are owned by UI_TEMPLATES, the single source.
        (ENGLISH, "nav_home", "NOT THE SOURCE"),
    ]);
    // The French override applies for French …
    assert_eq!(ui_string("nav_home", "fr", &catalog), "Accueil");
    // … does not leak to another language …
    assert_eq!(ui_string("nav_home", "zh", &catalog), "Home");
    // … and English always comes from the single source, never an override.
    assert_eq!(ui_string("nav_home", ENGLISH, &catalog), "Home");
}

#[test]
fn live_ui_overrides_target_known_keys_and_declared_languages() {
    // The forward drift gate over the SHIPPED catalogs. Empty today (no
    // `i18n/ontology-docs-templates.<lang>.po` exist yet); it becomes a real gate
    // the moment a translator adds one, keeping the single source authoritative.
    let model = common::cached_model();
    let keys = template_keys();
    let declared: BTreeSet<&str> = model
        .available_languages
        .iter()
        .map(String::as_str)
        .collect();

    for (lang, key) in override_pairs(&model.ui_catalog) {
        assert!(
            keys.contains(key.as_str()),
            "phantom UI key `{key}` in the `{lang}` override catalog (not in UI_TEMPLATES)"
        );
        assert_ne!(
            lang, ENGLISH,
            "English UI strings must come from UI_TEMPLATES, not an override"
        );
        assert!(
            declared.contains(lang.as_str()),
            "UI override targets undeclared language `{lang}`"
        );
    }
}
