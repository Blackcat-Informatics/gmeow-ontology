// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! First-class RDF list functions (#1009 §5).
//!
//! Six primitive `rdf:List` operations declared as FnO functions and emitted to
//! `generated/projections/list-functions.fno.ttl` (folded into `gmeow.gts`):
//! `listLength`, `listGet`, `listIndexOf`, `listSlice`, `listConcat`,
//! `listContains`. They give external `rdf:List` data (transcoder #671) and SPARQL
//! authors a named, typed surface for the operations the logic layer already
//! resolves recursively (`crates/logic/src/reason`).
//!
//! Unlike `functions.fno.ttl` (GMEOW→external projection transforms derived from
//! the mapping DSL) these are *primitives* — they bind no GMEOW data predicate, so
//! their parameters/outputs carry `fno:type` (the RDF type guard) but no
//! `fno:predicate`. This is a hand-shaped catalog like `dsl/mappings/transforms.fno.ttl`,
//! but emitted into `generated/` so it ships in the bundle. The output is fixed
//! (six functions), hence deterministic by construction.
//!
//! `rdfs:seeAlso` on each function points at its `logic:` backing predicate (the
//! reasoning-layer rules, #1009 part b / Task 4). An executable purrdf SPARQL
//! binding is deferred to the own-SPARQL-layer cutover (issue #1016, blocked on
//! purrdf #832); the document records that.

/// The `logic:` namespace the per-function backing predicates live under.
const LOGIC_NS: &str = "https://blackcatinformatics.ca/logic/";

/// One list-function declaration.
struct ListFn {
    /// Local name (the issue-named function: `listLength`, …).
    name: &'static str,
    label: &'static str,
    definition: &'static str,
    /// Ordered (param-IRI-local, …) the function expects.
    expects: &'static [&'static str],
    /// The output IRI-local (`o<Name>`).
    output: &'static str,
    /// The output `fno:type` (an absolute IRI).
    output_type: &'static str,
}

/// One parameter/output individual (deduped param or per-function output).
struct ListTerm {
    /// IRI-local (`pList`, `oListLength`, …).
    local: &'static str,
    label: &'static str,
    definition: &'static str,
    /// `fno:type` (an absolute IRI).
    ty: &'static str,
}

const RDF_LIST: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#List";
const RDFS_RESOURCE: &str = "http://www.w3.org/2000/01/rdf-schema#Resource";
const XSD_INTEGER: &str = "http://www.w3.org/2001/XMLSchema#integer";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

/// The six functions, in stable order.
const FUNCTIONS: &[ListFn] = &[
    ListFn {
        name: "listLength",
        label: "list length",
        definition: "The number of members in an rdf:List (the length of the rdf:first/rdf:rest chain to rdf:nil).",
        expects: &["pList"],
        output: "oListLength",
        output_type: XSD_INTEGER,
    },
    ListFn {
        name: "listGet",
        label: "list get",
        definition: "The member of an rdf:List at a zero-based index (the nth rdf:first along the chain).",
        expects: &["pList", "pIndex"],
        output: "oListGet",
        output_type: RDFS_RESOURCE,
    },
    ListFn {
        name: "listIndexOf",
        label: "list index of",
        definition: "The zero-based index of the first occurrence of a value in an rdf:List, or absent when the value is not a member.",
        expects: &["pList", "pValue"],
        output: "oListIndexOf",
        output_type: XSD_INTEGER,
    },
    ListFn {
        name: "listSlice",
        label: "list slice",
        definition: "A new rdf:List of the members in the half-open index range [start, end) of an rdf:List.",
        expects: &["pList", "pSliceStart", "pSliceEnd"],
        output: "oListSlice",
        output_type: RDF_LIST,
    },
    ListFn {
        name: "listConcat",
        label: "list concat",
        definition: "A new rdf:List that is the concatenation of two rdf:Lists (the members of the first followed by the members of the second).",
        expects: &["pListA", "pListB"],
        output: "oListConcat",
        output_type: RDF_LIST,
    },
    ListFn {
        name: "listContains",
        label: "list contains",
        definition: "True when a value is a member of an rdf:List, false otherwise.",
        expects: &["pList", "pValue"],
        output: "oListContains",
        output_type: XSD_BOOLEAN,
    },
];

