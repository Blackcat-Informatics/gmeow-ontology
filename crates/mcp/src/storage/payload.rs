// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The application's sole tool-call RDF owner, shared by both stores and replay.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;

use gmeow_errors::Result;
use purrdf::ir::RdfDatasetBuilder;
use purrdf::{RdfDataset, RdfLiteral, RdfQuad, RdfTerm};

use super::{err, record_id, require_iri};

const G: &str = "https://blackcatinformatics.ca/gmeow/";
const TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const CREATED: &str = "http://purl.org/dc/terms/created";
const CONFORMS: &str = "http://purl.org/dc/terms/conformsTo";
const XSD: &str = "http://www.w3.org/2001/XMLSchema#";

/// The explicitly selected policy for opaque text; structured graphs never use it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OpaquePayloadProfile {
    /// Preserve the supplied opaque literal verbatim.
    #[default]
    Verbatim,
    /// Preserve up to 4096 UTF-8 bytes, otherwise retain the legacy BLAKE3 locator.
    InlineOrBlake3,
}

impl OpaquePayloadProfile {
    fn iri(self) -> &'static str {
        match self {
            Self::Verbatim => "urn:gmeow:mcp:opaque-payload:verbatim-v1",
            Self::InlineOrBlake3 => "urn:gmeow:mcp:opaque-payload:inline-or-blake3-v1",
        }
    }

    fn retain(self, literal: &RdfLiteral) -> Result<RdfLiteral> {
        if self == Self::InlineOrBlake3 {
            if literal.datatype_iri() != format!("{XSD}string")
                || literal.language.is_some()
                || literal.direction.is_some()
            {
                return Err(err(
                    "inline-or-blake3 opaque profile requires an untagged xsd:string literal",
                ));
            }
            if literal.lexical_form.len() > 4096 {
                return Ok(RdfLiteral::simple(format!(
                    "blake3:{}",
                    purrdf_hash::hex::encode(
                        purrdf_hash::blake3::hash(literal.lexical_form.as_bytes()).as_bytes()
                    )
                )));
            }
        }
        Ok(literal.clone())
    }
}

/// One graph owns both optional role roots, preserving shared and cyclic nodes.
/// Native scalar terms are data, including IRIs spelling historical record IDs.
#[derive(Clone, Debug)]
pub struct StructuredPayload {
    pub graph: Arc<RdfDataset>,
    pub arguments: Option<RdfTerm>,
    pub result: Option<RdfTerm>,
}

/// The selected audit is part of the same publication as its recorded call.
#[derive(Clone, Copy)]
pub struct ToolCallAudit<'a> {
    pub schema: &'a str,
    pub obtains: &'a [&'a str],
}

/// Application recording inputs. Opaque and structured roles are independent;
/// their coexistence asserts no equivalence. Missing roots assert no null value.
#[derive(Default)]
pub struct ToolCallOptions<'a> {
    pub arguments: Option<&'a RdfLiteral>,
    pub result: Option<&'a RdfLiteral>,
    pub invocation: Option<&'a str>,
    pub generated: &'a [&'a str],
    pub payload: Option<&'a StructuredPayload>,
    pub opaque_profile: Option<OpaquePayloadProfile>,
    pub audit: Option<ToolCallAudit<'a>>,
}

/// A retained application call; ingestion evidence accompanies a native append.
#[derive(Clone, Debug, Default)]
pub struct ToolCallRecord {
    pub id: String,
    pub tool: String,
    pub arguments: Option<RdfLiteral>,
    pub result: Option<RdfLiteral>,
    pub invocation: Option<String>,
    pub created: Option<String>,
    pub generated: Vec<String>,
    pub payload: Option<StructuredPayload>,
    /// None means that no opaque recording profile was asserted.
    pub opaque_profile: Option<OpaquePayloadProfile>,
    pub ingestion: Option<purrdf::gts_compose::IngestReport>,
    /// Package-local append position, distinct from payload item positions.
    pub ordinal: Option<u64>,
}

