// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The ingest-run frame: the part of a lift that is the SAME for all three bridges.
//!
//! `MATHEMATICS-BRIDGES.md`'s shared bridge contract says a bridge run *is* the
//! mnemomorphic `put` leg of a `logic:Correspondence`. Mechanically that means four
//! obligations, all declared as OWL restrictions on `math:IngestRun`
//! (`slices/grounding/math/module.ttl:10524-10529`) and enforced as
//! `math:UngroundedIngestRun`:
//!
//! | edge | carries |
//! |---|---|
//! | `math:parseSource` | the retained, `logic:loadBearing` source witness |
//! | `logic:instantiatesSchema` | the process-layer action schema the run instantiates |
//! | `logic:instantiatesPlan` | the plan the run enacts |
//! | `math:ingestCorrespondence` | the law-spine `logic:Correspondence` |
//!
//! # `math:` never shadows the law
//!
//! `math:ingestCorrespondence`'s own definition is explicit: *"Everything about the lift
//! that is a law rather than a facet lives on that Correspondence […] `math:` never
//! shadows those properties; it only points at the node that holds them."* So the rung,
//! the preservation polarity, the relation, the determinacy, and the mnemomorphic witness
//! flag are emitted on the `logic:Correspondence` node and nowhere else.
//!
//! # Why the schema and plan are fully typed here
//!
//! The older in-bundle producer emits its schema/plan witnesses as bare
//! `math:MathematicalObject` nodes, reasoning that `math:IngestRunShape` only requires the
//! edges (min 1, no class) and that full typing would "drag in their own
//! capability/precondition/goal obligations the ingest witness does not carry."
//!
//! A real lift *does* carry them, so this frame types them properly and supplies the
//! obligations, matching the shipped conformance fixture
//! `tests/conformance-fixtures/ingest-run-grounded.ttl` rather than the weaker producer.
//! A parse genuinely has a capability and a source-available precondition; an ingest
//! genuinely has a goal and a success mode. Emitting the untyped node would be discarding
//! structure the front-end actually knows — the opposite of maximal information flow.

use crate::error::EmptyCodomain;
use crate::ns::{fnv1a_hex, gmeow, logic, math};
use crate::sink::Sink;

/// Which ingestion bridge a run belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BridgeKind {
    /// An R model/statistics script.
    R,
    /// An ONNX computation graph.
    Onnx,
    /// A proof / dependency DAG, as a TSTP derivation.
    Proof,
}

impl BridgeKind {
    /// The `math:` run subclass local name.
    #[must_use]
    pub fn run_class(self) -> &'static str {
        match self {
            Self::R => "RIngestRun",
            Self::Onnx => "ONNXIngestRun",
            Self::Proof => "ProofIngestRun",
        }
    }

    /// A short, stable slug used in minted IRIs.
    #[must_use]
    pub fn slug(self) -> &'static str {
        match self {
            Self::R => "r",
            Self::Onnx => "onnx",
            Self::Proof => "proof",
        }
    }
}

/// The law-spine rung a lift ACTUALLY achieves.
///
/// Every field is a `logic:` local name that the logic slice declares as a named
/// individual. This is a claim about the lift, so it is chosen per bridge and defended by
/// that bridge's tests — never set to the strongest available value by default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rung {
    /// `logic:MorphismClass` — the ordered law-spine rung.
    pub morphism_class: &'static str,
    /// `logic:PreservationKind` — the machine-readable preservation polarity.
    pub preservation_kind: &'static str,
    /// `logic:CorrespondenceRelation`.
    pub correspondence_relation: &'static str,
    /// `logic:Determinacy`.
    pub determinacy: &'static str,
    /// Whether the source witness is retained in band. A `false` value BARS any
    /// section/retraction claim (`logic:mnemomorphic`'s own definition), so the two are
    /// checked against each other in [`Rung::assert_coherent`].
    pub mnemomorphic: bool,
}

