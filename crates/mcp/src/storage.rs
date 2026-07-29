// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The storage seam — the one place `gmeow-mcp` touches persistence, and the reason
//! the engine is browser-runnable.
//!
//! Every write the MCP surface performs lands in exactly three places: the
//! grounded-memory claim package, the append-only conjecture library, and the
//! append-only candidate library. Every configuration read is an environment
//! variable. Before this module those were `std::fs` and `std::env` calls scattered
//! through `lib.rs`, which pinned the whole engine to a host with a filesystem — a
//! hard blocker for the wasm32 target the crate's whole boundary discipline exists to
//! reach.
//!
//! # The shape of the seam
//!
//! Three traits, one bundle:
//!
//! * [`ClaimStore`] — the grounded-memory triad (`store_claim` / `recall` /
//!   `revise_belief`) plus the tool-call trajectory and its audit segments.
//! * [`SegmentLibrary`] — an append-only GTS segment collection with an exclusive
//!   lock and an all-or-nothing replace. Both the conjecture library and the candidate
//!   library ARE one of these; the lock + whole-file replace is what makes a
//!   read → decide → append sequence atomic.
//! * [`Storage`] — the backend itself: environment lookup, the wall/logical clock, and
//!   the three stores. [`storage`] returns the process's backend.
//!
//! # The two backends, and why neither is a stub
//!
//! * **Native** ([`FsStorage`], `cfg(not(target_arch = "wasm32"))`) — real files at
//!   real paths, with the SAME `flock` sidecar, `tempfile` write-then-rename, and
//!   `GMEOW_*_PATH` / `HOME` / `USERPROFILE` resolution the engine has always used.
//!   The claim store delegates verbatim to `purrdf`'s `agent_memory::Memory`, so the
//!   on-disk `memory.gts` and every byte of native behaviour is unchanged.
//! * **Browser** ([`InMemoryStorage`]) — a REAL working store held in process memory.
//!   It is not a refusal and it is not a `Result::Err` factory: claims are stored,
//!   recalled with the same relevance ordering, revised, and superseded; tool calls
//!   are recorded; audit segment bytes are appended; libraries lock, read back, and
//!   replace. A browser session that stores a claim can recall it, and one that
//!   submits a candidate can list it.
//!
//! [`InMemoryStorage`] is compiled on EVERY target, not just wasm32. That is
//! deliberate: a backend nobody can test is a backend nobody can trust, and
//! `wasm32-unknown-unknown` has no test harness here. The native test suite exercises
//! the browser backend directly.
//!
//! # The clock
//!
//! `wasm32-unknown-unknown` has no ambient wall clock (`SystemTime::now` is not
//! implemented there), so the backend owns the timestamp: [`Storage::now_rfc3339`].
//! The native backend has none of its own — `purrdf`'s memory package stamps real UTC
//! internally, exactly as before. The browser backend stamps an explicitly LOGICAL
//! clock: a monotone counter rendered as an `xsd:dateTime` anchored at the Unix epoch
//! (`1970-01-01T00:00:00Z`, `…:01Z`, …). That is honest by construction — a reader
//! cannot mistake it for a wall time — and it preserves the only property the memory
//! package actually depends on, which is that later records stamp later instants.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::sync::{Arc, Mutex, OnceLock};

use purrdf::gts::examples::agent_memory::{
    Claim, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions, ToolCallRecord,
};

use gmeow_errors::Result;

use crate::error::Mcp;

