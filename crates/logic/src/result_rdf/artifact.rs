// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete artifact admission through native bounded discovery and streaming.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::sync::Arc;

use purrdf::gts::reader::{BlobPayload, BlobRefusal, FrameContext, StreamingSink};
use purrdf::{GraphMatch, RdfDataset, TermId, TermRef};
use purrdf_lex::cbor::Value;

use super::closure::{
    self, CLOSURE_MEDIA_TYPE, CLOSURE_REPRESENTATION, MAX_CLOSURE_BYTES, MAX_CLOSURE_ENCODED_BYTES,
};
use super::{
    NATIVE_RECEIPT_MEDIA_TYPE, NATIVE_RECEIPT_REPRESENTATION, NativeReceipt, NativeReceiptAddress,
    NativeReceiptRef, NativeReceiptResolver, NoNativeReceipts, RDF_TYPE, ReasoningResult,
    ResultGraph, RootSelection, XSD_STRING, logic, read_reasoning_result_expecting, result_err,
};
use crate::execution_cbor::MAX_RECEIPT_BYTES;
use crate::physical::LogicalGraph;

const MAX_ENCODED_BYTES: usize = gmeow_gts_profile::transport::MAX_NATIVE_RECEIPT_ENCODED_BYTES;
const MAX_METADATA_BYTES: usize = gmeow_gts_profile::transport::MAX_METADATA_BYTES;
const MAX_FRAME_BYTES: usize = gmeow_gts_profile::transport::MAX_OPAQUE_BYTES;

/// Render already admitted artifact bytes in the shared native/WASM JSON envelope.
/// The publisher/reader retains responsibility for complete artifact admission.
#[must_use]
pub fn artifact_envelope(bytes: &[u8]) -> serde_json::Value {
    serde_json::json!({
        "format": "gts",
        "media_type": gmeow_gts_profile::GMEOW_GTS_MEDIA_TYPE,
        "encoding": "base64",
        "bytes": bytes.len(),
        "output": purrdf::xsd::canonical_base64(bytes),
    })
}

/// Common native/WASM proof boundary. Facets become observable only after the
/// complete selected artifact and each of its required attachments are admitted.
///
/// # Errors
/// Refuses ambiguous results, invalid attachments, or unrepresentable RDF facets.
pub fn admit_reasoning_response(bytes: &[u8]) -> gmeow_errors::Result<serde_json::Value> {
    let dataset = discover(bytes)?;
    let candidates = roots(&dataset, GraphMatch::Default)?
        .into_iter()
        .collect::<Vec<_>>();
    let [owner] = candidates.as_slice() else {
        return Err(result_err(format!(
            "reasoning response requires one aggregate, found {}",
            candidates.len()
        )));
    };
    let admission = admit(
        bytes,
        &dataset,
        GraphMatch::Default,
        Some(Selection::Verdict(owner.clone())),
        None,
    )?;
    let result = admission
        .result
        .ok_or_else(|| result_err("selected response result was not authenticated".into()))?;
    let serialize = |dataset: &RdfDataset| -> gmeow_errors::Result<String> {
        let bytes = purrdf::serialize_dataset(
            dataset,
            "application/n-quads",
            purrdf::SerializeGraph::Dataset,
        )?;
        String::from_utf8(bytes).map_err(|cause| {
            gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(
                "serialize admitted reasoning facets".into(),
                cause,
            ))
        })
    };
    let mut response = serde_json::json!({
        "ok": true,
        "artifact": artifact_envelope(bytes),
        "judgment_nquads": serialize(&dataset)?,
        "evaluation": result.evaluation.wire(),
        "completeness": result.completeness.wire(),
    });
    if let Some(closure) = admission.closure {
        response["entailed_count"] = serde_json::json!(closure.quad_count());
        response["closure_nquads"] = serde_json::json!(serialize(&closure)?);
    }
    Ok(response)
}

#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Owner {
    graph: Option<TermId>,
    subject: String,
}

impl Owner {
    fn graph_match(&self) -> GraphMatch {
        self.graph.map_or(GraphMatch::Default, GraphMatch::Named)
    }
}