impl Rung {
    /// A lossy lens with a retained in-band witness, over a VAGUE source.
    ///
    /// The R rung. R's general computation and control flow do not survive into `math:`
    /// (they route to `logic:` or hard-fail), and a statistical script's intent is not
    /// crisply recoverable from its text, so the relation is `RelatedMatch` and the
    /// determinacy `Vague`. Matches the rung the existing in-bundle R producer declares.
    #[must_use]
    pub fn lossy_vague_with_witness() -> Self {
        Self {
            morphism_class: "LossyLens",
            preservation_kind: "ValidationOnly",
            correspondence_relation: "RelatedMatch",
            determinacy: "Vague",
            mnemomorphic: true,
        }
    }

    /// A lossy lens with a retained in-band witness, over a CRISP source.
    ///
    /// The ONNX rung. An ONNX graph is a precise artifact — operator types, tensor shapes,
    /// and the opset are exact, not interpreted — so the determinacy is `Crisp` rather
    /// than `Vague`. It is still a lossy lens because weight PAYLOADS are held by
    /// reference and never inlined (the blob-by-reference doctrine), so the tensor values
    /// do not survive the lift.
    #[must_use]
    pub fn lossy_crisp_with_witness() -> Self {
        Self {
            morphism_class: "LossyLens",
            preservation_kind: "ValidationOnly",
            correspondence_relation: "RelatedMatch",
            determinacy: "Crisp",
            mnemomorphic: true,
        }
    }

    /// A section/retraction: the source recovers from the lift plus its witness.
    ///
    /// The proof rung, and the one `math:ProofDependencyGraph`'s own definition names
    /// ("the DAG recovers from the lift and witness"). It is only honest if the lift
    /// carries every step name, inference rule, parent edge, and rendered conclusion —
    /// i.e. if the derivation genuinely reconstructs. The proof bridge owes a round-trip
    /// test for that claim; without one this constructor must not be used.
    #[must_use]
    pub fn section_retraction() -> Self {
        Self {
            morphism_class: "SectionRetraction",
            preservation_kind: "ExactPreservation",
            correspondence_relation: "Equiv",
            determinacy: "Crisp",
            mnemomorphic: true,
        }
    }

    /// A section/retraction claim requires a retained witness.
    ///
    /// `logic:mnemomorphic`'s definition states that a false value BARS any
    /// section/retraction claim, so an incoherent pair is a programming error here rather
    /// than something a downstream validator should have to discover.
    fn assert_coherent(self) {
        assert!(
            self.mnemomorphic || self.morphism_class != "SectionRetraction",
            "a section/retraction rung requires logic:mnemomorphic true"
        );
    }
}

/// How a retained source rides on the witness.
///
/// Textual artifacts ride verbatim; a binary artifact rides as `xsd:base64Binary`, which is
/// the same bytes in an RDF-legal lexical form. Either way the SOURCE is present, which is
/// what `math:parseSource` requires and what `logic:mnemomorphic` asserts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetainedSource {
    /// A textual source, retained verbatim.
    Text(String),
    /// A binary source, retained as base64.
    Binary(String),
}

impl RetainedSource {
    /// Retain `bytes`, choosing the encoding by whether they are valid UTF-8.
    ///
    /// A textual format stays readable in the graph; a binary one is never lossily
    /// stringified, which would retain a corrupted source and make the witness a lie.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        match std::str::from_utf8(bytes) {
            Ok(text) => Self::Text(text.to_owned()),
            Err(_) => Self::Binary(base64(bytes)),
        }
    }
}

