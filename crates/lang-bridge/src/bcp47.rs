// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The [`Bcp47Target`]: GENERATE the `gmeow:bcp47Tag` registry identifier of a
//! `lang:LanguageVariety` FROM the variety's own structure — never asserted, never the
//! hand-authored SSSOM alignment.
//!
//! A BCP-47 tag is assembled in canonical order `language[-Script][-REGION][-variant]` from
//! machine-readable subtags already carried by the model, all through existing vocabulary
//! (`skos:notation`, `lang:varietyOf`, `lang:orthographyFor`, `lang:usesScript`):
//!
//! * **language subtag** — the ISO 639 `skos:notation` of the variety's `lang:varietyOf`
//!   parent sign system (`fr`). GMEOW's sign systems deliberately keep their ISO codes as
//!   alignments rather than asserted data, so a parent with no `skos:notation` yields NO
//!   derivable tag — the variety is RECORDED as a data row, never a failure.
//! * **script subtag** — the ISO 15924 `skos:notation` of the script the variety's
//!   orthography uses (`Latn`), SUPPRESSED when it equals the parent language's own
//!   default-orthography script (the BCP-47 suppress-script rule, derived purely from the
//!   model's orthography structure: `fr` + Latin orthography → `fr`, not `fr-Latn`).
//! * **region / variant subtag** — the variety's OWN `skos:notation`, classified by shape
//!   (`CA` → region, `emodeng` → variant). This is the subtag that differentiates the
//!   variety from its parent.
//!
//! The walk is total over registry-having sign systems and honest about the rest: a variety
//! with no derivable primary subtag contributes a `gmeow:bcp47Tag`-absent note to the loss
//! residue rather than a fabricated tag. The projection is **lossy by construction** — a tag
//! carries no diachronic history, no `lang:varietyOf` relation structure, and no
//! orthography split — so it CARRIES the shared lossy-lens `logic:Correspondence` and folds
//! ONE honest `SoundUnder` ledger row, exactly like the other projections FROM the model.

use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::projections::ProjectionResult;
use purrdf::{RdfDataset, TermId};

use crate::bridge::IngestDiagnostic;
use crate::emit::{digest16, ntriples_sorted};
use crate::rdf_scan::{
    LANG_NS, iri_of, object_literal, objects, parse_lang_turtle, subjects_of_type,
    subjects_with_object, term_label, unrepresentable,
};
use crate::registry::{EmittedArtifact, LangEmission, LangProjectionInput, LangProjectionTarget};

/// The generated tag predicate, in the `gmeow:` namespace.
const GMEOW_BCP47_TAG: &str = "https://blackcatinformatics.ca/gmeow/bcp47Tag";
/// `skos:notation` — the machine-readable registry code carrier the model already uses for a
/// script's ISO 15924 code; the same slot carries a sign system's ISO 639 language subtag and
/// a variety's differentiating region/variant subtag.
const SKOS_NOTATION: &str = "http://www.w3.org/2004/02/skos/core#notation";

/// `logic:getLeg` program IRI: read a `lang:LanguageVariety` into its generated BCP-47 tag.
const BCP47_GET_LEG: &str = "https://blackcatinformatics.ca/lang/bcp47GenerateLeg";
/// The content-address base of the carried BCP-47 correspondence.
const BCP47_CORR_BASE: &str = "http://example.org/lang/bcp47-correspondence/";
/// The example-instance base the aggregate tag-set source IRI lives under.
const EXAMPLE_BASE: &str = "http://example.org/lang/";

/// The strata a BCP-47 tag flattens — enumerated per emission so the loss is carried and
/// flagged, never a footnote. Present only when the emission actually generates a tag.
const BCP47_UNSUPPORTED: &[&str] = &[
    "a BCP-47 tag carries no diachronic history of the variety (no lang: temporal-stage lineage)",
    "a BCP-47 tag carries no variety relations beyond the primary subtag (the lang:varietyOf \
     structure is flattened to a single differentiating subtag)",
    "a BCP-47 tag carries no orthography split (a sign system with several orthographies \
     collapses to one script subtag)",
];

/// One variety's derivation: its IRI, the generated tag where derivable, and — where NOT — an
/// honest note recording why the registry subtag could not be derived (a data row, never a
/// failure).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bcp47Derivation {
    /// The `lang:LanguageVariety` IRI the tag is generated for.
    pub variety_iri: String,
    /// The generated BCP-47 tag, or `None` when no primary subtag is derivable.
    pub tag: Option<String>,
    /// The `gmeow:bcp47Tag`-absent note when `tag` is `None`.
    pub absence: Option<String>,
}

