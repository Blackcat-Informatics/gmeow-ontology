// SPDX-FileCopyrightText: 2026 Blackcat Informatics Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! First-party RDF/XML codec (W3C RDF 1.1 / RDF-XML grammar) — EPIC #906.
//!
//! This module REPLACES the external `gmeow_gts::rdf_codecs::{from_rdf_xml,
//! to_rdf_xml}` codecs (the first-party mandate: RDF/XML must NOT be parsed or
//! serialized via the external crate). It implements the RDF/XML production rules
//! in-repo on top of a pure-Rust XML DOM (`roxmltree`), lowering to — and rising
//! from — the SAME `gmeow_gts::rdf` dataset model the prior path used, so the
//! resulting GTS bytes (parse) and the emitted text (serialize) are byte-identical
//! to the gmeow-gts path that produced the conformance and regenerate corpora.
//!
//! ## Why reuse `gmeow_gts::rdf` (not `rdf_codecs`)
//!
//! The forbidden symbols are the RDF/XML codec entry points themselves
//! (`rdf_codecs::from_rdf_xml` / `to_rdf_xml`). The lower-level `gmeow_gts::rdf`
//! dataset model + `from_rdf_dataset` (RDF dataset → GTS bytes) + `to_rdf_quads`
//! (GTS graph → RDF quads) are general transforms, NOT RDF/XML. Reusing them — and
//! implementing ONLY the XML↔dataset mapping first-party — keeps the GTS encoding
//! (term interning order, reifier folding, ill-typed-literal meta) provably
//! identical to the prior path while satisfying the mandate.
//!
//! ## Grammar coverage
//!
//! `rdf:RDF` root, `rdf:Description`, typed-node elements, property elements,
//! `rdf:about`/`rdf:resource`/`rdf:ID`/`rdf:nodeID`, property attributes,
//! `rdf:datatype`, `xml:lang`, `its:dir` (RDF 1.2 base direction),
//! `rdf:parseType="Resource"|"Literal"|"Collection"|"Triple"`, RDF 1.0 `rdf:ID`
//! reification, RDF 1.2 `rdf:annotation`/`rdf:annotationNodeID` reifiers, list
//! expansion, node/property striping, base-IRI resolution, and `xmlns` prefix
//! scoping. The grammar logic mirrors the W3C RDF/XML mapping the prior gmeow-gts
//! native adapter implemented (so the two are quad-for-quad equivalent).

use std::collections::BTreeMap;

use gmeow_gts::model::Graph as GtsGraph;
use gmeow_gts::rdf::{
    from_rdf_dataset, to_rdf_quads, BaseDirection, BlankNode, Dataset, GraphName, Iri, Literal,
    NamedOrBlankNode, RdfQuad, RdfTerm, RdfTriple,
};
use roxmltree::{Document, Node};

use crate::RdfDiagnostic;

const RDF_NS: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";
const XML_NS: &str = "http://www.w3.org/XML/1998/namespace";
const ITS_NS: &str = "http://www.w3.org/2005/11/its";
const XSD_NS: &str = "http://www.w3.org/2001/XMLSchema#";

const RDF_DESCRIPTION: &str = "Description";
const RDF_ABOUT: &str = "about";
const RDF_ID: &str = "ID";
const RDF_NODE_ID: &str = "nodeID";
const RDF_RESOURCE: &str = "resource";
const RDF_DATATYPE: &str = "datatype";
const RDF_PARSE_TYPE: &str = "parseType";
const RDF_TYPE: &str = "type";
const RDF_VERSION: &str = "version";
const RDF_ANNOTATION: &str = "annotation";
const RDF_ANNOTATION_NODE_ID: &str = "annotationNodeID";
const RDF_REIFIES: &str = "reifies";
const RDF_FIRST: &str = "first";
const RDF_REST: &str = "rest";
const RDF_NIL: &str = "nil";
const RDF_STATEMENT: &str = "Statement";
const RDF_SUBJECT: &str = "subject";
const RDF_PREDICATE: &str = "predicate";
const RDF_OBJECT: &str = "object";
const RDF_XML_LITERAL: &str = "XMLLiteral";
const XML_BASE: &str = "base";
const XML_LANG: &str = "lang";
const ITS_DIR: &str = "dir";
const ITS_VERSION: &str = "version";

fn parse_err(detail: impl Into<String>) -> RdfDiagnostic {
    RdfDiagnostic::error("native-codec-parse", format!("RDF/XML: {}", detail.into()))
}

fn serialize_err(detail: impl Into<String>) -> RdfDiagnostic {
    RdfDiagnostic::error(
        "native-codec-serialize",
        format!("RDF/XML: {}", detail.into()),
    )
}

fn adapter_err(error: gmeow_gts::rdf::RdfAdapterError) -> RdfDiagnostic {
    RdfDiagnostic::error("native-codec-rdfxml", error.detail().to_owned())
}

// ───────────────────────────────────────────────────────────────────────────────
// Parse: RDF/XML text → in-memory GtsGraph (via gmeow_gts::rdf::Dataset)
// ───────────────────────────────────────────────────────────────────────────────

/// Parse RDF/XML `text` into the in-memory [`GtsGraph`] the downstream
/// statement-layer fold consumes, applying the W3C RDF/XML grammar. `base_iri` is
/// the document base for relative-IRI / `rdf:ID` resolution.
///
/// The mapping lowers into a `gmeow_gts::rdf::Dataset` and re-encodes it via
/// `from_rdf_dataset` (RDF dataset → GTS bytes) → reader, producing the SAME
/// `GtsGraph` the prior `from_rdf_xml` path produced.
pub fn parse_rdfxml_to_gts_graph(
    text: &str,
    base_iri: Option<&str>,
) -> Result<GtsGraph, RdfDiagnostic> {
    let document = Document::parse(text).map_err(|e| parse_err(e.to_string()))?;
    let mut parser = RdfXmlParser {
        dataset: Dataset::new(),
        bnode_counter: 0,
        collection_counter: 0,
    };
    let context = ParseContext {
        base_iri: base_iri.map(str::to_string),
        ..Default::default()
    };
    parser.parse_document(document.root_element(), &context)?;
    let gts_bytes = from_rdf_dataset(&parser.dataset).map_err(adapter_err)?;
    crate::gts::read_all_segments(&gts_bytes)
}