/// Raise a storage-layer defect as the crate's typed `mcp` diagnostic.
fn err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(Mcp {
        message: message.into(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// The traits
// ─────────────────────────────────────────────────────────────────────────────

/// The grounded-memory claim package: the append-only store behind `store_claim`,
/// `recall`, `revise_belief`, and the tool-call trajectory the audit segments key on.
///
/// The option/record types are `purrdf`'s, not redeclared here: the native backend
/// IS `purrdf`'s memory package, and a parallel set of structs would be a second
/// vocabulary for the same claims.
pub trait ClaimStore: Send + Sync {
    /// Append one claim, returning the stored record (its minted id and stamp).
    ///
    /// # Errors
    ///
    /// An empty claim, an out-of-range confidence, or a backend write failure.
    fn store_claim(&self, text: &str, options: StoreOptions<'_>) -> Result<Claim>;

    /// Append a value-wise suppression for `claim_id`, optionally naming the successor
    /// that supersedes it.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn revise_claim(&self, claim_id: &str, options: RevisionOptions<'_>) -> Result<()>;

    /// Record one executed tool call on the trajectory, returning the stored record.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn record_tool_call(&self, tool: &str, options: ToolCallOptions<'_>) -> Result<ToolCallRecord>;

    /// The claims matching `options.query`, best match first.
    ///
    /// # Errors
    ///
    /// A backend read failure.
    fn recall(&self, options: RecallOptions<'_>) -> Result<Vec<Claim>>;

    /// Every claim in storage order, suppressed ones included and flagged.
    ///
    /// # Errors
    ///
    /// A backend read failure.
    fn claims(&self) -> Result<Vec<Claim>>;

    /// Every recorded tool call in storage order.
    ///
    /// # Errors
    ///
    /// A backend read failure.
    fn tool_calls(&self) -> Result<Vec<ToolCallRecord>>;

    /// Append one already-serialized trajectory-audit GTS segment to the SAME package
    /// the claims live in, so a cold audit of the package verifies the executed turn.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn append_audit_segment(&self, segment: &[u8]) -> Result<()>;

    /// This store's whole readable contents as the shared session-store transport segment
    /// (N-Quads) — see [`claim_segment`].
    ///
    /// A PROVIDED method with no backend override, deliberately: [`claim_segment`] is the
    /// SINGLE serializer, so the native package and the browser store emit the identical
    /// segment for identical contents. That identity is the whole point — it is what makes
    /// an exported browser session re-seedable into a native package and answerable there,
    /// and a per-backend serializer would be two shapes that could drift.
    ///
    /// # Errors
    ///
    /// A backend read failure, or a record the transport shape cannot carry (see
    /// [`claim_segment`]).
    fn segment_nquads(&self) -> Result<String> {
        claim_segment(&self.claims()?, &self.tool_calls()?)
    }
}

/// An exclusive hold on a [`SegmentLibrary`], released when the value is dropped.
///
/// A marker trait with no methods: the whole contract IS the lifetime. The native
/// implementation holds an `flock`ed sidecar file handle; the browser implementation
/// holds a mutex guard. Callers that must read-then-decide-then-append run the entire
/// sequence while one of these is alive, which is what closes the lost-update window.
pub trait LibraryLock {}

/// An append-only GTS segment collection with an exclusive lock and an
/// all-or-nothing replace — the shape both the conjecture library and the candidate
/// library have.
///
/// There is deliberately no `append` primitive. A commit assembles the WHOLE new
/// contents (current bytes + every new segment, in order) in memory and replaces the
/// library in one step, so a commit that writes more than one segment can never land
/// some of them and lose the rest.
pub trait SegmentLibrary: Send + Sync {
    /// The library's current bytes. A library that has never been written is EMPTY,
    /// not an error — a first-ever read of an untouched library is normal.
    ///
    /// # Errors
    ///
    /// A backend read failure other than "not yet written".
    fn read_bytes(&self) -> Result<Vec<u8>>;

    /// Replace the library's entire contents with `bytes`, all-or-nothing: either the
    /// whole new content lands, or the prior content is left completely untouched.
    ///
    /// # Errors
    ///
    /// A backend write failure.
    fn replace_bytes(&self, bytes: &[u8]) -> Result<()>;

    /// Take the library's exclusive lock, blocking until it is available. The lock is
    /// released when the returned value drops.
    ///
    /// # Errors
    ///
    /// A backend failure acquiring the lock.
    fn lock(&self) -> Result<Box<dyn LibraryLock + '_>>;
}

/// The process's persistence + configuration backend.
///
/// One trait rather than three free functions because the three stores and the
/// environment are ONE choice: a host either has a filesystem and an environment or it
/// does not, and mixing a real claim store with a synthetic environment would be a
/// backend nobody declared.
pub trait Storage: Send + Sync {
    /// The value of environment variable `key`, or `None` when it is unset or empty.
    ///
    /// An empty value reads as unset on purpose: every caller here treats an empty
    /// `GMEOW_*_PATH` as "not configured", and folding that into one place stops the
    /// three path resolvers from each re-deciding it.
    fn env_var(&self, key: &str) -> Option<String>;

    /// The instant to stamp on a record the ENGINE mints (the trajectory-audit
    /// segments). See the module docs for the browser backend's logical clock.
    fn now_rfc3339(&self) -> String;

    /// The grounded-memory claim package.
    ///
    /// # Errors
    ///
    /// A backend that cannot resolve or open its package (natively: neither `HOME` nor
    /// `USERPROFILE` set with `GMEOW_MEMORY_PATH` empty).
    fn claim_store(&self) -> Result<Arc<dyn ClaimStore>>;

    /// The append-only conjecture library.
    ///
    /// # Errors
    ///
    /// As [`claim_store`](Self::claim_store), for `GMEOW_CONJECTURE_PATH`.
    fn conjecture_library(&self) -> Result<Arc<dyn SegmentLibrary>>;

    /// The append-only candidate library.
    ///
    /// # Errors
    ///
    /// As [`claim_store`](Self::claim_store), for `GMEOW_CANDIDATE_PATH`.
    fn candidate_library(&self) -> Result<Arc<dyn SegmentLibrary>>;
}

/// The process's storage backend, selected by target at compile time: real files and
/// a real environment on a host that has them, an in-process store in the browser.
///
/// A `cfg` rather than a runtime switch because the choice is a property of the
/// TARGET, not of a call: a wasm image has no filesystem to fall back to and a native
/// host has no reason to pretend it lacks one.
#[must_use]
pub fn storage() -> &'static dyn Storage {
    #[cfg(not(target_arch = "wasm32"))]
    {
        static NATIVE: FsStorage = FsStorage;
        &NATIVE
    }
    #[cfg(target_arch = "wasm32")]
    {
        browser_storage()
    }
}

/// The browser backend's single process-wide instance.
///
/// One instance, so a claim stored by one tool call is recalled by the next — a fresh
/// store per call would be a store that forgets, which is not a store.
#[must_use]
pub fn browser_storage() -> &'static InMemoryStorage {
    static BROWSER: OnceLock<InMemoryStorage> = OnceLock::new();
    BROWSER.get_or_init(InMemoryStorage::new)
}

// ─────────────────────────────────────────────────────────────────────────────
// The session-store transport segment
// ─────────────────────────────────────────────────────────────────────────────
//
// The ONE serialization of a claim package's readable contents, shared by BOTH backends
// and used in both directions: [`claim_segment`] writes it, [`seed_claim_store`] reads it
// back through a store's PUBLIC write API. Together they are an isomorphism on exactly the
// state the store's API can express, which is what lets an exported browser session be
// re-seeded into a native package and answer identically there.
//
// # Why this is not `purrdf`'s on-disk shape
//
// `purrdf`'s `Memory` — the native backend's delegate — exposes `store` / `recall` /
// `claims` / `tool_calls` and no serializer at all: its on-disk claim encoding (reified
// RDF 1.2 statements in an append-only GTS `ai-package`, addressed under
// `https://example.org/memory/`) is genuinely private, and reproducing it here would be a
// second, silent source of truth for a shape we do not own. So the transport carries the
// GMEOW vocabulary instead — `gmeow:ClaimToken` and `gmeow:ToolCall` with the properties
// the agentic / provenance / epistemics slices already define — and mints no term.
//
// # Why the records are POSITION-addressed
//
// A record's address is `urn:gmeow:session:claim:0007`, not a digest of its content. An
// append-only store's identity for a record IS its position, and a position address is
// what makes the segment order-independent to READ: a parser recovers append order from
// the address alone rather than depending on file order surviving a parse. It is also why
// each recorded call carries that same address as its `gmeow:sessionStoreSegment` — the
// property whose whole purpose is locating a call's record inside an append-only store.
//
// # What the segment deliberately does NOT carry
//
// Neither the backend-minted record id nor the backend's creation stamp. Those are the two
// fields the two backends mint DIFFERENTLY by construction — `purrdf` content-addresses an
// id off the package's file length and stamps real UTC, while the browser store folds a
// session-local counter and stamps an explicitly logical instant — so carrying either
// would make the same contents serialize differently on the two sides and would carry a
// browser session's fake 1970 clock into a native package as though it were a wall time.
// What the segment carries is exactly what `StoreOptions` / `ToolCallOptions` accept back:
// that boundary is not information loss, it is the edge of what the store's public API can
// express, and anything past it is un-replayable by construction.

/// The namespace the exported session-store segment addresses its records under.
pub const SESSION_SEGMENT_NS: &str = "urn:gmeow:session:";

const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDF_VALUE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#value";
const XSD_DECIMAL: &str = "http://www.w3.org/2001/XMLSchema#decimal";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";

// The `gmeow:` predicates the segment writes and reads, spelled out once so the writer and
// the reader join on the SAME string rather than on two `format!`s that could drift.
const GMEOW_CLAIM_TOKEN: &str = "https://blackcatinformatics.ca/gmeow/ClaimToken";
const GMEOW_TOOL_CALL: &str = "https://blackcatinformatics.ca/gmeow/ToolCall";
const GMEOW_SOFTWARE_AGENT: &str = "https://blackcatinformatics.ca/gmeow/SoftwareAgent";
const GMEOW_CONFIDENCE: &str = "https://blackcatinformatics.ca/gmeow/confidence";
const GMEOW_ACCORDING_TO: &str = "https://blackcatinformatics.ca/gmeow/accordingTo";
const GMEOW_SOURCE_LOCATION: &str = "https://blackcatinformatics.ca/gmeow/sourceLocation";
const GMEOW_DISPLAYABLE: &str = "https://blackcatinformatics.ca/gmeow/displayable";
const GMEOW_USED_TOOL: &str = "https://blackcatinformatics.ca/gmeow/usedTool";
const GMEOW_TOOL_ARGUMENTS: &str = "https://blackcatinformatics.ca/gmeow/toolArguments";
const GMEOW_TOOL_RESULT: &str = "https://blackcatinformatics.ca/gmeow/toolResult";
const GMEOW_CALLED_BY_INVOCATION: &str = "https://blackcatinformatics.ca/gmeow/calledByInvocation";
const GMEOW_SESSION_STORE_SEGMENT: &str =
    "https://blackcatinformatics.ca/gmeow/sessionStoreSegment";
const GMEOW_WAS_GENERATED_BY: &str = "https://blackcatinformatics.ca/gmeow/wasGeneratedBy";

/// The position address of the `ordinal`-th record of `kind` (`claim` / `call`).
fn segment_node(kind: &str, ordinal: usize) -> String {
    format!("{SESSION_SEGMENT_NS}{kind}:{ordinal:04}")
}

/// The zero-padded segment identifier a recorded call carries as its
/// `gmeow:sessionStoreSegment` — the address's own local part, so the property and the node
/// name one position and not two.
fn segment_label(ordinal: usize) -> String {
    format!("{ordinal:04}")
}

/// Recover a record's ordinal from its position address, given the expected `kind`.
fn segment_ordinal(node: &str, kind: &str) -> Option<usize> {
    node.strip_prefix(SESSION_SEGMENT_NS)?
        .strip_prefix(kind)?
        .strip_prefix(':')?
        .parse()
        .ok()
}

/// One N-Triples/N-Quads literal, escaped per the grammar's ECHAR set.
fn nq_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            // Every other C0 control is escaped as `\uXXXX` rather than emitted raw: the
            // N-Triples STRING_LITERAL_QUOTE production excludes them outright, so a claim
            // carrying one would otherwise serialize to a segment that does not parse.
            other if (other as u32) < 0x20 || other as u32 == 0x7f => {
                let _ = write!(out, "\\u{:04X}", other as u32);
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// The same literal with an explicit datatype IRI.
fn nq_typed(value: &str, datatype: &str) -> String {
    format!("{}^^<{datatype}>", nq_literal(value))
}

/// Refuse a value the transport must carry as an IRI but which is not one.
///
/// `gmeow:usedTool`, `gmeow:calledByInvocation` and the `gmeow:wasGeneratedBy` targets are
/// object properties: a relative or empty value there would serialize to a segment that
/// does not parse, so it is a HARD FAIL naming the record and the field rather than a
/// quietly dropped edge.
fn require_iri(value: &str, field: &str, node: &str) -> Result<()> {
    if value.contains(':') && !value.contains(char::is_whitespace) && !value.contains(['<', '>']) {
        return Ok(());
    }
    Err(err(format!(
        "session store segment: {node} carries {field} {value:?}, which is not an absolute IRI"
    )))
}

/// Serialize a claim package's readable contents — its claims and its recorded tool calls —
/// as the shared session-store transport segment, in N-Quads, in append order.
///
/// The single serializer behind [`ClaimStore::segment_nquads`], used by BOTH backends. See
/// the section comment above for the shape, the addressing, and what it deliberately omits.
///
/// # Errors
///
/// A claim with empty text, or a record whose `gmeow:usedTool` /
/// `gmeow:calledByInvocation` / generated-entity value is not an absolute IRI — each named
/// individually, because a record the transport cannot carry must fail the export rather
/// than be dropped out of a segment that still calls itself a snapshot.
pub fn claim_segment(claims: &[Claim], calls: &[ToolCallRecord]) -> Result<String> {
    let mut out = String::new();
    // Backend claim id → this segment's position address, so a call's generated-entity edge
    // lands on the claim node the segment actually carries rather than on an id that only
    // means something inside the backend that minted it.
    let mut node_of: HashMap<&str, String> = HashMap::with_capacity(claims.len());

    for (ordinal, claim) in claims.iter().enumerate() {
        let node = segment_node("claim", ordinal);
        if claim.text.trim().is_empty() {
            return Err(err(format!(
                "session store segment: {node} carries empty claim text"
            )));
        }
        let _ = writeln!(out, "<{node}> <{RDF_TYPE}> <{GMEOW_CLAIM_TOKEN}> .");
        let _ = writeln!(out, "<{node}> <{RDF_VALUE}> {} .", nq_literal(&claim.text));
        if let Some(confidence) = claim.confidence {
            let _ = writeln!(
                out,
                "<{node}> <{GMEOW_CONFIDENCE}> {} .",
                nq_typed(&confidence.to_string(), XSD_DECIMAL)
            );
        }
        if let Some(according_to) = claim.according_to.as_deref() {
            // A plain literal, not an IRI: the memory package records `accordingTo` as an
            // opaque party identifier and never asserts that it names a resource, so
            // promoting it here would be a typing claim the store does not make.
            let _ = writeln!(
                out,
                "<{node}> <{GMEOW_ACCORDING_TO}> {} .",
                nq_literal(according_to)
            );
        }
        if let Some(source) = claim.source.as_deref() {
            let _ = writeln!(
                out,
                "<{node}> <{GMEOW_SOURCE_LOCATION}> {} .",
                nq_literal(source)
            );
        }
        if claim.suppressed {
            // P10 suppression, carried as the model's ONE display control rather than as a
            // deletion: the retired claim rides in the segment and is marked, never dropped.
            let _ = writeln!(
                out,
                "<{node}> <{GMEOW_DISPLAYABLE}> {} .",
                nq_typed("false", XSD_BOOLEAN)
            );
        }
        node_of.insert(claim.id.as_str(), node);
    }

    for (ordinal, call) in calls.iter().enumerate() {
        let node = segment_node("call", ordinal);
        require_iri(&call.tool, "gmeow:usedTool", &node)?;
        let _ = writeln!(out, "<{node}> <{RDF_TYPE}> <{GMEOW_TOOL_CALL}> .");
        let _ = writeln!(out, "<{node}> <{GMEOW_USED_TOOL}> <{}> .", call.tool);
        // gmeow:ToolCall requires its tool to be a gmeow:SoftwareAgent; the type rides with
        // the edge so the segment stands alone rather than leaning on an outside graph.
        let _ = writeln!(
            out,
            "<{}> <{RDF_TYPE}> <{GMEOW_SOFTWARE_AGENT}> .",
            call.tool
        );
        if let Some(arguments) = call.arguments.as_deref() {
            let _ = writeln!(
                out,
                "<{node}> <{GMEOW_TOOL_ARGUMENTS}> {} .",
                nq_literal(arguments)
            );
        }
        if let Some(result) = call.result.as_deref() {
            let _ = writeln!(
                out,
                "<{node}> <{GMEOW_TOOL_RESULT}> {} .",
                nq_literal(result)
            );
        }
        if let Some(invocation) = call.invocation.as_deref() {
            require_iri(invocation, "gmeow:calledByInvocation", &node)?;
            let _ = writeln!(
                out,
                "<{node}> <{GMEOW_CALLED_BY_INVOCATION}> <{invocation}> ."
            );
        }
        let _ = writeln!(
            out,
            "<{node}> <{GMEOW_SESSION_STORE_SEGMENT}> {} .",
            nq_literal(&segment_label(ordinal))
        );
        // The generated set is emitted in ADDRESS order, not in the order the backend
        // happened to hand it over: `gmeow:wasGeneratedBy` is a set of edges, and a
        // canonical order is what makes seed → re-serialize land on the same bytes.
        let targets: BTreeSet<String> = call
            .generated
            .iter()
            .map(|generated| {
                require_iri(generated, "gmeow:wasGeneratedBy target", &node)?;
                Ok(node_of
                    .get(generated.as_str())
                    .cloned()
                    .unwrap_or_else(|| generated.clone()))
            })
            .collect::<Result<_>>()?;
        for target in targets {
            let _ = writeln!(out, "<{target}> <{GMEOW_WAS_GENERATED_BY}> <{node}> .");
        }
    }

    Ok(out)
}

/// One claim read back out of a transport segment, before it is replayed.
#[derive(Default)]
struct SeededClaim {
    text: Option<String>,
    confidence: Option<String>,
    according_to: Option<String>,
    source: Option<String>,
    suppressed: bool,
}

/// One recorded call read back out of a transport segment.
#[derive(Default)]
struct SeededCall {
    tool: Option<String>,
    arguments: Option<String>,
    result: Option<String>,
    invocation: Option<String>,
    /// The entities the call generated, by segment address (or verbatim IRI for an entity
    /// the segment does not itself address).
    generated: BTreeSet<String>,
}

/// Replay a session-store transport segment into `store` through the store's PUBLIC write
/// API — [`ClaimStore::store_claim`], [`ClaimStore::record_tool_call`] and, for a
/// suppressed claim, [`ClaimStore::revise_claim`].
///
/// The exact inverse of [`claim_segment`]: a store seeded from a segment re-serializes to
/// the byte-identical segment. Natively that write path IS `purrdf`'s `Memory::store()`, so
/// the seeded package is a real, cold-auditable `memory.gts` written by its owner — the
/// on-disk shape stays `purrdf`'s and nothing here reproduces it.
///
/// Returns the `(claims, calls)` counts seeded.
///
/// # Errors
///
/// A segment that does not parse as N-Quads, a `gmeow:wasGeneratedBy` edge whose object is
/// not a segment call address, a claim with no `rdf:value` text, a claim whose confidence
/// is not a number, a call with no `gmeow:usedTool`, or a backend write failure.
pub fn seed_claim_store(store: &dyn ClaimStore, segment: &str) -> Result<(usize, usize)> {
    let dataset = purrdf::parse_dataset(segment.as_bytes(), "application/n-quads", None)
        .map_err(|e| err(format!("session store segment does not parse: {e}")))?;

    let mut claims: BTreeMap<usize, SeededClaim> = BTreeMap::new();
    let mut calls: BTreeMap<usize, SeededCall> = BTreeMap::new();
    // `gmeow:wasGeneratedBy` runs from the generated ENTITY to the call, so a call's
    // generated set is collected by walking that edge backwards.
    let mut generated: Vec<(usize, String)> = Vec::new();

    for quad in purrdf::flat_rdf_quads_from_dataset(&dataset) {
        let purrdf::RdfTerm::Iri(subject) = &quad.subject else {
            continue;
        };
        let object = match &quad.object {
            purrdf::RdfTerm::Iri(iri) => iri.clone(),
            purrdf::RdfTerm::Literal(literal) => literal.lexical_form.clone(),
            purrdf::RdfTerm::BlankNode(_) | purrdf::RdfTerm::Triple(_) => continue,
        };
        let predicate = quad.predicate.as_str();

        if predicate == GMEOW_WAS_GENERATED_BY {
            let call = segment_ordinal(&object, "call").ok_or_else(|| {
                err(format!(
                    "session store segment: {subject} was generated by {object}, which is not \
                     a segment call address"
                ))
            })?;
            generated.push((call, subject.clone()));
            continue;
        }

        if let Some(ordinal) = segment_ordinal(subject, "claim") {
            let claim = claims.entry(ordinal).or_default();
            match predicate {
                RDF_VALUE => claim.text = Some(object),
                GMEOW_CONFIDENCE => claim.confidence = Some(object),
                GMEOW_ACCORDING_TO => claim.according_to = Some(object),
                GMEOW_SOURCE_LOCATION => claim.source = Some(object),
                // The ONLY display control in the model: `false` is the P10 suppression.
                GMEOW_DISPLAYABLE => claim.suppressed = object == "false",
                _ => {}
            }
        } else if let Some(ordinal) = segment_ordinal(subject, "call") {
            let call = calls.entry(ordinal).or_default();
            match predicate {
                GMEOW_USED_TOOL => call.tool = Some(object),
                GMEOW_TOOL_ARGUMENTS => call.arguments = Some(object),
                GMEOW_TOOL_RESULT => call.result = Some(object),
                GMEOW_CALLED_BY_INVOCATION => call.invocation = Some(object),
                _ => {}
            }
        }
    }

    for (call, entity) in generated {
        calls.entry(call).or_default().generated.insert(entity);
    }

    // Replay in append order — the position addresses ARE that order, so a segment whose
    // lines were reordered by a parse still seeds the store in the order it was written.
    let mut seeded_ids: BTreeMap<usize, String> = BTreeMap::new();
    for (ordinal, claim) in &claims {
        let text = claim.text.as_deref().ok_or_else(|| {
            err(format!(
                "session store segment: claim {ordinal:04} carries no rdf:value text"
            ))
        })?;
        let confidence = claim
            .confidence
            .as_deref()
            .map(|raw| {
                raw.parse::<f64>().map_err(|e| {
                    err(format!(
                        "session store segment: claim {ordinal:04} confidence {raw:?}: {e}"
                    ))
                })
            })
            .transpose()?;
        let stored = store.store_claim(
            text,
            StoreOptions {
                source: claim.source.as_deref(),
                confidence,
                according_to: claim.according_to.as_deref(),
            },
        )?;
        seeded_ids.insert(*ordinal, stored.id);
    }
    // Suppressions land AFTER every claim is stored: a revision names the claim it retires
    // by the id the store just minted, which does not exist until the store has minted it.
    for (ordinal, claim) in &claims {
        if claim.suppressed {
            store.revise_claim(&seeded_ids[ordinal], RevisionOptions::default())?;
        }
    }

    for (ordinal, call) in &calls {
        let tool = call.tool.as_deref().ok_or_else(|| {
            err(format!(
                "session store segment: call {ordinal:04} carries no gmeow:usedTool"
            ))
        })?;
        // A generated entity the segment addresses is re-pointed at the id THIS store just
        // minted for it; anything else the store recorded rides verbatim.
        let generated: Vec<String> = call
            .generated
            .iter()
            .map(|entity| match segment_ordinal(entity, "claim") {
                Some(claim) => seeded_ids
                    .get(&claim)
                    .cloned()
                    .unwrap_or_else(|| entity.clone()),
                None => entity.clone(),
            })
            .collect();
        let generated: Vec<&str> = generated.iter().map(String::as_str).collect();
        store.record_tool_call(
            tool,
            ToolCallOptions {
                arguments: call.arguments.as_deref(),
                result: call.result.as_deref(),
                invocation: call.invocation.as_deref(),
                generated: &generated,
            },
        )?;
    }

    Ok((claims.len(), calls.len()))
}

// ─────────────────────────────────────────────────────────────────────────────
// The native, filesystem-backed backend
// ─────────────────────────────────────────────────────────────────────────────

/// The native backend: real environment variables and real files, byte-for-byte the
/// behaviour the engine had before the seam existed.
#[cfg(not(target_arch = "wasm32"))]
pub struct FsStorage;

#[cfg(not(target_arch = "wasm32"))]
pub use native::{fs_claim_store, fs_segment_library};

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use std::fs;
    use std::io::{self, Write as _};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use gmeow_errors::Result;
    use purrdf::gts::examples::agent_memory::{
        Claim, Memory, RecallOptions, RevisionOptions, StoreOptions, ToolCallOptions,
        ToolCallRecord,
    };

    use super::{ClaimStore, FsStorage, LibraryLock, SegmentLibrary, Storage, err};

    /// A native claim package at an EXPLICIT path, bypassing the environment resolution.
    ///
    /// The seam's normal entry is [`Storage::claim_store`], which reads
    /// `GMEOW_MEMORY_PATH`; this is for a caller that already knows the package it means
    /// (a launcher given an explicit `--memory` path, and the crate's own tests, which
    /// must address a temp package without mutating the process environment).
    ///
    /// # Errors
    ///
    /// If the package's parent directory cannot be created.
    pub fn fs_claim_store(path: impl Into<PathBuf>) -> Result<Arc<dyn ClaimStore>> {
        let path = path.into();
        ensure_parent(&path)?;
        Ok(Arc::new(FsClaimStore {
            memory: Memory::new(path.clone()),
            path,
        }))
    }

    /// A native append-only library at an EXPLICIT path. The [`fs_claim_store`] rationale,
    /// for the conjecture / candidate libraries.
    #[must_use]
    pub fn fs_segment_library(path: impl Into<PathBuf>) -> Arc<dyn SegmentLibrary> {
        Arc::new(FsSegmentLibrary { path: path.into() })
    }

    /// Expand a leading `~` / `~/` in a configured path against the home directory.
    /// A path with no `~`, or a host with no home, is returned unchanged.
    fn expand_home(raw: &str) -> PathBuf {
        if raw == "~" {
            return home_dir().map_or_else(|| PathBuf::from(raw), PathBuf::from);
        }
        if let Some(rest) = raw.strip_prefix("~/")
            && let Some(home) = home_dir()
        {
            return Path::new(&home).join(rest);
        }
        PathBuf::from(raw)
    }

    /// The user's home directory: `HOME`, else `USERPROFILE` (Windows).
    fn home_dir() -> Option<String> {
        std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .ok()
    }

    /// Resolve one of the three store paths: the override variable when it carries a
    /// non-empty value (home-expanded), else `~/.gmeow/<default_file>`. A host with no
    /// home and no override is a HARD FAIL naming both, never a silent fallback to a
    /// relative path in whatever directory the process happens to sit in.
    fn resolve_path(var: &str, default_file: &str) -> Result<PathBuf> {
        if let Some(raw) = FsStorage.env_var(var) {
            return Ok(expand_home(&raw));
        }
        let home = home_dir().ok_or_else(|| {
            err(format!(
                "neither HOME nor USERPROFILE is set and {var} is empty"
            ))
        })?;
        Ok(Path::new(&home).join(".gmeow").join(default_file))
    }

    /// Create the parent directory of `path`, so a first-ever write to a configured
    /// path under a not-yet-existing directory succeeds instead of failing on ENOENT.
    fn ensure_parent(path: &Path) -> Result<()> {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent)?;
        }
        Ok(())
    }

    impl Storage for FsStorage {
        fn env_var(&self, key: &str) -> Option<String> {
            std::env::var(key)
                .ok()
                .filter(|value| !value.trim().is_empty())
        }

        fn now_rfc3339(&self) -> String {
            // The native claim package stamps its own real UTC instant on every record
            // it mints; the ENGINE-minted audit segment reuses the stamp the package
            // just produced rather than taking a second, slightly-later reading of the
            // clock (two stamps for one turn would be a false ordering). Nothing on the
            // native path calls this, and returning the package's own fallback keeps the
            // contract total.
            "1970-01-01T00:00:00Z".to_owned()
        }

        fn claim_store(&self) -> Result<Arc<dyn ClaimStore>> {
            let path = resolve_path("GMEOW_MEMORY_PATH", "memory.gts")?;
            ensure_parent(&path)?;
            Ok(Arc::new(FsClaimStore {
                memory: Memory::new(path.clone()),
                path,
            }))
        }

        fn conjecture_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
            let path = resolve_path("GMEOW_CONJECTURE_PATH", "conjectures.gts")?;
            Ok(Arc::new(FsSegmentLibrary { path }))
        }

        fn candidate_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
            let path = resolve_path("GMEOW_CANDIDATE_PATH", "candidates.gts")?;
            Ok(Arc::new(FsSegmentLibrary { path }))
        }
    }

    /// The native claim store: `purrdf`'s append-only memory package at `path`.
    ///
    /// Every method delegates, so the on-disk `memory.gts` and the claim algebra
    /// (content-addressed ids, RDF 1.2 reified annotations, the recall ranking) are the
    /// upstream implementation, not a copy of it.
    pub(super) struct FsClaimStore {
        pub(super) memory: Memory,
        /// The package path, retained so the audit segment appends to the SAME file the
        /// claims and tool calls live in.
        pub(super) path: PathBuf,
    }

    impl ClaimStore for FsClaimStore {
        fn store_claim(&self, text: &str, options: StoreOptions<'_>) -> Result<Claim> {
            Ok(self.memory.store(text, options)?)
        }

        fn revise_claim(&self, claim_id: &str, options: RevisionOptions<'_>) -> Result<()> {
            Ok(self.memory.revise(claim_id, options)?)
        }

        fn record_tool_call(
            &self,
            tool: &str,
            options: ToolCallOptions<'_>,
        ) -> Result<ToolCallRecord> {
            Ok(self.memory.record_tool_call(tool, options)?)
        }

        fn recall(&self, options: RecallOptions<'_>) -> Result<Vec<Claim>> {
            Ok(self.memory.recall(options)?)
        }

        fn claims(&self) -> Result<Vec<Claim>> {
            Ok(self.memory.claims()?)
        }

        fn tool_calls(&self) -> Result<Vec<ToolCallRecord>> {
            Ok(self.memory.tool_calls()?)
        }

        fn append_audit_segment(&self, segment: &[u8]) -> Result<()> {
            let mut file = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            file.write_all(segment)?;
            Ok(())
        }
    }

    /// The native append-only library: a GTS file plus a sidecar `.lock`.
    pub(super) struct FsSegmentLibrary {
        pub(super) path: PathBuf,
    }

    /// The sidecar advisory-lock path: the library path with a literal `.lock` suffix.
    ///
    /// The lock file's own bytes are never read; it exists solely as a stable
    /// `flock`/`LockFileEx` target that survives the library file being replaced out
    /// from under it by [`FsSegmentLibrary::replace_bytes`]'s atomic rename (an `flock`
    /// on the DATA file itself would be silently dropped by a rename-replace, since the
    /// lock is bound to the inode, not the path).
    fn lock_path(library_path: &Path) -> PathBuf {
        let mut os = library_path.as_os_str().to_owned();
        os.push(".lock");
        PathBuf::from(os)
    }

    /// The native lock: an `flock`ed file handle, unlocked on drop.
    struct FsLibraryLock {
        file: fs::File,
    }

    impl LibraryLock for FsLibraryLock {}

    impl Drop for FsLibraryLock {
        fn drop(&mut self) {
            // Releasing is best-effort: closing the handle drops the lock regardless,
            // and a failure here has no recovery the caller could perform.
            let _ = self.file.unlock();
        }
    }

    impl SegmentLibrary for FsSegmentLibrary {
        fn read_bytes(&self) -> Result<Vec<u8>> {
            match fs::read(&self.path) {
                Ok(bytes) => Ok(bytes),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Vec::new()),
                Err(e) => Err(e.into()),
            }
        }

        fn replace_bytes(&self, bytes: &[u8]) -> Result<()> {
            // Same-directory temp file + fsync + atomic rename: a rename either lands
            // the WHOLE new file or leaves the PRIOR file completely untouched, so a
            // failure partway through can never leave a torn library.
            let dir = self
                .path
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), PathBuf::from);
            let mut tmp = tempfile::Builder::new()
                .prefix(".gmeow-library-")
                .suffix(".tmp")
                .tempfile_in(&dir)?;
            tmp.write_all(bytes)?;
            tmp.as_file().sync_all()?;
            tmp.persist(&self.path)
                .map_err(|e| err(format!("commit library {}: {e}", self.path.display())))?;
            Ok(())
        }

        fn lock(&self) -> Result<Box<dyn LibraryLock + '_>> {
            ensure_parent(&self.path)?;
            let file = fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(lock_path(&self.path))?;
            // Blocking exclusive lock (`flock(LOCK_EX)` / `LockFileEx` exclusive) — a
            // concurrent holder blocks here rather than racing past a TOCTOU window.
            file.lock()?;
            Ok(Box::new(FsLibraryLock { file }))
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The in-process backend (the browser's real store)
// ─────────────────────────────────────────────────────────────────────────────

/// The browser backend: a REAL store held in process memory.
///
/// Compiled on every target so the native suite can prove it works; selected by
/// [`storage`] only on `wasm32`.
pub struct InMemoryStorage {
    /// The configuration environment. A browser host has no process environment, so it
    /// is one the host populates through [`InMemoryStorage::set_env`] (a launcher
    /// setting `GMEOW_LANG` from the page's locale, say). Unset means unset — the same
    /// answer a native host gives for a variable nobody exported.
    env: Mutex<BTreeMap<String, String>>,
    claims: Arc<InMemoryClaimStore>,
    conjectures: Arc<InMemorySegmentLibrary>,
    candidates: Arc<InMemorySegmentLibrary>,
}

impl Default for InMemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

impl InMemoryStorage {
    /// An empty backend: no configuration, no claims, no library contents.
    #[must_use]
    pub fn new() -> Self {
        Self {
            env: Mutex::new(BTreeMap::new()),
            claims: Arc::new(InMemoryClaimStore::default()),
            conjectures: Arc::new(InMemorySegmentLibrary::default()),
            candidates: Arc::new(InMemorySegmentLibrary::default()),
        }
    }

    /// Set one configuration value — the browser host's equivalent of exporting an
    /// environment variable before launching the server.
    ///
    /// # Panics
    ///
    /// If the configuration lock was poisoned by a panic in another thread.
    pub fn set_env(&self, key: &str, value: &str) {
        self.env
            .lock()
            .expect("in-memory storage environment lock")
            .insert(key.to_owned(), value.to_owned());
    }
}

impl Storage for InMemoryStorage {
    fn env_var(&self, key: &str) -> Option<String> {
        self.env
            .lock()
            .expect("in-memory storage environment lock")
            .get(key)
            .filter(|value| !value.trim().is_empty())
            .cloned()
    }

    fn now_rfc3339(&self) -> String {
        self.claims.tick()
    }

    fn claim_store(&self) -> Result<Arc<dyn ClaimStore>> {
        Ok(Arc::clone(&self.claims) as Arc<dyn ClaimStore>)
    }

    fn conjecture_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
        Ok(Arc::clone(&self.conjectures) as Arc<dyn SegmentLibrary>)
    }

    fn candidate_library(&self) -> Result<Arc<dyn SegmentLibrary>> {
        Ok(Arc::clone(&self.candidates) as Arc<dyn SegmentLibrary>)
    }
}