fn add(
    builder: &mut RdfDatasetBuilder,
    subject: &RdfTerm,
    predicate: &str,
    object: RdfTerm,
) -> Result<()> {
    builder.push_owned_quad(&RdfQuad::new(subject.clone(), predicate.to_owned(), object))?;
    Ok(())
}

fn text(value: &str) -> RdfTerm {
    RdfTerm::Literal(RdfLiteral::simple(value))
}

/// Borrow native terms without changing their identity or graph authority.
struct RowIndex<'a> {
    by_subject: HashMap<&'a RdfTerm, HashMap<&'a str, Vec<&'a RdfTerm>>>,
}

impl<'a> RowIndex<'a> {
    fn new(rows: impl IntoIterator<Item = &'a RdfQuad>) -> Self {
        let mut by_subject: HashMap<&RdfTerm, HashMap<&str, Vec<&RdfTerm>>> = HashMap::new();
        for row in rows {
            by_subject
                .entry(&row.subject)
                .or_default()
                .entry(&row.predicate)
                .or_default()
                .push(&row.object);
        }
        Self { by_subject }
    }
}

fn objects(rows: &RowIndex<'_>, subject: &RdfTerm, predicate: &str) -> Vec<RdfTerm> {
    let mut seen = HashSet::new();
    rows.by_subject
        .get(subject)
        .and_then(|predicates| predicates.get(predicate))
        .into_iter()
        .flatten()
        .filter(|object| seen.insert(**object))
        .map(|object| (**object).clone())
        .collect()
}

fn one(rows: &RowIndex<'_>, subject: &RdfTerm, predicate: &str) -> Result<Option<RdfTerm>> {
    let values = objects(rows, subject, predicate);
    if values.len() > 1 {
        return Err(err(format!(
            "tool payload {subject:?} has distinct {predicate} values"
        )));
    }
    Ok(values.into_iter().next())
}

fn literal(value: Option<RdfTerm>, field: &str) -> Result<Option<RdfLiteral>> {
    value
        .map(|term| match term {
            RdfTerm::Literal(literal) => Ok(literal),
            _ => Err(err(format!("{field} requires a native RDF literal"))),
        })
        .transpose()
}

fn literal_text(value: Option<RdfTerm>, field: &str) -> Result<Option<String>> {
    literal(value, field).map(|value| value.map(|literal| literal.lexical_form))
}

fn iri(value: Option<RdfTerm>, field: &str) -> Result<Option<String>> {
    value
        .map(|term| match term {
            RdfTerm::Iri(iri) => Ok(iri),
            _ => Err(err(format!("{field} requires an IRI"))),
        })
        .transpose()
}

impl StructuredPayload {
    /// Admit native payload branches without closing the selected source graph.
    ///
    /// # Errors
    /// Conflicting branches or malformed facets, channels or positions.
    pub fn new(
        graph: Arc<RdfDataset>,
        arguments: Option<RdfTerm>,
        result: Option<RdfTerm>,
    ) -> Result<Self> {
        let value = Self {
            graph,
            arguments,
            result,
        };
        value.validate()?;
        Ok(value)
    }

    /// Convert only the information present in current parsed JSON values.
    /// Object order and original numeric spellings are deliberately not invented.
    pub fn from_json(
        arguments: Option<&serde_json::Value>,
        result: Option<&serde_json::Value>,
    ) -> Result<Self> {
        let mut builder = RdfDatasetBuilder::new();
        let mut next = 0;
        let arguments = arguments
            .map(|value| json_value(&mut builder, &mut next, value))
            .transpose()?;
        let result = result
            .map(|value| json_value(&mut builder, &mut next, value))
            .transpose()?;
        Self::new(builder.freeze()?, arguments, result)
    }

    fn validate(&self) -> Result<()> {
        let rows = purrdf::flat_rdf_quads_from_dataset(&self.graph);
        Self::validate_index(
            &RowIndex::new(&rows),
            self.arguments.as_ref(),
            self.result.as_ref(),
        )
    }