#[derive(Clone, Debug, Default)]
struct ParseContext {
    base_iri: Option<String>,
    language: Option<String>,
    direction: Option<BaseDirection>,
    /// `rdf:version="1.2"` declared on this element or an ancestor: gates the RDF 1.2
    /// features (triple terms via `parseType="Triple"`, ITS base direction).
    rdf_version_12: bool,
    /// `its:version` declared (ITS 2.0 processing mode).
    its_version: bool,
}

impl ParseContext {
    fn for_child(&self, element: Node<'_, '_>) -> Result<Self, RdfDiagnostic> {
        let mut next = self.clone();
        // Version flags are sticky once declared on any ancestor.
        if attr_rdf(element, RDF_VERSION) == Some("1.2") {
            next.rdf_version_12 = true;
        }
        if attr_its(element, ITS_VERSION).is_some() {
            next.its_version = true;
        }
        if let Some(base) = attr_xml(element, XML_BASE) {
            next.base_iri = Some(match &self.base_iri {
                Some(parent) => resolve_relative_iri(parent, base),
                None => base.to_string(),
            });
        }
        if let Some(language) = attr_xml(element, XML_LANG) {
            next.language = (!language.is_empty()).then(|| language.to_string());
        }
        if let Some(direction) = attr_its(element, ITS_DIR) {
            let parsed = match direction {
                "ltr" => BaseDirection::Ltr,
                "rtl" => BaseDirection::Rtl,
                other => return Err(parse_err(format!("invalid ITS direction {other:?}"))),
            };
            // RDF 1.2 base direction is suppressed in ITS 2.0 mode (`its:version`)
            // unless the document explicitly opts into RDF 1.2 via `rdf:version="1.2"`.
            next.direction = if next.its_version && !next.rdf_version_12 {
                None
            } else {
                Some(parsed)
            };
        }
        Ok(next)
    }
}

struct RdfXmlParser {
    dataset: Dataset,
    bnode_counter: usize,
    collection_counter: usize,
}

impl RdfXmlParser {
    fn parse_document(
        &mut self,
        root: Node<'_, '_>,
        context: &ParseContext,
    ) -> Result<(), RdfDiagnostic> {
        let context = context.for_child(root)?;
        if is_rdf(root, "RDF") {
            for child in element_children(root) {
                self.parse_node_element(child, &context)?;
            }
        } else {
            self.parse_node_element(root, &context)?;
        }
        Ok(())
    }

    fn parse_node_element(
        &mut self,
        element: Node<'_, '_>,
        parent_context: &ParseContext,
    ) -> Result<NamedOrBlankNode, RdfDiagnostic> {
        let context = parent_context.for_child(element)?;
        let subject = self.subject_for_node(element, &context)?;

        if !is_rdf(element, RDF_DESCRIPTION) {
            self.insert_statement(
                subject.clone(),
                rdf_iri(RDF_TYPE)?,
                element_iri(element)?.into(),
                None,
                None,
            )?;
        }
        if let Some(type_iri) = attr_rdf(element, RDF_TYPE) {
            self.insert_statement(
                subject.clone(),
                rdf_iri(RDF_TYPE)?,
                self.iri_ref(type_iri, &context)?.into(),
                None,
                None,
            )?;
        }

        for attr in property_attrs(element) {
            let predicate = name_iri(attr.namespace(), attr.name())?;
            let literal = self.context_literal(attr.value(), None, &context)?;
            self.insert_statement(subject.clone(), predicate, literal.into(), None, None)?;
        }

        for child in element_children(element) {
            self.parse_property_element(&subject, child, &context)?;
        }
        Ok(subject)
    }