struct ArtifactGraphs<'a> {
    dataset: &'a RdfDataset,
    readers: BTreeMap<Option<TermId>, ResultGraph<'a>>,
}

impl ArtifactGraphs<'_> {
    fn admit(
        &mut self,
        owner: &Owner,
        resolver: &dyn NativeReceiptResolver,
    ) -> gmeow_errors::Result<ReasoningResult> {
        use std::collections::btree_map::Entry;

        let reader = match self.readers.entry(owner.graph) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                entry.insert(ResultGraph::new(self.dataset, owner.graph_match())?)
            }
        };
        read_reasoning_result_expecting(
            reader,
            resolver,
            RootSelection::Aggregate(Some(&owner.subject)),
            None,
        )
    }
}

enum Selection {
    Verdict(Owner),
    WithReceipt(Owner),
}

impl Selection {
    fn owner(&self) -> &Owner {
        match self {
            Self::Verdict(owner) | Self::WithReceipt(owner) => owner,
        }
    }
}

struct Admission {
    result: Option<ReasoningResult>,
    probe_matches: bool,
    closure: Option<Arc<RdfDataset>>,
    retained_receipt: Option<NativeReceipt<'static>>,
}

#[derive(Default)]
struct Required {
    owners: Vec<Owner>,
    declared: bool,
    observed: bool,
    completed: bool,
    worlds: Vec<Vec<LogicalGraph>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    Native,
    Closure,
}

impl Role {
    fn representation(self) -> &'static str {
        match self {
            Self::Native => NATIVE_RECEIPT_REPRESENTATION,
            Self::Closure => CLOSURE_REPRESENTATION,
        }
    }
    fn media_type(self) -> &'static str {
        match self {
            Self::Native => NATIVE_RECEIPT_MEDIA_TYPE,
            Self::Closure => CLOSURE_MEDIA_TYPE,
        }
    }
    fn decoded_limit(self) -> usize {
        match self {
            Self::Native => MAX_RECEIPT_BYTES,
            Self::Closure => MAX_CLOSURE_BYTES,
        }
    }
    fn encoded_limit(self) -> usize {
        match self {
            Self::Native => MAX_ENCODED_BYTES,
            Self::Closure => MAX_CLOSURE_ENCODED_BYTES,
        }
    }
}

// Only this private admission lifecycle may defer an attachment. Its result
// never escapes until the subsequent closure pass and final diagnostics pass.
struct PendingClosure<'a> {
    receipt: NativeReceiptRef<'a>,
    expected: Option<&'a NativeReceiptAddress>,
    worlds: RefCell<Option<Vec<LogicalGraph>>>,
}

impl NativeReceiptResolver for PendingClosure<'_> {
    fn resolve(&self, address: &NativeReceiptAddress) -> gmeow_errors::Result<&[u8]> {
        self.receipt.resolve(address)
    }
    fn admit_closure(
        &self,
        address: &NativeReceiptAddress,
        worlds: &[LogicalGraph],
    ) -> gmeow_errors::Result<()> {
        if self.expected != Some(address) {
            return Err(result_err(
                "closure binding differs from the exact aggregate attachment".into(),
            ));
        }
        *self.worlds.borrow_mut() = Some(worlds.to_vec());
        Ok(())
    }
}

fn roots(dataset: &RdfDataset, graph: GraphMatch) -> gmeow_errors::Result<BTreeSet<Owner>> {
    let mut roots = BTreeSet::new();
    let mut children = BTreeSet::new();
    for row in dataset.quads() {
        if !graph.matches(row.g) {
            continue;
        }
        if matches!(dataset.resolve(row.p), TermRef::Iri(iri) if iri == logic("contextualResult"))
            && let TermRef::Iri(subject) = dataset.resolve(row.o)
        {
            children.insert(Owner {
                graph: row.g,
                subject: subject.to_owned(),
            });
        }
        if matches!(dataset.resolve(row.p), TermRef::Iri(RDF_TYPE))
            && matches!(dataset.resolve(row.o), TermRef::Iri(iri) if iri == logic("ReasoningResult"))
        {
            let TermRef::Iri(subject) = dataset.resolve(row.s) else {
                return Err(result_err(
                    "complete artifact aggregate result requires an IRI identity".into(),
                ));
            };
            roots.insert(Owner {
                graph: row.g,
                subject: subject.to_owned(),
            });
        }
    }
    Ok(roots.difference(&children).cloned().collect())
}

