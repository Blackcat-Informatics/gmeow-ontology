// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The carrier-borne diagnostic ledger: a hash-consed, append-only arena DAG.
//!
//! Every diagnostic that lands on the ledger is *content-addressed*: its
//! [`DiagFingerprint`] is `blake3` over `(code, category, source-context anchor,
//! focus)` — never the message or context frames (invariant 6). Content address
//! IS identity: two diagnostics with the same fingerprint are the same witness,
//! so attaching the second **merges** into the first rather than appending a
//! duplicate. The merge is order-independent — severity and standpoint take the
//! `⊑_t` lattice join, the [`Belnap`] knowledge values `⊑_k`-join, and the
//! producing `stage` collapses to the lexicographic minimum by id — so the
//! ledger, and the `(stage, fingerprint)` total order
//! [`emit_sorted`](DiagLedger::emit_sorted) keys on, is byte-stable under any
//! parallel fold order. DAG edges are stored as
//! content-addressed fingerprints, never in-process [`DiagRef`] handles, so the
//! serialized form encodes no arena index.
//!
//! Hard fails (no-optionality): a node whose stored fingerprint contradicts the
//! fingerprint recomputed from its own identity fields (a corrupt pin); a
//! fingerprint collision with a differing identity; a `DiagRef` arena overflow;
//! and an antecedent edge that makes a node its own ancestor (a cycle — the
//! witness structure must be a DAG).

use std::collections::{HashMap, HashSet};
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::diag::{
    Advice, Diag, DiagRef, Guidance, Label, Remediation, Slot, SourceContext, StageId,
};
use crate::grade::{Belnap, BoundedLattice, FindingCategory, GateVerdict, Grade, gate};
use crate::lower::lower;
use crate::model::DiagnosticAttribution;

/// A content-address fingerprint: `blake3` over the diagnostic's identity fields,
/// truncated to 16 bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct DiagFingerprint([u8; 16]);