/// Standard base64 (RFC 4648, padded) — the `xsd:base64Binary` lexical form.
///
/// Hand-rolled rather than a dependency: the alphabet is fixed and the transform is 20
/// lines, and this crate takes no dependency it can write correctly itself.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// The identity of one ingest run: its IRI and the IRIs of its four frame witnesses.
///
/// Every IRI is a pure function of the bridge kind, the mint base, and the source bytes,
/// so re-lifting the same artifact yields byte-identical Turtle. No clock, no counter, no
/// randomness — the in-bundle producers depend on that idempotence.
#[derive(Debug, Clone)]
pub struct RunFrame {
    /// Which bridge this run belongs to.
    pub kind: BridgeKind,
    /// The `math:*IngestRun` node.
    pub run_iri: String,
    /// The retained `math:parseSource` witness.
    pub source_witness_iri: String,
    /// The `logic:ActionSchema` the run instantiates.
    pub schema_iri: String,
    /// The `logic:Plan` the run enacts.
    pub plan_iri: String,
    /// The `logic:Correspondence` carrying the lift's law spine.
    pub correspondence_iri: String,
    /// The IRI prefix every codomain node this run generates is minted under.
    pub mint_base: String,
    /// The source itself, retained on the witness. This is what makes the witness
    /// mnemomorphic rather than amnesic.
    pub source: RetainedSource,
    /// Constructs the lift did NOT carry into `math:`, enumerated on the witness so a
    /// declared loss has queryable content. Empty for an exact lift.
    pub unmapped: Vec<String>,
}

impl RunFrame {
    /// Mint a run frame for `source` under `mint_base`.
    ///
    /// `mint_base` must end in `/` or `#`; the caller owns the namespace (the CLI mints
    /// under a per-invocation base, the in-bundle producers under `PRODUCER_NS`).
    #[must_use]
    pub fn mint(kind: BridgeKind, mint_base: &str, source: &[u8]) -> Self {
        let slug = kind.slug();
        let digest = fnv1a_hex(source);
        let run_iri = format!("{mint_base}{slug}-run-{digest}");
        Self {
            kind,
            source_witness_iri: format!("{mint_base}{slug}-src-{digest}"),
            schema_iri: format!("{mint_base}{slug}-schema-{digest}"),
            plan_iri: format!("{mint_base}{slug}-plan-{digest}"),
            correspondence_iri: format!("{mint_base}{slug}-corr-{digest}"),
            mint_base: format!("{run_iri}/"),
            run_iri,
            source: RetainedSource::of(source),
            unmapped: Vec::new(),
        }
    }

    /// Record a construct the lift did not carry into `math:`.
    ///
    /// Call before [`RunFrame::emit`]; the residue rides on the source witness. A lift that
    /// declares a rung weaker than `logic:ExactPreservation` and enumerates nothing is
    /// asserting a loss it cannot name.
    pub fn record_unmapped(&mut self, construct: impl Into<String>) {
        let construct = construct.into();
        if !self.unmapped.contains(&construct) {
            self.unmapped.push(construct);
        }
    }

    /// Mint a codomain-node IRI under this run, disambiguated by `role` and `key`.
    #[must_use]
    pub fn node(&self, role: &str, key: &str) -> String {
        format!("{}{role}-{}", self.mint_base, fnv1a_hex(key.as_bytes()))
    }