    fn parse_property_element(
        &mut self,
        subject: &NamedOrBlankNode,
        element: Node<'_, '_>,
        parent_context: &ParseContext,
    ) -> Result<(), RdfDiagnostic> {
        let context = parent_context.for_child(element)?;
        let predicate = element_iri(element)?;
        let reifier = attr_rdf(element, RDF_ID)
            .map(|id| self.rdf_id_iri(id, &context).map(NamedOrBlankNode::from))
            .transpose()?;
        // `rdf:annotation="IRI"` and `rdf:annotationNodeID="id"` both name the reifier
        // of the asserted triple; the former is an IRI, the latter a blank node.
        let annotation = match attr_rdf(element, RDF_ANNOTATION) {
            Some(annotation) => Some(self.iri_ref(annotation, &context)?.into()),
            None => match attr_rdf(element, RDF_ANNOTATION_NODE_ID) {
                Some(node_id) => Some(blank_node(node_id)?.into()),
                None => None,
            },
        };

        if let Some(resource) = attr_rdf(element, RDF_RESOURCE) {
            let object: NamedOrBlankNode = self.iri_ref(resource, &context)?.into();
            self.insert_statement(
                subject.clone(),
                predicate,
                named_or_blank_term(&object),
                reifier,
                annotation,
            )?;
            self.insert_property_attribute_statements(&object, element, &context)?;
            return Ok(());
        }
        if let Some(node_id) = attr_rdf(element, RDF_NODE_ID) {
            let object: NamedOrBlankNode = blank_node(node_id)?.into();
            self.insert_statement(
                subject.clone(),
                predicate,
                named_or_blank_term(&object),
                reifier,
                annotation,
            )?;
            self.insert_property_attribute_statements(&object, element, &context)?;
            return Ok(());
        }

        match attr_rdf(element, RDF_PARSE_TYPE) {
            Some("Resource") => {
                let object = self.fresh_bnode()?;
                self.insert_statement(
                    subject.clone(),
                    predicate,
                    named_or_blank_term(&object),
                    reifier,
                    annotation,
                )?;
                self.insert_property_attribute_statements(&object, element, &context)?;
                for child in element_children(element) {
                    self.parse_property_element(&object, child, &context)?;
                }
                return Ok(());
            }
            Some("Collection") => {
                let head = self.parse_collection(element, &context)?;
                return self.insert_statement(
                    subject.clone(),
                    predicate,
                    head,
                    reifier,
                    annotation,
                );
            }
            Some("Literal") => {
                let xml_literal = serialize_children_as_xml(element);
                let literal = Literal::new_typed_literal(xml_literal, rdf_iri(RDF_XML_LITERAL)?);
                return self.insert_statement(
                    subject.clone(),
                    predicate,
                    literal.into(),
                    reifier,
                    annotation,
                );
            }
            Some("Triple") => {
                // A triple term is an RDF 1.2 feature: without `rdf:version="1.2"` the
                // whole property is ignored (W3C `rdf12-xml-tt-01`, "Ignored triple term").
                if !context.rdf_version_12 {
                    return Ok(());
                }
                let triple = self.parse_triple_element(element, &context)?;
                return self.insert_statement(
                    subject.clone(),
                    predicate,
                    RdfTerm::Triple(Box::new(triple)),
                    reifier,
                    annotation,
                );
            }
            Some(other) => {
                return Err(parse_err(format!("unsupported rdf:parseType {other:?}")));
            }
            None => {}
        }

        let element_children: Vec<Node<'_, '_>> = element_children(element).collect();
        if let Some(datatype) = attr_rdf(element, RDF_DATATYPE) {
            if !element_children.is_empty() {
                return Err(parse_err(
                    "rdf:datatype property cannot contain node elements",
                ));
            }
            let literal = Literal::new_typed_literal(
                element_text(element),
                self.iri_ref(datatype, &context)?,
            );
            return self.insert_statement(
                subject.clone(),
                predicate,
                literal.into(),
                reifier,
                annotation,
            );
        }

        if element_children.len() == 1 {
            let object = self.parse_node_element(element_children[0], &context)?;
            return self.insert_statement(
                subject.clone(),
                predicate,
                named_or_blank_term(&object),
                reifier,
                annotation,
            );
        }
        if element_children.len() > 1 {
            return Err(parse_err(
                "property element contains more than one node element",
            ));
        }

        if property_attrs(element).next().is_some() {
            let object = self.fresh_bnode()?;
            self.insert_statement(
                subject.clone(),
                predicate,
                named_or_blank_term(&object),
                reifier,
                annotation,
            )?;
            self.insert_property_attribute_statements(&object, element, &context)?;
            return Ok(());
        }

        let literal = self.context_literal(&element_text(element), None, &context)?;
        self.insert_statement(
            subject.clone(),
            predicate,
            literal.into(),
            reifier,
            annotation,
        )
    }

    fn insert_property_attribute_statements(
        &mut self,
        subject: &NamedOrBlankNode,
        element: Node<'_, '_>,
        context: &ParseContext,
    ) -> Result<(), RdfDiagnostic> {
        for attr in property_attrs(element) {
            let literal = self.context_literal(attr.value(), None, context)?;
            self.insert_statement(
                subject.clone(),
                name_iri(attr.namespace(), attr.name())?,
                literal.into(),
                None,
                None,
            )?;
        }
        Ok(())
    }

    fn parse_collection(
        &mut self,
        element: Node<'_, '_>,
        context: &ParseContext,
    ) -> Result<RdfTerm, RdfDiagnostic> {
        let items: Vec<Node<'_, '_>> = element_children(element).collect();
        if items.is_empty() {
            return Ok(rdf_iri(RDF_NIL)?.into());
        }
        let nodes = (0..items.len())
            .map(|_| self.fresh_collection_bnode())
            .collect::<Result<Vec<_>, _>>()?;
        for (index, item) in items.iter().enumerate() {
            let object = self.parse_node_element(*item, context)?;
            self.insert_statement(
                nodes[index].clone(),
                rdf_iri(RDF_FIRST)?,
                named_or_blank_term(&object),
                None,
                None,
            )?;
            let rest: RdfTerm = if let Some(next) = nodes.get(index + 1) {
                named_or_blank_term(next)
            } else {
                rdf_iri(RDF_NIL)?.into()
            };
            self.insert_statement(nodes[index].clone(), rdf_iri(RDF_REST)?, rest, None, None)?;
        }
        Ok(named_or_blank_term(
            nodes.first().expect("non-empty collection has a head node"),
        ))
    }

    fn parse_triple_element(
        &mut self,
        element: Node<'_, '_>,
        context: &ParseContext,
    ) -> Result<RdfTriple, RdfDiagnostic> {
        let nodes: Vec<Node<'_, '_>> = element_children(element).collect();
        if nodes.len() != 1 {
            return Err(parse_err(
                "rdf:parseType=\"Triple\" requires one node element",
            ));
        }
        let node = nodes[0];
        let triple_subject = self.subject_for_node(node, context)?;
        let node_ctx = context.for_child(node)?;

        // The single predicate/object may come from a child property element, a
        // `rdf:type` attribute, or another property attribute (literal-valued).
        let type_attr = attr_rdf(node, RDF_TYPE);
        let prop_attrs: Vec<roxmltree::Attribute<'_, '_>> = property_attrs(node).collect();
        let child_props: Vec<Node<'_, '_>> = element_children(node).collect();
        if type_attr.is_some() as usize + prop_attrs.len() + child_props.len() != 1 {
            return Err(parse_err(
                "rdf:parseType=\"Triple\" requires exactly one predicate/object",
            ));
        }
        let (predicate, object): (Iri, RdfTerm) = if let Some(type_iri) = type_attr {
            (
                rdf_iri(RDF_TYPE)?,
                self.iri_ref(type_iri, &node_ctx)?.into(),
            )
        } else if let Some(attr) = prop_attrs.first() {
            (
                name_iri(attr.namespace(), attr.name())?,
                self.context_literal(attr.value(), None, &node_ctx)?.into(),
            )
        } else {
            (
                element_iri(child_props[0])?,
                self.triple_object(child_props[0], context)?,
            )
        };
        Ok(RdfTriple::new(triple_subject, predicate, object))
    }