/// One claim revision: the suppression, its reason, and the successor that supersedes
/// the retired claim. Retained (rather than collapsed into a boolean) because the
/// revision record IS part of the grounded memory — the native package writes it as
/// annotations on the suppression, and a browser session that revises a belief must be
/// able to say why.
#[derive(Clone)]
struct Revision {
    claim_id: String,
    reason: Option<String>,
    superseded_by: Option<String>,
}

/// The browser claim store's whole state.
#[derive(Default)]
struct ClaimState {
    /// Claims in storage (append) order — the order `recall` scores over.
    claims: Vec<Claim>,
    /// Claim ids retired by a revision.
    suppressed: BTreeSet<String>,
    /// Every revision in application order.
    revisions: Vec<Revision>,
    /// Recorded tool calls in storage order.
    calls: Vec<ToolCallRecord>,
    /// The concatenated trajectory-audit GTS segment bytes, kept verbatim so the
    /// browser's trajectory is a real auditable byte stream rather than a discard.
    audit: Vec<u8>,
    /// The monotone logical clock / id counter. See the module docs.
    seq: u64,
}

/// The browser's grounded-memory claim package.
///
/// # Why this is an implementation and not a copy
///
/// The native store IS `purrdf`'s memory package, whose claim algebra is expressed
/// over GTS segment bytes: it exists to make an append-only FILE auditable. In process
/// memory there is no file to make auditable, so re-encoding claims to GTS just to
/// decode them again on the next `recall` would be ceremony, not fidelity. What must
/// agree between the two backends is the OBSERVABLE contract — storage order, the
/// suppression rule, the confidence filter, and the relevance ranking — and that
/// contract is asserted against both backends by the crate's tests rather than assumed.
#[derive(Default)]
pub struct InMemoryClaimStore {
    state: Mutex<ClaimState>,
}