/// The BCP-47 / ISO 639 / ISO 15924 identification projection target.
pub struct Bcp47Target;

impl LangProjectionTarget for Bcp47Target {
    fn name(&self) -> &'static str {
        "bcp47"
    }

    fn emit(&self, input: &LangProjectionInput) -> Result<Vec<LangEmission>, IngestDiagnostic> {
        // Walk every LanguageVariety surface, aggregating derivations into ONE tag-set emission
        // (one committed `bcp47-tags.ttl`), so the projection presents a single registry surface
        // rather than a file per variety.
        let mut derivations: Vec<Bcp47Derivation> = Vec::new();
        for source in &input.varieties {
            let ds = parse_lang_turtle(&source.bytes, &source.name)?;
            for v in subjects_of_type(&ds, &format!("{LANG_NS}LanguageVariety")) {
                derivations.push(derive_bcp47_tag(&ds, v));
            }
        }
        if derivations.is_empty() {
            // No variety in any scanned surface — the driver folds one honest no-source row.
            return Ok(Vec::new());
        }
        derivations.sort_by(|a, b| a.variety_iri.cmp(&b.variety_iri));
        derivations.dedup();
        // After collapsing byte-identical derivations, any remaining same-variety pair carries a
        // CONFLICTING derivation (a different tag/context for one lang:LanguageVariety across the
        // scanned surfaces) — a hard fail, never a silently doubled or arbitrarily-picked tag.
        if let Some(conflict) = derivations
            .windows(2)
            .find(|w| w[0].variety_iri == w[1].variety_iri)
        {
            return Err(unrepresentable(format!(
                "lang:LanguageVariety <{}> yields conflicting BCP-47 derivations across the scanned \
                 surfaces; a variety must derive exactly one registry tag",
                conflict[0].variety_iri
            )));
        }
        Ok(vec![emit_tag_set(&derivations)])
    }
}

/// The REUSABLE variety-structure → registry-tag walk: GENERATE a variety's BCP-47 tag from
/// its `lang:varietyOf` parent, its orthography's `lang:Script`, and its own differentiating
/// `skos:notation`. Never asserts, never reads the hand-authored SSSOM. A variety whose parent
/// carries no ISO 639 `skos:notation` yields `tag: None` with an honest absence note.
#[must_use]
pub fn derive_bcp47_tag(ds: &RdfDataset, variety: TermId) -> Bcp47Derivation {
    let variety_iri = iri_of(ds, variety).unwrap_or_else(|| {
        format!(
            "{EXAMPLE_BASE}bcp47/blank/{}",
            digest16("lang-bcp47-blank", &term_label(ds, variety))
        )
    });

    // The parent sign system (lang:varietyOf) and its ISO 639 language subtag.
    let parent = objects(ds, variety, &format!("{LANG_NS}varietyOf"))
        .into_iter()
        .next();
    let language = parent
        .and_then(|p| object_literal(ds, p, SKOS_NOTATION))
        .map(|n| n.trim().to_owned())
        .filter(|n| is_iso639_language(n));

    let Some(language) = language else {
        return Bcp47Derivation {
            absence: Some(format!(
                "variety <{variety_iri}> has no derivable BCP-47 primary subtag: its \
                 lang:varietyOf parent carries no ISO 639 skos:notation (GMEOW sign systems keep \
                 ISO codes as alignments, not asserted data), so gmeow:bcp47Tag is absent — a \
                 data row, not a failure"
            )),
            variety_iri,
            tag: None,
        };
    };

    // The variety's own differentiating subtag (region or variant), by shape.
    let differentiator = object_literal(ds, variety, SKOS_NOTATION).map(|n| n.trim().to_owned());
    let (region, variant) = classify_differentiator(differentiator.as_deref());

    // The script subtag, suppressed when it is the parent language's default-orthography script.
    let variety_script = orthography_script_notation(ds, variety);
    let parent_script = parent.and_then(|p| orthography_script_notation(ds, p));
    let script = match variety_script {
        Some(s) if parent_script.as_deref() != Some(s.as_str()) => Some(s),
        _ => None,
    };

    let mut tag = language.to_lowercase();
    if let Some(s) = script {
        tag.push('-');
        tag.push_str(&titlecase_script(&s));
    }
    if let Some(r) = region {
        tag.push('-');
        tag.push_str(&r.to_uppercase());
    }
    if let Some(v) = variant {
        tag.push('-');
        tag.push_str(&v.to_lowercase());
    }

    Bcp47Derivation {
        variety_iri,
        tag: Some(tag),
        absence: None,
    }
}