    fn validate_index(
        rows: &RowIndex<'_>,
        arguments: Option<&RdfTerm>,
        result: Option<&RdfTerm>,
    ) -> Result<()> {
        let mut pending: VecDeque<RdfTerm> = arguments.into_iter().chain(result).cloned().collect();
        let mut seen = HashSet::new();
        while let Some(node) = pending.pop_front() {
            if !seen.insert(node.clone()) {
                continue;
            }
            require_resource(&node)?;
            let types = objects(rows, &node, TYPE);
            let is = |class: &str| types.contains(&RdfTerm::iri(format!("{G}{class}")));
            let scalar = one(rows, &node, &format!("{G}structuredTerm"))?;
            let map = is("StructuredMap");
            let sequence = is("StructuredSequence");
            let null = is("StructuredNull");
            if !is("StructuredValue") && !map && !sequence && !null {
                return Err(err(format!(
                    "scalar payload {node:?} is not a StructuredValue"
                )));
            }
            if usize::from(scalar.is_some())
                + usize::from(map)
                + usize::from(sequence)
                + usize::from(null)
                != 1
            {
                return Err(err(format!(
                    "tool payload {node:?} requires exactly one scalar/map/sequence/null branch"
                )));
            }
            let facets = objects(rows, &node, &format!("{G}hasFacet"));
            let items = objects(rows, &node, &format!("{G}structuredItem"));
            if (!map && !facets.is_empty()) || (!sequence && !items.is_empty()) {
                return Err(err(format!(
                    "tool payload {node:?} has entries outside its branch"
                )));
            }
            let mut positions = HashSet::new();
            for entry in facets.iter().chain(&items) {
                require_resource(entry)?;
                let ordinals = objects(rows, entry, &format!("{G}entryOrdinal"));
                let mut position = None;
                for ordinal in ordinals {
                    let RdfTerm::Literal(value) = ordinal else {
                        return Err(err("entryOrdinal requires a literal"));
                    };
                    if value.datatype_iri() != format!("{XSD}nonNegativeInteger")
                        || value.language.is_some()
                        || value.direction.is_some()
                    {
                        return Err(err("entryOrdinal requires xsd:nonNegativeInteger"));
                    }
                    let parsed =
                        purrdf::xsd::parse_by_iri(&value.lexical_form, value.datatype_iri())
                            .map_err(|e| err(format!("entryOrdinal: {e:?}")))?;
                    let Some(parsed) = parsed else {
                        return Err(err("entryOrdinal has no native value"));
                    };
                    let purrdf::xsd::XsdValue::Integer { value: parsed, .. } = parsed else {
                        return Err(err("entryOrdinal has no native integer value"));
                    };
                    // Native exact integers identify lexical aliases without
                    // changing the source RDF literals that carry them.
                    if position.as_ref().is_some_and(|old| old != &parsed) {
                        return Err(err("one entry has distinct numeric ordinals"));
                    }
                    position = Some(parsed);
                }
                if sequence && position.is_none() {
                    return Err(err("sequence item missing entryOrdinal"));
                }
                if let Some(position) = position
                    && !positions.insert(position)
                {
                    return Err(err("distinct entries have the same numeric ordinal"));
                }
                if map {
                    let key = one(rows, entry, &format!("{G}facetLabel"))?
                        .ok_or_else(|| err("map facet missing facetLabel"))?;
                    let RdfTerm::Literal(key) = key else {
                        return Err(err("facetLabel requires a string/token literal"));
                    };
                    if key.language.is_some() || key.direction.is_some() {
                        return Err(err("facetLabel must be untagged"));
                    }
                    match key.datatype_iri() {
                        "http://www.w3.org/2001/XMLSchema#string" => {
                            purrdf::xsd::simple::read_xml_string(&key.lexical_form)
                        }
                        "http://www.w3.org/2001/XMLSchema#token" => {
                            purrdf::xsd::simple::read_token(&key.lexical_form)
                        }
                        _ => return Err(err("facetLabel requires xsd:string or xsd:token")),
                    }
                    .map_err(|e| err(format!("facetLabel: {e}")))?;
                    let nested = one(rows, entry, &format!("{G}facetStructuredValue"))?;
                    let plain = one(rows, entry, &format!("{G}facetText"))?;
                    let typed = one(rows, entry, &format!("{G}facetValue"))?;
                    if let Some(plain) = &plain {
                        let RdfTerm::Literal(value) = plain else {
                            return Err(err("facetText requires a prose literal"));
                        };
                        if !matches!(
                            value.datatype_iri(),
                            "http://www.w3.org/2001/XMLSchema#string"
                                | "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
                                | "http://www.w3.org/1999/02/22-rdf-syntax-ns#dirLangString"
                        ) {
                            return Err(err(
                                "facetText requires xsd:string, rdf:langString or rdf:dirLangString",
                            ));
                        }
                        RdfLiteral::validate_components(
                            value.datatype_iri(),
                            value.language.as_deref(),
                            value.direction,
                        )
                        .map_err(|cause| err(format!("facetText: {cause}")))?;
                        if value.datatype_iri() == format!("{XSD}string") {
                            purrdf::xsd::simple::read_xml_string(&value.lexical_form)
                                .map_err(|cause| err(format!("facetText: {cause}")))?;
                        }
                    }
                    if typed
                        .as_ref()
                        .is_some_and(|value| !matches!(value, RdfTerm::Literal(_)))
                    {
                        return Err(err("facetValue requires a native RDF literal"));
                    }
                    if usize::from(nested.is_some())
                        + usize::from(plain.is_some())
                        + usize::from(typed.is_some())
                        > 1
                    {
                        return Err(err("facet has conflicting payload channels"));
                    }
                    if let Some(nested) = nested {
                        pending.push_back(nested);
                    }
                } else {
                    pending.push_back(entry.clone());
                }
            }
        }
        Ok(())
    }
}