impl InMemoryClaimStore {
    /// Advance the logical clock and render the new instant.
    fn tick(&self) -> String {
        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        logical_instant(state.seq)
    }

    /// Every revision recorded so far, in application order — the browser twin of the
    /// suppression annotations the native package writes into `memory.gts`.
    ///
    /// # Panics
    ///
    /// If the store lock was poisoned by a panic in another thread.
    #[must_use]
    pub fn revisions(&self) -> Vec<(String, Option<String>, Option<String>)> {
        self.state
            .lock()
            .expect("in-memory claim store lock")
            .revisions
            .iter()
            .map(|r| {
                (
                    r.claim_id.clone(),
                    r.reason.clone(),
                    r.superseded_by.clone(),
                )
            })
            .collect()
    }

    /// The accumulated trajectory-audit segment bytes.
    ///
    /// # Panics
    ///
    /// If the store lock was poisoned by a panic in another thread.
    #[must_use]
    pub fn audit_bytes(&self) -> Vec<u8> {
        self.state
            .lock()
            .expect("in-memory claim store lock")
            .audit
            .clone()
    }
}

/// The civil date `days` days after 1970-01-01, as `(year, month, day)`.
///
/// The standard proleptic-Gregorian `civil_from_days` shift-the-epoch-to-March algorithm:
/// re-anchoring on 0000-03-01 makes the leap day the LAST day of the year, which is what
/// collapses the month-length table into the exact affine map `mp = (5·doy + 2) / 153`.
/// Total over the whole `u64` day range — there is no month or year it cannot render.
fn civil_from_days(days: u64) -> (u64, u64, u64) {
    // Days from 0000-03-01 to 1970-01-01. Every quantity below is non-negative because the
    // logical clock is a counter anchored AT the epoch and never runs backwards.
    let z = days + 719_468;
    let era = z / 146_097;
    let doe = z % 146_097; // day of era, [0, 146_096]
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365; // [0, 399]
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // day of the March-anchored year
    let mp = (5 * doy + 2) / 153; // March-anchored month, [0, 11]
    let day = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let month = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    let year = yoe + era * 400 + u64::from(month <= 2);
    (year, month, day)
}

