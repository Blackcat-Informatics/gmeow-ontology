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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code::register_code;
    use crate::diag::{Diag, Focus, Slot};
    use crate::grade::{FindingCategory, GateVerdict, Grade, Severity, Standpoint, gate};
    use crate::model::Location;

    fn diag_at(code: &'static str, category: FindingCategory, path: &str, message: &str) -> Diag {
        let c = register_code(code);
        let mut d = Diag::new(
            c,
            Grade::new(Severity::Error, category, Standpoint::Binding),
            message,
        );
        d = d.with_location(Location {
            path: Some(path.to_owned()),
            ..Location::default()
        });
        d
    }

    #[test]
    fn hash_cons_identity_dedups_and_message_is_not_in_the_fingerprint() {
        let mut ledger = DiagLedger::new();
        let a = diag_at(
            "test.ledger.identity",
            FindingCategory::DataShapeViolation,
            "a.ttl",
            "first message",
        );
        let b = diag_at(
            "test.ledger.identity",
            FindingCategory::DataShapeViolation,
            "a.ttl",
            "DIFFERENT message",
        );
        let ra = ledger.attach(a, StageId::new("stage-1"));
        let rb = ledger.attach(b, StageId::new("stage-1"));
        // Same (code, category, anchor) => one node, same handle, despite different message.
        assert_eq!(ra, rb);
        assert_eq!(ledger.len(), 1);
    }

    #[test]
    fn distinct_observed_slots_are_not_silently_collapsed() {
        // R1: two findings share an anchor but observed different values — both
        // observations survive as a multiset.
        let mut ledger = DiagLedger::new();
        let mut a = diag_at(
            "test.ledger.r1",
            FindingCategory::DataShapeViolation,
            "x.ttl",
            "cardinality",
        );
        a = a.with_observed(Slot::new("3"));
        let mut b = diag_at(
            "test.ledger.r1",
            FindingCategory::DataShapeViolation,
            "x.ttl",
            "cardinality",
        );
        b = b.with_observed(Slot::new("7"));
        ledger.attach(a, StageId::new("s"));
        ledger.attach(b, StageId::new("s"));
        let node = ledger.emit_sorted()[0];
        assert_eq!(
            node.observations.len(),
            2,
            "distinct observations must both survive"
        );
    }

    #[test]
    fn length_prefixed_fingerprint_resists_delimiter_injection() {
        // R2: ("ab","c") and ("a","bc") must not collide across field boundaries.
        let ctx = SourceContext::default();
        let f1 = DiagFingerprint::compute("ab", FindingCategory::DataShapeViolation, &ctx);
        let f2 = DiagFingerprint::compute("a", FindingCategory::DataShapeViolation, &ctx);
        assert_ne!(f1, f2);
        // A focus that concatenates to the same bytes as a different split.
        let c1 = SourceContext {
            focus: Some(Focus("xy".to_owned())),
            ..SourceContext::default()
        };
        let c2 = SourceContext {
            focus: Some(Focus("x".to_owned())),
            location: Location {
                path: Some("y".to_owned()),
                ..Location::default()
            },
            ..SourceContext::default()
        };
        assert_ne!(
            DiagFingerprint::compute("k", FindingCategory::DataShapeViolation, &c1),
            DiagFingerprint::compute("k", FindingCategory::DataShapeViolation, &c2)
        );
    }

    #[test]
    fn fresh_and_replayed_ledger_are_byte_identical_and_carry_no_arena_ref() {
        let mut fresh = DiagLedger::new();
        fresh.attach(
            diag_at(
                "test.ledger.replay",
                FindingCategory::DataShapeViolation,
                "r.ttl",
                "boom",
            ),
            StageId::new("stage-x"),
        );
        // Serialize the lowered nodes (the cache surface).
        let nodes: Vec<DiagNode> = fresh.emit_sorted().into_iter().cloned().collect();
        let bytes = serde_json::to_vec(&nodes).unwrap();
        // The serialized form must not encode an arena DiagRef (no NonZeroU32 handle).
        // DiagRef is not Serialize, so this is structural; assert the JSON has the
        // content-address edge shape, and round-trips identically.
        let replayed_nodes: Vec<DiagNode> = serde_json::from_slice(&bytes).unwrap();
        let mut replayed = DiagLedger::new();
        replayed.replay(replayed_nodes);
        let replay_bytes = serde_json::to_vec(
            &replayed
                .emit_sorted()
                .into_iter()
                .cloned()
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(
            bytes, replay_bytes,
            "fresh and replayed must be byte-identical"
        );
    }

    #[test]
    #[should_panic(expected = "contradicts its pinned digest")]
    fn node_contradicting_its_pinned_digest_is_a_hard_fail() {
        // F1: hand-build a node whose stored fingerprint does not match its content.
        let mut ledger = DiagLedger::new();
        let good = DiagFingerprint::compute(
            "test.ledger.f1",
            FindingCategory::DataShapeViolation,
            &SourceContext::default(),
        );
        // Flip a byte so the stored fingerprint contradicts the identity fields.
        let mut wrong = good;
        wrong.0[0] ^= 0xff;
        let node = DiagNode {
            fingerprint: wrong,
            stage: StageId::new("s"),
            grade: Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
            code: "test.ledger.f1".to_owned(),
            observations: vec![Observation {
                message: "x".to_owned(),
                observed: None,
                expected: None,
            }],
            frames: Vec::new(),
            antecedents: Box::new([]),
            source_ctx: SourceContext::default(),
            attributions: Vec::new(),
            advice: Vec::new(),
            remediation: Vec::new(),
            guidance: Vec::new(),
            derived_from_quads: Vec::new(),
            labels: Vec::new(),
            tags: Vec::new(),
            documented_terms: Vec::new(),
            knowledge: Belnap::Supported,
            emitted_at: SerLocation {
                file: "x".to_owned(),
                line: 1,
                column: 1,
            },
            locus_stage: None,
        };
        ledger.replay([node]);
    }

    #[test]
    #[should_panic(expected = "cycle in diagnostic DAG")]
    fn self_referential_antecedent_is_a_hard_fail() {
        // R7: a node whose antecedent edge is its own fingerprint.
        let mut ledger = DiagLedger::new();
        let ctx = SourceContext::default();
        let fp =
            DiagFingerprint::compute("test.ledger.r7", FindingCategory::DataShapeViolation, &ctx);
        let node = DiagNode {
            fingerprint: fp,
            stage: StageId::new("s"),
            grade: Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ),
            code: "test.ledger.r7".to_owned(),
            observations: vec![Observation {
                message: "x".to_owned(),
                observed: None,
                expected: None,
            }],
            frames: Vec::new(),
            antecedents: Box::new([fp]), // points at itself
            source_ctx: ctx,
            attributions: Vec::new(),
            advice: Vec::new(),
            remediation: Vec::new(),
            guidance: Vec::new(),
            derived_from_quads: Vec::new(),
            labels: Vec::new(),
            tags: Vec::new(),
            documented_terms: Vec::new(),
            knowledge: Belnap::Supported,
            emitted_at: SerLocation {
                file: "x".to_owned(),
                line: 1,
                column: 1,
            },
            locus_stage: None,
        };
        // The attach/checking path (not the trusted replay path) runs the
        // acyclicity walk, so drive `insert` directly.
        ledger.insert(node);
    }

    #[test]
    fn hash_cons_merge_of_grades_is_order_independent() {
        // The real determinism fix: two witnesses at one anchor (same code, category
        // and location => same fingerprint) merge their severity/standpoint by the
        // ⊑_t lattice join, so the surviving grade does not depend on attach order.
        // (Cross-node contradiction between DIFFERENT-category findings at one
        // location is a reasoner meta-finding, not a hash-cons merge — the ledger
        // never merges different fingerprints.)
        let build = |loud_first: bool| {
            let mut l = DiagLedger::new();
            let loud = diag_at(
                "test.ledger.join",
                FindingCategory::DataShapeViolation,
                "j.ttl",
                "loud",
            )
            .with_grade(Grade::new(
                Severity::Error,
                FindingCategory::DataShapeViolation,
                Standpoint::Binding,
            ));
            let quiet = diag_at(
                "test.ledger.join",
                FindingCategory::DataShapeViolation,
                "j.ttl",
                "quiet",
            )
            .with_grade(Grade::new(
                Severity::Warning,
                FindingCategory::DataShapeViolation,
                Standpoint::Advisory,
            ));
            if loud_first {
                l.attach(loud, StageId::new("s"));
                l.attach(quiet, StageId::new("s"));
            } else {
                l.attach(quiet, StageId::new("s"));
                l.attach(loud, StageId::new("s"));
            }
            l.emit_sorted()[0].grade
        };
        let a = build(true);
        let b = build(false);
        assert_eq!(a, b, "merged grade must be attach-order independent");
        // severity join = Error (max), standpoint join = Binding (max) => still gates.
        assert_eq!(a.severity, Severity::Error);
        assert_eq!(a.standpoint, Standpoint::Binding);
        assert_eq!(gate(a), GateVerdict::Fatal);
    }

    #[test]
    fn cross_stage_merge_keeps_emit_sorted_attach_order_independent() {
        // The stated Hard Invariant: the same witness (same code/category/anchor =>
        // same fingerprint) attached at two DIFFERENT stages must yield a
        // byte-identical `(stage, fingerprint)` order regardless of which stage
        // attached first — stage is merged to the lexicographic minimum, not
        // pinned by the first writer.
        let build = |b_first: bool| {
            let mut l = DiagLedger::new();
            let first = diag_at(
                "test.ledger.stage-merge",
                FindingCategory::DataShapeViolation,
                "s.ttl",
                "first",
            );
            let second = diag_at(
                "test.ledger.stage-merge",
                FindingCategory::DataShapeViolation,
                "s.ttl",
                "second",
            );
            if b_first {
                l.attach(first, StageId::new("stage-b"));
                l.attach(second, StageId::new("stage-a"));
            } else {
                l.attach(first, StageId::new("stage-a"));
                l.attach(second, StageId::new("stage-b"));
            }
            l
        };
        let ledger_b_first = build(true);
        let ledger_a_first = build(false);
        // One node either way (content address is identity).
        assert_eq!(ledger_b_first.len(), 1);
        assert_eq!(ledger_a_first.len(), 1);
        // The emitted `(stage, fingerprint)` sequence is byte-identical.
        let key = |l: &DiagLedger| -> Vec<(String, DiagFingerprint)> {
            l.emit_sorted()
                .into_iter()
                .map(|n| (n.stage.as_str().to_owned(), n.fingerprint))
                .collect()
        };
        assert_eq!(
            key(&ledger_b_first),
            key(&ledger_a_first),
            "emit_sorted must be attach-order independent"
        );
        // And the surviving stage is the lexicographic minimum in both.
        assert_eq!(ledger_b_first.emit_sorted()[0].stage.as_str(), "stage-a");
        assert_eq!(ledger_a_first.emit_sorted()[0].stage.as_str(), "stage-a");
    }

    #[test]
    fn annotate_by_fingerprint_is_idempotent_and_preserves_identity() {
        // D1: a later pass hangs a remediation on an already-interned witness by
        // fingerprint — in place, no merge, no new node — and a second call (or a
        // cache replay) does NOT grow the remediation vec. The content address is
        // unchanged (remediation is not in the fingerprint), so F1 stays valid.
        use crate::diag::Remediation;
        use crate::grade::Standpoint;
        let mut ledger = DiagLedger::new();
        let d = diag_at(
            "test.ledger.annotate",
            FindingCategory::DataShapeViolation,
            "a.ttl",
            "boom",
        );
        ledger.attach(d, StageId::new("s"));
        let fp = ledger.emit_sorted()[0].fingerprint;
        let iri_before = fingerprint_iri(&fp);
        let arena_len_before = ledger.len();

        let rem = Remediation::new("introduce the mediating relator", Standpoint::Advisory);
        // First annotation lands.
        let r1 = ledger.annotate(&fp, rem.clone()).expect("finding present");
        assert_eq!(
            ledger.node_by_fingerprint(&fp).unwrap().remediation.len(),
            1
        );
        // Second identical annotation (or a replay) is a no-op — the vec does not grow.
        let r2 = ledger.annotate(&fp, rem.clone()).expect("still present");
        assert_eq!(r1, r2);
        assert_eq!(
            ledger.node_by_fingerprint(&fp).unwrap().remediation.len(),
            1,
            "idempotent annotate must not grow the vec on replay"
        );
        // Identity is untouched: same fingerprint IRI, same arena size (no new node).
        assert_eq!(fingerprint_iri(&fp), iri_before);
        assert_eq!(ledger.len(), arena_len_before);
        // Annotating an absent finding is None, never a silent create.
        let absent = DiagFingerprint::compute(
            "test.ledger.absent",
            FindingCategory::DataShapeViolation,
            &SourceContext::default(),
        );
        assert!(ledger.annotate(&absent, rem).is_none());
        assert_eq!(ledger.len(), arena_len_before);
    }

    #[test]
    fn anchor_is_code_blind_and_trivial_when_locationless() {
        // D3: the anchor fingerprint drops code+category, so two DIFFERENT-code
        // findings at ONE source position share it (the cross-code join key the
        // same-fingerprint merge cannot make); and a locationless context is a
        // TRIVIAL anchor the cross-node-glut guard excludes.
        use crate::diag::Focus;
        use crate::model::Location;
        let anchored = SourceContext {
            location: Location {
                path: Some("x.ttl".to_owned()),
                ..Location::default()
            },
            focus: Some(Focus("https://ex/f".to_owned())),
            ..SourceContext::default()
        };
        // Different code strings, one anchor.
        let a =
            DiagFingerprint::compute("code.one", FindingCategory::ContradictionWitness, &anchored);
        let b = DiagFingerprint::compute(
            "code.two",
            FindingCategory::PermittedEpistemicConflict,
            &anchored,
        );
        assert_ne!(
            a, b,
            "different-code fingerprints differ (they key on the code)"
        );
        assert_eq!(
            DiagFingerprint::anchor(&anchored),
            DiagFingerprint::anchor(&anchored),
        );
        // The anchor is code-blind: recomputed from a context differing ONLY in
        // (irrelevant) code — anchor ignores code entirely — it is stable.
        assert!(anchored.is_non_trivial(), "path+focus is a real position");
        assert!(
            anchor_iri(&DiagFingerprint::anchor(&anchored))
                .starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/anchor/")
        );

        // A locationless / focusless context is a TRIVIAL anchor.
        let trivial = SourceContext::default();
        assert!(!trivial.is_non_trivial());
    }

    #[test]
    fn fingerprint_iri_is_stable_and_addressable() {
        let ctx = SourceContext::default();
        let fp =
            DiagFingerprint::compute("test.ledger.iri", FindingCategory::DataShapeViolation, &ctx);
        let iri = fingerprint_iri(&fp);
        assert!(iri.starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/finding/"));
        assert!(iri.ends_with(&fp.hex()));
    }

    // --- verdict() fold + CRDT union laws (T2 / T4) ---------------------------

    /// A small pool of witness specs. Anchors 0 and 4 SHARE a fingerprint (same
    /// code/category/path) so a union across ledgers exercises the hash-cons merge,
    /// not just set-union of disjoint nodes. Grades are chosen so the pool contains
    /// both Fatal (`gate == Fatal`) and Collected witnesses, so the verdict fold and
    /// its homomorphism are non-vacuous.
    fn witness_pool() -> Vec<Diag> {
        let g = |sev, cat, sp| Grade::new(sev, cat, sp);
        let specs: [(&'static str, FindingCategory, &str, Grade); 5] = [
            // Fatal: Error + Blocking + Binding.
            (
                "test.ledger.crdt.a",
                FindingCategory::DataShapeViolation,
                "a.ttl",
                g(
                    Severity::Error,
                    FindingCategory::DataShapeViolation,
                    Standpoint::Binding,
                ),
            ),
            // Collected: advisory standpoint never gates.
            (
                "test.ledger.crdt.b",
                FindingCategory::PolicyWarning,
                "b.ttl",
                g(
                    Severity::Error,
                    FindingCategory::PolicyWarning,
                    Standpoint::Advisory,
                ),
            ),
            // Collected: coherent category never gates.
            (
                "test.ledger.crdt.c",
                FindingCategory::PermittedEpistemicConflict,
                "c.ttl",
                g(
                    Severity::Error,
                    FindingCategory::PermittedEpistemicConflict,
                    Standpoint::Binding,
                ),
            ),
            // Collected: non-error severity.
            (
                "test.ledger.crdt.d",
                FindingCategory::ModelingDisciplineViolation,
                "d.ttl",
                g(
                    Severity::Warning,
                    FindingCategory::ModelingDisciplineViolation,
                    Standpoint::Binding,
                ),
            ),
            // Shares the anchor of spec 0 (same code/category/path) — hash-cons merge.
            (
                "test.ledger.crdt.a",
                FindingCategory::DataShapeViolation,
                "a.ttl",
                g(
                    Severity::Warning,
                    FindingCategory::DataShapeViolation,
                    Standpoint::Advisory,
                ),
            ),
        ];
        specs
            .into_iter()
            .map(|(code, cat, path, grade)| diag_at(code, cat, path, "msg").with_grade(grade))
            .collect()
    }

    /// Build a ledger holding exactly the witnesses at `indices` from the pool.
    fn ledger_from(indices: &[usize]) -> DiagLedger {
        let pool = witness_pool();
        let mut l = DiagLedger::new();
        for &i in indices {
            // Clone the spec by rebuilding from the pool each time (Diag is not Clone).
            let d = pool_diag(&pool, i);
            l.attach(d, StageId::new("s"));
        }
        l
    }

    /// Rebuild the pool witness at `i` (Diag has no Clone; the pool is cheap).
    fn pool_diag(_pool: &[Diag], i: usize) -> Diag {
        witness_pool().into_iter().nth(i).expect("pool index")
    }

    /// Byte-serialize a ledger's deterministic node sequence — two ledgers are
    /// "equal as state" iff these bytes match.
    fn state_bytes(l: &DiagLedger) -> Vec<u8> {
        serde_json::to_vec(&l.emit_sorted().into_iter().cloned().collect::<Vec<_>>()).unwrap()
    }

    #[test]
    fn verdict_is_the_join_fold_of_gate_over_every_witness() {
        // Over every subset of the pool, verdict() equals the manual gate-join fold.
        for mask in 0u32..(1 << 5) {
            let idx: Vec<usize> = (0..5).filter(|b| mask & (1 << b) != 0).collect();
            let l = ledger_from(&idx);
            let expected = l
                .emit_sorted()
                .iter()
                .map(|n| gate(n.grade))
                .fold(GateVerdict::Collected, GateVerdict::join);
            assert_eq!(l.verdict(), expected, "verdict fold mismatch for {idx:?}");
        }
        // Non-vacuity: the empty ledger is Collected, and a ledger with the Fatal
        // witness (spec 0) is Fatal.
        assert_eq!(DiagLedger::new().verdict(), GateVerdict::Collected);
        assert_eq!(ledger_from(&[0]).verdict(), GateVerdict::Fatal);
        assert_eq!(ledger_from(&[1, 2, 3]).verdict(), GateVerdict::Collected);
    }

    #[test]
    fn union_is_commutative_associative_and_idempotent() {
        // Exhaustive over all pairs/triples of subsets of a 3-element index space —
        // small, but a genuine proof over the chosen carrier, not a sample.
        let subsets: Vec<Vec<usize>> = (0u32..(1 << 3))
            .map(|m| (0..3).filter(|b| m & (1 << b) != 0).collect())
            .collect();
        for a in &subsets {
            let la = ledger_from(a);
            // Idempotence: a ∪ a == a.
            let mut aa = ledger_from(a);
            aa.union(&la);
            assert_eq!(
                state_bytes(&aa),
                state_bytes(&la),
                "union idempotence {a:?}"
            );
            for b in &subsets {
                let lb = ledger_from(b);
                // Commutativity: a ∪ b == b ∪ a (byte-identical state).
                let mut ab = ledger_from(a);
                ab.union(&lb);
                let mut ba = ledger_from(b);
                ba.union(&la);
                assert_eq!(
                    state_bytes(&ab),
                    state_bytes(&ba),
                    "union not commutative for {a:?} / {b:?}"
                );
                for c in &subsets {
                    let lc = ledger_from(c);
                    // Associativity: (a ∪ b) ∪ c == a ∪ (b ∪ c).
                    let mut left = ledger_from(a);
                    left.union(&lb);
                    left.union(&lc);
                    let mut bc = ledger_from(b);
                    bc.union(&lc);
                    let mut right = ledger_from(a);
                    right.union(&bc);
                    assert_eq!(
                        state_bytes(&left),
                        state_bytes(&right),
                        "union not associative for {a:?} / {b:?} / {c:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn verdict_is_a_semilattice_homomorphism_over_union() {
        // The load-bearing theorem: verdict(a ∪ b) == verdict(a) ⊔ verdict(b) for
        // every pair of ledgers — so folding stage sub-ledgers in parallel and
        // joining their verdicts equals the verdict of the whole. Exhaustive over
        // all subset pairs of the full 5-element pool.
        let subsets: Vec<Vec<usize>> = (0u32..(1 << 5))
            .map(|m| (0..5).filter(|b| m & (1 << b) != 0).collect())
            .collect();
        for a in &subsets {
            for b in &subsets {
                let mut ab = ledger_from(a);
                ab.union(&ledger_from(b));
                let joined = ledger_from(a).verdict().join(ledger_from(b).verdict());
                assert_eq!(
                    ab.verdict(),
                    joined,
                    "verdict homomorphism broken for {a:?} / {b:?}"
                );
            }
        }
    }
}