fn require_resource(term: &RdfTerm) -> Result<()> {
    if matches!(term, RdfTerm::Iri(_) | RdfTerm::BlankNode(_)) {
        Ok(())
    } else {
        Err(err("payload occurrence must be an IRI or blank node"))
    }
}

fn json_value(
    builder: &mut RdfDatasetBuilder,
    next: &mut usize,
    value: &serde_json::Value,
) -> Result<RdfTerm> {
    let node = RdfTerm::BlankNode(format!("payload{next}"));
    *next += 1;
    add(
        builder,
        &node,
        TYPE,
        RdfTerm::iri(format!("{G}StructuredValue")),
    )?;
    match value {
        serde_json::Value::Null => add(
            builder,
            &node,
            TYPE,
            RdfTerm::iri(format!("{G}StructuredNull")),
        )?,
        serde_json::Value::Object(entries) => {
            add(
                builder,
                &node,
                TYPE,
                RdfTerm::iri(format!("{G}StructuredMap")),
            )?;
            for (key, value) in entries {
                let facet = RdfTerm::BlankNode(format!("facet{next}"));
                *next += 1;
                add(builder, &node, &format!("{G}hasFacet"), facet.clone())?;
                add(
                    builder,
                    &facet,
                    TYPE,
                    RdfTerm::iri(format!("{G}LabelledFacet")),
                )?;
                add(builder, &facet, &format!("{G}facetLabel"), text(key))?;
                let nested = json_value(builder, next, value)?;
                add(builder, &facet, &format!("{G}facetStructuredValue"), nested)?;
            }
        }
        serde_json::Value::Array(entries) => {
            add(
                builder,
                &node,
                TYPE,
                RdfTerm::iri(format!("{G}StructuredSequence")),
            )?;
            for (index, value) in entries.iter().enumerate() {
                let item = json_value(builder, next, value)?;
                add(builder, &node, &format!("{G}structuredItem"), item.clone())?;
                add(
                    builder,
                    &item,
                    &format!("{G}entryOrdinal"),
                    RdfTerm::Literal(RdfLiteral::typed(
                        index.to_string(),
                        format!("{XSD}nonNegativeInteger"),
                    )),
                )?;
            }
        }
        serde_json::Value::String(value) => {
            add(builder, &node, &format!("{G}structuredTerm"), text(value))?
        }
        serde_json::Value::Bool(value) => add(
            builder,
            &node,
            &format!("{G}structuredTerm"),
            RdfTerm::Literal(RdfLiteral::typed(
                value.to_string(),
                format!("{XSD}boolean"),
            )),
        )?,
        serde_json::Value::Number(value) => {
            let datatype = if value.is_i64() || value.is_u64() {
                "integer"
            } else {
                "double"
            };
            add(
                builder,
                &node,
                &format!("{G}structuredTerm"),
                RdfTerm::Literal(RdfLiteral::typed(
                    value.to_string(),
                    format!("{XSD}{datatype}"),
                )),
            )?;
        }
    }
    Ok(node)
}