    fn triple_object(
        &mut self,
        property: Node<'_, '_>,
        context: &ParseContext,
    ) -> Result<RdfTerm, RdfDiagnostic> {
        let context = context.for_child(property)?;
        if let Some(resource) = attr_rdf(property, RDF_RESOURCE) {
            return Ok(self.iri_ref(resource, &context)?.into());
        }
        if let Some(node_id) = attr_rdf(property, RDF_NODE_ID) {
            return Ok(blank_node(node_id)?.into());
        }
        if let Some("Triple") = attr_rdf(property, RDF_PARSE_TYPE) {
            return Ok(RdfTerm::Triple(Box::new(
                self.parse_triple_element(property, &context)?,
            )));
        }
        let nodes: Vec<Node<'_, '_>> = element_children(property).collect();
        if nodes.len() == 1 {
            let object = self.subject_for_node(nodes[0], &context)?;
            return Ok(named_or_blank_term(&object));
        }
        if nodes.len() > 1 {
            return Err(parse_err(
                "rdf:parseType=\"Triple\" object has multiple node elements",
            ));
        }
        Ok(self
            .context_literal(
                &element_text(property),
                attr_rdf(property, RDF_DATATYPE),
                &context,
            )?
            .into())
    }

    fn subject_for_node(
        &mut self,
        element: Node<'_, '_>,
        context: &ParseContext,
    ) -> Result<NamedOrBlankNode, RdfDiagnostic> {
        if let Some(about) = attr_rdf(element, RDF_ABOUT) {
            return Ok(self.iri_ref(about, context)?.into());
        }
        if let Some(id) = attr_rdf(element, RDF_ID) {
            return Ok(self.rdf_id_iri(id, context)?.into());
        }
        if let Some(node_id) = attr_rdf(element, RDF_NODE_ID) {
            return Ok(blank_node(node_id)?.into());
        }
        self.fresh_bnode()
    }

    fn insert_statement(
        &mut self,
        subject: NamedOrBlankNode,
        predicate: Iri,
        object: RdfTerm,
        reifier: Option<NamedOrBlankNode>,
        annotation: Option<NamedOrBlankNode>,
    ) -> Result<(), RdfDiagnostic> {
        self.dataset.insert(RdfQuad::new(
            subject.clone(),
            predicate.clone(),
            object.clone(),
            GraphName::DefaultGraph,
        ));
        // `rdf:ID` on a property element is RDF 1.0 reification (the classic
        // rdf:Statement/subject/predicate/object quads); `rdf:annotation` /
        // `rdf:annotationNodeID` is the RDF 1.2 reifier (rdf:reifies a triple term).
        if let Some(reifier) = reifier {
            self.insert_classic_reification(
                reifier,
                subject.clone(),
                predicate.clone(),
                object.clone(),
            )?;
        }
        if let Some(annotation) = annotation {
            self.insert_reifier(annotation, subject, predicate, object)?;
        }
        Ok(())
    }

    /// Emit the RDF 1.0 reification quads for a property element carrying `rdf:ID`.
    fn insert_classic_reification(
        &mut self,
        reifier: NamedOrBlankNode,
        subject: NamedOrBlankNode,
        predicate: Iri,
        object: RdfTerm,
    ) -> Result<(), RdfDiagnostic> {
        let g = GraphName::DefaultGraph;
        self.dataset.insert(RdfQuad::new(
            reifier.clone(),
            rdf_iri(RDF_TYPE)?,
            rdf_iri(RDF_STATEMENT)?,
            g.clone(),
        ));
        self.dataset.insert(RdfQuad::new(
            reifier.clone(),
            rdf_iri(RDF_SUBJECT)?,
            named_or_blank_term(&subject),
            g.clone(),
        ));
        self.dataset.insert(RdfQuad::new(
            reifier.clone(),
            rdf_iri(RDF_PREDICATE)?,
            predicate,
            g.clone(),
        ));
        self.dataset
            .insert(RdfQuad::new(reifier, rdf_iri(RDF_OBJECT)?, object, g));
        Ok(())
    }

    fn insert_reifier(
        &mut self,
        reifier: NamedOrBlankNode,
        subject: NamedOrBlankNode,
        predicate: Iri,
        object: RdfTerm,
    ) -> Result<(), RdfDiagnostic> {
        let quoted = RdfTerm::Triple(Box::new(RdfTriple::new(subject, predicate, object)));
        self.dataset.insert(RdfQuad::new(
            reifier,
            rdf_iri(RDF_REIFIES)?,
            quoted,
            GraphName::DefaultGraph,
        ));
        Ok(())
    }

    fn context_literal(
        &self,
        lexical: &str,
        datatype: Option<&str>,
        context: &ParseContext,
    ) -> Result<Literal, RdfDiagnostic> {
        if let Some(datatype) = datatype {
            return Ok(Literal::new_typed_literal(
                lexical,
                self.iri_ref(datatype, context)?,
            ));
        }
        if let Some(language) = &context.language {
            if let Some(direction) = context.direction {
                return Literal::new_directional_language_tagged_literal(
                    lexical, language, direction,
                )
                .map_err(adapter_err);
            }
            return Literal::new_language_tagged_literal(lexical, language).map_err(adapter_err);
        }
        Ok(Literal::new_simple_literal(lexical))
    }