/// The logical instant for sequence number `seq`: `seq` seconds after the Unix epoch,
/// rendered as an `xsd:dateTime`. Anchoring at the epoch is what makes the stamp
/// self-identifying as logical rather than a plausible-looking fake wall time.
///
/// The date is a REAL civil date computed from the epoch offset, not `1970-01-{seq/86400+1}`:
/// a session that records 2 678 400 times crosses out of January, and a stamp that read
/// `1970-01-32T…` would not be an `xsd:dateTime` at all — every consumer that parses the
/// stamp (the trajectory auditor orders on it) would reject the record rather than order it.
fn logical_instant(seq: u64) -> String {
    let rest = seq % 86_400;
    let (h, m, s) = (rest / 3600, (rest % 3600) / 60, rest % 60);
    let (year, month, day) = civil_from_days(seq / 86_400);
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// A deterministic opaque id for an in-memory record: the store kind, the sequence
/// number, and the record's identifying parts folded with SHA-256.
///
/// Content-addressed like the native package's ids (which fold blake3 over the file
/// length and the same parts) and, like them, opaque to every caller — the sequence
/// number is in the fold so two identical claims stored twice are still distinct
/// records.
fn record_id(kind: &str, seq: u64, parts: &[&str]) -> String {
    use sha2::{Digest as _, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(kind.as_bytes());
    hasher.update(seq.to_le_bytes());
    for part in parts {
        hasher.update([0u8]);
        hasher.update(part.as_bytes());
    }
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(hex, "{byte:02x}");
    }
    format!("urn:gmeow:mcp:{kind}:sha256:{hex}")
}

impl ClaimStore for InMemoryClaimStore {
    fn store_claim(&self, text: &str, options: StoreOptions<'_>) -> Result<Claim> {
        // The SAME two input rules the native package enforces, refused here rather
        // than stored and discovered later.
        if text.trim().is_empty() {
            return Err(err("store_claim: the claim text is empty"));
        }
        if let Some(confidence) = options.confidence
            && (!confidence.is_finite() || !(0.0..=1.0).contains(&confidence))
        {
            return Err(err(format!(
                "store_claim: confidence {confidence} is outside the inclusive range 0.0..=1.0"
            )));
        }

        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        let seq = state.seq;
        let created = logical_instant(seq);
        let confidence_text = options.confidence.map(|value| value.to_string());
        let id = record_id(
            "claim",
            seq,
            &[
                text,
                created.as_str(),
                options.source.unwrap_or(""),
                confidence_text.as_deref().unwrap_or(""),
                options.according_to.unwrap_or(""),
            ],
        );
        let claim = Claim {
            id,
            text: text.to_owned(),
            confidence: options.confidence,
            according_to: options.according_to.map(str::to_owned),
            source: options.source.map(str::to_owned),
            created: Some(created),
            suppressed: false,
        };
        state.claims.push(claim.clone());
        Ok(claim)
    }

    fn revise_claim(&self, claim_id: &str, options: RevisionOptions<'_>) -> Result<()> {
        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        state.suppressed.insert(claim_id.to_owned());
        state.revisions.push(Revision {
            claim_id: claim_id.to_owned(),
            reason: options.reason.map(str::to_owned),
            superseded_by: options.superseded_by.map(str::to_owned),
        });
        Ok(())
    }

    fn record_tool_call(&self, tool: &str, options: ToolCallOptions<'_>) -> Result<ToolCallRecord> {
        let mut state = self.state.lock().expect("in-memory claim store lock");
        state.seq += 1;
        let seq = state.seq;
        let created = logical_instant(seq);
        let id = record_id(
            "call",
            seq,
            &[
                tool,
                created.as_str(),
                options.arguments.unwrap_or(""),
                options.result.unwrap_or(""),
                options.invocation.unwrap_or(""),
            ],
        );
        let record = ToolCallRecord {
            id,
            tool: tool.to_owned(),
            arguments: options.arguments.map(str::to_owned),
            result: options.result.map(str::to_owned),
            invocation: options.invocation.map(str::to_owned),
            created: Some(created),
            generated: options.generated.iter().map(|g| (*g).to_owned()).collect(),
        };
        state.calls.push(record.clone());
        Ok(record)
    }

    fn recall(&self, options: RecallOptions<'_>) -> Result<Vec<Claim>> {
        // The ranking is the native package's, term for term: filter by suppression and
        // by the confidence floor; with no query terms return storage order REVERSED
        // (most recent first); otherwise score by token overlap and order by
        // (score desc, storage index desc), dropping every zero-overlap claim.
        let mut claims: Vec<Claim> = self
            .claims()?
            .into_iter()
            .filter(|claim| options.include_suppressed || !claim.suppressed)
            .filter(|claim| match options.min_confidence {
                None => true,
                Some(min) => claim.confidence.is_some_and(|got| got >= min),
            })
            .collect();

        let tokens: HashSet<String> = options
            .query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        if tokens.is_empty() {
            claims.reverse();
        } else {
            let mut scored: Vec<(usize, usize, Claim)> = claims
                .into_iter()
                .enumerate()
                .map(|(index, claim)| {
                    let claim_tokens: HashSet<String> = claim
                        .text
                        .to_lowercase()
                        .split_whitespace()
                        .map(str::to_owned)
                        .collect();
                    let score = tokens.intersection(&claim_tokens).count();
                    (score, index, claim)
                })
                .filter(|(score, _, _)| *score > 0)
                .collect();
            scored.sort_by_key(|(score, index, _)| {
                (std::cmp::Reverse(*score), std::cmp::Reverse(*index))
            });
            claims = scored.into_iter().map(|(_, _, claim)| claim).collect();
        }
        claims.truncate(options.limit);
        Ok(claims)
    }

    fn claims(&self) -> Result<Vec<Claim>> {
        let state = self.state.lock().expect("in-memory claim store lock");
        Ok(state
            .claims
            .iter()
            .map(|claim| {
                let mut claim = claim.clone();
                claim.suppressed = state.suppressed.contains(&claim.id);
                claim
            })
            .collect())
    }

    fn tool_calls(&self) -> Result<Vec<ToolCallRecord>> {
        Ok(self
            .state
            .lock()
            .expect("in-memory claim store lock")
            .calls
            .clone())
    }

    fn append_audit_segment(&self, segment: &[u8]) -> Result<()> {
        self.state
            .lock()
            .expect("in-memory claim store lock")
            .audit
            .extend_from_slice(segment);
        Ok(())
    }
}

/// The browser's append-only segment library: the same bytes the native backend would
/// hold in a file, held in process memory instead.
#[derive(Default)]
pub struct InMemorySegmentLibrary {
    /// The library's bytes. Empty means "never written" — the same thing an absent
    /// file means natively.
    bytes: Mutex<Vec<u8>>,
    /// The exclusive lock, held for the whole read → decide → replace sequence. A
    /// SEPARATE mutex from `bytes` so a caller holding the library lock can still read
    /// and replace through it; sharing one mutex would deadlock the very sequence the
    /// lock exists to protect.
    gate: Mutex<()>,
}

/// The browser lock: a mutex guard, released on drop.
struct InMemoryLibraryLock<'a> {
    _guard: std::sync::MutexGuard<'a, ()>,
}