/// The ISO 15924 `skos:notation` of the script the sign system's orthography uses, if the model
/// carries one — the shared read for both the variety and its parent (for suppress-script).
fn orthography_script_notation(ds: &RdfDataset, sign_system: TermId) -> Option<String> {
    for orthography in subjects_with_object(ds, &format!("{LANG_NS}orthographyFor"), sign_system) {
        for script in objects(ds, orthography, &format!("{LANG_NS}usesScript")) {
            if let Some(notation) = object_literal(ds, script, SKOS_NOTATION) {
                let notation = notation.trim();
                if is_iso15924_script(notation) {
                    return Some(notation.to_owned());
                }
            }
        }
    }
    None
}

/// Classify a variety's differentiating `skos:notation` into a `(region, variant)` subtag by
/// BCP-47 shape: a region is two letters or three digits (`CA`, `419`); a variant is 5–8
/// alphanumerics or a digit-led 4 (`emodeng`, `1996`). Anything else differentiates nothing.
fn classify_differentiator(notation: Option<&str>) -> (Option<String>, Option<String>) {
    match notation {
        Some(n) if is_region(n) => (Some(n.to_owned()), None),
        Some(n) if is_variant(n) => (None, Some(n.to_owned())),
        _ => (None, None),
    }
}

/// Whether `s` is an ISO 639 primary language subtag shape (2–3 ASCII letters).
fn is_iso639_language(s: &str) -> bool {
    matches!(s.len(), 2 | 3) && s.bytes().all(|b| b.is_ascii_alphabetic())
}

/// Whether `s` is an ISO 15924 script subtag shape (exactly 4 ASCII letters).
fn is_iso15924_script(s: &str) -> bool {
    s.len() == 4 && s.bytes().all(|b| b.is_ascii_alphabetic())
}

