// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Tests for validation-shape derivation from authored sources, driven by Turtle fragments
//! passed to [`derive_validation_shapes`].

use super::*;

/// Parse a Turtle fragment into a dataset for [`derive_validation_shapes`]. The `g:` prefix is
/// the GMEOW authoring namespace, so its classes are in-scope for derivation.
pub(super) fn shape_dataset(ttl: &str) -> std::sync::Arc<RdfDataset> {
    let full = format!(
        "@prefix rdf:  <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n\
         @prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n\
         @prefix owl:  <http://www.w3.org/2002/07/owl#> .\n\
         @prefix xsd:  <http://www.w3.org/2001/XMLSchema#> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix g:    <https://blackcatinformatics.ca/gmeow/> .\n{ttl}"
    );
    parse_dataset(full.as_bytes(), "text/turtle", None).expect("parse dataset ok")
}

/// Flatten a component and its nested inners (a `QualifiedValueShape`'s inner shape and a
/// `Not`'s inner component) into `out`, so a classification assertion still bites even after
/// `someValuesFrom` began wrapping its classification in a `QualifiedValueShape{min:1}`.
fn flatten_component(c: &ConstraintComponent, out: &mut Vec<ConstraintComponent>) {
    out.push(c.clone());
    match c {
        ConstraintComponent::QualifiedValueShape { shape, .. } => {
            for inner in shape {
                flatten_component(inner, out);
            }
        }
        ConstraintComponent::Not(inner) => flatten_component(inner, out),
        _ => {}
    }
}

/// Every constraint component of every shape — both path components and focus-node components —
/// flattened so nested `QualifiedValueShape` / `Not` inners are visible to a `matches!` assertion.
fn all_components(shapes: &[ValidationShapeIr]) -> Vec<ConstraintComponent> {
    let mut out = Vec::new();
    for s in shapes {
        for p in &s.properties {
            for c in &p.components {
                flatten_component(c, &mut out);
            }
        }
        for c in &s.node_components {
            flatten_component(c, &mut out);
        }
    }
    out
}

/// Canonical restrictions retain projection parity and explicit typed severity through derivation.
#[test]
fn derive_logic_restrictions_matches_the_lowered_owl_spelling() {
    let cases = [
        (
            "same-path allValuesFrom + maxQualifiedCardinality/onClass",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:item ; owl:allValuesFrom g:Item ] ,
               [ a owl:Restriction ; owl:onProperty g:item ;
                 owl:maxQualifiedCardinality 1 ; owl:onClass g:Item ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:item ; logic:allValuesFrom g:Item ] ,
               [ a logic:Restriction ; logic:onProperty g:item ;
                 logic:maxQualifiedCardinality 1 ; logic:onClass g:Item ] ."#,
        ),
        (
            "someValuesFrom",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:someItem ; owl:someValuesFrom g:Item ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:someItem ; logic:someValuesFrom g:Item ] ."#,
        ),
        (
            "hasValue",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:state ; owl:hasValue g:active ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:state ; logic:hasValue g:active ] ."#,
        ),
        (
            "unqualified cardinalities",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:minItem ; owl:minCardinality 1 ] ,
               [ a owl:Restriction ; owl:onProperty g:maxItem ; owl:maxCardinality 2 ] ,
               [ a owl:Restriction ; owl:onProperty g:exactItem ; owl:cardinality 1 ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:minItem ; logic:minCardinality 1 ] ,
               [ a logic:Restriction ; logic:onProperty g:maxItem ; logic:maxCardinality 2 ] ,
               [ a logic:Restriction ; logic:onProperty g:exactItem ; logic:cardinality 1 ] ."#,
        ),
        (
            "qualified cardinalities with class qualifiers",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:exactMember ; owl:qualifiedCardinality 1 ; owl:onClass g:Item ] ,
               [ a owl:Restriction ; owl:onProperty g:minMember ; owl:minQualifiedCardinality 1 ; owl:onClass g:Item ] ,
               [ a owl:Restriction ; owl:onProperty g:maxMember ; owl:maxQualifiedCardinality 2 ; owl:onClass g:Item ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:exactMember ; logic:qualifiedCardinality 1 ; logic:onClass g:Item ] ,
               [ a logic:Restriction ; logic:onProperty g:minMember ; logic:minQualifiedCardinality 1 ; logic:onClass g:Item ] ,
               [ a logic:Restriction ; logic:onProperty g:maxMember ; logic:maxQualifiedCardinality 2 ; logic:onClass g:Item ] ."#,
        ),
        (
            "qualified cardinality with data-range qualifier",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:score ; owl:maxQualifiedCardinality 1 ; owl:onDataRange xsd:decimal ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:score ; logic:maxQualifiedCardinality 1 ; logic:onDataRange xsd:decimal ] ."#,
        ),
        (
            "unionOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:member ;
                 owl:allValuesFrom [ owl:unionOf ( g:Item g:OtherItem ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:member ;
                 logic:allValuesFrom [ logic:unionOf ( g:Item g:OtherItem ) ] ] ."#,
        ),
        (
            "disjointUnionOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:member ;
                 owl:allValuesFrom [ owl:disjointUnionOf ( g:Item g:OtherItem ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:member ;
                 logic:allValuesFrom [ logic:disjointUnionOf ( g:Item g:OtherItem ) ] ] ."#,
        ),
        (
            "oneOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:state ;
                 owl:allValuesFrom [ owl:oneOf ( g:active g:inactive ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:state ;
                 logic:allValuesFrom [ logic:oneOf ( g:active g:inactive ) ] ] ."#,
        ),
        (
            "complementOf filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:member ;
                 owl:allValuesFrom [ owl:complementOf g:ForbiddenItem ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:member ;
                 logic:allValuesFrom [ logic:complementOf g:ForbiddenItem ] ] ."#,
        ),
        (
            "faceted datatype onDatatype/withRestrictions filler",
            r#"g:Record a owl:Class ; rdfs:subClassOf
               [ a owl:Restriction ; owl:onProperty g:code ; owl:allValuesFrom
                 [ a rdfs:Datatype ; owl:onDatatype xsd:string ;
                   owl:withRestrictions ( [ xsd:minLength 2 ] ) ] ] ."#,
            r#"g:Record a owl:Class ; logic:subClassOf
               [ a logic:Restriction ; logic:onProperty g:code ; logic:allValuesFrom
                 [ a rdfs:Datatype ; logic:onDatatype xsd:string ;
                   logic:withRestrictions ( [ xsd:minLength 2 ] ) ] ] ."#,
        ),
    ];

    let mut merged_logic_shapes = None;
    for (name, owl_ttl, logic_ttl) in cases {
        let owl = shape_dataset(owl_ttl);
        let logic = shape_dataset(logic_ttl);
        let owl_shapes = derive_validation_shapes(owl.as_ref())
            .unwrap_or_else(|error| panic!("derive OWL spelling for {name}: {error}"));
        let logic_shapes = derive_validation_shapes(logic.as_ref())
            .unwrap_or_else(|error| panic!("derive canonical logic spelling for {name}: {error}"));
        assert_eq!(
            logic_shapes, owl_shapes,
            "canonical logic: spelling must match its OWL projection for {name}"
        );
        if name.starts_with("same-path") {
            merged_logic_shapes = Some(logic_shapes);
        }
    }

    let logic_shapes = merged_logic_shapes.expect("the same-path merge case ran");
    let record = logic_shapes
        .iter()
        .find(|shape| shape.iri.ends_with("/Record-shape"))
        .expect("logic-authored Record restriction must produce a class shape");
    assert_eq!(record.properties.len(), 1, "same-path restrictions merge");
    assert_eq!(
        record.properties[0].path,
        "https://blackcatinformatics.ca/gmeow/item"
    );
    assert_eq!(record.properties[0].severity, None);

    let restriction = "g:Record a logic:Class ; logic:subClassOf
        [ a logic:Restriction ; logic:onProperty g:item ;
          logic:allValuesFrom g:Item ; logic:severity \"Warning\" ] .";
    let string = derive_validation_shapes(shape_dataset(restriction).as_ref()).unwrap();
    let token = derive_validation_shapes(
        shape_dataset(&restriction.replace("\"Warning\"", "\"Warning\"^^xsd:token")).as_ref(),
    )
    .unwrap();
    assert_eq!(string, token);
    let record = token
        .iter()
        .find(|shape| shape.iri.ends_with("/Record-shape"))
        .unwrap();
    assert_eq!(record.properties[0].severity, Some(ShaclSeverity::Warning));
    assert!(
        crate::projections::shapes::project_validation_shape_shacl(record)
            .contains("sh:severity sh:Warning")
    );
    for severity in [
        "\"Warning\"@en--ltr",
        "\"Warning\", \"Info\"",
        "\"Warning\", \"Warning\"^^xsd:token",
    ] {
        let malformed = restriction.replace("\"Warning\"", severity);
        let error = derive_validation_shapes(shape_dataset(&malformed).as_ref())
            .expect_err("malformed restriction severity must refuse derivation");
        assert!(
            error.message().contains("logic:severity"),
            "{severity}: {error}"
        );
    }
}

#[test]
fn derive_anonymous_one_of_filler_lowers_to_sh_in() {
    // An allValuesFrom whose filler is an anonymous enumeration `[ owl:oneOf ( a b ) ]` reads
    // closed-world as a value set on the path (`sh:in`), never a class-membership check.
    let ds = shape_dataset(
        "g:Trial a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:sidedness ; \
           owl:allValuesFrom [ owl:oneOf ( g:oneSided g:twoSided ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::In(vs) if vs.len() == 2
        )),
        "expected an sh:in over the two enumerated individuals: {comps:?}"
    );
}

