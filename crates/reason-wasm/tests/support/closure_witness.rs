// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Pure native computation of the small, explicit browser closure witness.

/// A self-contained EDB whose structured-DL closure is non-empty: an RDFS subclass
/// axiom + a typed individual entails the individual's membership in the superclass.
pub const INPUT: &str = "@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
     @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
     @prefix ex: <https://example.org/> .\n\
     ex:Cat rdfs:subClassOf ex:Animal .\n\
     ex:Animal rdfs:subClassOf ex:Organism .\n\
     ex:felix rdf:type ex:Cat .\n";

/// The exact pipeline the wasm `reason(data, format)` shim runs.
pub fn reason_artifact(data: &str, format: &str) -> String {
    gmeow_reason_wasm::reason(data, format)
        .expect("the tiny caller-owned theory publishes a complete admitted artifact")
}

/// Separate the canonical inline GTS from its response without reserializing the envelope.
pub fn split_artifact(
    response: &str,
    artifact: &serde_json::Value,
) -> gmeow_errors::Result<(String, Vec<u8>)> {
    let encoded = artifact["output"]
        .as_str()
        .ok_or_else(|| std::io::Error::other("reason witness is missing its artifact output"))?;
    let bytes = purrdf::xsd::parse_base64(encoded).map_err(|error| {
        std::io::Error::other(format!("invalid reason witness base64: {error}"))
    })?;
    if artifact["format"] != "gts"
        || artifact["encoding"] != "base64"
        || artifact["bytes"].as_u64() != u64::try_from(bytes.len()).ok()
        || encoded.is_empty()
        || purrdf::xsd::canonical_base64(&bytes) != encoded
    {
        return Err(std::io::Error::other("reason witness requires a canonical inline GTS").into());
    }
    let field = format!(r#""output":"{encoded}""#);
    let (before, after) = response
        .split_once(&field)
        .filter(|(_, after)| !after.contains(&field))
        .ok_or_else(|| {
            std::io::Error::other("reason witness has no unique artifact output field")
        })?;
    Ok((format!(r#"{before}"output":""{after}"#), bytes))
}
