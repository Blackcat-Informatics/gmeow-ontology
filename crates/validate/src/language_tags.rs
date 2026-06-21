// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Language-tag policy core: the Rust authority for the ``x-gmeow-*`` private-use
//! tag discipline and the ``gmeow:Language`` → BCP-47 mapping.
//!
//! Principle 4 (one canonical source) + Principle 9 (co-equal, non-privileged
//! facets): canonical authored literals carry internal ``x-gmeow-*`` tags; public
//! projections emit BCP-47.  All policy logic lives here; the Python
//! ``language_tags`` module routes through these functions.

use std::collections::{BTreeSet, HashMap, HashSet};

use oxigraph::io::{RdfFormat, RdfParser};
use oxigraph::model::{NamedOrBlankNode, Term as OxTerm};

use gmeow_rdf::{BlankScope, RdfDatasetBuilder, RdfLiteral, TermRef};

/// The GMEOW namespace prefix for term IRIs.
const NAMESPACE: &str = "https://blackcatinformatics.ca/gmeow/";
/// RDF type predicate IRI.
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";

/// Return whether `lang` is a GMEOW internal private-use tag (``x-gmeow-*``).
///
/// The pattern is ``^x-gmeow-[a-z0-9\-]+$``, matched case-insensitively.
pub fn is_internal_tag(lang: &str) -> bool {
    let lower = lang.to_lowercase();
    if !lower.starts_with("x-gmeow-") {
        return false;
    }
    let suffix = &lower["x-gmeow-".len()..];
    !suffix.is_empty()
        && suffix
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
}

/// The shared language-preference sort key.
///
/// Returns ``(0, lang.lower())`` for ``x-gmeow-english`` and
/// ``(1, lang.lower())`` for everything else, so the carrier language wins
/// deterministically in multilingual sorts.
pub fn rank_language(lang: &str) -> (u8, String) {
    let lower = lang.to_lowercase();
    let rank = if lower == "x-gmeow-english" { 0 } else { 1 };
    (rank, lower)
}

/// Parse `rdf_bytes` in `format` and build a mapping from GMEOW internal
/// language tag to BCP-47 tag.
///
/// Scans individuals typed ``gmeow:Language``, ``gmeow:FormalLanguage``, and
/// ``gmeow:ProgrammingLanguage`` for ``gmeow:languageTag`` and
/// ``gmeow:bcp47Tag`` property values.
///
/// Each property must have exactly one distinct lexical value per individual:
/// - Missing either property → individual is silently skipped.
/// - More than one distinct value for either property → returns `Err`.
///
/// Returns ``{internal_tag: bcp47_tag}``.
pub fn load_tag_map(rdf_bytes: &[u8], format: &str) -> Result<HashMap<String, String>, String> {
    let ox_format = parse_format(format)?;

    // Ingest via the gmeow-rdf IR builder (same pattern as turtle_normalize.rs).
    let mut builder = RdfDatasetBuilder::new();
    let parser = RdfParser::from_format(ox_format)
        .lenient()
        .for_reader(rdf_bytes);
    for quad_result in parser {
        let quad = quad_result.map_err(|e| format!("RDF parse error: {e}"))?;
        let s = intern_named_or_blank_node(&mut builder, &quad.subject);
        let p = builder.intern_iri(quad.predicate.as_str().to_owned());
        let o = intern_ox_term(&mut builder, &quad.object)?;
        builder.push_quad(s, p, o, None);
    }
    let dataset = builder.freeze().map_err(|e| e.to_string())?;

    build_tag_map(&dataset)
}

