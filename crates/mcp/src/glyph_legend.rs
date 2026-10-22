// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Actual tool payload composition over an already-admitted glyph registry.

/// Build the compact legend/ok payload without parsing the legend's String exit.
///
/// # Errors
/// Retains the shared builder's named missing-pin diagnostic unchanged.
pub(crate) fn response(
    registry: &gmeow_lang_bridge::GmnGlyphRegistry,
) -> gmeow_errors::Result<String> {
    let legend = gmeow_lang_bridge::glyph_legend_value(registry)?;
    let payload = purrdf_lex::json::Object::new()
        .with("legend", legend)
        .with("ok", true);
    Ok(purrdf_lex::json::write_compact(&payload.into()))
}

#[cfg(test)]
mod tests;