fn bindings(
    dataset: &RdfDataset,
    graph: GraphMatch,
    roots: &BTreeSet<Owner>,
    relation: &str,
) -> gmeow_errors::Result<BTreeMap<NativeReceiptAddress, Required>> {
    let mut required = BTreeMap::<NativeReceiptAddress, Required>::new();
    let mut bound = BTreeSet::new();
    for row in dataset.quads() {
        if !graph.matches(row.g) {
            continue;
        }
        if !matches!(dataset.resolve(row.p), TermRef::Iri(iri) if iri == logic(relation)) {
            continue;
        }
        let TermRef::Iri(subject) = dataset.resolve(row.s) else {
            return Err(result_err(
                "native receipt binding requires an aggregate IRI subject".into(),
            ));
        };
        let owner = Owner {
            graph: row.g,
            subject: subject.to_owned(),
        };
        if !roots.contains(&owner) {
            return Err(result_err(
                "native receipt is attached outside its aggregate result, or to a contextual child"
                    .into(),
            ));
        }
        if !bound.insert(owner.clone()) {
            return Err(result_err(format!(
                "duplicate native receipt binding for {}",
                owner.subject
            )));
        }
        let TermRef::Literal {
            lexical,
            datatype,
            language: None,
            direction: None,
        } = dataset.resolve(row.o)
        else {
            return Err(result_err(
                "native receipt binding requires a canonical xsd:string address".into(),
            ));
        };
        if !matches!(dataset.resolve(datatype), TermRef::Iri(XSD_STRING)) {
            return Err(result_err(
                "native receipt binding requires xsd:string".into(),
            ));
        }
        required
            .entry(NativeReceiptAddress::parse(lexical)?)
            .or_default()
            .owners
            .push(owner);
    }
    Ok(required)
}

fn metadata(
    address: &NativeReceiptAddress,
    value: Option<&Value>,
    role: Role,
) -> gmeow_errors::Result<()> {
    let Some(Value::Map(entries)) = value else {
        return Err(result_err(format!(
            "native receipt {} requires public digest/rep/mt metadata",
            address.as_str()
        )));
    };
    if purrdf_lex::cbor::encoded_len(value.expect("map matched")) > MAX_METADATA_BYTES {
        return Err(result_err(format!(
            "native receipt {} metadata exceeds {MAX_METADATA_BYTES} bytes",
            address.as_str()
        )));
    }
    let mut selected = BTreeMap::new();
    for (key, value) in entries {
        if let Value::Text(key) = key
            && matches!(key.as_str(), "digest" | "rep" | "mt")
            && selected.insert(key.as_str(), value).is_some()
        {
            return Err(result_err(format!(
                "native receipt {} duplicates metadata key {key}",
                address.as_str()
            )));
        }
    }
    for (key, expected) in [
        ("digest", address.as_str()),
        ("rep", role.representation()),
        ("mt", role.media_type()),
    ] {
        if !matches!(selected.get(key), Some(Value::Text(actual)) if actual == expected) {
            return Err(result_err(format!(
                "native receipt {} requires exact public {key}={expected}",
                address.as_str()
            )));
        }
    }
    Ok(())
}

struct Audit<'a> {
    graphs: ArtifactGraphs<'a>,
    required: BTreeMap<NativeReceiptAddress, Required>,
    selected: Option<Selection>,
    restored: Option<ReasoningResult>,
    retained_receipt: Option<NativeReceipt<'static>>,
    probe: Option<NativeReceiptRef<'a>>,
    dedicated_blob: bool,
    first_refusal: gmeow_errors::FirstDiagnostic,
    role: Role,
    closures: BTreeMap<NativeReceiptAddress, Required>,
    closure_by_owner: BTreeMap<Owner, (NativeReceiptAddress, usize)>,
    restored_closure: Option<Arc<RdfDataset>>,
}