    fn iri_ref(&self, value: &str, context: &ParseContext) -> Result<Iri, RdfDiagnostic> {
        let iri = if has_iri_scheme(value) {
            value.to_string()
        } else if let Some(base) = &context.base_iri {
            resolve_relative_iri(base, value)
        } else {
            value.to_string()
        };
        Iri::new(iri).map_err(adapter_err)
    }

    fn rdf_id_iri(&self, value: &str, context: &ParseContext) -> Result<Iri, RdfDiagnostic> {
        if value.is_empty() {
            return Err(parse_err("empty rdf:ID"));
        }
        let Some(base) = &context.base_iri else {
            return Iri::new(format!("#{value}")).map_err(adapter_err);
        };
        let base_without_fragment = base
            .split_once('#')
            .map_or(base.as_str(), |(before, _)| before);
        Iri::new(format!("{base_without_fragment}#{value}")).map_err(adapter_err)
    }

    fn fresh_bnode(&mut self) -> Result<NamedOrBlankNode, RdfDiagnostic> {
        let id = self.bnode_counter;
        self.bnode_counter += 1;
        Ok(blank_node(&deterministic_label("rdfxml_", id as u128))?.into())
    }

    fn fresh_collection_bnode(&mut self) -> Result<NamedOrBlankNode, RdfDiagnostic> {
        let id = self.collection_counter;
        self.collection_counter += 1;
        Ok(blank_node(&deterministic_label("rdfxml_list_", id as u128))?.into())
    }
}

// ── roxmltree element/attribute helpers (RDF/XML name matching) ─────────────────

fn is_rdf(element: Node<'_, '_>, local: &str) -> bool {
    element.tag_name().namespace() == Some(RDF_NS) && element.tag_name().name() == local
}

/// The IRI of a node element / property element: `namespace + local`.
fn element_iri(element: Node<'_, '_>) -> Result<Iri, RdfDiagnostic> {
    name_iri(element.tag_name().namespace(), element.tag_name().name())
}

fn name_iri(namespace: Option<&str>, local: &str) -> Result<Iri, RdfDiagnostic> {
    Iri::new(format!("{}{local}", namespace.unwrap_or_default())).map_err(adapter_err)
}

fn attr_rdf<'a>(element: Node<'a, '_>, local: &str) -> Option<&'a str> {
    attr_in_ns(element, RDF_NS, local)
}

fn attr_xml<'a>(element: Node<'a, '_>, local: &str) -> Option<&'a str> {
    attr_in_ns(element, XML_NS, local)
}

fn attr_its<'a>(element: Node<'a, '_>, local: &str) -> Option<&'a str> {
    attr_in_ns(element, ITS_NS, local)
}

fn attr_in_ns<'a>(element: Node<'a, '_>, namespace: &str, local: &str) -> Option<&'a str> {
    element
        .attributes()
        .find(|attr| attr.namespace() == Some(namespace) && attr.name() == local)
        .map(|attr| attr.value())
}

/// Property attributes: every attribute that is NOT an `xml:`/`its:` attribute or one
/// of the reserved `rdf:` mapping attributes — exactly the gmeow-gts `property_attrs`
/// filter.
fn property_attrs<'a, 'input>(
    element: Node<'a, 'input>,
) -> impl Iterator<Item = roxmltree::Attribute<'a, 'input>> {
    element
        .attributes()
        .filter(|attr| attr.namespace() != Some(XML_NS))
        .filter(|attr| attr.namespace() != Some(ITS_NS))
        .filter(|attr| {
            !(attr.namespace() == Some(RDF_NS)
                && matches!(
                    attr.name(),
                    RDF_ABOUT
                        | RDF_ID
                        | RDF_NODE_ID
                        | RDF_RESOURCE
                        | RDF_DATATYPE
                        | RDF_PARSE_TYPE
                        | RDF_TYPE
                        | RDF_VERSION
                        | RDF_ANNOTATION
                        | RDF_ANNOTATION_NODE_ID
                ))
        })
}

/// Element children of `node`, in document order (skipping text / comment nodes).
fn element_children<'a, 'input>(node: Node<'a, 'input>) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(Node::is_element)
}

/// Concatenate the direct text-node children of `element` (the literal text content).
fn element_text(element: Node<'_, '_>) -> String {
    element
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect()
}

fn rdf_iri(local: &str) -> Result<Iri, RdfDiagnostic> {
    Iri::new(format!("{RDF_NS}{local}")).map_err(adapter_err)
}

fn blank_node(label: &str) -> Result<BlankNode, RdfDiagnostic> {
    BlankNode::new(label).map_err(adapter_err)
}

fn named_or_blank_term(node: &NamedOrBlankNode) -> RdfTerm {
    match node {
        NamedOrBlankNode::Iri(iri) => iri.clone().into(),
        NamedOrBlankNode::BlankNode(node) => node.clone().into(),
    }
}

/// Reproduce gmeow-gts's crate-private `deterministic_label(prefix, counter)`:
/// `prefix` + the Crockford-Base32 ULID rendering of the zero-timestamp counter.
/// Both `Ulid::from_counter` and its `Display` are public, so this keeps generated
/// RDF/XML blank labels byte-identical to the prior path.
fn deterministic_label(prefix: &str, counter: u128) -> String {
    let ulid = gmeow_gts::ulid::Ulid::from_counter(0, counter)
        .expect("counter-derived labels fit in the 80-bit ULID field");
    format!("{prefix}{ulid}")
}

