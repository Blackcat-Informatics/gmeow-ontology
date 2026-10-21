// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Independent registry literals; no canonical registry or corpus is consumed.
use super::*;

const REGISTRY: &str = r#"
@prefix gm: <https://blackcatinformatics.ca/gmeow/> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
<urn:authored:procedure> a gm:CatalogFamily ; gm:catalogFamilyName "procedure" ;
    gm:catalogNamespace "https://schema.org/HowTo"^^xsd:anyURI,
        "https://schema.org/Recipe"^^xsd:anyURI, "https://schema.org/supply"^^xsd:anyURI,
        "https://schema.org/tool"^^xsd:anyURI, "https://schema.org/result"^^xsd:anyURI,
        "https://schema.org/step"^^xsd:anyURI ;
    gm:catalogOwner <https://blackcatinformatics.ca/gmeow/slices/logic> ; gm:catalogTargetMinimum 7 .
<urn:authored:value> a gm:CatalogFamily ; gm:catalogFamilyName "value" ;
    gm:catalogNamespace "https://schema.org/value"^^xsd:anyURI ;
    gm:catalogOwner <https://blackcatinformatics.ca/gmeow/slices/lang>,
        <https://blackcatinformatics.ca/gmeow/slices/math> ; gm:catalogTargetMinimum 2 .
<urn:authored:exemption> a gm:ResidueRatchetExemption ;
    gm:exemptCatalogFamily <urn:authored:procedure> ; gm:exemptRationale
    "The authored fixture retains seven procedure publication bridges under their existing explicitly bounded exemption." ;
    gm:exemptRowCeiling 7 .
"#;

fn rows() -> Vec<(&'static str, &'static str)> {
    vec![
        ("urn:row:plan", "https://schema.org/HowTo"),
        ("urn:row:step", "https://schema.org/HowToStep"),
        ("urn:row:recipe", "https://schema.org/Recipe"),
        ("urn:row:supply", "https://schema.org/supply"),
        ("urn:row:tool", "https://schema.org/tool"),
        ("urn:row:result", "https://schema.org/result"),
        ("urn:row:ordered-step", "https://schema.org/step"),
        ("urn:row:math", "https://schema.org/value"),
        ("urn:row:lang", "https://schema.org/value"),
    ]
}

#[test]
fn projection_literal_catalog_partitions_seven_procedure_and_two_value_bridges() {
    let dataset = purrdf::parse_dataset(REGISTRY.as_bytes(), "text/turtle", None).unwrap();
    let families = catalog_families_from_dataset(&dataset).unwrap();
    let measured = check_target_catalogs(&families, rows(), "authored").unwrap();
    assert_eq!(measured["procedure"], 7);
    assert_eq!(measured["value"], 2);
    let exemptions = residue_exemptions_from_dataset(&dataset).unwrap();
    assert_eq!(exemptions[0].row_ceiling, 7);
    let guarded = BTreeSet::from(["https://schema.org/value".to_owned()]);
    check_residue_exemptions(&families, &exemptions, &guarded, &measured, "authored").unwrap();
    assert_eq!(
        families_for_target(&families, "https://schema.org/valueReference")[0].name,
        "value"
    );
    assert_eq!(
        families_for_target(&families, "https://schema.org/HowToSupply")[0].name,
        "procedure"
    );
}

#[test]
fn projection_literal_catalog_refuses_old_broad_overlap_unknown_and_guarded_exemption() {
    let broad = REGISTRY.replace("https://schema.org/HowTo\"", "https://schema.org/\"");
    let dataset = purrdf::parse_dataset(broad.as_bytes(), "text/turtle", None).unwrap();
    assert!(catalog_families_from_dataset(&dataset).is_err());
    let dataset = purrdf::parse_dataset(REGISTRY.as_bytes(), "text/turtle", None).unwrap();
    let families = catalog_families_from_dataset(&dataset).unwrap();
    let mut unknown = rows();
    unknown.push(("urn:row:unknown", "https://schema.org/unregistered"));
    assert!(check_target_catalogs(&families, unknown, "authored").is_err());
    let measured = check_target_catalogs(&families, rows(), "authored").unwrap();
    let exemptions = residue_exemptions_from_dataset(&dataset).unwrap();
    assert!(
        check_residue_exemptions(
            &families,
            &exemptions,
            &BTreeSet::from(["https://schema.org/HowTo".to_owned()]),
            &measured,
            "authored"
        )
        .is_err()
    );
    let mut grown = measured;
    grown.insert("procedure".to_owned(), 8);
    assert!(
        check_residue_exemptions(&families, &exemptions, &BTreeSet::new(), &grown, "authored")
            .is_err()
    );
}