#[test]
fn derive_union_of_bare_existential_restrictions_lowers_to_or_properties() {
    // `K ⊑ (∃p1.Thing ⊔ ∃p2.Thing)` — the either-of-these-properties existence obligation —
    // reads closed-world as the node-level property-alternatives disjunction.
    let ds = shape_dataset(
        "g:Framed a owl:Class ; rdfs:subClassOf [ owl:unionOf ( \
           [ a owl:Restriction ; owl:onProperty g:hasFrame ; owl:someValuesFrom owl:Thing ] \
           [ a owl:Restriction ; owl:onProperty g:hasModel ; owl:someValuesFrom owl:Thing ] ) ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let framed = shapes
        .iter()
        .find(|s| s.iri.contains("Framed"))
        .expect("Framed shape derived");
    assert!(
        framed.node_components.iter().any(|c| matches!(
            c,
            ConstraintComponent::OrProperties(paths)
                if paths.len() == 2
                    && paths.iter().any(|p| p.ends_with("hasFrame"))
                    && paths.iter().any(|p| p.ends_with("hasModel"))
        )),
        "expected an OrProperties node component: {:?}",
        framed.node_components
    );
    // The emitted SHACL carries the node-level sh:or over sh:path branches.
    let ttl = crate::projections::shapes::project_validation_shape_shacl(framed);
    assert!(
        ttl.contains(
            "sh:or ( [ sh:path <https://blackcatinformatics.ca/gmeow/hasFrame> ; sh:minCount 1 ]"
        ),
        "{ttl}"
    );
}

#[test]
fn derive_union_with_a_non_existential_member_stays_in_the_canon() {
    // A union carrying a NAMED-class member is a genuine class disjunction — it must NOT be
    // partially read as a property-alternatives disjunction.
    let ds = shape_dataset(
        "g:Mixed a owl:Class ; rdfs:subClassOf [ owl:unionOf ( \
           [ a owl:Restriction ; owl:onProperty g:hasFrame ; owl:someValuesFrom owl:Thing ] \
           g:NamedAlternative ) ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| s
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::OrProperties(_)))),
        "a mixed union must not derive a partial property disjunction: {shapes:?}"
    );
}

#[test]
fn derive_blank_restriction_domain_lowers_to_subjects_of_property_shape() {
    // A ClosedWorldClosure-opted-in property whose rdfs:domain is an anonymous restriction
    // `[ owl:onProperty q ; owl:minCardinality 1 ]` derives the required-companion condition on
    // the SubjectsOf(P) domain shape: every subject of P carries at least one q.
    let ds = shape_dataset(
        "g:lowersTo a owl:ObjectProperty ; \
           rdfs:domain [ a owl:Restriction ; owl:onProperty g:denotation ; owl:minCardinality 1 ] . \
         [ a <https://blackcatinformatics.ca/logic/ClosureEntry> ; \
           <https://blackcatinformatics.ca/logic/closureKey> \"https://blackcatinformatics.ca/gmeow/lowersTo\" ; \
           <https://blackcatinformatics.ca/logic/closureValue> <https://blackcatinformatics.ca/logic/ClosedWorldClosure> ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let domain_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("lowersTo")))
        .expect("a SubjectsOf(lowersTo) domain shape is derived");
    assert!(
        domain_shape
            .properties
            .iter()
            .any(|p| p.path.ends_with("denotation") && p.min_count == Some(1)),
        "expected a min-1 companion property on the domain shape: {domain_shape:?}"
    );
}

#[test]
fn derive_blank_restriction_domain_without_opt_in_derives_nothing() {
    // Without the ClosedWorldClosure opt-in, an anonymous restriction domain stays open-world:
    // no SubjectsOf shape is derived (domain/range are inference axioms by default).
    let ds = shape_dataset(
        "g:lowersTo a owl:ObjectProperty ; \
           rdfs:domain [ a owl:Restriction ; owl:onProperty g:denotation ; owl:minCardinality 1 ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("lowersTo"))),
        "an un-opted-in anonymous domain must derive no shape: {shapes:?}"
    );
}

#[test]
fn derive_datatype_target_lowers_to_sh_datatype_not_sh_class() {
    // A someValuesFrom whose target is a datatype must NOT become sh:class — a literal is never
    // an instance of a class, so sh:class would flag every focus node. It becomes sh:datatype.
    let ds = shape_dataset(
        "g:Block a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:blockNumber ; owl:someValuesFrom xsd:integer ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(d) if d.ends_with("integer"))),
        "expected sh:datatype for an xsd:integer target: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(_))),
        "a datatype target must never emit sh:class: {comps:?}"
    );
}

#[test]
fn derive_language_literal_datatypes_are_not_classes() {
    for datatype in ["langString", "dirLangString"] {
        let ds = shape_dataset(&format!(
            "g:Record a logic:Class ; logic:subClassOf [ a logic:Restriction ;\n\
                logic:onProperty g:words ; logic:allValuesFrom rdf:{datatype} ] .\n\
             g:words a logic:DatatypeProperty ; logic:range rdf:{datatype} .\n\
             [] a logic:ClosureEntry ; logic:closureKey g:words ;\n\
                logic:closureValue logic:ClosedWorldClosure ."
        ));
        let shapes = derive_validation_shapes(&ds).unwrap();
        assert_eq!(shapes.len(), 2, "class and property shapes: {datatype}");
        let components = all_components(&shapes);
        assert_eq!(
            components
                .iter()
                .filter(|c| matches!(c, ConstraintComponent::Datatype(d) if d.ends_with(datatype)))
                .count(),
            2
        );
        assert!(
            !components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::Class(_)))
        );
    }
}

#[test]
fn derive_canonical_named_datarange_uses_structural_typing() {
    for typing in ["a", "logic:instanceOf"] {
        let ds = shape_dataset(&format!(
            "g:Digest {typing} logic:Datarange .\n\
             g:Record a logic:Class ; logic:subClassOf [ a logic:Restriction ;\n\
                 logic:onProperty g:digest ; logic:allValuesFrom g:Digest ] ."
        ));
        let shapes = derive_validation_shapes(&ds).unwrap();
        let components = all_components(&shapes);
        assert!(
            components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::Datatype(d) if d.ends_with("/Digest")))
        );
        assert!(
            !components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::Class(_)))
        );
    }
}

/// Both global closure polarities preserve admitted key meanings and reject incomplete authority.
#[test]
fn derive_anonymous_closed_range_preserves_optin_and_optout() {
    for range in ["rdfs:range", "logic:range"] {
        for (closure, expected) in [
            ("", 0),
            (
                "[] a logic:ClosureEntry ; logic:closureKey g:words ; logic:closureValue logic:ClosedWorldClosure .",
                1,
            ),
            (
                "[] a logic:ClosureEntry ; logic:closureKey g:words ; logic:closureValue logic:ClosedWorldClosure . [] a logic:ClosureEntry ; logic:closureKey g:words ; logic:closureValue logic:OpenWorldClosure .",
                0,
            ),
        ] {
            let ds = shape_dataset(&format!(
                "g:words a logic:DatatypeProperty ; {range} [ logic:unionOf (rdf:langString rdf:dirLangString) ] . {closure}"
            ));
            let shapes = derive_validation_shapes(&ds).unwrap();
            assert_eq!(shapes.len(), expected, "{range}: {closure}");
            if expected == 1 {
                assert!(
                    matches!(&shapes[0].target, ShapeTarget::ObjectsOf(p) if p.ends_with("/words"))
                );
                let [ConstraintComponent::Or(branches)] = shapes[0].node_components.as_slice()
                else {
                    panic!("expected a range union: {shapes:?}");
                };
                assert_eq!(branches.len(), 2);
                assert!(
                    branches
                        .iter()
                        .all(|c| matches!(c, ConstraintComponent::Datatype(_)))
                );
            }
        }
    }
    for closure_value in ["OpenWorldClosure", "ClosedWorldClosure"] {
        let source = format!(
            "g:words a logic:DatatypeProperty ; logic:range [ logic:unionOf (rdf:langString rdf:dirLangString) ] .
             [] a logic:ClosureEntry ; logic:closureKey g:words ; logic:closureValue logic:{closure_value} ."
        );
        let expected = derive_validation_shapes(&shape_dataset(&source)).unwrap();
        for key in [
            "\"https://blackcatinformatics.ca/gmeow/words\"",
            "\"https://blackcatinformatics.ca/gmeow/words\"^^xsd:token",
            "g:words, g:words",
        ] {
            let selected = source.replace(
                "logic:closureKey g:words",
                &format!("logic:closureKey {key}"),
            );
            assert_eq!(
                derive_validation_shapes(&shape_dataset(&selected)).unwrap(),
                expected
            );
        }
        for key in [
            None,
            Some("_:key"),
            Some("\"p\"@en--ltr"),
            Some("\"p\"^^xsd:anyURI"),
            Some("\" p\"^^xsd:token"),
            Some("g:words, \"https://blackcatinformatics.ca/gmeow/words\""),
        ] {
            let field = key.map_or_else(String::new, |key| format!("logic:closureKey {key} ;"));
            let selected = source.replace("logic:closureKey g:words ;", &field);
            let error = derive_validation_shapes(&shape_dataset(&selected)).unwrap_err();
            assert!(
                error.message().contains("closureKey"),
                "{closure_value}: {key:?}: {error}"
            );
        }
    }
}

#[test]
fn derive_closed_range_rejects_malformed_union_without_partial_members() {
    for expression in [
        "[ logic:unionOf [ rdf:first rdf:langString ] ]",
        "[ logic:unionOf [ rdf:first rdf:langString ; rdf:rest 7 ] ]",
        "[ logic:unionOf [ rdf:first rdf:langString ; rdf:rest rdf:nil, <urn:tail> ] ]",
        "[ logic:unionOf (rdf:langString \"not a datatype\") ]",
        "[ logic:unionOf (rdf:langString [ a logic:Class ]) ]",
        "[ logic:unionOf (rdf:langString), (rdf:dirLangString) ]",
        "[ logic:unionOf [ rdf:first rdfs:Resource ] ]",
        "[ logic:unsupportedExpression g:Value ]",
        "[ logic:onDatatype xsd:string ; logic:withRestrictions () ]",
        "\"not a range\"",
    ] {
        let ds = shape_dataset(&format!(
            "g:words a logic:DatatypeProperty ; logic:range {expression} .\n\
             [] a logic:ClosureEntry ; logic:closureKey g:words ;\n\
                logic:closureValue logic:ClosedWorldClosure ."
        ));
        let error = derive_validation_shapes(&ds).expect_err(expression);
        assert_eq!(
            gmeow_errors::code::code_str(error.code()),
            "logic-compile.frontend"
        );
        assert_eq!(
            error
                .inner()
                .source_ctx
                .focus
                .as_ref()
                .map(|focus| focus.0.as_str()),
            Some("https://blackcatinformatics.ca/gmeow/words")
        );
    }
}

#[test]
fn derive_closed_range_preserves_empty_and_universal_union_meaning() {
    let closed = "[] a logic:ClosureEntry ; logic:closureKey g:value ; logic:closureValue logic:ClosedWorldClosure .";
    let empty = shape_dataset(&format!(
        "g:value a logic:DatatypeProperty ; logic:range [ logic:unionOf () ] . {closed}"
    ));
    let shapes = derive_validation_shapes(&empty).unwrap();
    assert_eq!(shapes.len(), 1);
    assert!(
        matches!(shapes[0].node_components.as_slice(), [ConstraintComponent::In(values)] if values.is_empty())
    );
    let universal = shape_dataset(&format!(
        "g:value a logic:DatatypeProperty ; logic:range [ logic:unionOf (rdf:langString rdfs:Resource) ] . {closed}"
    ));
    assert!(derive_validation_shapes(&universal).unwrap().is_empty());
}

