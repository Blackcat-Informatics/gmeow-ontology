// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

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
    // The residue is read back from the emission's loss store, keyed by the row target.
    let residue = e.loss.projection_drops_for(&e.ledger[0].target).join("\n");
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