impl LibraryLock for InMemoryLibraryLock<'_> {}

impl SegmentLibrary for InMemorySegmentLibrary {
    fn read_bytes(&self) -> Result<Vec<u8>> {
        Ok(self
            .bytes
            .lock()
            .expect("in-memory segment library lock")
            .clone())
    }

    fn replace_bytes(&self, bytes: &[u8]) -> Result<()> {
        // Wholesale replacement under one lock IS the all-or-nothing guarantee here:
        // there is no partial write for a reader to observe.
        *self.bytes.lock().expect("in-memory segment library lock") = bytes.to_vec();
        Ok(())
    }

    fn lock(&self) -> Result<Box<dyn LibraryLock + '_>> {
        let guard = self
            .gate
            .lock()
            .map_err(|_| err("in-memory segment library lock was poisoned by a panic"))?;
        Ok(Box::new(InMemoryLibraryLock { _guard: guard }))
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// The seam's own suite
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// The `xsd:dateTime` datatype IRI, so the clock is validated by a REAL XSD parser
    /// rather than by a shape assertion written here.
    const XSD_DATE_TIME: &str = "http://www.w3.org/2001/XMLSchema#dateTime";

    /// Parse one logical stamp as an `xsd:dateTime`, hard-failing when it is not one.
    fn as_datetime(stamp: &str) -> purrdf::xsd::XsdValue {
        purrdf::xsd::parse_by_iri(stamp, XSD_DATE_TIME)
            .unwrap_or_else(|e| panic!("{stamp:?} is not a valid xsd:dateTime: {e:?}"))
            .expect("xsd:dateTime is in the XSD value space")
    }

