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
//! `⊑_t` lattice join and the [`Belnap`] knowledge values `⊑_k`-join — so the
//! ledger is byte-stable under any parallel fold order. DAG edges are stored as
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

use crate::diag::{Advice, Diag, DiagRef, Label, Slot, SourceContext, StageId};
use crate::grade::{Belnap, BoundedLattice, FindingCategory, Grade};
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

impl DiagFingerprint {
    /// Compute the fingerprint from the identity fields — `(code, category,
    /// source-context anchor, focus)`. Never keys on message/frames/grade.
    pub fn compute(code: &str, category: FindingCategory, ctx: &SourceContext) -> Self {
        let mut hasher = blake3::Hasher::new();
        feed(&mut hasher, b"code", code.as_bytes());
        feed(&mut hasher, b"category", category.as_str().as_bytes());
        feed(
            &mut hasher,
            b"path",
            ctx.location.path.as_deref().unwrap_or("").as_bytes(),
        );
        feed(
            &mut hasher,
            b"line",
            &ctx.location.line.unwrap_or(0).to_le_bytes(),
        );
        feed(
            &mut hasher,
            b"column",
            &ctx.location.column.unwrap_or(0).to_le_bytes(),
        );
        feed(
            &mut hasher,
            b"logical",
            ctx.location.logical.as_deref().unwrap_or("").as_bytes(),
        );
        let role = ctx.term_role.map(|r| format!("{r:?}")).unwrap_or_default();
        feed(&mut hasher, b"role", role.as_bytes());
        feed(
            &mut hasher,
            b"focus",
            ctx.focus
                .as_ref()
                .map(|f| f.0.as_str())
                .unwrap_or("")
                .as_bytes(),
        );
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
    pub labels: Vec<Label>,
    pub tags: Vec<String>,
    /// The Belnap knowledge value; [`Belnap::Both`] flags a merged glut.
    pub knowledge: Belnap,
    pub emitted_at: SerLocation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locus_stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locus_shard: Option<u32>,
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
#[derive(Debug, Default)]
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
    /// ledger as a fresh fold.
    pub fn replay(&mut self, nodes: impl IntoIterator<Item = DiagNode>) {
        for node in nodes {
            self.insert(node);
        }
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

    fn insert(&mut self, node: DiagNode) -> DiagRef {
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
            self.assert_acyclic(existing);
            return existing;
        }

        let dref = DiagRef::from_index(self.arena.len());
        self.intern.insert(node.fingerprint, dref);
        self.arena.push(node);
        self.assert_acyclic(dref);
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
            labels: Vec::new(),
            tags: Vec::new(),
            knowledge: Belnap::Supported,
            emitted_at: SerLocation {
                file: "x".to_owned(),
                line: 1,
                column: 1,
            },
            locus_stage: None,
            locus_shard: None,
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
            labels: Vec::new(),
            tags: Vec::new(),
            knowledge: Belnap::Supported,
            emitted_at: SerLocation {
                file: "x".to_owned(),
                line: 1,
                column: 1,
            },
            locus_stage: None,
            locus_shard: None,
        };
        ledger.replay([node]);
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
    fn fingerprint_iri_is_stable_and_addressable() {
        let ctx = SourceContext::default();
        let fp =
            DiagFingerprint::compute("test.ledger.iri", FindingCategory::DataShapeViolation, &ctx);
        let iri = fingerprint_iri(&fp);
        assert!(iri.starts_with("https://blackcatinformatics.ca/gmeow/diagnostics/finding/"));
        assert!(iri.ends_with(&fp.hex()));
    }
}