impl Audit<'_> {
    fn payload(&mut self, mut payload: BlobPayload<'_>) -> gmeow_errors::Result<()> {
        let Some((address, required)) = self.required.get_key_value(payload.digest) else {
            return Ok(());
        };
        if !payload.payload_present {
            return Ok(());
        }
        if !self.dedicated_blob || !payload.metadata_declared {
            return Err(result_err(format!(
                "native receipt {} requires a dedicated payload frame with its own complete public metadata",
                address.as_str()
            )));
        }
        metadata(address, payload.metadata, self.role)?;
        if payload.encoded_len > self.role.encoded_limit() {
            return Err(result_err(format!(
                "required {} {} has {} encoded bytes, exceeding {}",
                self.role.representation(),
                address.as_str(),
                payload.encoded_len,
                self.role.encoded_limit(),
            )));
        }
        let source = payload
            .bytes
            .reader()
            .ok_or_else(|| result_err("required native receipt has no readable payload".into()))?;
        let mut decoded = purrdf::gts::codec::decode_chain_reader(
            payload.codecs,
            source,
            self.role.decoded_limit(),
        )
        .map_err(|cause| {
            gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(
                "decode required native receipt".into(),
                cause,
            ))
        })?;
        let mut hash = purrdf_hash::blake3::Hasher::new();
        let mut buffer = [0_u8; 65_536];
        let mut bytes = Vec::new();
        let mut total = 0_usize;
        let probe = self.probe.filter(|probe| probe.address() == address);
        loop {
            let read = decoded.read(&mut buffer).map_err(|cause| {
                gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(
                    "read complete required native receipt".into(),
                    cause,
                ))
            })?;
            if read == 0 {
                break;
            }
            let end = total
                .checked_add(read)
                .ok_or_else(|| result_err("native receipt length overflow".into()))?;
            if end > self.role.decoded_limit() {
                return Err(result_err(format!(
                    "required {} {} exceeds its decoded limit {}",
                    self.role.representation(),
                    address.as_str(),
                    self.role.decoded_limit(),
                )));
            }
            if let Some(probe) = probe
                && probe.bytes().get(total..end) != Some(&buffer[..read])
            {
                return Err(result_err(
                    "existing native receipt differs from incoming publication bytes".into(),
                ));
            }
            hash.update(&buffer[..read]);
            if (!required.completed || self.role == Role::Closure) && !required.owners.is_empty() {
                bytes.extend_from_slice(&buffer[..read]);
            }
            total = end;
        }
        if probe.is_some_and(|probe| probe.bytes().len() != total) {
            return Err(result_err(
                "existing native receipt differs from incoming publication length".into(),
            ));
        }
        if purrdf::gts::wire::digest_label(hash.finalize().as_bytes()) != address.as_str() {
            return Err(result_err(format!(
                "native receipt payload does not match {}",
                address.as_str()
            )));
        }
        if self.role == Role::Closure {
            for (owner, worlds) in required.owners.iter().zip(&required.worlds) {
                let dataset = closure::admit_leaf(address, &bytes, worlds)?;
                if self.selected.as_ref().map(Selection::owner) == Some(owner) {
                    self.restored_closure = Some(dataset);
                }
            }
        } else if !required.completed && !required.owners.is_empty() {
            let receipt = NativeReceiptRef::admit(address, &bytes)?;
            for owner in &required.owners {
                let attachment = self.closure_by_owner.get(owner);
                let expected = attachment.map(|(address, _)| address);
                let resolver = PendingClosure {
                    receipt,
                    expected,
                    worlds: RefCell::new(None),
                };
                let result = self.graphs.admit(owner, &resolver)?;
                let worlds = resolver.worlds.into_inner();
                if let Some((address, slot)) = attachment {
                    let worlds = worlds.ok_or_else(|| {
                        result_err(
                            "required closure binding was not admitted by its owning aggregate"
                                .into(),
                        )
                    })?;
                    let closure = self
                        .closures
                        .get_mut(address)
                        .expect("discovered closure retained");
                    // Multiple owners may share a digest. Preserve exact owner
                    // order so each authenticated world catalog is checked.
                    if closure.worlds.len() < closure.owners.len() {
                        closure.worlds.resize_with(closure.owners.len(), Vec::new);
                    }
                    closure.worlds[*slot] = worlds;
                }
                if self.selected.as_ref().map(Selection::owner) == Some(owner) {
                    self.restored = Some(result);
                }
            }
            if let Some(Selection::WithReceipt(owner)) = &self.selected
                && required.owners.contains(owner)
            {
                let closure = self
                    .closure_by_owner
                    .get(owner)
                    .map(|(address, _)| address.clone());
                self.retained_receipt = Some(NativeReceipt::from_admitted_payload(
                    address.clone(),
                    bytes,
                    closure,
                ));
            }
        }
        self.required
            .get_mut(payload.digest)
            .expect("required digest retained")
            .completed = true;
        Ok(())
    }
}

