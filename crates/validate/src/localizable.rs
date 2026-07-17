// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single authority for the **localizable-predicate set** — the vocabulary
//! surface whose literal objects carry natural-language text.
//!
//! Exactly one enumeration lives here. Three policies derive from it, and none
//! keeps a parallel copy:
//! * the i18n gettext extraction/`.po` surface (`gmeow-docs`, re-exported below);
//! * the Check-2 external-language-tag policy ([`crate::lint::default_annotation_predicates`]),
//!   which forbids an external (`@en`) tag where the internal `x-gmeow-` private-use
//!   tag is required — the GMEOW-namespace members are handled by Check-1's own
//!   namespace guard, so their presence here is a harmless no-op for Check-2;
//! * the authoring language-tag-presence gate over slice/authored source files
//!   ([`crate::authoring_integrity`]), which requires every such literal to carry
//!   *some* tag.
//!
//! `crates/validate` is the lowest crate every consumer can reach (`gmeow-docs`,
//! `gmeow-pipeline`, `gmeow-slice-quality` depend on it directly; `gmeow-slice-brief`
//! reaches it through the `gmeow-docs` re-export), so the authority lives here and
//! nowhere above it.

/// Full IRIs of the predicates whose literal objects carry natural-language,
/// localizable text. This is the single source of truth; do not re-enumerate it.
pub const LOCALIZABLE_PREDICATES: &[&str] = &[
    "http://www.w3.org/2000/01/rdf-schema#label",
    "http://www.w3.org/2000/01/rdf-schema#comment",
    "http://www.w3.org/2004/02/skos/core#definition",
    "http://www.w3.org/2004/02/skos/core#scopeNote",
    "http://www.w3.org/2004/02/skos/core#example",
    "http://www.w3.org/2004/02/skos/core#prefLabel",
    "http://www.w3.org/2004/02/skos/core#altLabel",
    "http://www.w3.org/2004/02/skos/core#note",
    "http://purl.org/dc/terms/title",
    "http://purl.org/dc/terms/description",
    "https://blackcatinformatics.ca/gmeow/name",
    "https://blackcatinformatics.ca/gmeow/title",
    "https://blackcatinformatics.ca/gmeow/description",
    "https://blackcatinformatics.ca/gmeow/fullName",
];

#[cfg(test)]
mod tests {
    use super::LOCALIZABLE_PREDICATES;

    /// A botched relocation that drops entries fails immediately.
    #[test]
    fn authority_pins_the_full_localizable_surface() {
        assert_eq!(
            LOCALIZABLE_PREDICATES.len(),
            14,
            "the localizable authority must carry all 14 predicates"
        );
        // No duplicates snuck in.
        let mut sorted = LOCALIZABLE_PREDICATES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), LOCALIZABLE_PREDICATES.len());
    }
}