    /// The logical clock is monotone AND every value it emits is a real `xsd:dateTime` —
    /// checked across both rollovers the old `1970-01-{seq/86400+1}` rendering broke on.
    ///
    /// The second rollover is the one that mattered: at `seq >= 2_678_400` the old code
    /// emitted `1970-01-32T00:00:00Z`, which no `xsd:dateTime` parser accepts, so every
    /// consumer that reads the stamp — the trajectory auditor orders trajectories ON it —
    /// would reject the record rather than order it. Monotonicity is asserted on the PARSED
    /// values, not on the strings: a lexical comparison would pass for two stamps that no
    /// parser accepts at all.
    #[test]
    fn the_logical_clock_is_monotone_and_every_stamp_is_a_valid_xsd_date_time() {
        // Around the day rollover, around the month rollover, and well past both.
        let seqs: Vec<u64> = [
            0u64,
            1,
            59,
            60,
            3_599,
            3_600,
            86_398,
            86_399,
            86_400,
            86_401,
            172_800,
            2_678_398,
            2_678_399,
            2_678_400,
            2_678_401,
            5_097_600,
            31_535_999,
            31_536_000,
            1_000_000_000,
        ]
        .to_vec();

        assert_eq!(
            logical_instant(0),
            "1970-01-01T00:00:00Z",
            "the clock is anchored AT the Unix epoch — that anchor is what makes the stamp \
             self-identifying as logical rather than a plausible fake wall time"
        );
        assert_eq!(
            logical_instant(86_400),
            "1970-01-02T00:00:00Z",
            "one day of records advances the day"
        );
        assert_eq!(
            logical_instant(2_678_400),
            "1970-02-01T00:00:00Z",
            "31 days of records advances the MONTH; the old rendering emitted 1970-01-32"
        );
        assert_eq!(
            logical_instant(31_536_000),
            "1971-01-01T00:00:00Z",
            "365 days of records advances the year"
        );
        // 1972 is a leap year: 1972-02-29 must exist, and 1973-01-01 must be 366 days
        // after 1972-01-01 — the leap rule is exercised, not assumed.
        assert_eq!(logical_instant(68_169_600), "1972-02-29T00:00:00Z");
        assert_eq!(logical_instant(68_256_000), "1972-03-01T00:00:00Z");
        assert_eq!(logical_instant(94_694_400), "1973-01-01T00:00:00Z");

        let mut previous: Option<(u64, purrdf::xsd::XsdValue)> = None;
        for seq in seqs {
            let stamp = logical_instant(seq);
            let value = as_datetime(&stamp);
            if let Some((prior_seq, prior)) = &previous {
                assert_eq!(
                    purrdf::xsd::value_cmp(prior, &value),
                    Some(std::cmp::Ordering::Less),
                    "seq {prior_seq} -> {seq} must stamp a strictly later instant, got \
                     {} -> {stamp}",
                    logical_instant(*prior_seq)
                );
            }
            previous = Some((seq, value));
        }

        // Dense sweep straight through the month rollover: every single second in the
        // window parses and every step is strictly increasing.
        let mut prior = as_datetime(&logical_instant(2_678_390));
        for seq in 2_678_391..=2_678_410u64 {
            let value = as_datetime(&logical_instant(seq));
            assert_eq!(
                purrdf::xsd::value_cmp(&prior, &value),
                Some(std::cmp::Ordering::Less),
                "the clock stalled or went backwards at seq {seq}"
            );
            prior = value;
        }
    }