impl StreamingSink for Audit<'_> {
    fn frame(&mut self, context: FrameContext<'_>) {
        self.dedicated_blob = context.frame_type == "blob";
    }
    fn blob(&mut self, _segment: u64, digest: &str, value: Option<&Value>) {
        if let Some((address, required)) = self.required.get_key_value(digest) {
            let result = if value.is_some() {
                metadata(address, value, self.role)
            } else if required.declared {
                Ok(())
            } else {
                Err(result_err(format!(
                    "native receipt {digest} has no admitted metadata declaration"
                )))
            };
            let valid = result.is_ok();
            let required = self
                .required
                .get_mut(digest)
                .expect("required digest retained");
            required.observed = true;
            if valid && value.is_some() {
                required.declared = true;
            }
            self.first_refusal.record(result);
        }
    }
    fn blob_payload(&mut self, payload: BlobPayload<'_>) {
        let result = self.payload(payload);
        self.first_refusal.record(result);
    }
    fn blob_refused(&mut self, refusal: BlobRefusal<'_>) {
        if refusal
            .digest
            .is_some_and(|digest| self.required.contains_key(digest))
        {
            self.first_refusal.record(Err(result_err(format!(
                "required native receipt payload refused: {}",
                refusal.detail
            ))));
        }
    }
    fn blob_encoded_limit(&self) -> Option<usize> {
        Some(MAX_CLOSURE_ENCODED_BYTES)
    }
    fn blob_decode_limit(&self) -> Option<usize> {
        Some(MAX_CLOSURE_BYTES)
    }
    fn frame_decode_limit(&self) -> Option<usize> {
        Some(MAX_FRAME_BYTES)
    }
}

fn discover(bytes: &[u8]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let mut limits = purrdf::GtsBlobLimits::new(MAX_CLOSURE_BYTES, 0);
    limits.max_encoded_bytes = MAX_CLOSURE_ENCODED_BYTES;
    limits.max_frame_decoded_bytes = MAX_FRAME_BYTES;
    let imported = purrdf::import_gts_events_with_blobs(bytes, &[], limits).map_err(|cause| {
        gmeow_errors::Diag::of_kind(crate::error::Result::caused_by(
            "discover complete reasoning artifact".into(),
            cause,
        ))
    })?;
    Ok(imported.bundle.dataset)
}