/// Build the tag map from an already-frozen `RdfDataset`.
///
/// Extracted for testability.
fn build_tag_map(dataset: &gmeow_rdf::RdfDataset) -> Result<HashMap<String, String>, String> {
    let lang_class = format!("{NAMESPACE}Language");
    let formal_class = format!("{NAMESPACE}FormalLanguage");
    let prog_class = format!("{NAMESPACE}ProgrammingLanguage");
    let tag_prop = format!("{NAMESPACE}languageTag");
    let bcp_prop = format!("{NAMESPACE}bcp47Tag");

    // Collect the string-form subjects that are typed as a language class.
    // We use string matching via quad_refs() since TermId is crate-private.
    let mut lang_subjects: HashSet<String> = HashSet::new();
    for qr in dataset.quad_refs() {
        if let (TermRef::Iri(p), TermRef::Iri(o)) = (qr.p, qr.o) {
            if p == RDF_TYPE && (o == lang_class || o == formal_class || o == prog_class) {
                if let TermRef::Iri(s) = qr.s {
                    lang_subjects.insert(s.to_owned());
                }
            }
        }
    }

    // For each subject IRI, collect distinct literal values for both properties.
    // Index: subject_iri → (prop_iri → set_of_lexical_values)
    let mut props: HashMap<String, HashMap<String, BTreeSet<String>>> = HashMap::new();
    for qr in dataset.quad_refs() {
        if let TermRef::Iri(s) = qr.s {
            if !lang_subjects.contains(s) {
                continue;
            }
            if let TermRef::Iri(p) = qr.p {
                if p == tag_prop || p == bcp_prop {
                    if let TermRef::Literal { lexical, .. } = qr.o {
                        props
                            .entry(s.to_owned())
                            .or_default()
                            .entry(p.to_owned())
                            .or_default()
                            .insert(lexical.to_owned());
                    }
                }
            }
        }
    }

    let mut tag_map = HashMap::new();
    for subject in &lang_subjects {
        let subject_props = props.get(subject);
        let int_vals = subject_props
            .and_then(|m| m.get(&tag_prop))
            .cloned()
            .unwrap_or_default();
        let bcp_vals = subject_props
            .and_then(|m| m.get(&bcp_prop))
            .cloned()
            .unwrap_or_default();

        // Missing either → skip (SHACL enforces completeness at authoring time).
        if int_vals.is_empty() || bcp_vals.is_empty() {
            continue;
        }
        if int_vals.len() > 1 {
            return Err(format!(
                "individual <{subject}> has ambiguous languageTag values: {int_vals:?}; \
                 tag-map projection requires a single canonical value"
            ));
        }
        if bcp_vals.len() > 1 {
            return Err(format!(
                "individual <{subject}> has ambiguous bcp47Tag values: {bcp_vals:?}; \
                 tag-map projection requires a single canonical value"
            ));
        }
        let int_val = int_vals.into_iter().next().unwrap();
        let bcp_val = bcp_vals.into_iter().next().unwrap();
        tag_map.insert(int_val, bcp_val);
    }

    Ok(tag_map)
}

// ── helpers ─────────────────────────────────────────────────────────────────────

/// Parse a format string into an oxigraph `RdfFormat`.
fn parse_format(format: &str) -> Result<RdfFormat, String> {
    match format.to_ascii_lowercase().as_str() {
        "turtle" | "text/turtle" | "ttl" => Ok(RdfFormat::Turtle),
        "n-triples" | "ntriples" | "nt" | "application/n-triples" => Ok(RdfFormat::NTriples),
        "n-quads" | "nquads" | "nq" | "application/n-quads" => Ok(RdfFormat::NQuads),
        "trig" | "application/trig" => Ok(RdfFormat::TriG),
        _ => Err(format!("unsupported RDF format: {format:?}")),
    }
}

/// Intern an `oxigraph::model::Term` into the IR builder.
fn intern_ox_term(
    builder: &mut RdfDatasetBuilder,
    term: &OxTerm,
) -> Result<gmeow_rdf::TermId, String> {
    Ok(match term {
        OxTerm::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        OxTerm::BlankNode(b) => builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT),
        OxTerm::Literal(l) => {
            let direction = l.direction().map(|d| match d {
                oxigraph::model::BaseDirection::Ltr => gmeow_rdf::RdfTextDirection::Ltr,
                oxigraph::model::BaseDirection::Rtl => gmeow_rdf::RdfTextDirection::Rtl,
            });
            builder.intern_literal(RdfLiteral {
                lexical_form: l.value().to_owned(),
                datatype: Some(l.datatype().as_str().to_owned()),
                language: l.language().map(str::to_owned),
                direction,
            })
        }
        OxTerm::Triple(t) => {
            let s = intern_named_or_blank_node(builder, &t.subject);
            let p = builder.intern_iri(t.predicate.as_str().to_owned());
            let o = intern_ox_term(builder, &t.object)?;
            builder.intern_triple(s, p, o)
        }
    })
}