// ── XML-literal (`rdf:parseType="Literal"`) inclusive canonicalization ──────────

/// Serialize an element's children as the canonical XML-literal lexical form, the
/// `rdf:parseType="Literal"` object value. The literal's apex elements carry the
/// in-scope namespace declarations (inclusive canonicalization); descendants inherit
/// them and add none — matching the prior gmeow-gts XML-literal canonicalization.
fn serialize_children_as_xml(element: Node<'_, '_>) -> String {
    // In-scope namespace declarations on the literal apex, in declaration order
    // (excluding the implicit `xml` prefix, which is never rendered).
    let apex_ns: Vec<(String, String)> = element
        .namespaces()
        .filter(|ns| ns.name() != Some("xml"))
        .map(|ns| {
            (
                ns.name().unwrap_or_default().to_string(),
                ns.uri().to_string(),
            )
        })
        .collect();
    let mut out = String::new();
    for child in element.children() {
        if child.is_element() || child.is_text() {
            serialize_xml_node(child, Some(&apex_ns), &mut out);
        }
    }
    out
}

fn serialize_xml_node(node: Node<'_, '_>, apex_ns: Option<&[(String, String)]>, out: &mut String) {
    if node.is_text() {
        if let Some(text) = node.text() {
            out.push_str(&escape_xml_text(text));
        }
        return;
    }
    if !node.is_element() {
        return;
    }
    let raw = raw_name(node);
    out.push('<');
    out.push_str(&raw);
    if let Some(namespaces) = apex_ns {
        for (prefix, iri) in namespaces {
            if prefix.is_empty() {
                out.push_str(&format!(" xmlns=\"{}\"", escape_xml_attr(iri)));
            } else {
                out.push_str(&format!(" xmlns:{prefix}=\"{}\"", escape_xml_attr(iri)));
            }
        }
    }
    for attr in node.attributes() {
        out.push(' ');
        out.push_str(&raw_attr_name(node, attr));
        out.push_str("=\"");
        out.push_str(&escape_xml_attr(attr.value()));
        out.push('"');
    }
    // Canonical XML has no self-closing form: always emit a start/end pair.
    out.push('>');
    for child in node.children() {
        if child.is_element() || child.is_text() {
            serialize_xml_node(child, None, out);
        }
    }
    out.push_str("</");
    out.push_str(&raw);
    out.push('>');
}

/// The raw (prefixed) element name as it would be written: `prefix:local` when the
/// element's namespace has a bound prefix, else its local name.
fn raw_name(node: Node<'_, '_>) -> String {
    let name = node.tag_name();
    qualify(node, name.namespace(), name.name())
}

/// The raw (prefixed) attribute name. An unprefixed attribute carries no namespace.
fn raw_attr_name(node: Node<'_, '_>, attr: roxmltree::Attribute<'_, '_>) -> String {
    match attr.namespace() {
        Some(ns) => qualify(node, Some(ns), attr.name()),
        None => attr.name().to_string(),
    }
}

fn qualify(node: Node<'_, '_>, namespace: Option<&str>, local: &str) -> String {
    match namespace {
        Some(ns) => match node.lookup_prefix(ns) {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}:{local}"),
            _ => local.to_string(),
        },
        None => local.to_string(),
    }
}

fn escape_xml_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_xml_attr(value: &str) -> String {
    escape_xml_text(value).replace('"', "&quot;")
}

// ───────────────────────────────────────────────────────────────────────────────
// Serialize: GtsGraph → RDF/XML text (via gmeow_gts::rdf::to_rdf_quads)
// ───────────────────────────────────────────────────────────────────────────────

/// Serialize a folded default-graph [`GtsGraph`] to RDF/XML text, exporting its quads
/// through `to_rdf_quads` and grouping by subject. Named graphs are rejected (RDF/XML
/// is a single-graph syntax). Byte-identical to the prior gmeow-gts `to_rdf_xml`.
pub fn serialize_gts_graph_to_rdfxml(graph: &GtsGraph) -> Result<String, RdfDiagnostic> {
    let mut subjects: BTreeMap<String, Vec<(Iri, RdfTerm)>> = BTreeMap::new();
    let mut subject_nodes: BTreeMap<String, NamedOrBlankNode> = BTreeMap::new();
    for quad in to_rdf_quads(graph).map_err(adapter_err)? {
        if !quad.graph_name.is_default_graph() {
            return Err(serialize_err(format!(
                "cannot serialize named graph {}",
                quad.graph_name
            )));
        }
        let key = subject_key(&quad.subject);
        subject_nodes
            .entry(key.clone())
            .or_insert_with(|| quad.subject.clone());
        subjects
            .entry(key)
            .or_default()
            .push((quad.predicate, quad.object));
    }

    let namespaces = serializer_namespaces(&subjects);
    let mut out = String::from(
        "<?xml version=\"1.0\"?>\n<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\" xmlns:xsd=\"http://www.w3.org/2001/XMLSchema#\"",
    );
    for (namespace, prefix) in &namespaces {
        if prefix != "rdf" && prefix != "xsd" {
            out.push_str(&format!(
                " xmlns:{prefix}=\"{}\"",
                escape_xml_attr(namespace)
            ));
        }
    }
    // Declare RDF 1.2 so a round-trip preserves triple terms and base direction (their
    // parse is gated on `rdf:version="1.2"`).
    out.push_str(" rdf:version=\"1.2\">\n");

    for (key, properties) in subjects {
        let subject = subject_nodes
            .get(&key)
            .expect("subject node exists for every grouped subject");
        out.push_str("  <rdf:Description");
        match subject {
            NamedOrBlankNode::Iri(iri) => {
                out.push_str(&format!(" rdf:about=\"{}\"", escape_xml_attr(iri.as_str())));
            }
            NamedOrBlankNode::BlankNode(node) => {
                out.push_str(&format!(
                    " rdf:nodeID=\"{}\"",
                    escape_xml_attr(node.as_str())
                ));
            }
        }
        out.push_str(">\n");
        for (predicate, object) in properties {
            write_property(&mut out, "    ", &predicate, &object, &namespaces)?;
        }
        out.push_str("  </rdf:Description>\n");
    }

    out.push_str("</rdf:RDF>\n");
    Ok(out)
}