fn admit(
    bytes: &[u8],
    dataset: &RdfDataset,
    graph: GraphMatch,
    selected: Option<Selection>,
    probe: Option<NativeReceiptRef<'_>>,
) -> gmeow_errors::Result<Admission> {
    gmeow_gts_profile::validate_mandated_frames(bytes)?;
    let roots = roots(dataset, graph)?;
    let mut required = bindings(dataset, graph, &roots, "resultNativeExecution")?;
    let closures = bindings(dataset, graph, &roots, "resultClosureBlob")?;
    let closure_by_owner = closures
        .iter()
        .flat_map(|(address, required)| {
            required
                .owners
                .iter()
                .enumerate()
                .map(|(slot, owner)| (owner.clone(), (address.clone(), slot)))
        })
        .collect();
    if closures
        .keys()
        .any(|address| required.contains_key(address))
    {
        return Err(result_err(
            "one blob address cannot have conflicting native-receipt and RDF-closure roles".into(),
        ));
    }
    let mut graphs = ArtifactGraphs {
        dataset,
        readers: BTreeMap::new(),
    };
    let mut restored = None;
    for owner in roots.iter().filter(|owner| {
        !required
            .values()
            .any(|required| required.owners.contains(owner))
    }) {
        let result = graphs.admit(owner, &NoNativeReceipts)?;
        if selected.as_ref().map(Selection::owner) == Some(owner) {
            restored = Some(result);
        }
    }
    if let Some(probe) = probe {
        required.entry(probe.address().clone()).or_default();
    }
    let mut audit = Audit {
        graphs,
        required,
        selected,
        restored,
        retained_receipt: None,
        probe,
        dedicated_blob: false,
        first_refusal: gmeow_errors::FirstDiagnostic::default(),
        role: Role::Native,
        closures,
        closure_by_owner,
        restored_closure: None,
    };
    audit_stream(bytes, &mut audit)?;
    let reused = probe.is_some_and(|probe| {
        audit
            .required
            .get(probe.address())
            .is_some_and(|required| required.completed)
    });
    if !audit.closures.is_empty() {
        for required in audit.closures.values() {
            if required.owners.len() != required.worlds.len() {
                return Err(result_err(
                    "closure attachment has no authenticated execution-world catalog".into(),
                ));
            }
        }
        audit.required = std::mem::take(&mut audit.closures);
        audit.role = Role::Closure;
        audit.probe = None;
        audit_stream(bytes, &mut audit)?;
    }
    Ok(Admission {
        result: audit.restored,
        probe_matches: reused,
        closure: audit.restored_closure,
        retained_receipt: audit.retained_receipt,
    })
}

fn audit_stream(bytes: &[u8], audit: &mut Audit<'_>) -> gmeow_errors::Result<()> {
    let result = purrdf::gts::reader::read_to_sink(bytes, true, None, audit);
    if let Some(diagnostic) = result.diagnostics.into_iter().next() {
        return Err(gmeow_errors::Diag::of_kind(
            gmeow_gts_profile::error::TransportDiagnostic {
                detail: format!("{}: {}", diagnostic.code, diagnostic.detail),
                diagnostic: Arc::new(diagnostic),
            },
        ));
    }
    if let Some(refusal) = audit.first_refusal.take() {
        return Err(refusal);
    }
    if result.torn.is_some()
        || result.segment_heads.is_empty()
        || result.segment_heads.iter().any(Vec::is_empty)
        || result.segment_heads.len() != result.segment_profiles.len()
    {
        return Err(result_err(
            "reasoning artifact has torn or incomplete native segment evidence".into(),
        ));
    }
    for (address, required) in &audit.required {
        if (!required.owners.is_empty() || required.observed) && !required.completed {
            return Err(result_err(format!(
                "required native receipt {} has no admitted inline payload",
                address.as_str()
            )));
        }
    }
    Ok(())
}

/// Authenticate the library's aggregate results in its exact default graph,
/// retaining no decoded receipt history. Other graphs remain RDF data, outside
/// this operation's proof-admission claim.
///
/// # Errors
/// Refuses incomplete bindings, every invalid physical required payload, conflicting
/// metadata, contextual native receipts and all native transport diagnostics.
pub fn admit_reasoning_artifact(bytes: &[u8]) -> gmeow_errors::Result<Arc<RdfDataset>> {
    let dataset = discover(bytes)?;
    admit(bytes, &dataset, GraphMatch::Default, None, None)?;
    Ok(dataset)
}

/// Admit the complete contextual artifact for one explicitly selected request.
///
/// # Errors
/// Refuses invalid transport, a missing or ambiguous request/result link, native
/// contextual attachments and every malformed selected result field.
pub fn admit_contextual_artifact(
    bytes: &[u8],
    request: &str,
) -> gmeow_errors::Result<Arc<RdfDataset>> {
    Ok(admit_contextual(bytes, request)?.0)
}

pub(super) fn admit_contextual(
    bytes: &[u8],
    request: &str,
) -> gmeow_errors::Result<(Arc<RdfDataset>, ReasoningResult)> {
    let dataset = discover(bytes)?;
    admit(bytes, &dataset, GraphMatch::Any, None, None)?;
    let result = super::read_contextual_result(&dataset, request)?;
    Ok((dataset, result))
}