/// Deterministic RDF serialization, with one native blank-node canonicalization.
pub(super) fn serialize(graph: &RdfDataset) -> Result<String> {
    let graph = purrdf::canonical_relabel(
        graph,
        &mut purrdf::CanonicalizationControl::new(purrdf::RDFC_V2_CORPUS_POLICY, None),
    )?;
    let bytes = purrdf::serialize_dataset(
        &graph,
        "application/n-quads",
        purrdf::SerializeGraph::Dataset,
    )
    .map_err(|cause| err(format!("tool payload RDF serialization: {cause}")))?;
    String::from_utf8(bytes).map_err(|cause| {
        err(format!(
            "native N-Quads serialization is not UTF-8: {cause}"
        ))
    })
}

pub(super) fn make_call(
    tool: &str,
    options: &ToolCallOptions<'_>,
    sequence: u64,
    created: String,
) -> Result<ToolCallRecord> {
    require_iri(tool, "gmeow:usedTool", "new call")?;
    if let Some(invocation) = options.invocation {
        require_iri(invocation, "gmeow:calledByInvocation", "new call")?;
    }
    for generated in options.generated {
        require_iri(generated, "generated entity", "new call")?;
    }
    if let Some(payload) = options.payload {
        payload.validate()?;
    }
    let arguments = options
        .arguments
        .map(|value| options.opaque_profile.unwrap_or_default().retain(value))
        .transpose()?;
    let result = options
        .result
        .map(|value| options.opaque_profile.unwrap_or_default().retain(value))
        .transpose()?;
    let mut record = ToolCallRecord {
        id: String::new(),
        tool: tool.to_owned(),
        arguments,
        result,
        invocation: options.invocation.map(str::to_owned),
        created: Some(created),
        generated: options
            .generated
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        payload: options.payload.cloned(),
        opaque_profile: options.opaque_profile,
        ingestion: None,
        ordinal: Some(sequence),
    };
    // The existing application ID scheme hashes one canonical complete input graph;
    // roles and generated links participate, and the append position separates repeats.
    let identity_graph = call_graph(&record, "urn:gmeow:mcp:call-input", &record.generated)?;
    let identity = serialize(identity_graph.as_ref())?;
    record.id = record_id("call", sequence, &[&identity]);
    Ok(record)
}

/// Reserve one application blank scope disjoint from every native source term,
/// including blanks nested in quoted triples and graph names.
fn envelope_scope(graph: &RdfDataset) -> Result<purrdf::BlankScope> {
    let mut occupied = BTreeSet::new();
    for index in 0..graph.term_count() {
        let id = purrdf::TermId::from_index(
            u32::try_from(index).map_err(|_| err("payload term identity space exhausted"))?,
        )?;
        if let purrdf::TermValue::Blank { scope, .. } = graph.term_value(id) {
            occupied.insert(scope.ordinal());
        }
    }
    let mut scope = 0u32;
    while occupied.contains(&scope) {
        scope = scope
            .checked_add(1)
            .ok_or_else(|| err("no blank scope remains for call graph ownership"))?;
    }
    Ok(purrdf::BlankScope(scope))
}