    /// Emit the four mandatory frame edges and their witnesses.
    ///
    /// The caller then emits its own codomain, linking each produced node back with
    /// [`RunFrame::generated`].
    pub fn emit(&self, sink: &mut Sink, rung: Rung) {
        rung.assert_coherent();

        let run = &self.run_iri;
        sink.typed(run, &math(self.kind.run_class()));
        sink.iri(run, &math("parseSource"), &self.source_witness_iri);
        sink.iri(run, &logic("instantiatesSchema"), &self.schema_iri);
        sink.iri(run, &logic("instantiatesPlan"), &self.plan_iri);
        sink.iri(run, &math("ingestCorrespondence"), &self.correspondence_iri);

        // The retained source witness. It is a NODE, not a literal: math:parseSource's
        // definition rejects "a bare droppable string — an opaque blob is the amnesic
        // case". Its IRI is content-addressed on the source bytes, so the witness names
        // exactly one artifact.
        //
        // It must also CARRY the source. That definition demands a witness "carrying enough
        // of the source to recover what the lift did not map", and a node bearing only a
        // type and logic:loadBearing carries strictly LESS than the opaque blob the same
        // sentence already rejects. Since logic:mnemomorphic — asserted below — is what
        // discharges the section law, an empty witness would make that flag a decoration and
        // the proof bridge's SectionRetraction rung unearned.
        sink.typed(&self.source_witness_iri, &math("MathematicalObject"));
        sink.boolean(&self.source_witness_iri, &logic("loadBearing"), true);
        match &self.source {
            RetainedSource::Text(text) => {
                sink.string(&self.source_witness_iri, &math("retainedSource"), text);
            }
            RetainedSource::Binary(b64) => {
                sink.base64(&self.source_witness_iri, &math("retainedSource"), b64);
            }
        }
        for construct in &self.unmapped {
            sink.string(
                &self.source_witness_iri,
                &math("unmappedConstruct"),
                construct,
            );
        }

        // The process layer: what the run is an instance of, and what it enacts.
        let capability = format!("{}capability", self.mint_base);
        let precondition = format!("{}source-available", self.mint_base);
        sink.typed(&self.schema_iri, &logic("ActionSchema"));
        sink.iri(&self.schema_iri, &logic("capability"), &capability);
        sink.iri(&self.schema_iri, &logic("precondition"), &precondition);

        let goal = format!("{}ingest-goal", self.mint_base);
        let success = format!("{}all-steps-succeed", self.mint_base);
        sink.typed(&self.plan_iri, &logic("Plan"));
        sink.iri(&self.plan_iri, &logic("planGoal"), &goal);
        sink.iri(&self.plan_iri, &logic("planSuccessMode"), &success);
        sink.typed(&goal, &gmeow("Goal"));
        sink.typed(&success, &logic("PlanSuccessMode"));

        // The law spine. Every law-bearing property lives HERE and nowhere else.
        let corr = &self.correspondence_iri;
        sink.typed(corr, &logic("Correspondence"));
        sink.iri(corr, &logic("morphismClass"), &logic(rung.morphism_class));
        sink.iri(
            corr,
            &logic("preservationKind"),
            &logic(rung.preservation_kind),
        );
        sink.iri(
            corr,
            &logic("correspondenceRelation"),
            &logic(rung.correspondence_relation),
        );
        sink.iri(corr, &logic("hasDeterminacy"), &logic(rung.determinacy));
        sink.boolean(corr, &logic("mnemomorphic"), rung.mnemomorphic);
    }

    /// Link a produced codomain node back to this run.
    ///
    /// This is the edge the native `math:UnliftableIngest` lint looks for. Every node the
    /// lift creates carries it, so the run's product is enumerable by query rather than by
    /// IRI-prefix convention.
    pub fn generated(&self, sink: &mut Sink, node_iri: &str) {
        sink.iri(node_iri, &gmeow("wasGeneratedBy"), &self.run_iri);
    }
}

/// The product of one lift.
#[derive(Debug, Clone)]
pub struct Lifted {
    /// The `math:*IngestRun` node the lift produced.
    pub run_iri: String,
    /// Canonical Turtle.
    pub turtle: String,
    /// How many structured codomain nodes the run generated.
    pub codomain_nodes: usize,
}