/// Length-prefixed, domain-separated field feed — a length prefix before every
/// field makes cross-field delimiter-injection collisions impossible (R2).
fn feed(hasher: &mut blake3::Hasher, tag: &[u8], bytes: &[u8]) {
    hasher.update(&(tag.len() as u64).to_le_bytes());
    hasher.update(tag);
    hasher.update(&(bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

/// Feed the **code-blind source-position identity** — `(path, line, column,
/// logical, term_role, focus)` in this EXACT order — into `hasher`. The SINGLE
/// definition of the anchor field-feed, shared by [`DiagFingerprint::compute`]
/// (after its `code`+`category` prefix) and [`DiagFingerprint::anchor`], so the two
/// paths are structurally identical and cannot silently drift apart — a drift would
/// break the cross-node join key the glut meta-rule depends on.
fn feed_source_ctx(hasher: &mut blake3::Hasher, ctx: &SourceContext) {
    feed(
        hasher,
        b"path",
        ctx.location.path.as_deref().unwrap_or("").as_bytes(),
    );
    feed(
        hasher,
        b"line",
        &ctx.location.line.unwrap_or(0).to_le_bytes(),
    );
    feed(
        hasher,
        b"column",
        &ctx.location.column.unwrap_or(0).to_le_bytes(),
    );
    feed(
        hasher,
        b"logical",
        ctx.location.logical.as_deref().unwrap_or("").as_bytes(),
    );
    let role = ctx.term_role.map(|r| format!("{r:?}")).unwrap_or_default();
    feed(hasher, b"role", role.as_bytes());
    feed(
        hasher,
        b"focus",
        ctx.focus
            .as_ref()
            .map(|f| f.0.as_str())
            .unwrap_or("")
            .as_bytes(),
    );
}

impl DiagFingerprint {
    /// Compute the fingerprint from the identity fields — `(code, category,
    /// source-context anchor, focus)`. Never keys on message/frames/grade.
    pub fn compute(code: &str, category: FindingCategory, ctx: &SourceContext) -> Self {
        let mut hasher = blake3::Hasher::new();
        feed(&mut hasher, b"code", code.as_bytes());
        feed(&mut hasher, b"category", category.as_str().as_bytes());
        feed_source_ctx(&mut hasher, ctx);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        DiagFingerprint(bytes)
    }

    /// The **code-blind source anchor** fingerprint: `blake3` over the source
    /// position `(path, line, column, logical, term_role, focus)` ONLY —
    /// deliberately EXCLUDING `code` and `category`. Two findings with DIFFERENT
    /// codes at the SAME source anchor therefore share one anchor fingerprint,
    /// which is the cross-node join key the same-fingerprint merge (which keys on
    /// the code) structurally cannot make — the seam the cross-node-glut meta-rule
    /// joins on.
    pub fn anchor(ctx: &SourceContext) -> Self {
        let mut hasher = blake3::Hasher::new();
        feed_source_ctx(&mut hasher, ctx);
        let digest = hasher.finalize();
        let mut bytes = [0u8; 16];
        bytes.copy_from_slice(&digest.as_bytes()[..16]);
        DiagFingerprint(bytes)
    }

    /// Lowercase hex spelling, for the stable finding IRI.
    pub fn hex(&self) -> String {
        use std::fmt::Write;
        let mut out = String::with_capacity(32);
        for byte in self.0 {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}

impl fmt::Display for DiagFingerprint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.hex())
    }
}

/// The stable finding IRI for a fingerprint (addressable by `gmeow explain`).
pub fn fingerprint_iri(fingerprint: &DiagFingerprint) -> String {
    format!(
        "https://blackcatinformatics.ca/gmeow/diagnostics/finding/{}",
        fingerprint.hex()
    )
}

/// The stable **anchor IRI** for a code-blind [`anchor`](DiagFingerprint::anchor)
/// fingerprint — the `gmeow:findingAnchor` value two different-code findings at
/// one source position share.
pub fn anchor_iri(fingerprint: &DiagFingerprint) -> String {
    format!(
        "https://blackcatinformatics.ca/gmeow/diagnostics/anchor/{}",
        fingerprint.hex()
    )
}

/// One flattened link of the source chain (or a context frame), captured once at
/// lowering time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerFrame {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<SerLocation>,
}

/// A serialized Rust source location (the emit site / a context-frame site).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SerLocation {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

impl SerLocation {
    pub fn from_caller(loc: &std::panic::Location<'_>) -> Self {
        SerLocation {
            file: loc.file().to_owned(),
            line: loc.line(),
            column: loc.column(),
        }
    }
}

/// One distinct observation carried by a node. Merging preserves every distinct
/// observation (a multiset) rather than overwriting — no silent data loss when
/// two findings share an anchor but observed different values (R1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Observation {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observed: Option<Slot>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected: Option<Slot>,
}

/// A lowered, serializable, content-addressed diagnostic node. This — never the
/// live [`Diag`] — is what is cached, replayed, ordered, and projected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagNode {
    pub fingerprint: DiagFingerprint,
    pub stage: StageId,
    pub grade: Grade,
    pub code: String,
    pub observations: Vec<Observation>,
    pub frames: Vec<SerFrame>,
    /// Content-addressed DAG edges — never in-process handles.
    pub antecedents: Box<[DiagFingerprint]>,
    pub source_ctx: SourceContext,
    pub attributions: Vec<DiagnosticAttribution>,
    pub advice: Vec<Advice>,
    /// registry-authored remediations — the "how to fix" payload projected as
    /// `gmeow:findingRemediation`. Not part of the identity fingerprint (like
    /// [`advice`](DiagNode::advice)), so a later annotation pass can append one to
    /// an interned node without changing its content address.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remediation: Vec<Remediation>,
    /// Per-term usage guidance (howToUse/useWhen/avoidWhen) joined from the bundle
    /// documentation graph. Not part of the identity fingerprint (like
    /// [`remediation`](DiagNode::remediation)).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub guidance: Vec<Guidance>,
    /// The logic-world quad-reifier IRIs this witness's verdict derives FROM
    /// (`gmeow:findingDerivedFromQuad`). Not part of the identity fingerprint.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub derived_from_quads: Vec<String>,
    pub labels: Vec<Label>,
    pub tags: Vec<String>,
    /// The DOCUMENTED ontology terms this witness structurally concerns — payload,
    /// NOT part of the identity fingerprint (like [`tags`](DiagNode::tags)), so a
    /// SHACL violation can attribute to its constrained property without perturbing
    /// its content address. Projected onto
    /// [`Finding::documented_terms`](crate::model::Finding) for the docs per-term
    /// diagnostics join. `skip_serializing_if` keeps it out of the node wire form
    /// when empty so non-attributed nodes are byte-unchanged.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub documented_terms: Vec<String>,
    /// The TYPED conformance-failure class this witness instantiates (the IRI the
    /// violated law declares through `gmeow:enforcesFailureClass`) — payload, NOT
    /// part of the identity fingerprint, projected onto
    /// [`Finding::failure_class`](crate::model::Finding). `skip_serializing_if` keeps
    /// it out of the node wire form when absent so class-less nodes are byte-unchanged.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_class: Option<String>,
    /// The Belnap knowledge value; [`Belnap::Both`] flags a merged glut.
    pub knowledge: Belnap,
    pub emitted_at: SerLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locus_stage: Option<String>,
}

