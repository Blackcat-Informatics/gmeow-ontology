// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Real payload composition over tiny admitted RDF, without a server or dictionary load.

use super::*;
use gmeow_lang_bridge::GmnGlyphRegistry;

const TINY_REGISTRY: &str = include_str!("../../../lang-bridge/tests/fixtures/glyph-legend.ttl");

#[test]
fn json_glyph_legend_mcp_composition_pins_complete_envelope_order_and_shared_rows() {
    let dataset = purrdf::parse_dataset(TINY_REGISTRY.as_bytes(), "text/turtle", None).unwrap();
    let registry = GmnGlyphRegistry::from_dataset(&dataset).unwrap();
    let expected = r#"{"legend":[{"glyph":"+","tokenCost":1},{"glyph":"¬","tokenCost":1},{"glyph":"⊑","tokenCost":3}],"ok":true}"#;
    assert_eq!(response(&registry).unwrap(), expected);
    let payload: purrdf_lex::json::Value =
        purrdf_lex::json::record::from_slice(expected.as_bytes()).unwrap();
    assert_eq!(
        payload["legend"],
        gmeow_lang_bridge::glyph_legend_value(&registry).unwrap()
    );
    assert_eq!(payload["ok"].as_bool(), Some(true));
}

#[test]
fn json_glyph_legend_mcp_empty_registry_retains_explicit_success_and_empty_array() {
    let empty = TINY_REGISTRY.split_once("# Executable bindings").unwrap().0;
    let dataset = purrdf::parse_dataset(empty.as_bytes(), "text/turtle", None).unwrap();
    let registry = GmnGlyphRegistry::from_dataset(&dataset).unwrap();
    assert_eq!(response(&registry).unwrap(), r#"{"legend":[],"ok":true}"#);
}

#[test]
fn json_glyph_legend_mcp_missing_pin_keeps_shared_typed_failure() {
    let unpinned = TINY_REGISTRY.replace("U+2291", "U+2297");
    let dataset = purrdf::parse_dataset(unpinned.as_bytes(), "text/turtle", None).unwrap();
    let registry = GmnGlyphRegistry::from_dataset(&dataset).unwrap();
    let error = response(&registry).unwrap_err();
    assert_eq!(
        error.code(),
        gmeow_lang_bridge::error::GmnUnpinnedGlyphCost::register()
    );
    assert!(error.is::<gmeow_lang_bridge::error::GmnUnpinnedGlyphCost>());
    assert!(error.to_string().contains('⊗'));
}