impl Lifted {
    /// Seal a lift, rejecting an empty codomain.
    ///
    /// # Errors
    ///
    /// [`EmptyCodomain`] when `codomain_nodes` is zero: such a run would be rejected by
    /// the native `math:UnliftableIngest` lint downstream, so it is refused here rather
    /// than serialized and left for a later pass to catch.
    pub fn seal(frame: &RunFrame, sink: Sink, codomain_nodes: usize) -> gmeow_errors::Result<Self> {
        if codomain_nodes == 0 {
            return Err(gmeow_errors::Diag::of_kind(EmptyCodomain {
                detail: format!(
                    "the {} lift produced no structured math: codomain for run <{}>: an \
                     ingest run that generates nothing is an unliftable ingest, not a \
                     lift",
                    frame.kind.slug(),
                    frame.run_iri
                ),
            }));
        }
        Ok(Self {
            run_iri: frame.run_iri.clone(),
            turtle: sink.serialize(),
            codomain_nodes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "https://blackcatinformatics.ca/gmeow/examples/math/lift/";

    fn framed(kind: BridgeKind, rung: Rung, source: &[u8]) -> (RunFrame, String) {
        let frame = RunFrame::mint(kind, BASE, source);
        let mut sink = Sink::new();
        frame.emit(&mut sink, rung);
        let ttl = sink.serialize();
        (frame, ttl)
    }

    #[test]
    fn the_frame_carries_every_ingest_run_shape_obligation() {
        let (_, ttl) = framed(
            BridgeKind::R,
            Rung::lossy_vague_with_witness(),
            b"fit <- lm(y ~ x)",
        );
        for required in [
            "RIngestRun",
            "parseSource",
            "instantiatesSchema",
            "instantiatesPlan",
            "ingestCorrespondence",
            "loadBearing",
            "Correspondence",
            "morphismClass",
            "preservationKind",
            "mnemomorphic",
        ] {
            assert!(
                ttl.contains(required),
                "frame is missing `{required}`:\n{ttl}"
            );
        }
    }

    #[test]
    fn the_same_source_mints_the_same_iris() {
        let (a, ttl_a) = framed(
            BridgeKind::Onnx,
            Rung::lossy_crisp_with_witness(),
            b"\x08\x07",
        );
        let (b, ttl_b) = framed(
            BridgeKind::Onnx,
            Rung::lossy_crisp_with_witness(),
            b"\x08\x07",
        );
        assert_eq!(a.run_iri, b.run_iri, "run IRI is a function of the source");
        assert_eq!(ttl_a, ttl_b, "a re-lift is byte-identical (idempotent)");
    }

    #[test]
    fn a_different_source_mints_a_different_run() {
        let (a, _) = framed(
            BridgeKind::R,
            Rung::lossy_vague_with_witness(),
            b"lm(y ~ x)",
        );
        let (b, _) = framed(
            BridgeKind::R,
            Rung::lossy_vague_with_witness(),
            b"lm(y ~ z)",
        );
        assert_ne!(a.run_iri, b.run_iri);
    }

    #[test]
    fn each_bridge_kind_emits_its_own_run_class() {
        for (kind, class) in [
            (BridgeKind::R, "RIngestRun"),
            (BridgeKind::Onnx, "ONNXIngestRun"),
            (BridgeKind::Proof, "ProofIngestRun"),
        ] {
            let (_, ttl) = framed(kind, Rung::lossy_crisp_with_witness(), b"src");
            assert!(ttl.contains(class), "{kind:?} must emit math:{class}");
        }
    }

    #[test]
    fn an_empty_codomain_is_refused_before_serialization() {
        let frame = RunFrame::mint(BridgeKind::Proof, BASE, b"src");
        let mut sink = Sink::new();
        frame.emit(&mut sink, Rung::section_retraction());
        let err = Lifted::seal(&frame, sink, 0).expect_err("an empty codomain must not seal");
        assert!(
            format!("{err}").contains("unliftable ingest"),
            "unexpected diagnostic: {err}"
        );
    }

    #[test]
    fn a_generated_node_carries_the_back_edge_the_native_lint_requires() {
        let frame = RunFrame::mint(BridgeKind::R, BASE, b"src");
        let mut sink = Sink::new();
        frame.emit(&mut sink, Rung::lossy_vague_with_witness());
        let node = frame.node("fit", "mtcarsFit");
        sink.typed(&node, &math("FittedModel"));
        frame.generated(&mut sink, &node);
        let lifted = Lifted::seal(&frame, sink, 1).expect("one codomain node seals");
        assert!(lifted.turtle.contains("wasGeneratedBy"));
        assert_eq!(lifted.run_iri, frame.run_iri);
    }

    #[test]
    #[should_panic(expected = "section/retraction rung requires")]
    fn a_section_retraction_without_a_witness_is_a_programming_error() {
        let rung = Rung {
            mnemomorphic: false,
            ..Rung::section_retraction()
        };
        let frame = RunFrame::mint(BridgeKind::Proof, BASE, b"src");
        frame.emit(&mut Sink::new(), rung);
    }
}