impl DiagNode {
    /// The fingerprint recomputed from this node's own identity fields. Must equal
    /// the stored [`fingerprint`](DiagNode::fingerprint) — otherwise the node
    /// contradicts its pinned digest.
    fn recomputed_fingerprint(&self) -> DiagFingerprint {
        DiagFingerprint::compute(&self.code, self.grade.category, &self.source_ctx)
    }

    /// Whether this node is a merged glut (contradictory witnesses).
    pub fn is_glut(&self) -> bool {
        self.knowledge.is_glut()
    }
}

/// A recorded operation rejection, lowered once at the artifact boundary.
/// Unlike a message string, this retains typed grade, standpoint, source context,
/// provenance, advice and every causal edge. It is observation data; it cannot be
/// converted back into a live [`Diag`] or used as a checked execution result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedDiag {
    /// The operation's root rejection, including all distinct observations.
    pub root: Box<DiagNode>,
    /// Its complete causal closure, excluding the root, in deterministic order.
    pub causes: Vec<DiagNode>,
}

impl fmt::Display for RecordedDiag {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, observation) in self.root.observations.iter().enumerate() {
            if index != 0 {
                f.write_str("; ")?;
            }
            f.write_str(&observation.message)?;
        }
        if f.alternate() {
            for frame in &self.root.frames {
                write!(f, "\n  {}", frame.message)?;
            }
            for cause in &self.causes {
                for observation in &cause.observations {
                    write!(f, "\n  {}: {}", cause.code, observation.message)?;
                }
            }
        }
        Ok(())
    }
}

/// The hash-consed, append-only arena DAG. No `Arc`: the ledger is single-owner
/// and folded single-threaded at each stage join.
#[derive(Debug, Default, Clone)]
pub struct DiagLedger {
    arena: Vec<DiagNode>,
    intern: HashMap<DiagFingerprint, DiagRef>,
}