#[test]
fn derive_class_target_stays_sh_class() {
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes)
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "a real class target must emit sh:class"
    );
}

#[test]
fn derived_shape_failure_class_dedupes_identical_values() {
    let ds = shape_dataset(
        "g:Widget a owl:Class ;
             g:enforcesFailureClass g:Failure, g:Failure ;
             rdfs:subClassOf [ a owl:Restriction ; owl:onProperty g:id ; owl:minCardinality 1 ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(
            |shape| matches!(&shape.target, ShapeTarget::Class(class) if class.ends_with("Widget")),
        )
        .expect("Widget shape");
    assert_eq!(
        shape.failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/Failure")
    );
}

/// A domain shape's focus set is "every subject of P", so the PROPERTY is the only authored
/// term the shape belongs to — the failure identity must be readable there, exactly as a
/// class-targeted shape reads it off its class. Without this, the findings of a
/// property-scoped law resolve to no failure class at all, and a conformance cell pinning
/// `gmeow:expectedFailureClass` cannot say which class the gate just proved.
#[test]
fn derived_domain_shape_carries_the_propertys_failure_class() {
    let ds = shape_dataset(
        "g:lowersTo g:enforcesFailureClass g:UndeclaredLowering ;
             rdfs:domain [ a owl:Restriction ; owl:onProperty g:kind ; owl:minCardinality 1 ] .
         [] a logic:ClosureEntry ;
             logic:closureKey \"https://blackcatinformatics.ca/gmeow/lowersTo\" ;
             logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(
            |shape| matches!(&shape.target, ShapeTarget::SubjectsOf(p) if p.ends_with("lowersTo")),
        )
        .expect("lowersTo domain shape");
    assert_eq!(
        shape.failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/UndeclaredLowering")
    );
}

/// The range (`sh:targetObjectsOf`) counterpart reads the same term for the same reason.
#[test]
fn derived_range_shape_carries_the_propertys_failure_class() {
    let ds = shape_dataset(
        "g:Formula a owl:Class .
         g:lowersTo g:enforcesFailureClass g:UndeclaredLowering ; rdfs:range g:Formula .
         [] a logic:ClosureEntry ;
             logic:closureKey \"https://blackcatinformatics.ca/gmeow/lowersTo\" ;
             logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|shape| matches!(&shape.target, ShapeTarget::ObjectsOf(p) if p.ends_with("lowersTo")))
        .expect("lowersTo range shape");
    assert_eq!(
        shape.failure_class.as_deref(),
        Some("https://blackcatinformatics.ca/gmeow/UndeclaredLowering")
    );
}

/// A property carrying two distinct failure identities is as malformed as a class carrying
/// two: the shape would have to claim it proves both, so it is a hard error, never a pick.
#[test]
fn derived_domain_shape_rejects_distinct_property_failure_classes() {
    let ds = shape_dataset(
        "g:lowersTo g:enforcesFailureClass g:FailureA, g:FailureB ;
             rdfs:domain [ a owl:Restriction ; owl:onProperty g:kind ; owl:minCardinality 1 ] .
         [] a logic:ClosureEntry ;
             logic:closureKey \"https://blackcatinformatics.ca/gmeow/lowersTo\" ;
             logic:closureValue logic:ClosedWorldClosure .",
    );
    let err = derive_validation_shapes(ds.as_ref()).expect_err("distinct metadata must fail");
    assert!(err.message().contains("distinct"), "{err}");
}

#[test]
fn derived_shape_failure_class_rejects_distinct_values() {
    let ds = shape_dataset(
        "g:Widget a owl:Class ;
             g:enforcesFailureClass g:FailureA, g:FailureB ;
             rdfs:subClassOf [ a owl:Restriction ; owl:onProperty g:id ; owl:minCardinality 1 ] .",
    );
    let err = derive_validation_shapes(ds.as_ref()).expect_err("distinct metadata must fail");
    assert!(err.message().contains("distinct"), "{err}");
}

#[test]
fn derive_owl_thing_target_lowers_to_node_kind_not_sh_class() {
    // A someValuesFrom owl:Thing is an intentionally-open range ("any individual"). Under
    // spec-conformant SHACL, sh:class owl:Thing would demand a never-materialized rdf:type
    // owl:Thing edge and flag every value; the faithful projection is sh:nodeKind
    // sh:BlankNodeOrIRI (any resource, not a literal).
    let ds = shape_dataset(
        "g:Observation a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:observedFeature ; owl:someValuesFrom owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::BlankNodeOrIri)
        )),
        "an owl:Thing target must emit sh:nodeKind sh:BlankNodeOrIRI: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(_))),
        "an owl:Thing target must never emit sh:class: {comps:?}"
    );
}

#[test]
fn derive_owl_thing_on_a_datatype_property_lowers_to_literal_node_kind() {
    // The two bounded universal tops sit on opposite sides of the object/data divide:
    // `owl:Thing` tops the INDIVIDUAL domain, `rdfs:Literal` the DATA domain. An
    // `owl:someValuesFrom owl:Thing` authored on a declared `owl:DatatypeProperty` means "any
    // value" in the only domain that property has, so its faithful projection is
    // `sh:nodeKind sh:Literal`. The individual-domain reading (`sh:BlankNodeOrIRI`) there is
    // an INVERTED constraint — it rejects every correct literal the property is declared to
    // carry, which is exactly how the shipped `gmeow:Chunk` shape came to reject a correct
    // integer `gmeow:spanStart` / `gmeow:spanEnd` offset.
    let ds = shape_dataset(
        "g:spanStart a owl:DatatypeProperty ; rdfs:range xsd:nonNegativeInteger .\n\
         g:Chunk a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:spanStart ; owl:someValuesFrom owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::Literal)
        )),
        "an owl:Thing filler on a declared owl:DatatypeProperty must emit \
         sh:nodeKind sh:Literal: {comps:?}"
    );
    assert!(
        !comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::BlankNodeOrIri)
        )),
        "an owl:Thing filler on a declared owl:DatatypeProperty must never emit \
         sh:nodeKind sh:BlankNodeOrIRI — it would reject every literal value: {comps:?}"
    );
}

#[test]
fn derive_span_offset_axioms_project_the_declared_data_range() {
    // The shipped `gmeow:Chunk` offset axioms, verbatim in shape: a data-range-qualified
    // exact-one (`owl:onDataRange`, min + max) plus the existential that carries the same
    // range. Both arms must land on ONE `sh:datatype xsd:nonNegativeInteger` (deduped by
    // `merge_same_path_properties`) with `sh:minCount 1 ; sh:maxCount 1` — and on NO node
    // kind at all, since a node kind is the strictly weaker reading and the closed-world
    // JSON-Schema projection of a bare `sh:nodeKind sh:Literal` cannot express a numeric
    // value (it admits only a string or a `{"@value": …}` object), so it would reject the
    // very integer offsets the SHACL shape accepts.
    let ds = shape_dataset(
        "g:spanStart a owl:DatatypeProperty ; rdfs:range xsd:nonNegativeInteger .\n\
         g:Chunk a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:spanStart ; \
           owl:minQualifiedCardinality 1 ; owl:onDataRange xsd:nonNegativeInteger ] , \
         [ a owl:Restriction ; owl:onProperty g:spanStart ; \
           owl:maxQualifiedCardinality 1 ; owl:onDataRange xsd:nonNegativeInteger ] , \
         [ a owl:Restriction ; owl:onProperty g:spanStart ; \
           owl:someValuesFrom xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let offsets: Vec<_> = shapes
        .iter()
        .flat_map(|s| s.properties.iter())
        .filter(|p| p.path == "https://blackcatinformatics.ca/gmeow/spanStart")
        .collect();
    assert_eq!(
        offsets.len(),
        1,
        "the three arms merge into ONE property shape: {offsets:?}"
    );
    let offset = offsets[0];
    assert_eq!(offset.min_count, Some(1), "{offset:?}");
    assert_eq!(offset.max_count, Some(1), "{offset:?}");
    assert_eq!(
        offset.components,
        vec![ConstraintComponent::Datatype(
            "http://www.w3.org/2001/XMLSchema#nonNegativeInteger".to_owned()
        )],
        "the offset must project exactly one sh:datatype and no node kind: {offset:?}"
    );
}

#[test]
fn derive_owl_thing_on_an_object_property_keeps_the_individual_top() {
    // The redirect above is keyed on the property's OWN declaration, so it can never narrow an
    // object-valued path: a declared `owl:ObjectProperty` keeps the individual-domain reading.
    let ds = shape_dataset(
        "g:chunkOf a owl:ObjectProperty .\n\
         g:Chunk a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:chunkOf ; owl:someValuesFrom owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::BlankNodeOrIri)
        )),
        "an owl:Thing filler on a declared owl:ObjectProperty must keep \
         sh:nodeKind sh:BlankNodeOrIRI: {comps:?}"
    );
    assert!(
        !comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::Literal)
        )),
        "an owl:Thing filler on an object property must never emit sh:nodeKind sh:Literal: \
         {comps:?}"
    );
}

#[test]
fn derive_rdfs_literal_target_lowers_to_node_kind_not_sh_datatype() {
    // A someValuesFrom rdfs:Literal is an intentionally-open literal range ("any literal").
    // Under spec-conformant SHACL, sh:datatype rdfs:Literal never matches a concrete literal
    // (rdfs:Literal is the class of all literals, not a lexical datatype), so it would flag
    // every value; the faithful projection is sh:nodeKind sh:Literal.
    let ds = shape_dataset(
        "g:Artifact a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:artifactMediaType ; owl:someValuesFrom rdfs:Literal ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::Literal)
        )),
        "an rdfs:Literal target must emit sh:nodeKind sh:Literal: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(_))),
        "an rdfs:Literal target must never emit sh:datatype: {comps:?}"
    );
}