/// The application's source-graph envelope uses the existing StructuredMap
/// vocabulary. Each repeated source-graph facet is also the stored graph address;
/// its scalar/null payload retains the original named/default graph identity.
fn write_payload(
    builder: &mut RdfDatasetBuilder,
    subject: &str,
    payload: &StructuredPayload,
) -> Result<()> {
    payload.validate()?;
    let map = RdfTerm::iri(format!("{subject}#payload-graphs"));
    add(
        builder,
        &map,
        TYPE,
        RdfTerm::iri(format!("{G}StructuredValue")),
    )?;
    add(
        builder,
        &map,
        TYPE,
        RdfTerm::iri(format!("{G}StructuredMap")),
    )?;
    let scope = envelope_scope(&payload.graph)?;
    let rows = purrdf::flat_rdf_quads_from_dataset(&payload.graph);
    let mut graphs: Vec<Option<RdfTerm>> = payload.graph.owned_named_graphs().map(Some).collect();
    for row in &rows {
        if !graphs.contains(&row.graph_name) {
            graphs.push(row.graph_name.clone());
        }
    }
    for (index, original) in graphs.into_iter().enumerate() {
        let address = RdfTerm::BlankNode(
            scope
                .qualify_label(&format!("payloadGraph{index}"))
                .into_owned(),
        );
        let name = RdfTerm::BlankNode(
            scope
                .qualify_label(&format!("payloadGraphName{index}"))
                .into_owned(),
        );
        add(builder, &map, &format!("{G}hasFacet"), address.clone())?;
        add(
            builder,
            &address,
            TYPE,
            RdfTerm::iri(format!("{G}LabelledFacet")),
        )?;
        add(
            builder,
            &address,
            &format!("{G}facetLabel"),
            text("source-graph"),
        )?;
        add(
            builder,
            &address,
            &format!("{G}facetStructuredValue"),
            name.clone(),
        )?;
        add(
            builder,
            &name,
            TYPE,
            RdfTerm::iri(format!("{G}StructuredValue")),
        )?;
        match &original {
            Some(original) => add(
                builder,
                &name,
                &format!("{G}structuredTerm"),
                original.clone(),
            )?,
            None => add(
                builder,
                &name,
                TYPE,
                RdfTerm::iri(format!("{G}StructuredNull")),
            )?,
        }
        // Flat native RDF rows keep payload reification as data. They must not
        // become native memory-claim/reifier frames visible to delegated Memory.
        for row in rows.iter().filter(|row| row.graph_name == original) {
            let mut stored = row.clone();
            stored.graph_name = Some(address.clone());
            builder.push_owned_quad(&stored)?;
        }
    }
    Ok(())
}

fn read_payload(
    graphs: &HashMap<&RdfTerm, Vec<&RdfQuad>>,
    metadata: &RowIndex<'_>,
    subject: &str,
    arguments: Option<RdfTerm>,
    result: Option<RdfTerm>,
) -> Result<Option<StructuredPayload>> {
    let map = RdfTerm::iri(format!("{subject}#payload-graphs"));
    let classes = objects(metadata, &map, TYPE);
    if !classes.contains(&RdfTerm::iri(format!("{G}StructuredMap"))) {
        if arguments.is_some() || result.is_some() {
            return Err(err(
                "structured call has no complete source-graph ownership envelope",
            ));
        }
        return Ok(None);
    }
    // The envelope itself uses the same canonical branch/facet owner; decoding
    // its source-graph profile never grants a second, weaker map grammar.
    StructuredPayload::validate_index(metadata, Some(&map), None)?;
    let mut graph = RdfDatasetBuilder::new();
    let mut originals = HashSet::new();
    for address in objects(metadata, &map, &format!("{G}hasFacet")) {
        require_resource(&address)?;
        let key = one(metadata, &address, &format!("{G}facetLabel"))?
            .ok_or_else(|| err("source graph entry has no key"))?;
        let RdfTerm::Literal(key) = key else {
            return Err(err("source graph envelope key must be a literal"));
        };
        if key.lexical_form != "source-graph" {
            return Err(err("unknown source graph envelope key"));
        }
        let name = one(metadata, &address, &format!("{G}facetStructuredValue"))?
            .ok_or_else(|| err("source graph entry has no identity occurrence"))?;
        let scalar = one(metadata, &name, &format!("{G}structuredTerm"))?;
        let null =
            objects(metadata, &name, TYPE).contains(&RdfTerm::iri(format!("{G}StructuredNull")));
        if scalar.is_some() == null {
            return Err(err(
                "source graph identity must be one native graph name or explicit default-graph null",
            ));
        }
        if let Some(original) = &scalar {
            require_resource(original)?;
            let id = graph.intern_owned_term(original)?;
            graph.declare_named_graph(id)?;
        }
        if !originals.insert(scalar.clone()) {
            return Err(err(
                "source graph envelope repeats an original graph identity",
            ));
        }
        for row in graphs.get(&address).into_iter().flatten() {
            let mut restored = (**row).clone();
            restored.graph_name = scalar.clone();
            graph.push_owned_quad(&restored)?;
        }
    }
    Ok(Some(StructuredPayload::new(
        graph.freeze()?,
        arguments,
        result,
    )?))
}