impl DiagLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.arena.len()
    }

    pub fn is_empty(&self) -> bool {
        self.arena.is_empty()
    }

    /// Attach a live diagnostic, stamping the producing stage (pin-on-attach). The
    /// diagnostic is lowered here — exactly once. Returns the handle to the interned
    /// node (the existing one on a fingerprint collision).
    pub fn attach(&mut self, diag: Diag, stage: StageId) -> DiagRef {
        // Resolve in-process antecedent handles to content-addressed fingerprints.
        let arena = &self.arena;
        let node = lower(&diag, stage, |r: DiagRef| arena[r.index()].fingerprint);
        self.insert(node)
    }

    /// Record one operation's rejection at the serialization boundary, retaining
    /// its complete causal subgraph. Antecedent handles must belong to this ledger,
    /// exactly as for [`attach`](Self::attach). Unrelated observations are excluded.
    /// The resulting data is not a live error or an execution certificate.
    pub fn record(&mut self, diag: Diag, stage: StageId) -> RecordedDiag {
        let root = self.attach(diag, stage);
        let root = self.arena[root.index()].clone();
        let mut pending = root.antecedents.to_vec();
        let mut seen = HashSet::new();
        let mut causes = Vec::new();
        while let Some(fingerprint) = pending.pop() {
            if !seen.insert(fingerprint) {
                continue;
            }
            let node = self
                .node_by_fingerprint(&fingerprint)
                .expect("attached diagnostic has a complete causal ledger");
            pending.extend(node.antecedents.iter().copied());
            causes.push(node.clone());
        }
        causes.sort_by(|a, b| (&a.stage, a.fingerprint).cmp(&(&b.stage, b.fingerprint)));
        RecordedDiag {
            root: Box::new(root),
            causes,
        }
    }

    /// Replay pre-lowered nodes from a cache hit — idempotent, never re-lowers.
    /// Fresh and replayed nodes are byte-identical, so replay yields the same
    /// ledger as a fresh fold. Replayed nodes are trusted: each was cycle-checked
    /// when it was first attached and the cache surface is byte-identical, so the
    /// redundant per-node ancestor traversal is skipped (replaying N nodes would
    /// otherwise cost O(N²)). The F1 pin-digest self-consistency check still runs
    /// on every replayed node.
    pub fn replay(&mut self, nodes: impl IntoIterator<Item = DiagNode>) {
        for node in nodes {
            self.insert_inner(node, false);
        }
    }

    /// Resolve an interned node by its content address, without mutating. `None`
    /// when no witness with that fingerprint has landed.
    pub fn node_by_fingerprint(&self, fingerprint: &DiagFingerprint) -> Option<&DiagNode> {
        self.intern
            .get(fingerprint)
            .map(|&dref| &self.arena[dref.index()])
    }

    /// Attach a [`Remediation`] to an already-interned witness, addressed by its
    /// content-address fingerprint — the D1 annotate-by-fingerprint seam a later
    /// pass uses to hang "how to fix" guidance on a finding the producers already
    /// emitted. This is deliberately NOT the [`attach`](DiagLedger::attach) merge
    /// path: it appends to the existing node in place and does not re-lower,
    /// re-fingerprint, or fold in a new witness. Because remediation is not part of
    /// the [`DiagFingerprint`], the node's content address is unchanged and the F1
    /// pin-digest self-consistency invariant stays valid. It is **idempotent**:
    /// appending a remediation the node already carries is a no-op, so a cache
    /// replay or a second annotation pass never grows the vec. Returns the handle
    /// to the annotated node, or `None` when no witness with that fingerprint
    /// exists (annotation of an absent finding is a caller error to surface, never
    /// a silent create).
    pub fn annotate(
        &mut self,
        fingerprint: &DiagFingerprint,
        remediation: Remediation,
    ) -> Option<DiagRef> {
        let &dref = self.intern.get(fingerprint)?;
        let node = &mut self.arena[dref.index()];
        if !node.remediation.contains(&remediation) {
            node.remediation.push(remediation);
        }
        Some(dref)
    }

    /// Attach an [`Advice`] to an already-interned witness by content address — the
    /// [`Advice`] twin of [`annotate`](DiagLedger::annotate). Same discipline: in
    /// place, no merge, no re-fingerprint, idempotent (dedup by equality), and
    /// `None` when the finding is absent.
    pub fn annotate_advice(
        &mut self,
        fingerprint: &DiagFingerprint,
        advice: Advice,
    ) -> Option<DiagRef> {
        let &dref = self.intern.get(fingerprint)?;
        let node = &mut self.arena[dref.index()];
        if !node.advice.contains(&advice) {
            node.advice.push(advice);
        }
        Some(dref)
    }

    /// The nodes in total deterministic order: `(stage, fingerprint)`. Because the
    /// arena's insertion order is never used here, the emitted sequence is
    /// byte-stable under any parallel fold interleaving.
    pub fn emit_sorted(&self) -> Vec<&DiagNode> {
        let mut refs: Vec<&DiagNode> = self.arena.iter().collect();
        refs.sort_by(|a, b| {
            (a.stage.as_str(), &a.fingerprint).cmp(&(b.stage.as_str(), &b.fingerprint))
        });
        refs
    }

    /// **The** aggregate gate verdict of the whole ledger: the `⊔` join-fold of
    /// the single [`gate`] policy morphism over every interned witness. This is
    /// the one operation every verdict surface reduces to — the validate report's
    /// `ok()`, a scoreboard's gate result, the conformance divergence verdict —
    /// so a Fatal witness anywhere makes the ledger Fatal, and an empty ledger is
    /// [`Collected`](GateVerdict::Collected) (the bottom). Being a fold of a
    /// monotone map through the [`GateVerdict`] semilattice join, it is a
    /// semilattice homomorphism: `verdict(a ∪ b) == verdict(a) ⊔ verdict(b)`, so
    /// folding stage sub-ledgers in parallel and joining their verdicts equals the
    /// verdict of the whole — the algebraic reason parallel stage scheduling is
    /// sound (proved in the tests).
    pub fn verdict(&self) -> GateVerdict {
        self.arena
            .iter()
            .map(|n| gate(n.grade))
            .fold(GateVerdict::Collected, GateVerdict::join)
    }

    /// Fold another ledger's witnesses into this one — the state-based CRDT union.
    /// Every node of `other` is re-attached, hash-consing by content address, so a
    /// shared anchor merges by the same order-independent `⊑_t`/`⊑_k` joins a fresh
    /// fold would apply. The ledger state is therefore a join-semilattice: union is
    /// commutative, associative, and idempotent (proved exhaustively in the tests),
    /// which is what lets the parallel scheduler fold shards in any order and still
    /// reach a byte-identical ledger.
    pub fn union(&mut self, other: &DiagLedger) {
        // `emit_sorted` gives a deterministic, arena-index-free order; each node is
        // re-inserted through the same hash-consing merge path as `attach`.
        for node in other.emit_sorted() {
            self.insert(node.clone());
        }
    }

    /// Attach a lowered node on the trust-checking path: verifies the F1 pin
    /// digest AND that the resulting node introduces no cycle.
    fn insert(&mut self, node: DiagNode) -> DiagRef {
        self.insert_inner(node, true)
    }

    /// Attach a lowered node. The F1 pin-digest self-consistency check always
    /// runs; `check_cycles` additionally runs the O(ancestors) acyclicity walk.
    /// [`replay`](DiagLedger::replay) passes `false` for cached, already-validated
    /// nodes — re-checking them would make replay O(N²).
    fn insert_inner(&mut self, node: DiagNode, check_cycles: bool) -> DiagRef {
        // F1: a node must be self-consistent — its stored fingerprint must equal
        // the fingerprint recomputed from its identity fields.
        assert!(
            node.recomputed_fingerprint() == node.fingerprint,
            "diagnostic node contradicts its pinned digest: stored {} != recomputed {} for code `{}`",
            node.fingerprint,
            node.recomputed_fingerprint(),
            node.code
        );

        if let Some(&existing) = self.intern.get(&node.fingerprint) {
            self.merge_into(existing, node);
            if check_cycles {
                self.assert_acyclic(existing);
            }
            return existing;
        }

        let dref = DiagRef::from_index(self.arena.len());
        self.intern.insert(node.fingerprint, dref);
        self.arena.push(node);
        if check_cycles {
            self.assert_acyclic(dref);
        }
        dref
    }

    fn merge_into(&mut self, existing: DiagRef, incoming: DiagNode) {
        let slot = &mut self.arena[existing.index()];
        // Same fingerprint MUST mean same identity — otherwise a hash collision or
        // corruption is contradicting the pinned digest.
        assert!(
            slot.code == incoming.code && slot.grade.category == incoming.grade.category,
            "fingerprint collision with differing identity: `{}` vs `{}`",
            slot.code,
            incoming.code
        );
        // ⊑_t truth-join the grade (order-independent); ⊑_k-join the knowledge so a
        // contradiction surfaces as a glut instead of an overwrite.
        slot.grade = slot.grade.merge(incoming.grade).grade;
        slot.knowledge = slot.knowledge.join(incoming.knowledge);
        // Collapse the producing stage to the lexicographic minimum by id — a
        // total, attach-order-independent choice — so the `(stage, fingerprint)`
        // key emit_sorted orders on cannot depend on which stage attached first.
        if incoming.stage.as_str() < slot.stage.as_str() {
            slot.stage = incoming.stage;
        }
        // Multiset-merge observations — never silently drop a distinct observation.
        for obs in incoming.observations {
            if !slot.observations.contains(&obs) {
                slot.observations.push(obs);
            }
        }
        // Union the content-addressed antecedent edges.
        let mut edges = slot.antecedents.to_vec();
        for fp in incoming.antecedents.into_vec() {
            if !edges.contains(&fp) {
                edges.push(fp);
            }
        }
        slot.antecedents = edges.into_boxed_slice();
        for attribution in incoming.attributions {
            if !slot.attributions.contains(&attribution) {
                slot.attributions.push(attribution);
            }
        }
        for advice in incoming.advice {
            if !slot.advice.contains(&advice) {
                slot.advice.push(advice);
            }
        }
        for remediation in incoming.remediation {
            if !slot.remediation.contains(&remediation) {
                slot.remediation.push(remediation);
            }
        }
        for guidance in incoming.guidance {
            if !slot.guidance.contains(&guidance) {
                slot.guidance.push(guidance);
            }
        }
        for quad in incoming.derived_from_quads {
            if !slot.derived_from_quads.contains(&quad) {
                slot.derived_from_quads.push(quad);
            }
        }
        for label in incoming.labels {
            if !slot.labels.contains(&label) {
                slot.labels.push(label);
            }
        }
        for tag in incoming.tags {
            if !slot.tags.contains(&tag) {
                slot.tags.push(tag);
            }
        }
        // Union the documented-term attributions — two witnesses hash-consing onto
        // one anchor may each concern a distinct documented term; keep them all.
        for term in incoming.documented_terms {
            if !slot.documented_terms.contains(&term) {
                slot.documented_terms.push(term);
            }
        }
        // The typed failure class. Two witnesses hash-cons onto ONE node when they
        // share `(code, category, anchor)` — which for SHACL means two DISTINCT laws
        // raising the SAME generic component code at the SAME focus node, so the two
        // may legitimately declare different classes. The slot keeps the
        // lexicographic minimum (a total, attach-order-independent choice, matching
        // the stage collapse above) and the loser is NOT dropped: it rides on as a
        // context frame, which `to_finding` folds into the finding detail — the same
        // treatment a merged witness's extra observations get.
        match (&mut slot.failure_class, incoming.failure_class) {
            (slot_class @ None, incoming_class) => *slot_class = incoming_class,
            (Some(_), None) => {}
            (Some(resident), Some(incoming_class)) if *resident == incoming_class => {}
            (Some(resident), Some(incoming_class)) => {
                let (kept, folded) = if incoming_class < *resident {
                    let displaced = std::mem::replace(resident, incoming_class);
                    (resident.clone(), displaced)
                } else {
                    (resident.clone(), incoming_class)
                };
                debug_assert_ne!(kept, folded);
                let note = format!("also enforces failure class {folded}");
                if !slot.frames.iter().any(|f| f.message == note) {
                    slot.frames.push(SerFrame {
                        message: note,
                        at: None,
                    });
                }
            }
        }
    }

    /// A node must not be its own ancestor — the witness structure is a DAG.
    fn assert_acyclic(&self, start: DiagRef) {
        let start_fp = self.arena[start.index()].fingerprint;
        let mut work: Vec<DiagFingerprint> = self.arena[start.index()].antecedents.to_vec();
        let mut seen: HashSet<DiagFingerprint> = HashSet::new();
        while let Some(fp) = work.pop() {
            assert!(
                fp != start_fp,
                "cycle in diagnostic DAG: node `{start_fp}` is its own antecedent"
            );
            if !seen.insert(fp) {
                continue;
            }
            if let Some(&r) = self.intern.get(&fp) {
                work.extend(self.arena[r.index()].antecedents.iter().copied());
            }
        }
    }
}

#[path = "ledger.tests.rs"]
#[cfg(test)]
mod tests;