#[test]
fn derive_owl_thing_target_lowers_to_node_kind_even_when_classified_as_datatype() {
    // Robustness guard for the universal-top projection. `is_datatype` treats anything typed
    // `a rdfs:Datatype` as a datatype range, so a quirk axiom `owl:Thing a rdfs:Datatype`
    // classifies the range with the datatype flag set. The projection must still emit the
    // faithful open node-kind (sh:BlankNodeOrIRI) rather than falling through to a vacuous
    // `sh:datatype owl:Thing` — i.e. the special-case guard is decoupled from the flag.
    let ds = shape_dataset(
        "owl:Thing a rdfs:Datatype .\n\
         g:Observation a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:observedFeature ; owl:someValuesFrom owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps = all_components(&shapes);
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::NodeKindShacl(crate::ir::ShaclNodeKind::BlankNodeOrIri)
        )),
        "an owl:Thing target must emit sh:nodeKind sh:BlankNodeOrIRI even when flagged a datatype: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(_))),
        "an owl:Thing target must never emit sh:datatype: {comps:?}"
    );
}

#[test]
fn derive_owl_cardinality_lifts_with_owl_provenance() {
    // Unqualified owl:min/maxCardinality lower to sh:minCount/sh:maxCount tagged as
    // OwlRestriction provenance — the open-world axiom read closed-world (ValidationOnly).
    let ds = shape_dataset(
        "g:Rec a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:id ; \
           owl:minCardinality \"1\"^^xsd:nonNegativeInteger ; \
           owl:maxCardinality \"3\"^^xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let card = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.min_count.is_some() || p.max_count.is_some())
        .expect("a cardinality property shape");
    assert_eq!(card.min_count, Some(1));
    assert_eq!(card.max_count, Some(3));
    assert_eq!(
        card.cardinality_provenance,
        Some(ConstraintProvenance::OwlRestriction),
        "OWL-restriction cardinality is a closed-world reading, never OptNative"
    );
}

#[test]
fn derive_merges_cardinality_and_class_on_same_path_into_one_property() {
    // A class that authors a cardinality restriction AND an owl:allValuesFrom class restriction on
    // ONE property must project to a SINGLE property shape carrying both — the conjunctive SHACL
    // reading of a single hand-authored `sh:property` block. Unmerged, the two same-path property
    // shapes would key distinctly from that block and defeat enforcement equivalence.
    let ds = shape_dataset(
        "g:StandpointClaim a owl:Class . \
         g:InferenceCommitment a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:conclusion ; \
           owl:cardinality \"1\"^^xsd:nonNegativeInteger ] , \
         [ a owl:Restriction ; owl:onProperty g:conclusion ; \
           owl:allValuesFrom g:StandpointClaim ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("InferenceCommitment")))
        .expect("a Class(InferenceCommitment) shape");
    let on_conclusion: Vec<_> = shape
        .properties
        .iter()
        .filter(|p| p.path.ends_with("conclusion"))
        .collect();
    assert_eq!(
        on_conclusion.len(),
        1,
        "cardinality + class on one path must merge into ONE property shape: {:?}",
        shape.properties
    );
    let p = on_conclusion[0];
    assert_eq!(p.min_count, Some(1), "exact cardinality gives min 1");
    assert_eq!(p.max_count, Some(1), "exact cardinality gives max 1");
    assert!(
        p.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("StandpointClaim"))),
        "the merged property carries the sh:class component: {:?}",
        p.components
    );
}

#[test]
fn derive_contradictory_cardinality_hard_fails() {
    // min > max is structurally impossible: the derivation HARD-FAILS rather than silently
    // dropping the malformed restriction (no fail-soft).
    let ds = shape_dataset(
        "g:Bad a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:id ; \
           owl:minCardinality \"5\"^^xsd:nonNegativeInteger ; \
           owl:maxCardinality \"2\"^^xsd:nonNegativeInteger ] .",
    );
    assert!(
        derive_validation_shapes(ds.as_ref()).is_err(),
        "min > max cardinality must hard-fail, not drop the shape"
    );
}

// ── Full closed-world OWL fragment: per family ───────────────────────────────

#[test]
fn derive_some_values_from_uses_class_membership_under_approximation() {
    // owl:someValuesFrom is EXISTENTIAL ("K ⊑ ∃P.C"), but a validation shape is a
    // `logic:ValidationOnly` UNDER-approximation that must never over-claim: a `qualifiedMinCount 1`
    // existential would false-positive on the ontology's own open-world value-vocabulary
    // individuals (instances of a restricted class that legitimately do not populate the relation).
    // So the shape projects the class-membership under-approximation (a bare sh:class, vacuously
    // true when the property is absent); the existence obligation is carried in the canon, not the
    // shape. It must NOT be wrapped in a qualified value shape.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps: Vec<_> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "someValuesFrom emits the class-membership under-approximation (bare sh:class): {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "someValuesFrom must NOT over-claim with a qualifiedMinCount existential: {comps:?}"
    );
}

#[test]
fn derive_class_scoped_closed_all_values_from_requires_the_path() {
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:allValuesFrom g:Doc ] . \
         [] a logic:ClosureEntry ; logic:onClass g:Article ; logic:closureKey g:cites ; \
            logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let cites = shapes
        .iter()
        .flat_map(|shape| &shape.properties)
        .find(|property| property.path.ends_with("cites"))
        .expect("class-scoped closed universal projects a cites property shape");
    assert_eq!(cites.min_count, Some(1));
    assert!(
        cites.components.iter().any(
            |component| matches!(component, ConstraintComponent::Class(iri) if iri.ends_with("Doc"))
        ),
        "class-scoped closed universal retains its value class: {:?}",
        cites.components
    );
}

/// INVARIANT: an individual-scoped closure entry does not widen to the property or the class.
/// Contract-owned entries that close `g:cites` for one resource, one with and one without
/// `logic:onClass`, derive no closed-world range opt-in for the property and no class-wide
/// required path, while the same property and restriction derive their shapes as usual.
#[test]
fn individual_scoped_closure_entries_do_not_widen_to_property_or_class() {
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:cites a owl:ObjectProperty ; rdfs:range g:Doc . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:allValuesFrom g:Doc ] . \
         g:contract a logic:ReasoningContract ; \
            logic:closureEntry g:propertyEntry , g:classEntry . \
         g:propertyEntry a logic:ClosureEntry ; logic:closureIndividual g:article1 ; \
            logic:closureKey g:cites ; logic:closureValue logic:ClosedWorldClosure . \
         g:classEntry a logic:ClosureEntry ; logic:onClass g:Article ; \
            logic:closureIndividual g:article1 ; \
            logic:closureKey g:cites ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|shape| matches!(
            &shape.target,
            ShapeTarget::SubjectsOf(path) | ShapeTarget::ObjectsOf(path) if path.ends_with("cites")
        )),
        "an individual-scoped entry must not opt the property into a closed-world range/domain \
         reading: {:?}",
        shapes.iter().map(|shape| &shape.target).collect::<Vec<_>>()
    );
    let cites = shapes
        .iter()
        .flat_map(|shape| &shape.properties)
        .find(|property| property.path.ends_with("cites"))
        .expect("the open-world allValuesFrom still projects a cites property shape");
    assert_eq!(
        cites.min_count, None,
        "an individual-scoped entry must not make the path required for the whole class"
    );
}

#[test]
fn derive_all_values_from_emits_bare_class_not_wrapped() {
    // owl:allValuesFrom is UNIVERSAL: every value is a g:Doc → a bare sh:class on the path,
    // never wrapped in a qualified value shape.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:allValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps: Vec<_> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "allValuesFrom emits a bare sh:class: {comps:?}"
    );
    assert!(
        !comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "allValuesFrom must NOT wrap in a qualified value shape: {comps:?}"
    );
}

#[test]
fn derive_rdfs_domain_targets_subjects_of_with_node_class() {
    // domain/range are open-world by default (inference axioms); a shape is derived only for a
    // property opted IN via a `logic:ClosedWorldClosure` closure entry.
    let ds = shape_dataset(
        "g:Doc a owl:Class . g:cites a owl:ObjectProperty ; rdfs:domain g:Doc .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:cites ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("cites")))
        .expect("a SubjectsOf(cites) shape");
    assert!(
        shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "domain must attach a node-level sh:class g:Doc: {:?}",
        shape.node_components
    );
}

#[test]
fn derive_rdfs_domain_range_is_open_world_by_default_without_optin() {
    // rdfs:domain/range are inference axioms → OPEN-WORLD by default: with NO ClosedWorldClosure
    // opt-in, neither a SubjectsOf(domain) nor an ObjectsOf(range) shape is derived. (Falsifiable
    // proof of the open-world default; the opt-in tests above show the closed reading fires.)
    let ds = shape_dataset(
        "g:Doc a owl:Class . g:cites a owl:ObjectProperty ; rdfs:domain g:Doc ; rdfs:range g:Doc .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::SubjectsOf(p) | ShapeTarget::ObjectsOf(p) if p.ends_with("cites")
        )),
        "domain/range must derive NO shape without a ClosedWorldClosure opt-in: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
}

#[test]
fn derive_rdfs_range_targets_objects_of_class_and_datatype() {
    // A class range → sh:class node component on an ObjectsOf shape. domain/range are open-world
    // by default; opt the property IN with a `logic:ClosedWorldClosure` closure entry.
    let class_ds = shape_dataset(
        "g:Doc a owl:Class . g:cites a owl:ObjectProperty ; rdfs:range g:Doc .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:cites ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let class_shapes = derive_validation_shapes(class_ds.as_ref()).expect("derive ok");
    let class_shape = class_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("cites")))
        .expect("an ObjectsOf(cites) shape");
    assert!(
        class_shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "class range → sh:class node component: {:?}",
        class_shape.node_components
    );
    // An xsd:integer range → sh:datatype node component.
    let dt_ds = shape_dataset(
        "g:blockNumber a owl:DatatypeProperty ; rdfs:range xsd:integer .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:blockNumber ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let dt_shapes = derive_validation_shapes(dt_ds.as_ref()).expect("derive ok");
    let dt_shape = dt_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("blockNumber")))
        .expect("an ObjectsOf(blockNumber) shape");
    assert!(
        dt_shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(d) if d.ends_with("integer"))),
        "datatype range → sh:datatype node component: {:?}",
        dt_shape.node_components
    );
}