/// Authenticate an existing artifact and compare every occurrence of an incoming
/// receipt with that exact publication. Absence permits its first append.
///
/// # Errors
/// Refuses a corrupt existing artifact or any conflicting receipt occurrence.
pub fn contains_admitted_native_receipt(
    bytes: &[u8],
    receipt: NativeReceiptRef<'_>,
) -> gmeow_errors::Result<bool> {
    let dataset = discover(bytes)?;
    Ok(admit(bytes, &dataset, GraphMatch::Default, None, Some(receipt))?.probe_matches)
}

/// Recover the sole aggregate in the default graph of a complete artifact.
///
/// # Errors
/// Refuses ambiguous selection and every required binary receipt/transport failure.
pub fn parse_reasoning_artifact(bytes: &[u8]) -> gmeow_errors::Result<ReasoningResult> {
    parse_reasoning_artifact_graph(bytes, None)
}

/// Recover a full-closure operation only after its exact aggregate, native
/// evidence and isolated closure have all passed complete transport admission.
/// The dataset is RDF closure, never fabricated rich inferred-axiom evidence.
///
/// # Errors
/// Refuses a missing closure, ambiguous aggregate or any required admission failure.
pub fn parse_reasoning_artifact_with_closure(
    bytes: &[u8],
) -> gmeow_errors::Result<(ReasoningResult, Arc<RdfDataset>)> {
    let dataset = discover(bytes)?;
    let candidates = roots(&dataset, GraphMatch::Default)?
        .into_iter()
        .collect::<Vec<_>>();
    let [owner] = candidates.as_slice() else {
        return Err(result_err(format!(
            "full-closure artifact requires one exact aggregate, found {}",
            candidates.len()
        )));
    };
    let admission = admit(
        bytes,
        &dataset,
        GraphMatch::Default,
        Some(Selection::Verdict(owner.clone())),
        None,
    )?;
    Ok((
        admission
            .result
            .ok_or_else(|| result_err("selected full-closure result is absent".into()))?,
        admission.closure.ok_or_else(|| {
            result_err("selected operation requires a complete RDF closure attachment".into())
        })?,
    ))
}

/// Recover the sole aggregate in exactly the selected named or default graph.
///
/// # Errors
/// Refuses missing graphs, ambiguous aggregates and invalid complete artifacts.
pub fn parse_reasoning_artifact_graph(
    bytes: &[u8],
    graph: Option<&str>,
) -> gmeow_errors::Result<ReasoningResult> {
    Ok(admit_graph(bytes, graph, Selection::Verdict)?.0)
}

/// Admit a selected aggregate and keep its exact native receipt for reuse.
/// The selected payload comes from the same admission as the result.
/// # Errors
/// Refuses missing graphs, ambiguous aggregates and invalid complete artifacts.
pub fn parse_reasoning_artifact_graph_with_receipt(
    bytes: &[u8],
    graph: Option<&str>,
) -> gmeow_errors::Result<(ReasoningResult, Option<NativeReceipt<'static>>)> {
    admit_graph(bytes, graph, Selection::WithReceipt)
}

fn admit_graph(
    bytes: &[u8],
    graph: Option<&str>,
    select: fn(Owner) -> Selection,
) -> gmeow_errors::Result<(ReasoningResult, Option<NativeReceipt<'static>>)> {
    let dataset = discover(bytes)?;
    let selected_graph = graph
        .map(|graph| {
            dataset.term_id_by_iri(graph).ok_or_else(|| {
                result_err(format!("reasoning artifact has no selected graph {graph}"))
            })
        })
        .transpose()?;
    let graph = selected_graph.map_or(GraphMatch::Default, GraphMatch::Named);
    let candidates = roots(&dataset, graph)?.into_iter().collect::<Vec<_>>();
    let [owner] = candidates.as_slice() else {
        return Err(result_err(format!(
            "complete artifact requires one aggregate in its selected graph, found {}",
            candidates.len()
        )));
    };
    let admission = admit(bytes, &dataset, graph, Some(select(owner.clone())), None)?;
    let result = admission
        .result
        .ok_or_else(|| result_err("selected aggregate was not authenticated".into()))?;
    Ok((result, admission.retained_receipt))
}