/// The deduped parameter individuals (first-use order across `FUNCTIONS`).
const PARAMS: &[ListTerm] = &[
    ListTerm {
        local: "pList",
        label: "list",
        definition: "The input rdf:List the operation reads.",
        ty: RDF_LIST,
    },
    ListTerm {
        local: "pIndex",
        label: "index",
        definition: "A zero-based position into an rdf:List.",
        ty: XSD_INTEGER,
    },
    ListTerm {
        local: "pValue",
        label: "value",
        definition: "A value to locate as a member of an rdf:List.",
        ty: RDFS_RESOURCE,
    },
    ListTerm {
        local: "pSliceStart",
        label: "slice start",
        definition: "The inclusive zero-based start index of a slice.",
        ty: XSD_INTEGER,
    },
    ListTerm {
        local: "pSliceEnd",
        label: "slice end",
        definition: "The exclusive zero-based end index of a slice.",
        ty: XSD_INTEGER,
    },
    ListTerm {
        local: "pListA",
        label: "first list",
        definition: "The first (left) rdf:List of a concatenation.",
        ty: RDF_LIST,
    },
    ListTerm {
        local: "pListB",
        label: "second list",
        definition: "The second (right) rdf:List of a concatenation.",
        ty: RDF_LIST,
    },
];

/// The GMEOW namespace prefix the function/param/output IRIs are minted under.
const GMEOW_NS: &str = "https://blackcatinformatics.ca/gmeow/";

/// The document node banner (`rdfs:comment` after `to_quads`, like
/// `functions.fno.ttl`). Note the predicate shift from the legacy hand-Turtle's
/// `skos:definition` to `rdfs:comment` is intentional — the FnO doc-node idiom uses
/// `rdfs:comment` for its generated banner.
const BANNER: &str =
    "GENERATED by `gmeow regenerate` (mappings) — DO NOT EDIT. Six primitive rdf:List operations \
     (listLength, listGet, listIndexOf, listSlice, listConcat, listContains) declared as FnO. \
     listContains is computed in the reasoning layer today via the recursive rdf:first/rdf:rest \
     walk (conformance case goal-rdf-list-functions, the backward-goal analog of the \
     rl:list-member recursion). The indexed/counting/constructing operations (listLength, listGet, \
     listIndexOf, listSlice, listConcat) need arithmetic/value-construction beyond the current \
     relational engine; their executable backing — and a purrdf SPARQL binding for all six — is \
     deferred to #1016.";