#[test]
fn derive_rdfs_resource_range_domain_is_vacuous_no_class_component() {
    // rdfs:Resource is the UNIVERSAL TOP (the class of everything, literals included). A
    // range/domain of rdfs:Resource is VACUOUS: `sh:class rdfs:Resource` would demand a
    // never-materialized `rdf:type rdfs:Resource` edge and false-positive universally, so the
    // the derivation must emit NO node/class component for it.
    // Opt the property IN to closed-world domain/range reading so a shape IS derived and the
    // "no vacuous rdfs:Resource class component" assertion below stays falsifiable.
    let range_ds = shape_dataset(
        "g:usesTerm a owl:ObjectProperty ; rdfs:range rdfs:Resource .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:usesTerm ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let range_shapes = derive_validation_shapes(range_ds.as_ref()).expect("derive ok");
    if let Some(shape) = range_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("usesTerm")))
    {
        assert!(
            !shape.node_components.iter().any(|c| matches!(
                c,
                ConstraintComponent::Class(d) if d.ends_with("Resource")
            )),
            "rdfs:Resource range must emit NO sh:class node component (vacuous top): {:?}",
            shape.node_components
        );
    }
    // Same for a domain of rdfs:Resource.
    let domain_ds = shape_dataset(
        "g:usesTerm a owl:ObjectProperty ; rdfs:domain rdfs:Resource .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:usesTerm ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let domain_shapes = derive_validation_shapes(domain_ds.as_ref()).expect("derive ok");
    if let Some(shape) = domain_shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("usesTerm")))
    {
        assert!(
            !shape.node_components.iter().any(|c| matches!(
                c,
                ConstraintComponent::Class(d) if d.ends_with("Resource")
            )),
            "rdfs:Resource domain must emit NO sh:class node component (vacuous top): {:?}",
            shape.node_components
        );
    }
}

#[test]
fn derive_functional_property_subjects_of_max_one() {
    // A functional characteristic is read from the canonical logic: carrier (a
    // logic:PropertyCharacteristicAssertion joining logic:characterizes + logic:characteristicSort),
    // NOT the deprecated owl:FunctionalProperty marker. With no rdfs:domain, only the
    // property-scoped SubjectsOf(P) cap is derived.
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:id ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("id")))
        .expect("a SubjectsOf(id) shape");
    let pc = shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("id"))
        .expect("a property on id");
    assert_eq!(pc.max_count, Some(1), "functional → sh:maxCount 1");
    assert!(!pc.inverse, "functional is a forward path, not inverse");
}

#[test]
fn derive_functional_property_deprecated_owl_marker_is_not_projected() {
    // The bare owl:FunctionalProperty marker no longer projects a cap: it is a deprecated source,
    // superseded by the logic: carrier. A property carrying ONLY the marker (no carrier record)
    // derives no functional shape at all.
    let ds = shape_dataset("g:id a owl:FunctionalProperty .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::SubjectsOf(p) if p.ends_with("id")
        )),
        "deprecated owl:FunctionalProperty marker must not project a SubjectsOf cap: {shapes:?}"
    );
}

#[test]
fn derive_inverse_functional_property_objects_of_inverted_max_one() {
    // Inverse-functional is likewise read from the logic: carrier (zero live instances in the
    // repo; a tested capability). With no rdfs:range, only the property-scoped ObjectsOf(P) cap.
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:isbn ; \
             logic:characteristicSort logic:inverseFunctionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("isbn")))
        .expect("an ObjectsOf(isbn) shape");
    let pc = shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("isbn"))
        .expect("a property on isbn");
    assert_eq!(pc.max_count, Some(1), "inverse-functional → sh:maxCount 1");
    assert!(pc.inverse, "inverse-functional is an inverted path");
}

#[test]
fn derive_functional_carrier_caps_domain_class_node_shape() {
    // The functional cap must ALSO land on the domain CLASS node shape (sh:targetClass C), which is
    // what the declarative class-node reader (Pydantic/ShEx) consults to narrow the field to scalar.
    // A functional carrier record on P + P rdfs:domain C ⇒ the {C} Class shape carries a forward
    // maxCount=1 on P (in addition to the property-scoped SubjectsOf(P) cap).
    let ds = shape_dataset(
        "g:Book a owl:Class . \
         g:primaryAuthor rdfs:domain g:Book .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:primaryAuthor ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let class_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Book")))
        .expect("a Class(Book) shape");
    let pc = class_shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("primaryAuthor"))
        .expect("Class(Book) carries a property on primaryAuthor");
    assert_eq!(
        pc.max_count,
        Some(1),
        "functional cap lands on the domain class node shape"
    );
    assert!(!pc.inverse, "a forward functional cap, not inverse");
    // And the property-scoped cap is still present.
    assert!(
        shapes.iter().any(
            |s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("primaryAuthor"))
                && s.properties
                    .iter()
                    .any(|p| p.path.ends_with("primaryAuthor") && p.max_count == Some(1))
        ),
        "the property-scoped SubjectsOf(primaryAuthor) cap is still emitted: {shapes:?}"
    );
}

#[test]
fn derive_functional_carrier_no_domain_fabricates_no_class() {
    // A functional property with NO rdfs:domain (the gmeow:unit shape) gets ONLY the property-scoped
    // cap — no class node shape is synthesized (there is no domain class to attach it to).
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:unit ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("unit"))),
        "the property-scoped SubjectsOf(unit) cap is emitted"
    );
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::Class(_))),
        "no class node shape is fabricated for a domain-less functional property: {shapes:?}"
    );
}

#[test]
fn derive_inverse_functional_carrier_caps_range_class_node_shape() {
    // The inverse-functional cap must ALSO land on the range CLASS node shape as an INVERTED
    // maxCount=1: an inverse-functional carrier record on P + P rdfs:range C ⇒ the {C} Class shape
    // carries an inverted maxCount=1 on P.
    let ds = shape_dataset(
        "g:Isbn a owl:Class . \
         g:isbn rdfs:range g:Isbn .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:isbn ; \
             logic:characteristicSort logic:inverseFunctionalProperty .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let class_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Isbn")))
        .expect("a Class(Isbn) shape");
    let pc = class_shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("isbn"))
        .expect("Class(Isbn) carries a property on isbn");
    assert_eq!(pc.max_count, Some(1), "inverse-functional cap → maxCount 1");
    assert!(pc.inverse, "inverse-functional cap is an inverted path");
}

#[test]
fn derive_colourspace_maxqualified_thing_idiom_unchanged_by_carrier() {
    // The colourspace idiom — a NON-functional property capped by a class-scoped
    // `logic:maxQualifiedCardinality 1 ; logic:onClass owl:Thing` restriction — is derived by the
    // FAMILY 1 restriction walk and is UNCHANGED by the functional-carrier rewrite (there is no
    // functional carrier record; the cap comes from the restriction, not a characteristic).
    let ds = shape_dataset(
        "g:Availability a owl:Class ; rdfs:subClassOf \
         [ a logic:Restriction ; logic:onProperty g:availabilityStatus ; \
           logic:maxQualifiedCardinality 1 ; logic:onClass owl:Thing ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let class_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Availability")))
        .expect("a Class(Availability) shape");
    let pc = class_shape
        .properties
        .iter()
        .find(|p| p.path.ends_with("availabilityStatus"))
        .expect("Class(Availability) carries a property on availabilityStatus");
    assert_eq!(
        pc.max_count,
        Some(1),
        "maxQualifiedCardinality 1 ; onClass owl:Thing degrades to a plain sh:maxCount 1"
    );
    assert!(!pc.inverse, "the restriction cap is a forward path");
    // No functional carrier record exists, so no property-scoped SubjectsOf cap is spuriously added.
    assert!(
        !shapes.iter().any(
            |s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("availabilityStatus"))
        ),
        "the non-functional property gets no SubjectsOf cap: {shapes:?}"
    );
}

#[test]
fn functional_completeness_invariant_flags_carrierless_and_clears_when_carried() {
    // An owl:FunctionalProperty declaration with NO logic: carrier record is an authoring gap: the
    // invariant returns the offending property.
    let gap = shape_dataset("g:legacyId a owl:FunctionalProperty .");
    let missing = functional_properties_missing_logic_carrier(gap.as_ref());
    assert!(
        missing.iter().any(|p| p.ends_with("legacyId")),
        "carrierless owl:FunctionalProperty is flagged: {missing:?}"
    );
    // Add the carrier record and the gap clears.
    let carried = shape_dataset(
        "g:legacyId a owl:FunctionalProperty .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:legacyId ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    assert!(
        functional_properties_missing_logic_carrier(carried.as_ref()).is_empty(),
        "a carrier record clears the authoring gap"
    );
}

#[test]
fn functional_carrier_integrity_flags_reintroduced_owl_marker() {
    // The retained RE-introduction guard: a bare owl:FunctionalProperty declaration with no
    // carrier is a ReintroducedOwlMarker violation — the invariant still bites if the deprecated
    // marker returns.
    let ds = shape_dataset("g:legacyId a owl:FunctionalProperty .");
    let violations = functional_carrier_integrity(ds.as_ref());
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::ReintroducedOwlMarker { property } if property.ends_with("legacyId")
        )),
        "re-introduced owl:FunctionalProperty is a ReintroducedOwlMarker violation: {violations:?}"
    );
}

#[test]
fn functional_carrier_integrity_flags_orphan_carrier() {
    // A functional carrier whose logic:characterizes names an IRI declared by NO property type is
    // an OrphanCarrier violation (a misspelled / never-declared target).
    let ds = shape_dataset(
        "[] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:neverDeclaredProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let violations = functional_carrier_integrity(ds.as_ref());
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::OrphanCarrier { property } if property.ends_with("neverDeclaredProp")
        )),
        "a carrier characterizing an undeclared IRI is an OrphanCarrier violation: {violations:?}"
    );
    // A declared property clears the orphan check (only the ledger-drift noise remains for it).
    let declared = shape_dataset(
        "g:realProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:realProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    assert!(
        !functional_carrier_integrity(declared.as_ref()).iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::OrphanCarrier { property } if property.ends_with("realProp")
        )),
        "a declared property is not an orphan"
    );
}

#[test]
fn functional_carrier_integrity_flags_duplicate_carrier() {
    // Two functional carrier records naming the same property is a DuplicateCarrier violation.
    let ds = shape_dataset(
        "g:dupProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:dupProp ; \
             logic:characteristicSort logic:functionalProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:dupProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let violations = functional_carrier_integrity(ds.as_ref());
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::DuplicateCarrier { property, count }
                if property.ends_with("dupProp") && *count == 2
        )),
        "two carriers for one property is a DuplicateCarrier violation with count 2: {violations:?}"
    );
    // A single carrier for the same property does not trip the duplicate check.
    let single = shape_dataset(
        "g:dupProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:dupProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    assert!(
        !functional_carrier_integrity(single.as_ref()).iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::DuplicateCarrier { property, .. } if property.ends_with("dupProp")
        )),
        "a single carrier is not a duplicate"
    );
}

