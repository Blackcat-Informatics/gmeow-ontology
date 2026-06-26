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

/// Emit the FnO catalog of the six list functions as deterministic Turtle.
///
/// Public-tag (`@en`) raw-text writer — the bundled, public artifact form (the
/// `@x-gmeow-english` internal tag is for graph-serialized generators, #330). The
/// content is fixed, so re-running is byte-identical.
pub fn emit_list_functions() -> String {
    let mut out = String::new();
    out.push_str("@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n");
    out.push_str("@prefix logic: <https://blackcatinformatics.ca/logic/> .\n");
    out.push_str("@prefix fno: <https://w3id.org/function/ontology#> .\n");
    out.push_str("@prefix owl: <http://www.w3.org/2002/07/owl#> .\n");
    out.push_str("@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n");
    out.push_str("@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n");
    out.push_str("@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n");
    out.push_str("@prefix skos: <http://www.w3.org/2004/02/skos/core#> .\n");
    out.push_str("@prefix dcterms: <http://purl.org/dc/terms/> .\n");
    out.push('\n');

    // Document node.
    out.push_str("gmeow:list-functions a owl:Ontology ;\n");
    out.push_str("    rdfs:label \"GMEOW first-class RDF list functions (FnO)\"@en ;\n");
    out.push_str(
        "    skos:definition \"GENERATED by `gmeow regenerate` (mappings) — DO NOT EDIT. Six \
         primitive rdf:List operations (listLength, listGet, listIndexOf, listSlice, listConcat, \
         listContains) declared as FnO. listContains is computed in the reasoning layer today via \
         the recursive rdf:first/rdf:rest walk (conformance case goal-rdf-list-functions, the \
         backward-goal analog of the rl:list-member recursion). The indexed/counting/constructing \
         operations (listLength, listGet, listIndexOf, listSlice, listConcat) need \
         arithmetic/value-construction beyond the current relational engine; their executable \
         backing — and a purrdf SPARQL binding for all six — is deferred to #1016.\"@en ;\n",
    );
    out.push_str("    dcterms:isPartOf <https://blackcatinformatics.ca/gmeow> .\n\n");

    // Functions.
    for f in FUNCTIONS {
        out.push_str(&format!("gmeow:{} a fno:Function ;\n", f.name));
        out.push_str(&format!("    rdfs:label \"{}\"@en ;\n", f.label));
        out.push_str(&format!("    skos:definition \"{}\"@en ;\n", f.definition));
        let expects = f
            .expects
            .iter()
            .map(|p| format!("gmeow:{p}"))
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&format!("    fno:expects ( {expects} ) ;\n"));
        out.push_str(&format!("    fno:returns ( gmeow:{} ) ;\n", f.output));
        // Backing predicate in the reasoning layer (#1009 part b).
        out.push_str(&format!("    rdfs:seeAlso <{LOGIC_NS}{}> .\n\n", f.name));
    }

    // Parameters (deduped).
    for p in PARAMS {
        out.push_str(&format!(
            "gmeow:{} a fno:Parameter ; fno:type <{}> ; fno:required true ;\n",
            p.local, p.ty
        ));
        out.push_str(&format!("    rdfs:label \"{}\"@en ;\n", p.label));
        out.push_str(&format!(
            "    skos:definition \"{}\"@en .\n\n",
            p.definition
        ));
    }

    // Outputs (one per function).
    for f in FUNCTIONS {
        let label = format!("{} result", f.label);
        out.push_str(&format!(
            "gmeow:{} a fno:Output ; fno:type <{}> ;\n",
            f.output, f.output_type
        ));
        out.push_str(&format!("    rdfs:label \"{label}\"@en ;\n"));
        out.push_str(&format!(
            "    skos:definition \"The result of gmeow:{}.\"@en .\n\n",
            f.name
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_six_functions_are_declared() {
        let ttl = emit_list_functions();
        for name in [
            "listLength",
            "listGet",
            "listIndexOf",
            "listSlice",
            "listConcat",
            "listContains",
        ] {
            assert!(
                ttl.contains(&format!("gmeow:{name} a fno:Function")),
                "missing function {name}"
            );
            // Each function points at its logic backing predicate.
            assert!(
                ttl.contains(&format!("rdfs:seeAlso <{LOGIC_NS}{name}>")),
                "missing logic seeAlso for {name}"
            );
        }
        assert_eq!(ttl.matches("a fno:Function").count(), 6);
        assert_eq!(ttl.matches("a fno:Output").count(), 6);
        assert_eq!(ttl.matches("a fno:Parameter").count(), PARAMS.len());
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
    fn output_types_are_as_specified() {
        let by_name: std::collections::BTreeMap<&str, &ListFn> =
            FUNCTIONS.iter().map(|f| (f.name, f)).collect();
        assert_eq!(by_name["listLength"].output_type, XSD_INTEGER);
        assert_eq!(by_name["listGet"].output_type, RDFS_RESOURCE);
        assert_eq!(by_name["listIndexOf"].output_type, XSD_INTEGER);
        assert_eq!(by_name["listSlice"].output_type, RDF_LIST);
        assert_eq!(by_name["listConcat"].output_type, RDF_LIST);
        assert_eq!(by_name["listContains"].output_type, XSD_BOOLEAN);
    }

    #[test]
    fn is_deterministic() {
        assert_eq!(emit_list_functions(), emit_list_functions());
    }
}