/// Build the FnO catalog of the six primitive list functions from the
/// [`FUNCTIONS`]/[`PARAMS`] consts (the single source of truth).
///
/// These are PRIMITIVES: their params/outputs bind NO `fno:predicate` and the
/// functions are typed `fno:Function` ONLY (`kind_types` is empty — they are NOT
/// `gmeow:ProjectionFunction`). The maximal-information-flow `rdfs:label` /
/// `skos:definition` on every function, param, and output is carried via the
/// optional model fields and survives the shared [`gmeow_rdf::fno::to_quads`] path.
pub fn list_functions_catalog() -> gmeow_rdf::fno::FnoCatalog {
    use gmeow_rdf::fno::{FnFunction, FnOutput, FnParam, FnoCatalog};

    let functions: Vec<FnFunction> = FUNCTIONS
        .iter()
        .map(|f| FnFunction {
            iri: format!("{GMEOW_NS}{}", f.name),
            label: f.label.to_owned(),
            description: Some(f.definition.to_owned()),
            // Primitive — `fno:Function` only.
            kind_types: vec![],
            // Backing predicate in the reasoning layer (#1009 part b); the
            // dangling-target fix is a later commit (G3).
            see_also: format!("{LOGIC_NS}{}", f.name),
            expects: f.expects.iter().map(|p| format!("{GMEOW_NS}{p}")).collect(),
            output: FnOutput {
                iri: format!("{GMEOW_NS}{}", f.output),
                predicate: None,
                r#type: f.output_type.to_owned(),
                label: Some(format!("{} result", f.label)),
                description: Some(format!("The result of gmeow:{}.", f.name)),
            },
        })
        .collect();

    let params: Vec<FnParam> = PARAMS
        .iter()
        .map(|p| FnParam {
            iri: format!("{GMEOW_NS}{}", p.local),
            predicate: None,
            r#type: p.ty.to_owned(),
            required: true,
            label: Some(p.label.to_owned()),
            description: Some(p.definition.to_owned()),
        })
        .collect();

    FnoCatalog {
        ontology_iri: "https://blackcatinformatics.ca/gmeow".to_owned(),
        // Keep the legacy doc-node IRI (`gmeow:list-functions`).
        document_iri: "https://blackcatinformatics.ca/gmeow/list-functions".to_owned(),
        doc_label: "GMEOW first-class RDF list functions (FnO)".to_owned(),
        banner: BANNER.to_owned(),
        functions,
        params,
        implementations: vec![],
        mappings: vec![],
    }
}