#[test]
fn functional_carrier_ledger_drift_names_missing_and_unexpected() {
    // Prove the completeness ledger is NON-VACUOUS: a small store carries NONE of the frozen
    // ledger's properties, so every ledger entry surfaces as a LedgerMissing that NAMES it —
    // the exact "a property silently lost its carrier" hard-fail. The store's own lone carrier

    // (g:unexpectedProp, absent from the ledger) surfaces as a LedgerUnexpected that names it.
    //
    // The expected count is READ FROM THE LEDGER, never restated as a literal here. The claim is
    // "EVERY entry surfaces", which is a statement about the ledger's own size; a second copy of
    // that size drifts silently the moment the ledger is re-blessed (it did, and this test then
    // failed for a reason that had nothing to do with the behaviour it guards). Non-vacuity is
    // carried by the two assertions below — a non-empty ledger naming a specific property — not
    // by a hand-copied number.
    let ledger_size = functional_carrier_ledger().len();
    assert!(
        ledger_size >= 500,
        "the frozen ledger must be substantial for this non-vacuity proof to mean anything; \
         got {ledger_size} entries"
    );
    let ds = shape_dataset(
        "g:unexpectedProp a owl:ObjectProperty . \
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:unexpectedProp ; \
             logic:characteristicSort logic:functionalProperty .",
    );
    let violations = functional_carrier_integrity(ds.as_ref());
    let missing: Vec<&String> = violations
        .iter()
        .filter_map(|v| match v {
            FunctionalCarrierViolation::LedgerMissing { property } => Some(property),
            _ => None,
        })
        .collect();
    let ledger_size = super::functional_carrier_ledger().len();
    assert!(
        ledger_size > 0,
        "non-vacuity: the frozen functional-carrier ledger must not be empty"
    );
    assert_eq!(
        missing.len(),
        ledger_size,
        "every frozen ledger entry with no live carrier is named as LedgerMissing"
    );
    assert!(
        missing.iter().any(|p| p.ends_with("acceptanceStatus")),
        "a specific ledger property is named as missing when its carrier is absent"
    );
    assert!(
        violations.iter().any(|v| matches!(
            v,
            FunctionalCarrierViolation::LedgerUnexpected { property } if property.ends_with("unexpectedProp")
        )),
        "an un-blessed carrier surfaces as a LedgerUnexpected naming it: {violations:?}"
    );
}

#[test]
fn derive_has_value_iri_emits_sh_has_value() {
    let ds = shape_dataset(
        "g:Publisher a owl:Class . \
         g:Book a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:publisher ; owl:hasValue g:Publisher ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes).iter().any(|c| matches!(
            c,
            ConstraintComponent::HasValue(crate::ir::ShapeValue::Iri(v)) if v.ends_with("Publisher")
        )),
        "owl:hasValue (IRI) → sh:hasValue: {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_has_value_typed_literal_preserves_datatype() {
    // A typed fixed value `owl:hasValue "1"^^xsd:integer` must derive a TYPED sh:hasValue,
    // carrying the datatype IRI — never a bare untyped `"1"` (which would match the wrong
    // literal). This exercises the graphutil `Node::Lit` datatype-preservation fix.
    let ds = shape_dataset(
        "g:Prob a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:totalMass ; owl:hasValue \"1\"^^xsd:integer ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let has_typed = all_components(&shapes).iter().any(|c| matches!(
        c,
        ConstraintComponent::HasValue(crate::ir::ShapeValue::Literal(purrdf::RdfLiteral { lexical_form: lexical, datatype, language: lang, direction: None, }))
            if lexical == "1"
                && datatype.as_deref() == Some("http://www.w3.org/2001/XMLSchema#integer")
                && lang.is_none()
    ));
    assert!(
        has_typed,
        "owl:hasValue \"1\"^^xsd:integer must derive a TYPED sh:hasValue (datatype preserved): {:?}",
        all_components(&shapes)
    );
    // The projected SHACL surface carries the datatype, not a bare untyped literal.
    let ttl = crate::projections::shapes::project_validation_shapes_shacl(
        &crate::ir::LogicProgram::new(vec![], vec![], vec![], None).with_validation_shapes(shapes),
    );
    assert!(
        ttl.contains("sh:hasValue \"1\"^^<http://www.w3.org/2001/XMLSchema#integer>"),
        "the derived SHACL must carry the typed literal: {ttl}"
    );
}

#[test]
fn derive_has_value_plain_literal_stays_untyped() {
    // A plain `xsd:string` fixed value normalizes to an untyped carrier (datatype None), so an
    // authored `"foo"` and an equivalent `"foo"^^xsd:string` derive the same untyped sh:hasValue.
    let ds = shape_dataset(
        "g:Doc a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:label ; owl:hasValue \"foo\" ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let untyped = all_components(&shapes).iter().any(|c| matches!(
        c,
        ConstraintComponent::HasValue(crate::ir::ShapeValue::Literal(purrdf::RdfLiteral { lexical_form: lexical, datatype, language: lang, direction: None, }))
            if lexical == "foo" && datatype.is_none() && lang.is_none()
    ));
    assert!(
        untyped,
        "a plain literal must derive an untyped sh:hasValue (datatype None): {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_has_value_blank_hard_fails() {
    // A fixed value cannot be an anonymous node — hard-fail, never a silent drop.
    let ds = shape_dataset(
        "g:Book a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:publisher ; owl:hasValue [ a owl:Thing ] ] .",
    );
    assert!(
        derive_validation_shapes(ds.as_ref()).is_err(),
        "owl:hasValue on a blank node must hard-fail"
    );
}

#[test]
fn derive_disjoint_with_emits_not_class() {
    let ds =
        shape_dataset("g:Animal a owl:Class . g:Plant a owl:Class ; owl:disjointWith g:Animal .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes).iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("Animal"))
        )),
        "owl:disjointWith → sh:not [ sh:class D ]: {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_complement_of_emits_not_class() {
    let ds = shape_dataset("g:Dead a owl:Class . g:Alive a owl:Class ; owl:complementOf g:Dead .");
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        all_components(&shapes).iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("Dead"))
        )),
        "owl:complementOf → sh:not [ sh:class D ]: {:?}",
        all_components(&shapes)
    );
}

#[test]
fn derive_one_of_emits_sh_in() {
    let ds = shape_dataset(
        "g:Suit a owl:Class ; owl:oneOf ( g:Hearts g:Spades ) . \
         g:Hearts a owl:Thing . g:Spades a owl:Thing .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let in_comp = all_components(&shapes)
        .into_iter()
        .find(|c| matches!(c, ConstraintComponent::In(_)))
        .expect("an sh:in component");
    match in_comp {
        ConstraintComponent::In(members) => {
            assert!(
                members
                    .iter()
                    .any(|v| matches!(v, crate::ir::ShapeValue::Iri(i) if i.ends_with("Hearts")))
                    && members.iter().any(
                        |v| matches!(v, crate::ir::ShapeValue::Iri(i) if i.ends_with("Spades"))
                    ),
                "sh:in must carry both enumerated members: {members:?}"
            );
        }
        other => panic!("expected In, got {other:?}"),
    }
}

#[test]
fn derive_all_disjoint_classes_cross_links_not_class() {
    let ds = shape_dataset(
        "g:A a owl:Class . g:B a owl:Class . \
         [] a owl:AllDisjointClasses ; owl:members ( g:A g:B ) .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let a_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("gmeow/A")))
        .expect("a shape for g:A");
    assert!(
        a_shape.node_components.iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("gmeow/B"))
        )),
        "g:A must carry sh:not [ sh:class g:B ]: {:?}",
        a_shape.node_components
    );
    let b_shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("gmeow/B")))
        .expect("a shape for g:B");
    assert!(
        b_shape.node_components.iter().any(|c| matches!(
            c,
            ConstraintComponent::Not(inner)
                if matches!(inner.as_ref(), ConstraintComponent::Class(d) if d.ends_with("gmeow/A"))
        )),
        "g:B must carry sh:not [ sh:class g:A ]: {:?}",
        b_shape.node_components
    );
}

#[test]
fn derive_qualified_cardinality_emits_qualified_value_shape() {
    let ds = shape_dataset(
        "g:Wheel a owl:Class . \
         g:Car a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:hasPart ; owl:onClass g:Wheel ; \
           owl:qualifiedCardinality \"1\"^^xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comp = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .find(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. }))
        .expect("a qualified value shape");
    match comp {
        ConstraintComponent::QualifiedValueShape { shape, min, max } => {
            assert_eq!(*min, Some(1));
            assert_eq!(*max, Some(1));
            assert!(
                shape
                    .iter()
                    .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Wheel"))),
                "qualified inner shape must be sh:class g:Wheel: {shape:?}"
            );
        }
        other => panic!("expected QualifiedValueShape, got {other:?}"),
    }
    // And it carries NO plain min/max_count (the count is on the qualified values).
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.path.ends_with("hasPart"))
        .expect("a property on hasPart");
    assert_eq!(
        pc.min_count, None,
        "qualified count is not a plain min_count"
    );
    assert_eq!(
        pc.max_count, None,
        "qualified count is not a plain max_count"
    );
}

#[test]
fn derive_qualified_cardinality_on_data_range_emits_plain_datatype_and_count() {
    // `owl:maxQualifiedCardinality 1 ; owl:onDataRange xsd:decimal` is the DATATYPE-qualified peer
    // of `owl:onClass`. It must read as a PLAIN `sh:datatype xsd:decimal` + `sh:maxCount 1` (a bare
    // datatype the JSON-Schema deriver can read), never a hard-fail for "requires owl:onClass".
    let ds = shape_dataset(
        "g:Measurement a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:appraisalValue ; \
           owl:maxQualifiedCardinality \"1\"^^xsd:nonNegativeInteger ; \
           owl:onDataRange xsd:decimal ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("onDataRange must derive, not fail");
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.path.ends_with("appraisalValue"))
        .expect("a property on appraisalValue");
    assert_eq!(
        pc.max_count,
        Some(1),
        "maxQualifiedCardinality 1 → sh:maxCount 1"
    );
    assert_eq!(pc.min_count, None, "no min on a max-qualified cardinality");
    assert!(
        pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Datatype(d)
                if d == "http://www.w3.org/2001/XMLSchema#decimal")),
        "onDataRange xsd:decimal → sh:datatype xsd:decimal: {:?}",
        pc.components
    );
    assert!(
        !pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "a datatype filler degrades to a plain datatype, not a qualified value shape: {:?}",
        pc.components
    );
}