/// Whether `s` is a BCP-47 region subtag shape (2 ASCII letters or 3 ASCII digits).
fn is_region(s: &str) -> bool {
    (s.len() == 2 && s.bytes().all(|b| b.is_ascii_alphabetic()))
        || (s.len() == 3 && s.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether `s` is a BCP-47 variant subtag shape (5–8 alphanumerics, or 4 with a leading digit).
fn is_variant(s: &str) -> bool {
    let alnum = |b: u8| b.is_ascii_alphanumeric();
    match s.len() {
        5..=8 => s.bytes().all(alnum),
        4 => s.as_bytes()[0].is_ascii_digit() && s.bytes().all(alnum),
        _ => false,
    }
}

/// Canonicalize an ISO 15924 subtag to titlecase (`latn` → `Latn`).
fn titlecase_script(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for (i, ch) in s.chars().enumerate() {
        if i == 0 {
            out.extend(ch.to_uppercase());
        } else {
            out.extend(ch.to_lowercase());
        }
    }
    out
}

/// Fold every derivation into ONE aggregate tag-set emission: the generated tags land in a
/// single `bcp47-tags.ttl`, the absences land in the honest loss residue, and the emission
/// carries the shared lossy-lens correspondence (never exact).
fn emit_tag_set(derivations: &[Bcp47Derivation]) -> LangEmission {
    let mut tag_lines: Vec<String> = Vec::new();
    let mut residue: Vec<String> = Vec::new();
    let mut any_tag = false;
    let mut corr_key = String::new();

    for d in derivations {
        match (&d.tag, &d.absence) {
            (Some(tag), _) => {
                any_tag = true;
                tag_lines.push(format!(
                    "<{}> <{GMEOW_BCP47_TAG}> {} .",
                    d.variety_iri,
                    nt_literal(tag)
                ));
                corr_key.push_str(&d.variety_iri);
                corr_key.push('=');
                corr_key.push_str(tag);
                corr_key.push('\u{1f}');
            }
            (None, Some(note)) => {
                residue.push(note.clone());
                corr_key.push_str(&d.variety_iri);
                corr_key.push_str("=∅\u{1f}");
            }
            (None, None) => {}
        }
    }

    // The lossy strata apply only where a tag is actually generated.
    if any_tag {
        residue.extend(BCP47_UNSUPPORTED.iter().map(|s| (*s).to_owned()));
    }
    residue.sort();
    residue.dedup();

    let source_iri = format!(
        "{EXAMPLE_BASE}bcp47/tag-set/{}",
        digest16("lang-bcp47-tagset", &corr_key)
    );

    // The generated `<variety> gmeow:bcp47Tag "tag"` triples land BOTH in the committed
    // `bcp47-tags.ttl` artifact AND — via `source_rdf` — in the reasoned corpus graph
    // (`graph/lang-projection-corpus`), so SPARQL projection consumers resolve a variety's tag
    // from the bundle, joining through `lang:varietyOf`. Byte-identical, sorted N-Triples.
    let (artifacts, source_rdf) = if any_tag {
        let tags_nt = ntriples_sorted(tag_lines);
        (
            vec![EmittedArtifact {
                path_suffix: "bcp47-tags.ttl".to_owned(),
                bytes: tags_nt.clone(),
                is_rdf: true,
            }],
            tags_nt,
        )
    } else {
        (Vec::new(), Vec::new())
    };

    let correspondence =
        crate::rdf_scan::lossy_lens_correspondence(BCP47_CORR_BASE, &corr_key, BCP47_GET_LEG, None);

    LangEmission {
        artifacts,
        correspondence,
        ledger: vec![ProjectionResult {
            target: "bcp47:tag-set".to_owned(),
            content: String::new(),
            is_rdf: true,
            preservation: PreservationKind::SoundUnder,
            complexity: "n/a".to_owned(),
            lossy_drops: Vec::new(),
            actual_drops: residue.clone(),
        }],
        leg_pair: None,
        emitted_reading_count: None,
        source_iri,
        unsupported: residue,
        round_trip_holds: false,
        lossy_kind: PreservationKind::SoundUnder,
        source_rdf,
    }
}

/// Escape a string as an N-Triples quoted literal.
fn nt_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::is_exact_correspondence;
    use crate::registry::NamedSource;

    /// A French/Quebec-French scene: French carries the ISO 639 subtag `fr` and a Latin
    /// orthography; the Quebec-French variety differentiates itself with region `CA` and its own
    /// Latin orthography. The generated tag SUPPRESSES the redundant `Latn` script (it is
    /// French's default orthography script) → `fr-CA`, not `fr-Latn-CA`.
    const FR_CA: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix ex:   <http://example.org/lang/> .

ex:french a lang:SignSystem ; skos:notation "fr" .
ex:latinScript a lang:Script ; skos:notation "Latn" .
ex:frenchOrth a lang:Orthography ; lang:orthographyFor ex:french ; lang:usesScript ex:latinScript .

ex:quebecFrench a lang:LanguageVariety ; lang:varietyOf ex:french ; skos:notation "CA" .
ex:quebecOrth a lang:Orthography ; lang:orthographyFor ex:quebecFrench ; lang:usesScript ex:latinScript .
"#;

    /// A Serbian scene whose default orthography is Latin but whose variety is written in
    /// Cyrillic: the script subtag is NOT suppressed (it differs from the parent default) →
    /// `sr-Cyrl`.
    const SR_CYRL: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix skos: <http://www.w3.org/2004/02/skos/core#> .
@prefix ex:   <http://example.org/lang/> .

ex:serbian a lang:SignSystem ; skos:notation "sr" .
ex:latn a lang:Script ; skos:notation "Latn" .
ex:cyrl a lang:Script ; skos:notation "Cyrl" .
ex:serbianLatinOrth a lang:Orthography ; lang:orthographyFor ex:serbian ; lang:usesScript ex:latn .

ex:serbianCyrillic a lang:LanguageVariety ; lang:varietyOf ex:serbian .
ex:serbianCyrillicOrth a lang:Orthography ; lang:orthographyFor ex:serbianCyrillic ; lang:usesScript ex:cyrl .
"#;

    /// A variety whose parent carries no ISO 639 notation — no primary subtag is derivable, so the
    /// tag is absent and the derivation records an honest note.
    const NO_REGISTRY: &str = r#"
@prefix lang: <https://blackcatinformatics.ca/lang/> .
@prefix ex:   <http://example.org/lang/> .

ex:conlang a lang:SignSystem .
ex:conlangVariety a lang:LanguageVariety ; lang:varietyOf ex:conlang .
"#;

    fn only(turtle: &str) -> Bcp47Derivation {
        let ds = parse_lang_turtle(turtle.as_bytes(), "test").expect("parse");
        let mut vs = subjects_of_type(&ds, &format!("{LANG_NS}LanguageVariety"));
        assert_eq!(vs.len(), 1, "one variety in the scene");
        derive_bcp47_tag(&ds, vs.remove(0))
    }

    #[test]
    fn french_canada_variety_generates_fr_ca_suppressing_default_script() {
        let d = only(FR_CA);
        assert_eq!(d.tag.as_deref(), Some("fr-CA"), "{d:?}");
        assert!(d.absence.is_none());
    }

    #[test]
    fn non_default_script_is_not_suppressed() {
        let d = only(SR_CYRL);
        assert_eq!(d.tag.as_deref(), Some("sr-Cyrl"), "{d:?}");
    }

    #[test]
    fn variety_without_registry_parent_records_an_absence_row_not_a_failure() {
        let d = only(NO_REGISTRY);
        assert!(d.tag.is_none(), "{d:?}");
        let note = d.absence.expect("absence note");
        assert!(
            note.contains("no derivable BCP-47 primary subtag"),
            "{note}"
        );
        assert!(note.contains("data row"), "{note}");
    }

    #[test]
    fn emit_folds_one_lossy_tag_set_emission() {
        let input = LangProjectionInput {
            varieties: vec![
                NamedSource {
                    name: "fr".to_owned(),
                    bytes: FR_CA.as_bytes().to_vec(),
                },
                NamedSource {
                    name: "no-registry".to_owned(),
                    bytes: NO_REGISTRY.as_bytes().to_vec(),
                },
            ],
            ..Default::default()
        };
        let emissions = Bcp47Target.emit(&input).expect("emit");
        assert_eq!(emissions.len(), 1, "one aggregate tag-set emission");
        let e = &emissions[0];

        // The generated tag lands in the single bcp47-tags.ttl artifact.
        assert_eq!(e.artifacts.len(), 1);
        assert_eq!(e.artifacts[0].path_suffix, "bcp47-tags.ttl");
        assert!(e.artifacts[0].is_rdf);
        let ttl = String::from_utf8(e.artifacts[0].bytes.clone()).unwrap();
        assert!(
            ttl.contains(&format!("<{GMEOW_BCP47_TAG}> \"fr-CA\"")),
            "{ttl}"
        );

        // The generated tag triples are ALSO folded into source_rdf, so they enter the reasoned
        // corpus graph the SPARQL projection consumers query (byte-identical to the artifact).
        assert_eq!(
            e.source_rdf, e.artifacts[0].bytes,
            "the generated tag triples must ride source_rdf into the bundle graph"
        );
        let src = String::from_utf8(e.source_rdf.clone()).unwrap();
        assert!(
            src.contains(&format!(
                "<http://example.org/lang/quebecFrench> <{GMEOW_BCP47_TAG}> \"fr-CA\""
            )),
            "the folded source_rdf carries the <variety> gmeow:bcp47Tag triple: {src}"
        );

        // The projection is honestly lossy: SoundUnder, never exact, with the flattened strata
        // AND the honest absence note in the residue.
        assert!(!is_exact_correspondence(&e.correspondence));
        assert_eq!(e.lossy_kind, PreservationKind::SoundUnder);
        let residue = e.ledger[0].actual_drops.join("\n");
        assert!(residue.contains("no diachronic history"), "{residue}");
        assert!(
            residue.contains("no derivable BCP-47 primary subtag"),
            "{residue}"
        );
    }

    #[test]
    fn emitter_is_byte_reproducible() {
        let input = LangProjectionInput {
            varieties: vec![NamedSource {
                name: "fr".to_owned(),
                bytes: FR_CA.as_bytes().to_vec(),
            }],
            ..Default::default()
        };
        let a = Bcp47Target.emit(&input).expect("a");
        let b = Bcp47Target.emit(&input).expect("b");
        assert_eq!(a[0].artifacts[0].bytes, b[0].artifacts[0].bytes);
    }

    #[test]
    fn empty_input_yields_no_emission() {
        let emissions = Bcp47Target
            .emit(&LangProjectionInput::default())
            .expect("emit");
        assert!(
            emissions.is_empty(),
            "no varieties ⇒ driver folds a no-source row"
        );
    }
}