/// Encode metadata and both role roots together. Only transport-owned links remap.
pub(super) fn call_graph(
    call: &ToolCallRecord,
    subject: &str,
    generated: &[String],
) -> Result<Arc<RdfDataset>> {
    require_iri(&call.tool, "gmeow:usedTool", subject)?;
    if let Some(invocation) = &call.invocation {
        require_iri(invocation, "gmeow:calledByInvocation", subject)?;
    }
    for entity in generated {
        require_iri(entity, "generated entity", subject)?;
    }
    let mut builder = RdfDatasetBuilder::new();
    let node = RdfTerm::iri(subject);
    add(
        &mut builder,
        &node,
        TYPE,
        RdfTerm::iri(format!("{G}ToolCall")),
    )?;
    add(
        &mut builder,
        &node,
        &format!("{G}usedTool"),
        RdfTerm::iri(&call.tool),
    )?;
    add(
        &mut builder,
        &RdfTerm::iri(&call.tool),
        TYPE,
        RdfTerm::iri(format!("{G}SoftwareAgent")),
    )?;
    for (field, value) in [
        ("toolArguments", &call.arguments),
        ("toolResult", &call.result),
    ] {
        if let Some(value) = value {
            add(
                &mut builder,
                &node,
                &format!("{G}{field}"),
                RdfTerm::Literal(value.clone()),
            )?;
        }
    }
    if let Some(ordinal) = call.ordinal {
        add(
            &mut builder,
            &node,
            &format!("{G}entryOrdinal"),
            RdfTerm::Literal(RdfLiteral::typed(
                ordinal.to_string(),
                format!("{XSD}nonNegativeInteger"),
            )),
        )?;
    }
    if let Some(profile) = call.opaque_profile {
        add(&mut builder, &node, CONFORMS, RdfTerm::iri(profile.iri()))?;
    }
    if let Some(invocation) = &call.invocation {
        add(
            &mut builder,
            &node,
            &format!("{G}calledByInvocation"),
            RdfTerm::iri(invocation),
        )?;
    }
    if let Some(created) = &call.created {
        add(
            &mut builder,
            &node,
            CREATED,
            RdfTerm::Literal(RdfLiteral::typed(created, format!("{XSD}dateTime"))),
        )?;
    }
    for entity in generated {
        add(
            &mut builder,
            &RdfTerm::iri(entity),
            &format!("{G}wasGeneratedBy"),
            node.clone(),
        )?;
    }
    if let Some(payload) = &call.payload {
        write_payload(&mut builder, subject, payload)?;
        for (field, root) in [
            ("toolArgumentsStructured", &payload.arguments),
            ("toolResultStructured", &payload.result),
        ] {
            if let Some(root) = root {
                add(&mut builder, &node, &format!("{G}{field}"), root.clone())?;
            }
        }
    }
    Ok(builder.freeze()?)
}