#[test]
fn derive_qualified_numeric_data_ranges_retains_union_and_integer_bounds() {
    let ds = shape_dataset(
        "g:Approximation a logic:Class ; logic:subClassOf \
         [ a logic:Restriction ; logic:onProperty g:error ; \
           logic:minQualifiedCardinality 1 ; \
           logic:onDataRange [ logic:unionOf ( xsd:decimal xsd:double xsd:float ) ] ] . \
         g:RootSystem a logic:Class ; logic:subClassOf \
         [ a logic:Restriction ; logic:onProperty g:rank ; \
           logic:minQualifiedCardinality 1 ; logic:onDataRange xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).unwrap();
    let property = |name: &str| {
        shapes
            .iter()
            .flat_map(|shape| &shape.properties)
            .find(|property| property.path.ends_with(name))
            .unwrap()
    };
    let error = property("error");
    assert_eq!(error.min_count, Some(1));
    let union = error
        .components
        .iter()
        .find_map(|component| match component {
            ConstraintComponent::Or(branches) => Some(branches),
            _ => None,
        })
        .expect("the numeric data range must survive projection");
    assert_eq!(union.len(), 3);
    for datatype in ["decimal", "double", "float"] {
        assert!(union.contains(&ConstraintComponent::Datatype(format!(
            "http://www.w3.org/2001/XMLSchema#{datatype}"
        ))));
    }
    let rank = property("rank");
    assert_eq!(rank.min_count, Some(1));
    assert!(rank.components.contains(&ConstraintComponent::Datatype(
        "http://www.w3.org/2001/XMLSchema#nonNegativeInteger".into()
    )));
    for shape in &shapes {
        let wire = crate::projections::shapes::project_validation_shape_shacl(shape);
        assert!(wire.contains("sh:datatype"), "{wire}");
        assert!(!wire.contains("sh:Literal"), "{wire}");
    }
}

#[test]
fn derive_qualified_cardinality_omits_plain_class_without_a_backing_universal() {
    // `owl:maxQualifiedCardinality 1 ; owl:onClass g:C` counts ONLY the values that ARE a C; it
    // does NOT entail that EVERY value of the property is a C. So the faithful projection is the
    // `sh:qualifiedValueShape` ALONE — a bare `sh:class g:C` would over-claim the universal
    // (caught by the lift/derive round-trip `certify` invariant). No `rdfs:range`/`allValuesFrom`
    // backs the filler here, so NO plain class is emitted.
    let ds = shape_dataset(
        "g:C a owl:Class . \
         g:K a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:mediates ; \
           owl:maxQualifiedCardinality \"1\"^^xsd:nonNegativeInteger ; owl:onClass g:C ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .find(|p| p.path.ends_with("mediates"))
        .expect("a property on mediates");
    assert!(
        !pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(_))),
        "a qualified cardinality with no backing universal must NOT emit a plain sh:class (over-claim): {:?}",
        pc.components
    );
    assert!(
        pc.components.iter().any(|c| matches!(
            c,
            ConstraintComponent::QualifiedValueShape { max, .. } if *max == Some(1)
        )),
        "the faithful qualified value shape carries the count: {:?}",
        pc.components
    );
}

#[test]
fn derive_qualified_cardinality_emits_plain_class_when_a_closed_range_backs_it() {
    // The bare `sh:class g:C` — which the JSON-Schema deriver / purrdf object-class node-ref path
    // read (they ignore the class nested inside a `sh:qualifiedValueShape`) — IS emitted for a
    // qualified cardinality when a genuine universal backs it: here a closed-world-opted-in
    // `rdfs:range g:mediates g:C`. Then both the plain class (the universal) and the qualified
    // value shape (the count) are present, and the round trip stays equivalent.
    let ds = shape_dataset(
        "g:C a owl:Class . \
         g:mediates a owl:ObjectProperty ; rdfs:range g:C . \
         g:K a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:mediates ; \
           owl:maxQualifiedCardinality \"1\"^^xsd:nonNegativeInteger ; owl:onClass g:C ] .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:mediates ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let pc = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("mediates"))
        .find(|p| {
            p.components
                .iter()
                .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. }))
        })
        .expect("the qualified property shape on mediates");
    assert!(
        pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("/C"))),
        "a closed-world range g:C must emit the plain sh:class for the JSON-Schema deriver: {:?}",
        pc.components
    );
    assert!(
        pc.components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. })),
        "the faithful qualified value shape is still emitted: {:?}",
        pc.components
    );
}

#[test]
fn derive_single_property_has_key_emits_inverse_functional_shape() {
    // A `logic:KeyAssertion` (logic:keyClass C ; logic:keyProperty P) is the canonical carrier of a
    // datatype/single-property key — the greenfield replacement for `C owl:hasKey ( P )` (an
    // owl:InverseFunctionalProperty on a datatype property would be OWL 2 Full). Its closed-world
    // reading is the same inverse sh:maxCount 1 the InverseFunctionalProperty arm emits.
    let ds = shape_dataset(
        "g:GTSSegment a owl:Class . \
         g:gtsHeadId a owl:DatatypeProperty ; rdfs:domain g:GTSSegment ; rdfs:range xsd:string . \
         logic:gtsSegmentHeadKey a logic:KeyAssertion ; \
             logic:keyClass g:GTSSegment ; logic:keyProperty g:gtsHeadId .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let shape = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("gtsHeadId")))
        .expect("an ObjectsOf(gtsHeadId) shape from the logic:KeyAssertion carrier");
    let prop = &shape.properties[0];
    assert!(
        prop.inverse,
        "a single-property key must derive an inverse-path shape"
    );
    assert_eq!(
        prop.max_count,
        Some(1),
        "key → each key value has ≤1 subject"
    );
}

#[test]
fn derive_composite_has_key_derives_no_single_path_shape() {
    // A COMPOSITE key (a logic:KeyAssertion naming several logic:keyProperty values) asserts the
    // TUPLE is unique, not each part — it has no single-path SHACL form, so no per-part uniqueness
    // shape may be derived.
    let ds = shape_dataset(
        "g:C a owl:Class . g:p1 a owl:DatatypeProperty . g:p2 a owl:DatatypeProperty . \
         logic:cCompositeKey a logic:KeyAssertion ; \
             logic:keyClass g:C ; logic:keyProperty g:p1 , g:p2 .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::ObjectsOf(p) if p.ends_with("/p1") || p.ends_with("/p2")
        )),
        "a composite key must derive no single-path uniqueness shape: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
}

#[test]
fn derive_min_qualified_cardinality_uses_the_owl2_standard_keyword() {
    // The OWL 2 RDF keyword is `owl:minQualifiedCardinality` (NOT `owl:qualifiedMinCardinality`).
    // Regression guard: the derivation must read the standard local name, else a real qualified
    // min-cardinality restriction is silently invisible and the arm ships zero shapes.
    let ds = shape_dataset(
        "g:Posting a owl:Class . \
         g:JournalEntry a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:entryPostings ; owl:onClass g:Posting ; \
           owl:minQualifiedCardinality \"2\"^^xsd:nonNegativeInteger ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comp = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .flat_map(|p| &p.components)
        .find(|c| matches!(c, ConstraintComponent::QualifiedValueShape { .. }))
        .expect("owl:minQualifiedCardinality must derive a qualified value shape");
    match comp {
        ConstraintComponent::QualifiedValueShape { shape, min, max } => {
            assert_eq!(
                *min,
                Some(2),
                "minQualifiedCardinality 2 → qualifiedMinCount 2"
            );
            assert_eq!(*max, None);
            assert!(
                shape
                    .iter()
                    .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Posting"))),
                "inner shape must be sh:class g:Posting: {shape:?}"
            );
        }
        other => panic!("expected QualifiedValueShape, got {other:?}"),
    }
}

#[test]
fn derive_all_values_from_faceted_datatype_emits_length_and_pattern_facets() {
    // An owl:allValuesFrom whose filler is a faceted rdfs:Datatype
    // (owl:onDatatype + owl:withRestrictions) reads as the SHACL length / pattern facets its
    // values must satisfy — the derivation's owl:withRestrictions arm.
    let ds = shape_dataset(
        "g:FinancialAccount a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:bic ; owl:allValuesFrom \
           [ a rdfs:Datatype ; owl:onDatatype xsd:string ; owl:withRestrictions ( \
              [ xsd:minLength \"8\"^^xsd:nonNegativeInteger ] \
              [ xsd:maxLength \"11\"^^xsd:nonNegativeInteger ] \
              [ xsd:pattern \"^[A-Z]{6}[A-Z0-9]{2}([A-Z0-9]{3})?$\" ] ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let comps: Vec<&ConstraintComponent> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("bic"))
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::MinLength(8))),
        "xsd:minLength 8 → sh:minLength 8: {comps:?}"
    );
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::MaxLength(11))),
        "xsd:maxLength 11 → sh:maxLength 11: {comps:?}"
    );
    assert!(
        comps
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Pattern { regex, .. } if regex.starts_with("^[A-Z]{6}"))),
        "xsd:pattern → sh:pattern: {comps:?}"
    );
    assert!(
        comps.iter().any(|c| matches!(
            c,
            ConstraintComponent::Datatype(d) if d.ends_with("string")
        )),
        "owl:onDatatype xsd:string → sh:datatype xsd:string: {comps:?}"
    );
}

#[test]
fn derive_all_values_from_faceted_datatype_emits_numeric_range_facets() {
    // An owl:allValuesFrom whose filler is a faceted rdfs:Datatype with numeric bound
    // restrictions (xsd:minInclusive / xsd:maxInclusive and their exclusive peers) reads as a
    // single SHACL NumericRange the values must satisfy — the closed unit interval [0, 1] and a
    // half-open lower-bound-exclusive interval.
    let ds = shape_dataset(
        "g:ConceptCategorization a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:typicality ; owl:allValuesFrom \
           [ a rdfs:Datatype ; owl:onDatatype xsd:decimal ; owl:withRestrictions ( \
              [ xsd:minInclusive \"0\"^^xsd:decimal ] \
              [ xsd:maxInclusive \"1\"^^xsd:decimal ] ) ] ] .\n\
         g:RationalValue a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:denominator ; owl:allValuesFrom \
           [ a rdfs:Datatype ; owl:onDatatype xsd:integer ; owl:withRestrictions ( \
              [ xsd:minExclusive \"0\"^^xsd:integer ] ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let typicality: Vec<&ConstraintComponent> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("typicality"))
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        typicality.iter().any(|c| matches!(
            c,
            ConstraintComponent::NumericRange {
                min: Some(lo),
                max: Some(hi),
                min_inclusive: true,
                max_inclusive: true,
            } if *lo == 0.0 && *hi == 1.0
        )),
        "closed unit interval → sh:minInclusive 0 / sh:maxInclusive 1: {typicality:?}"
    );
    let denominator: Vec<&ConstraintComponent> = shapes
        .iter()
        .flat_map(|s| &s.properties)
        .filter(|p| p.path.ends_with("denominator"))
        .flat_map(|p| &p.components)
        .collect();
    assert!(
        denominator.iter().any(|c| matches!(
            c,
            ConstraintComponent::NumericRange {
                min: Some(lo),
                max: None,
                min_inclusive: false,
                ..
            } if *lo == 0.0
        )),
        "lower-bound-exclusive → sh:minExclusive 0, no upper: {denominator:?}"
    );
}