/// Emit the FnO catalog of the six list functions as deterministic N-Triples.
///
/// Routes through the SAME validated [`gmeow_rdf::fno::to_quads`] serializer as
/// `functions.fno.ttl` (§19 one-path), then retags the internal `@x-gmeow-english`
/// language tag to the public `@en` and renders each quad as one N-Triples line.
/// The content is fixed, so re-running is byte-identical.
pub fn emit_list_functions() -> String {
    let cat = list_functions_catalog();
    let tag_map: std::collections::BTreeMap<String, String> =
        std::collections::BTreeMap::from([("x-gmeow-english".to_owned(), "en".to_owned())]);
    let quads: Vec<gmeow_rdf::RdfQuad> = gmeow_rdf::fno::to_quads(&cat)
        .into_iter()
        .map(|q| crate::fno_emit::retag_quad(q, &tag_map))
        .collect();
    quads.iter().map(gmeow_rdf::turtle::emit_quad).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use oxigraph::io::{RdfFormat, RdfParser};
    use oxigraph::store::Store;

    const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
    const FNO_FUNCTION: &str = "https://w3id.org/function/ontology#Function";
    const FNO_OUTPUT: &str = "https://w3id.org/function/ontology#Output";
    const FNO_PARAMETER: &str = "https://w3id.org/function/ontology#Parameter";
    const FNO_TYPE: &str = "https://w3id.org/function/ontology#type";
    const FNO_PREDICATE: &str = "https://w3id.org/function/ontology#predicate";
    const GMEOW_PROJECTION_FUNCTION: &str =
        "https://blackcatinformatics.ca/gmeow/ProjectionFunction";

    /// Parse the emitted N-Triples into a store (the new committed-artifact form).
    fn emitted_store() -> Store {
        let store = Store::new().unwrap();
        let text = emit_list_functions();
        for quad in RdfParser::from_format(RdfFormat::NTriples)
            .lenient()
            .for_reader(text.as_bytes())
        {
            store.insert(&quad.unwrap()).unwrap();
        }
        store
    }

    /// Every named-node subject of `?s a <type_iri>`.
    fn subjects_of_type(store: &Store, type_iri: &str) -> std::collections::BTreeSet<String> {
        use oxigraph::model::{NamedNode, NamedOrBlankNode, Term};
        let rdf_type = NamedNode::new(RDF_TYPE).unwrap();
        let class: Term = NamedNode::new(type_iri).unwrap().into();
        store
            .quads_for_pattern(None, Some(rdf_type.as_ref()), Some(class.as_ref()), None)
            .filter_map(|q| match q.unwrap().subject {
                NamedOrBlankNode::NamedNode(nn) => Some(nn.as_str().to_owned()),
                NamedOrBlankNode::BlankNode(_) => None,
            })
            .collect()
    }

    #[test]
    fn six_functions_typed_fno_function_and_not_projection() {
        let store = emitted_store();
        let functions = subjects_of_type(&store, FNO_FUNCTION);
        assert_eq!(functions.len(), 6, "expected six fno:Function declarations");
        for name in [
            "listLength",
            "listGet",
            "listIndexOf",
            "listSlice",
            "listConcat",
            "listContains",
        ] {
            assert!(
                functions.contains(&format!("{GMEOW_NS}{name}")),
                "missing function {name}"
            );
        }
        // Primitives are NOT gmeow:ProjectionFunction.
        assert!(
            subjects_of_type(&store, GMEOW_PROJECTION_FUNCTION).is_empty(),
            "list functions must not be gmeow:ProjectionFunction"
        );
    }

    #[test]
    fn six_outputs_and_correct_param_count() {
        let store = emitted_store();
        assert_eq!(subjects_of_type(&store, FNO_OUTPUT).len(), 6);
        assert_eq!(
            subjects_of_type(&store, FNO_PARAMETER).len(),
            PARAMS.len(),
            "one fno:Parameter per deduped PARAMS entry"
        );
    }

    #[test]
    fn each_output_carries_its_specified_fno_type() {
        use oxigraph::model::{NamedNode, Term};
        let store = emitted_store();
        let fno_type = NamedNode::new(FNO_TYPE).unwrap();
        for f in FUNCTIONS {
            let out = NamedNode::new(format!("{GMEOW_NS}{}", f.output)).unwrap();
            let want: Term = NamedNode::new(f.output_type).unwrap().into();
            let found = store
                .quads_for_pattern(Some((&out).into()), Some(fno_type.as_ref()), None, None)
                .any(|q| q.unwrap().object == want);
            assert!(found, "{}: output fno:type != {}", f.name, f.output_type);
        }
    }

    #[test]
    fn every_param_and_output_carries_an_rdfs_label() {
        use oxigraph::model::NamedNode;
        let store = emitted_store();
        let rdfs_label = NamedNode::new(RDFS_LABEL).unwrap();
        let mut targets: Vec<String> = PARAMS
            .iter()
            .map(|p| format!("{GMEOW_NS}{}", p.local))
            .collect();
        targets.extend(FUNCTIONS.iter().map(|f| format!("{GMEOW_NS}{}", f.output)));
        for iri in targets {
            let node = NamedNode::new(&iri).unwrap();
            let has_label = store
                .quads_for_pattern(Some((&node).into()), Some(rdfs_label.as_ref()), None, None)
                .next()
                .is_some();
            assert!(has_label, "{iri} missing rdfs:label");
        }
    }

    #[test]
    fn no_fno_predicate_triples_exist_primitive_check() {
        use oxigraph::model::NamedNode;
        let store = emitted_store();
        let fno_predicate = NamedNode::new(FNO_PREDICATE).unwrap();
        let count = store
            .quads_for_pattern(None, Some(fno_predicate.as_ref()), None, None)
            .count();
        assert_eq!(count, 0, "primitives must bind no fno:predicate");
    }

    #[test]
    fn every_expected_param_is_defined() {
        // No function may reference a parameter that is not defined (dangling ref).
        let defined: std::collections::BTreeSet<&str> = PARAMS.iter().map(|p| p.local).collect();
        for f in FUNCTIONS {
            for p in f.expects {
                assert!(
                    defined.contains(p),
                    "function {} expects undefined {p}",
                    f.name
                );
            }
        }
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(emit_list_functions(), emit_list_functions());
    }
}