    /// A store seeded from a transport segment re-serializes to the BYTE-IDENTICAL segment.
    ///
    /// This is the whole contract [`claim_segment`] and [`seed_claim_store`] exist to hold:
    /// the pair is an isomorphism on exactly the state the store's public write API can
    /// express, so an exported session can be re-seeded into a different store — natively,
    /// through `purrdf`'s own `Memory::store()` — and answer identically there.
    #[test]
    fn a_store_seeded_from_a_segment_reserializes_to_the_same_bytes() {
        let origin = InMemoryClaimStore::default();
        let blue = origin
            .store_claim(
                "widgets are blue",
                StoreOptions {
                    source: Some("mcp:test"),
                    confidence: Some(0.9),
                    according_to: Some("urn:gmeow:party:lab"),
                },
            )
            .expect("stores");
        origin
            .store_claim(
                "gadgets are red",
                StoreOptions {
                    source: None,
                    confidence: None,
                    according_to: None,
                },
            )
            .expect("stores");
        let retired = origin
            .store_claim(
                "sprockets are green",
                StoreOptions {
                    source: None,
                    confidence: Some(0.25),
                    according_to: None,
                },
            )
            .expect("stores");
        origin
            .revise_claim(
                &retired.id,
                RevisionOptions {
                    reason: Some("measured again"),
                    superseded_by: None,
                },
            )
            .expect("revises");
        origin
            .record_tool_call(
                "urn:gmeow:tool:store_claim",
                ToolCallOptions {
                    arguments: Some(r#"{"text":"widgets are blue"}"#),
                    result: Some(r#"{"ok":true}"#),
                    invocation: Some("urn:gmeow:invocation:0"),
                    generated: &[blue.id.as_str()],
                },
            )
            .expect("records");

        let segment = origin.segment_nquads().expect("serializes");
        assert!(
            segment.contains("<urn:gmeow:session:claim:0000>"),
            "records are position-addressed: {segment}"
        );

        let seeded = InMemoryClaimStore::default();
        let (claims, calls) = seed_claim_store(&seeded, &segment).expect("seeds");
        assert_eq!((claims, calls), (3, 1), "every record is replayed");
        assert_eq!(
            seeded.segment_nquads().expect("re-serializes"),
            segment,
            "seed → re-serialize must land on the SAME bytes"
        );

        // The seeded store is a real store, not a transcript: the suppression took, and
        // the recorded call points at the id THIS store minted rather than at the address.
        let seeded_claims = seeded.claims().expect("reads");
        assert_eq!(seeded_claims.len(), 3);
        assert!(!seeded_claims[0].suppressed);
        assert!(
            seeded_claims[2].suppressed,
            "a suppressed claim seeds back suppressed, not dropped"
        );
        assert_eq!(seeded_claims[0].text, "widgets are blue");
        assert_eq!(seeded_claims[0].confidence, Some(0.9));
        assert_eq!(seeded_claims[0].source.as_deref(), Some("mcp:test"));
        assert_eq!(
            seeded_claims[0].according_to.as_deref(),
            Some("urn:gmeow:party:lab")
        );
        let seeded_calls = seeded.tool_calls().expect("reads");
        assert_eq!(seeded_calls[0].generated, vec![seeded_claims[0].id.clone()]);
    }

    /// An empty store serializes to an EMPTY segment — an answer, not a failure — and
    /// seeding from it is a no-op rather than an error.
    #[test]
    fn an_empty_store_serializes_to_an_empty_segment() {
        let store = InMemoryClaimStore::default();
        let segment = store.segment_nquads().expect("serializes");
        assert_eq!(segment, "", "nothing stored, nothing serialized");
        let seeded = InMemoryClaimStore::default();
        assert_eq!(
            seed_claim_store(&seeded, &segment).expect("seeds"),
            (0, 0),
            "seeding an empty segment stores nothing and raises nothing"
        );
    }

    /// The emitted segment PARSES as N-Quads and carries the GMEOW vocabulary the
    /// transport declares — asserted structurally over the parsed quads, never over a
    /// substring of the text.
    #[test]
    fn the_segment_parses_and_carries_the_declared_vocabulary() {
        let store = InMemoryClaimStore::default();
        store
            .store_claim(
                "the segment parses",
                StoreOptions {
                    source: None,
                    confidence: None,
                    according_to: None,
                },
            )
            .expect("stores");
        store
            .record_tool_call(
                "urn:gmeow:tool:recall",
                ToolCallOptions {
                    arguments: Some("{}"),
                    result: Some(r#"{"ok":true}"#),
                    invocation: None,
                    generated: &[],
                },
            )
            .expect("records");

        let segment = store.segment_nquads().expect("serializes");
        let dataset = purrdf::parse_dataset(segment.as_bytes(), "application/n-quads", None)
            .expect("the emitted segment parses as N-Quads");
        let quads = purrdf::flat_rdf_quads_from_dataset(&dataset);
        let typed_as = |class: &str| {
            quads.iter().any(|quad| {
                quad.predicate == RDF_TYPE
                    && matches!(&quad.object, purrdf::RdfTerm::Iri(iri) if iri == class)
            })
        };
        assert!(
            typed_as(GMEOW_CLAIM_TOKEN),
            "the claim is a gmeow:ClaimToken"
        );
        assert!(typed_as(GMEOW_TOOL_CALL), "the call is a gmeow:ToolCall");
        assert!(
            typed_as(GMEOW_SOFTWARE_AGENT),
            "the called tool is typed a gmeow:SoftwareAgent, so the segment stands alone"
        );
        assert!(
            quads
                .iter()
                .any(|quad| quad.predicate == GMEOW_SESSION_STORE_SEGMENT),
            "each call carries the segment identifier that locates its record"
        );
    }

    /// A control character in a claim survives the round trip: it is escaped on the way
    /// out, so the segment still PARSES, and comes back byte-identical.
    ///
    /// N-Triples excludes the C0 controls from its quoted-literal production outright, so a
    /// raw one would produce a segment no parser accepts — an export that silently could
    /// not be read back.
    #[test]
    fn a_control_character_in_a_claim_survives_the_round_trip() {
        let store = InMemoryClaimStore::default();
        store
            .store_claim(
                "a bell \u{7} and a vertical tab \u{b} and a tab \t",
                StoreOptions {
                    source: None,
                    confidence: None,
                    according_to: None,
                },
            )
            .expect("stores");
        let segment = store.segment_nquads().expect("serializes");
        purrdf::parse_dataset(segment.as_bytes(), "application/n-quads", None)
            .expect("a segment carrying a control character must still parse");

        let seeded = InMemoryClaimStore::default();
        seed_claim_store(&seeded, &segment).expect("seeds");
        assert_eq!(
            seeded.claims().expect("reads")[0].text,
            "a bell \u{7} and a vertical tab \u{b} and a tab \t"
        );
        assert_eq!(seeded.segment_nquads().expect("re-serializes"), segment);
    }

    /// A record the transport cannot carry is a HARD FAIL naming the field, never a
    /// silently dropped edge — the difference between an export that refuses and an export
    /// that ships an incomplete snapshot.
    #[test]
    fn a_record_the_transport_cannot_carry_fails_naming_the_field() {
        let calls = [ToolCallRecord {
            id: "urn:gmeow:mcp:call:0".to_owned(),
            tool: "not an iri".to_owned(),
            arguments: None,
            result: None,
            invocation: None,
            created: None,
            generated: Vec::new(),
        }];
        let diag = claim_segment(&[], &calls).expect_err("a non-IRI tool must be refused");
        let message = format!("{diag:?}");
        assert!(
            message.contains("gmeow:usedTool"),
            "the refusal must name the field: {message}"
        );
    }
}