#[test]
fn derive_non_faceted_blank_filler_is_skipped() {
    // A blank allValuesFrom filler that is neither a faceted datatype nor a readable class
    // expression (union / disjoint-union / complement) — here an `owl:intersectionOf`, which has
    // no faithful single-component shape form — is carried in the canon, never a bare blank shape.
    let ds = shape_dataset(
        "g:C a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:p ; owl:allValuesFrom \
           [ a owl:Class ; owl:intersectionOf ( g:A g:B ) ] ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes
            .iter()
            .flat_map(|s| &s.properties)
            .any(|p| p.path.ends_with("/p")),
        "a non-faceted blank filler must derive no property shape: {shapes:?}"
    );
}

#[test]
fn derive_qualified_cardinality_without_on_class_hard_fails() {
    // A qualified cardinality with no owl:onClass is malformed — hard-fail, never a silent drop.
    let ds = shape_dataset(
        "g:Car a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:hasPart ; \
           owl:qualifiedCardinality \"1\"^^xsd:nonNegativeInteger ] .",
    );
    assert!(
        derive_validation_shapes(ds.as_ref()).is_err(),
        "qualifiedCardinality without owl:onClass must hard-fail"
    );
}

#[test]
fn derive_domain_and_functional_merge_into_one_subjects_of_shape() {
    // A domain axiom (opted IN to closed-world reading) AND a functional carrier record on the
    // SAME property must fold into ONE SubjectsOf(P) shape carrying both the node Class and the
    // maxCount-1 property. (The functional maxCount derives regardless; the domain node-class
    // needs the ClosedWorldClosure opt-in since domain is open-world by default.)
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:isbn rdfs:domain g:Doc .\n\
         [] a logic:PropertyCharacteristicAssertion ; \
             logic:characterizes g:isbn ; \
             logic:characteristicSort logic:functionalProperty .\n\
         [] a logic:ClosureEntry ; logic:closureKey g:isbn ; logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let subjects_of: Vec<_> = shapes
        .iter()
        .filter(|s| matches!(&s.target, ShapeTarget::SubjectsOf(p) if p.ends_with("isbn")))
        .collect();
    assert_eq!(
        subjects_of.len(),
        1,
        "domain + functional must produce exactly ONE SubjectsOf(isbn) shape: {subjects_of:?}"
    );
    let shape = subjects_of[0];
    assert!(
        shape
            .node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "merged shape carries the domain node Class: {:?}",
        shape.node_components
    );
    assert!(
        shape
            .properties
            .iter()
            .any(|p| p.path.ends_with("isbn") && p.max_count == Some(1)),
        "merged shape carries the functional maxCount-1 property: {:?}",
        shape.properties
    );
}

// ── R3: the per-property / per-class closed-world-reading opt-out (closure selector) ──────

#[test]
fn derive_default_is_derive_all_without_any_closure_annotation() {
    // MAXIMAL UTILITY: with NO closure annotation, every eligible axiom derives a shape.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes.iter().any(|s| s.iri.contains("Article")),
        "default (no annotation) must derive the Article shape: {shapes:?}"
    );
}

#[test]
fn derive_property_optout_suppresses_that_property_shape() {
    // A closure entry keyed by the PROPERTY IRI with logic:OpenWorldClosure marks the property
    // genuinely open-world-only, so no closed-world shape is derived for it.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] logic:closureKey <https://blackcatinformatics.ca/gmeow/cites> ; \
            logic:closureValue logic:OpenWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| s.iri.contains("Article")),
        "an OpenWorldClosure opt-out on g:cites must suppress the Article shape: {shapes:?}"
    );
}

#[test]
fn derive_class_optout_suppresses_the_whole_class_shape() {
    // A closure entry keyed by the CLASS IRI opts the entire class out of the closed-world reading.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] logic:closureKey <https://blackcatinformatics.ca/gmeow/Article> ; \
            logic:closureValue logic:OpenWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        !shapes.iter().any(|s| s.iri.contains("Article")),
        "an OpenWorldClosure opt-out on g:Article must suppress its shape entirely: {shapes:?}"
    );
}

#[test]
fn derive_closed_world_closure_entry_does_not_suppress() {
    // Only OpenWorldClosure is an opt-out; a ClosedWorldClosure entry (the default reading made
    // explicit) leaves derivation on — the shape is still produced.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] logic:closureKey <https://blackcatinformatics.ca/gmeow/cites> ; \
            logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes.iter().any(|s| s.iri.contains("Article")),
        "a ClosedWorldClosure entry is the default reading, not an opt-out: {shapes:?}"
    );
}

/// Complete class-scoped keys require only their own path and never grant global closure.
#[test]
fn derive_class_scoped_closed_entry_derives_no_global_domain_range_shape() {
    // A ClosedWorldClosure entry carrying logic:onClass is CLASS-SCOPED: it turns the class's
    // owl:allValuesFrom into a required (minCount 1) path on THAT class's shape, and it must NOT
    // leak into the property-global opt-in set — no corpus-wide sh:targetSubjectsOf /
    // sh:targetObjectsOf domain/range shape may be derived from it (closing a predicate on one
    // class asserts nothing about the predicate's other subjects/objects). The closureKey is a
    // string literal here, matching the authored slice form.
    let source = "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:allValuesFrom g:Doc ] . \
         g:cites a owl:ObjectProperty ; rdfs:domain g:Article ; rdfs:range g:Doc . \
         [] a logic:ClosureEntry ; logic:onClass g:Article ; \
            logic:closureKey \"https://blackcatinformatics.ca/gmeow/cites\" ; \
            logic:closureValue logic:ClosedWorldClosure .";
    let ds = shape_dataset(source);
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let article = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::Class(c) if c.ends_with("Article")))
        .expect("the class-scoped entry keeps the Article class shape");
    assert!(
        article
            .properties
            .iter()
            .any(|p| p.path.ends_with("cites") && p.min_count == Some(1)),
        "class-scoped closed universal requires the path on the class shape: {:?}",
        article.properties
    );
    assert!(
        !shapes.iter().any(|s| matches!(
            &s.target,
            ShapeTarget::SubjectsOf(p) | ShapeTarget::ObjectsOf(p) if p.ends_with("cites")
        )),
        "a class-scoped entry must derive NO global domain/range shape: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
    for key in [
        "g:cites",
        "\"https://blackcatinformatics.ca/gmeow/cites\"^^xsd:token",
    ] {
        let selected = source.replace("\"https://blackcatinformatics.ca/gmeow/cites\"", key);
        assert_eq!(
            derive_validation_shapes(&shape_dataset(&selected)).unwrap(),
            shapes
        );
    }
    for key in [
        None,
        Some("<<( g:s g:p g:o )>>"),
        Some("g:cites, \"https://blackcatinformatics.ca/gmeow/cites\""),
    ] {
        let field = key.map_or_else(String::new, |key| format!("logic:closureKey {key} ;"));
        let selected = source.replace(
            "logic:closureKey \"https://blackcatinformatics.ca/gmeow/cites\" ;",
            &field,
        );
        let error = derive_validation_shapes(&shape_dataset(&selected)).unwrap_err();
        assert!(error.message().contains("closureKey"), "{key:?}: {error}");
    }
}

#[test]
fn derive_property_global_closed_entry_is_unaffected_by_a_class_scoped_sibling() {
    // The two entry scopes coexist without cross-talk: a property-GLOBAL entry (no logic:onClass)
    // still derives its domain/range shapes, while the class-scoped sibling on another property
    // derives none.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:refs a owl:ObjectProperty ; rdfs:range g:Doc . \
         [] a logic:ClosureEntry ; logic:closureKey g:refs ; \
            logic:closureValue logic:ClosedWorldClosure . \
         g:cites a owl:ObjectProperty ; rdfs:range g:Doc . \
         [] a logic:ClosureEntry ; logic:onClass g:Article ; logic:closureKey g:cites ; \
            logic:closureValue logic:ClosedWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    let refs = shapes
        .iter()
        .find(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("refs")))
        .expect("the property-global opt-in still derives its ObjectsOf(refs) range shape");
    assert!(
        refs.node_components
            .iter()
            .any(|c| matches!(c, ConstraintComponent::Class(d) if d.ends_with("Doc"))),
        "the global entry's range shape carries its sh:class: {:?}",
        refs.node_components
    );
    assert!(
        !shapes
            .iter()
            .any(|s| matches!(&s.target, ShapeTarget::ObjectsOf(p) if p.ends_with("cites"))),
        "the class-scoped sibling must not derive a global range shape: {:?}",
        shapes.iter().map(|s| &s.target).collect::<Vec<_>>()
    );
}

#[test]
fn derive_class_scoped_open_entry_does_not_suppress_globally() {
    // The symmetric discrimination for the opt-out polarity: an OpenWorldClosure entry carrying
    // logic:onClass is class-scoped and must NOT sweep its key into the property-global opt-out
    // set — the closed-world reading of the property's restrictions on OTHER classes stays on.
    let ds = shape_dataset(
        "g:Doc a owl:Class . \
         g:Article a owl:Class ; rdfs:subClassOf \
         [ a owl:Restriction ; owl:onProperty g:cites ; owl:someValuesFrom g:Doc ] . \
         [] a logic:ClosureEntry ; logic:onClass g:Other ; logic:closureKey g:cites ; \
            logic:closureValue logic:OpenWorldClosure .",
    );
    let shapes = derive_validation_shapes(ds.as_ref()).expect("derive ok");
    assert!(
        shapes.iter().any(|s| s.iri.contains("Article")),
        "a class-scoped OpenWorldClosure entry must not opt the property out corpus-wide: {shapes:?}"
    );
}