/// Read historical opaque and current graph calls without JSON or media inference.
pub(super) fn read_calls(dataset: &RdfDataset) -> Result<Vec<ToolCallRecord>> {
    let rows = purrdf::flat_rdf_quads_from_dataset(dataset);
    let metadata_rows: Vec<&RdfQuad> = rows.iter().filter(|row| row.graph_name.is_none()).collect();
    let metadata = RowIndex::new(metadata_rows.iter().copied());
    let mut graphs: HashMap<&RdfTerm, Vec<&RdfQuad>> = HashMap::new();
    let mut backlinks: HashMap<&RdfTerm, Vec<&RdfQuad>> = HashMap::new();
    for row in &rows {
        if let Some(name) = &row.graph_name {
            graphs.entry(name).or_default().push(row);
        } else if row.predicate == format!("{G}wasGeneratedBy")
            || row.predicate == "https://example.org/memory/wasGeneratedBy"
        {
            backlinks.entry(&row.object).or_default().push(row);
        }
    }
    let mut subjects = Vec::new();
    let mut seen_subjects = HashSet::new();
    for row in &metadata_rows {
        if row.predicate == TYPE
            && [
                RdfTerm::iri(format!("{G}ToolCall")),
                RdfTerm::iri("https://example.org/memory/ToolCall"),
            ]
            .contains(&row.object)
            && seen_subjects.insert(&row.subject)
        {
            subjects.push(row.subject.clone());
        }
    }
    let mut calls = Vec::new();
    for subject in subjects {
        let RdfTerm::Iri(id) = &subject else {
            return Err(err("recorded call requires an IRI identity"));
        };
        let namespace =
            if objects(&metadata, &subject, TYPE).contains(&RdfTerm::iri(format!("{G}ToolCall"))) {
                G
            } else {
                "https://example.org/memory/"
            };
        let arguments_root = one(&metadata, &subject, &format!("{G}toolArgumentsStructured"))?;
        let result_root = one(&metadata, &subject, &format!("{G}toolResultStructured"))?;
        let payload = read_payload(&graphs, &metadata, id, arguments_root, result_root)?;
        let profile = iri(one(&metadata, &subject, CONFORMS)?, CONFORMS)?
            .map(|profile| match profile.as_str() {
                "urn:gmeow:mcp:opaque-payload:verbatim-v1" => Ok(OpaquePayloadProfile::Verbatim),
                "urn:gmeow:mcp:opaque-payload:inline-or-blake3-v1" => {
                    Ok(OpaquePayloadProfile::InlineOrBlake3)
                }
                _ => Err(err(format!("unknown tool-call opaque profile {profile}"))),
            })
            .transpose()?;
        let generated = backlinks
            .get(&subject)
            .into_iter()
            .flatten()
            .filter(|row| row.predicate == format!("{namespace}wasGeneratedBy"))
            .map(|row| match &row.subject {
                RdfTerm::Iri(iri) => Ok(iri.clone()),
                _ => Err(err("generated entity requires an IRI")),
            })
            .collect::<Result<Vec<_>>>()?;
        calls.push(ToolCallRecord {
            id: id.clone(),
            tool: iri(
                one(&metadata, &subject, &format!("{namespace}usedTool"))?,
                "usedTool",
            )?
            .ok_or_else(|| err("call missing usedTool"))?,
            arguments: literal(
                one(&metadata, &subject, &format!("{namespace}toolArguments"))?,
                "toolArguments",
            )?,
            result: literal(
                one(&metadata, &subject, &format!("{namespace}toolResult"))?,
                "toolResult",
            )?,
            invocation: iri(
                one(
                    &metadata,
                    &subject,
                    &format!("{namespace}calledByInvocation"),
                )?,
                "calledByInvocation",
            )?,
            created: literal_text(one(&metadata, &subject, CREATED)?, CREATED)?,
            generated,
            payload,
            opaque_profile: profile,
            ingestion: None,
            ordinal: literal_text(
                one(&metadata, &subject, &format!("{G}entryOrdinal"))?,
                "call entryOrdinal",
            )?
            .map(|value| {
                value
                    .parse::<u64>()
                    .map_err(|error| err(format!("call entryOrdinal: {error}")))
            })
            .transpose()?,
        });
    }
    calls.sort_by_key(|call| call.ordinal);
    Ok(calls)
}