/// Intern a `NamedOrBlankNode` subject into the IR builder.
fn intern_named_or_blank_node(
    builder: &mut RdfDatasetBuilder,
    subject: &NamedOrBlankNode,
) -> gmeow_rdf::TermId {
    match subject {
        NamedOrBlankNode::NamedNode(n) => builder.intern_iri(n.as_str().to_owned()),
        NamedOrBlankNode::BlankNode(b) => {
            builder.intern_blank(b.as_str().to_owned(), BlankScope::DEFAULT)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_internal_tag_basic() {
        assert!(is_internal_tag("x-gmeow-english"));
        assert!(is_internal_tag("x-gmeow-mandarin"));
        assert!(is_internal_tag("X-GMEOW-FRENCH"));
        assert!(is_internal_tag("x-gmeow-foo-bar"));
        assert!(!is_internal_tag("en"));
        assert!(!is_internal_tag("fr"));
        assert!(!is_internal_tag("x-gmeow-")); // empty suffix
        assert!(!is_internal_tag("xx-gmeow-no")); // wrong prefix
        assert!(!is_internal_tag("x-gmeow")); // no suffix segment
    }

    #[test]
    fn rank_language_carrier_wins() {
        let (r_en, _) = rank_language("x-gmeow-english");
        let (r_fr, _) = rank_language("x-gmeow-french");
        let (r_bcp, _) = rank_language("en");
        assert_eq!(r_en, 0);
        assert_eq!(r_fr, 1);
        assert_eq!(r_bcp, 1);
    }

    #[test]
    fn rank_language_case_insensitive() {
        let (r, key) = rank_language("X-GMEOW-ENGLISH");
        assert_eq!(r, 0);
        assert_eq!(key, "x-gmeow-english");
    }

    #[test]
    fn load_tag_map_parses_turtle() {
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:bcp47Tag "en" .

gmeow:French a gmeow:Language ;
    gmeow:languageTag "x-gmeow-french" ;
    gmeow:bcp47Tag "fr" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
        assert_eq!(map.get("x-gmeow-french"), Some(&"fr".to_owned()));
    }

    #[test]
    fn load_tag_map_ambiguous_err() {
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" ;
    gmeow:languageTag "x-gmeow-english-alt" ;
    gmeow:bcp47Tag "en" .
"#;
        assert!(load_tag_map(ttl.as_bytes(), "turtle").is_err());
    }

    #[test]
    fn load_tag_map_missing_tag_skipped() {
        // An individual with only one of the two required properties is silently
        // skipped (SHACL enforces completeness; we don't fabricate).
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:English a gmeow:Language ;
    gmeow:languageTag "x-gmeow-english" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert!(map.is_empty(), "incomplete individual must be skipped");
    }

    #[test]
    fn load_tag_map_formal_and_prog_language() {
        let ttl = r#"
@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .
@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .

gmeow:Rust a gmeow:ProgrammingLanguage ;
    gmeow:languageTag "x-gmeow-rust" ;
    gmeow:bcp47Tag "en" .

gmeow:Prolog a gmeow:FormalLanguage ;
    gmeow:languageTag "x-gmeow-prolog" ;
    gmeow:bcp47Tag "en" .
"#;
        let map = load_tag_map(ttl.as_bytes(), "turtle").expect("parse");
        assert!(map.contains_key("x-gmeow-rust"));
        assert!(map.contains_key("x-gmeow-prolog"));
    }

    #[test]
    fn load_tag_map_ntriples_format() {
        let nt = "\
<https://blackcatinformatics.ca/gmeow/English> \
<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> \
<https://blackcatinformatics.ca/gmeow/Language> .\n\
<https://blackcatinformatics.ca/gmeow/English> \
<https://blackcatinformatics.ca/gmeow/languageTag> \
\"x-gmeow-english\" .\n\
<https://blackcatinformatics.ca/gmeow/English> \
<https://blackcatinformatics.ca/gmeow/bcp47Tag> \
\"en\" .\n";
        let map = load_tag_map(nt.as_bytes(), "ntriples").expect("parse");
        assert_eq!(map.get("x-gmeow-english"), Some(&"en".to_owned()));
    }
}