fn subject_key(subject: &NamedOrBlankNode) -> String {
    match subject {
        NamedOrBlankNode::Iri(iri) => format!("I{}", iri.as_str()),
        NamedOrBlankNode::BlankNode(node) => format!("B{}", node.as_str()),
    }
}

fn serializer_namespaces(
    subjects: &BTreeMap<String, Vec<(Iri, RdfTerm)>>,
) -> BTreeMap<String, String> {
    let mut namespaces = BTreeMap::from([
        (RDF_NS.to_string(), "rdf".to_string()),
        (XSD_NS.to_string(), "xsd".to_string()),
    ]);
    let mut next = 0usize;
    for properties in subjects.values() {
        for (predicate, _) in properties {
            let namespace = split_property_iri(predicate.as_str()).0;
            if namespaces.contains_key(namespace) {
                continue;
            }
            namespaces.insert(namespace.to_string(), format!("ns{next}"));
            next += 1;
        }
    }
    namespaces
}

fn write_property(
    out: &mut String,
    indent: &str,
    predicate: &Iri,
    object: &RdfTerm,
    namespaces: &BTreeMap<String, String>,
) -> Result<(), RdfDiagnostic> {
    let name = serializer_qname(predicate.as_str(), namespaces);
    match object {
        RdfTerm::Iri(iri) => {
            out.push_str(&format!(
                "{indent}<{name} rdf:resource=\"{}\"/>\n",
                escape_xml_attr(iri.as_str())
            ));
        }
        RdfTerm::BlankNode(node) => {
            out.push_str(&format!(
                "{indent}<{name} rdf:nodeID=\"{}\"/>\n",
                escape_xml_attr(node.as_str())
            ));
        }
        RdfTerm::Literal(literal) => {
            out.push_str(&format!("{indent}<{name}"));
            if let Some(language) = &literal.language {
                out.push_str(&format!(" xml:lang=\"{}\"", escape_xml_attr(language)));
            }
            if let Some(direction) = literal.direction {
                out.push_str(&format!(
                    " xmlns:its=\"{ITS_NS}\" its:dir=\"{}\"",
                    direction.as_str()
                ));
            }
            if let Some(datatype) = &literal.datatype {
                out.push_str(&format!(
                    " rdf:datatype=\"{}\"",
                    escape_xml_attr(datatype.as_str())
                ));
            }
            out.push_str(&format!(
                ">{}</{name}>\n",
                escape_xml_text(&literal.lexical)
            ));
        }
        RdfTerm::Triple(triple) => {
            out.push_str(&format!("{indent}<{name} rdf:parseType=\"Triple\">\n"));
            write_triple_node(out, &format!("{indent}  "), triple, namespaces)?;
            out.push_str(&format!("{indent}</{name}>\n"));
        }
    }
    Ok(())
}

fn write_triple_node(
    out: &mut String,
    indent: &str,
    triple: &RdfTriple,
    namespaces: &BTreeMap<String, String>,
) -> Result<(), RdfDiagnostic> {
    out.push_str(&format!("{indent}<rdf:Description"));
    match &triple.subject {
        NamedOrBlankNode::Iri(iri) => {
            out.push_str(&format!(" rdf:about=\"{}\"", escape_xml_attr(iri.as_str())));
        }
        NamedOrBlankNode::BlankNode(node) => {
            out.push_str(&format!(
                " rdf:nodeID=\"{}\"",
                escape_xml_attr(node.as_str())
            ));
        }
    }
    out.push_str(">\n");
    write_property(
        out,
        &format!("{indent}  "),
        &triple.predicate,
        &triple.object,
        namespaces,
    )?;
    out.push_str(&format!("{indent}</rdf:Description>\n"));
    Ok(())
}

fn serializer_qname(iri: &str, namespaces: &BTreeMap<String, String>) -> String {
    let (namespace, local) = split_property_iri(iri);
    let prefix = namespaces
        .get(namespace)
        .map(String::as_str)
        .unwrap_or("ns");
    format!("{prefix}:{local}")
}

fn split_property_iri(iri: &str) -> (&str, &str) {
    let split = iri
        .rfind(['#', '/', ':'])
        .map(|index| index + 1)
        .unwrap_or(0);
    let (namespace, local) = iri.split_at(split);
    if local.is_empty() || !is_xml_name(local) {
        (iri, "property")
    } else {
        (namespace, local)
    }
}

fn is_xml_name(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_xml_name_start(first) {
        return false;
    }
    chars.all(is_xml_name_char)
}

fn is_xml_name_start(ch: char) -> bool {
    ch == '_' || ch.is_alphabetic()
}

fn is_xml_name_char(ch: char) -> bool {
    is_xml_name_start(ch) || ch.is_numeric() || matches!(ch, '-' | '.')
}

// ── Relative-IRI resolution (mirrors gmeow_gts::rdf_xml::resolve_relative_iri) ───

fn has_iri_scheme(value: &str) -> bool {
    let mut chars = value.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !first.is_ascii_alphabetic() {
        return false;
    }
    for ch in chars {
        if ch == ':' {
            return true;
        }
        if !(ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.')) {
            return false;
        }
    }
    false
}

fn remove_dot_segments(path: &str) -> String {
    let absolute = path.starts_with('/');
    let keep_trailing_slash = path.ends_with('/')
        || path.ends_with("/.")
        || path.ends_with("/..")
        || path == "."
        || path == "..";
    let mut segments = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" | "." => {}
            ".." => {
                segments.pop();
            }
            segment => segments.push(segment),
        }
    }

    let mut normalized = String::new();
    if absolute {
        normalized.push('/');
    }
    normalized.push_str(&segments.join("/"));
    if keep_trailing_slash && !normalized.ends_with('/') {
        normalized.push('/');
    }
    if normalized.is_empty() && absolute {
        normalized.push('/');
    }
    normalized
}

fn split_raw_path_suffix(raw: &str) -> (&str, &str) {
    let split = raw.find(['?', '#']).unwrap_or(raw.len());
    (&raw[..split], &raw[split..])
}

fn split_base_for_path(base: &str) -> (String, &str) {
    let Some(scheme_end) = base.find(':') else {
        return (String::new(), base);
    };
    let scheme_prefix = &base[..=scheme_end];
    let rest = &base[scheme_end + 1..];
    if let Some(after_slashes) = rest.strip_prefix("//") {
        let authority_end = after_slashes.find('/').unwrap_or(after_slashes.len());
        let authority = &after_slashes[..authority_end];
        let path = &after_slashes[authority_end..];
        (format!("{scheme_prefix}//{authority}"), path)
    } else {
        (scheme_prefix.to_string(), rest)
    }
}

fn resolve_relative_iri(base: &str, raw: &str) -> String {
    if has_iri_scheme(raw) {
        return raw.to_string();
    }

    let base_without_fragment = base.split_once('#').map_or(base, |(before, _)| before);
    if raw.is_empty() {
        return base_without_fragment.to_string();
    }
    if raw.starts_with('#') {
        return format!("{base_without_fragment}{raw}");
    }

    let base_without_query = base_without_fragment
        .split_once('?')
        .map_or(base_without_fragment, |(before, _)| before);
    if raw.starts_with('?') {
        return format!("{base_without_query}{raw}");
    }

    if raw.starts_with("//") {
        if let Some(scheme_end) = base.find(':') {
            return format!("{}:{raw}", &base[..scheme_end]);
        }
        return raw.to_string();
    }

    let (prefix, base_path) = split_base_for_path(base_without_query);
    let (raw_path, suffix) = split_raw_path_suffix(raw);
    let merged_path = if raw_path.starts_with('/') {
        raw_path.to_string()
    } else {
        let base_dir = if base_path.is_empty() {
            "/"
        } else {
            base_path
                .rfind('/')
                .map(|index| &base_path[..=index])
                .unwrap_or("")
        };
        format!("{base_dir}{raw_path}")
    };
    format!("{prefix}{}{}", remove_dot_segments(&merged_path), suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gts::dataset_from_gts_graph;

    /// Parse RDF/XML straight into a frozen dataset, for assertions over quads.
    fn parse(text: &str, base: Option<&str>) -> std::sync::Arc<crate::RdfDataset> {
        let graph = parse_rdfxml_to_gts_graph(text, base).expect("parse rdf/xml");
        dataset_from_gts_graph(&graph).expect("fold to dataset")
    }

    #[test]
    fn description_with_property_round_trips() {
        let text = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:eg="http://example.org/">
  <rdf:Description rdf:about="http://example.org/s">
    <eg:p rdf:resource="http://example.org/o"/>
  </rdf:Description>
</rdf:RDF>"#;
        let ds = parse(text, None);
        assert_eq!(ds.quad_count(), 1);
        // Serialize → re-parse must be isomorphic.
        let graph = parse_rdfxml_to_gts_graph(text, None).expect("parse");
        let xml = serialize_gts_graph_to_rdfxml(&graph).expect("serialize");
        let reparsed = parse(&xml, None);
        assert!(
            crate::datasets_isomorphic(&ds, &reparsed),
            "rdf/xml round-trip must be isomorphic"
        );
    }

    #[test]
    fn typed_node_emits_rdf_type() {
        let text = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:eg="http://example.org/">
  <eg:Thing rdf:about="http://example.org/s"/>
</rdf:RDF>"#;
        let ds = parse(text, None);
        assert_eq!(ds.quad_count(), 1, "typed node element emits rdf:type quad");
    }

    #[test]
    fn literal_with_lang_and_datatype() {
        let text = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:eg="http://example.org/">
  <rdf:Description rdf:about="http://example.org/s">
    <eg:label xml:lang="en">hello</eg:label>
    <eg:count rdf:datatype="http://www.w3.org/2001/XMLSchema#integer">42</eg:count>
  </rdf:Description>
</rdf:RDF>"#;
        let ds = parse(text, None);
        assert_eq!(ds.quad_count(), 2);
    }

    #[test]
    fn collection_expands_to_list() {
        let text = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:eg="http://example.org/">
  <rdf:Description rdf:about="http://example.org/s">
    <eg:items rdf:parseType="Collection">
      <rdf:Description rdf:about="http://example.org/a"/>
      <rdf:Description rdf:about="http://example.org/b"/>
    </eg:items>
  </rdf:Description>
</rdf:RDF>"#;
        let ds = parse(text, None);
        // head quad + 2*(first,rest) = 1 + 4.
        assert_eq!(ds.quad_count(), 5, "two-item collection expands to a list");
    }

    #[test]
    fn rdf_id_resolves_against_base() {
        let text = r#"<?xml version="1.0"?>
<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
         xmlns:eg="http://example.org/">
  <rdf:Description rdf:ID="x">
    <eg:p rdf:resource="http://example.org/o"/>
  </rdf:Description>
</rdf:RDF>"#;
        let ds = parse(text, Some("http://base.example/doc"));
        assert!(
            ds.term_id_by_value(&crate::TermValue::Iri(
                "http://base.example/doc#x".to_owned()
            ))
            .is_some(),
            "rdf:ID resolves to base#x"
        );
    }
}
