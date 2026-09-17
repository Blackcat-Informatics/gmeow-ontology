// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The single loss store: one [`gmeow_errors::DiagLedger`] every loss
//! SERIALIZATION projects from.
//!
//! Three surfaces used to each own a bespoke loss container and serialize
//! directly out of it:
//! - the transcode hub's realized-loss JSON (`from → to` codec-pair losses),
//! - the coherence certificate's `projection_losses` set (folded into the
//!   content hash), and
//! - the F2 projection report's per-target `gmeow:lossyDrop` records.
//!
//! [`LossLedger`] is the one newtype they all route through. Each loss is
//! interned as a non-gating `ProjectionLoss`-graded [`Diag`] witness whose
//! fingerprint keys on `(code, category, location, focus)` — never the message.
//! Per **R1**, the projection TARGET (report) or codec PAIR (transcode) goes into
//! `.with_focus(...)`, so distinct per-target / per-pair losses never
//! hash-cons-merge and collapse into one witness. The typed `record_*` methods
//! intern producer rows; the typed read-back methods reproduce each serializer's
//! EXACT ordering, so the committed goldens stay byte-identical while the loss
//! serialization now flows through the ONE substrate ledger.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::{
    Diag, DiagLedger, DiagNode, DiagRef, FindingCategory, Grade, Severity, Slot, StageId,
    Standpoint, register_code,
};

use crate::ir::PreservationKind;

/// Every interned loss code is a `preservation.rung.<code>` finding code, the open
/// preservation-rung family the substrate registry reserves for loss witnesses.
const RUNG_PREFIX: &str = "preservation.rung.";

/// The `from␟to` codec-pair focus separator (ASCII unit separator). Codec names
/// are kebab-case ASCII, so this byte never appears inside one — the split is
/// unambiguous on read-back.
const PAIR_SEP: char = '\u{1f}';

/// The stage-3 discriminator codes: structural (target-metadata) drops vs. the
/// concrete per-run actual drops. Two witnesses per target (one each), so all the
/// structural notes of a target accumulate as observations on one node and all
/// the actual notes on another — the multiset the substrate ledger preserves.
const STRUCTURAL_CODE: &str = "structural";
const ACTUAL_CODE: &str = "actual";

/// The `observed` [`Slot`] datatype that marks an actual-drop observation whose value
/// is the DOCUMENTED SOURCE TERM the drop concerns (an `xsd:anyURI` IRI). A term-specific
/// projection drop — e.g. a `gmeow:` term whose SSSOM alignment cannot carry a distinction
/// when projected DOWN to an external vocabulary — carries the source term as this structured
/// slot (NOT scraped from the free-text note), so the report serialization can attribute the
/// drop to that term's page. An actual drop with no single source term (a genuinely
/// program-wide structural limitation) carries no such slot and stays whole-program.
const SOURCE_TERM_DATATYPE: &str = "http://www.w3.org/2001/XMLSchema#anyURI";

/// One transcode realized-loss row read back from the ledger, in the shape the
/// transcode hub serializes to `loss.json`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscodeLossRow {
    /// The bare loss code (the `preservation.rung.` prefix stripped).
    pub code: String,
    /// Source codec name.
    pub from: String,
    /// Target codec name.
    pub to: String,
    /// Human-readable note (the static ledger explanation).
    pub note: String,
    /// Runtime count of dropped items.
    pub count: u64,
}

/// The single loss store — a newtype over the substrate [`DiagLedger`]. Each
/// serializer builds one, records its producer rows through the typed `record_*`
/// methods, and reads them back through the matching typed method, so the three
/// loss serializations are thin projections over one ledger.
#[derive(Debug, Default, Clone)]
pub struct LossLedger {
    ledger: DiagLedger,
}

/// Read the report-visible union without cloning or replaying diagnostic nodes.
/// Complete source stores retain all other fields, including causal edges and
/// standpoint. This view only projects the same set-valued report observations.
/// Its borrowed index visits each store once and is bounded by the supplied
/// observations; rendering another target never scans the other targets' losses.
pub struct ProjectionLossView<'a> {
    targets: BTreeMap<&'a str, ProjectionTargetLoss<'a>>,
}

/// Borrowed report observations for one target, in their required output order.
#[derive(Default)]
struct ProjectionTargetLoss<'a> {
    structural: BTreeSet<&'a str>,
    actual: BTreeSet<&'a str>,
    /// Source first, then note; the public report surface returns note/source.
    attributed: BTreeSet<(&'a str, &'a str)>,
}

impl<'a> ProjectionLossView<'a> {
    /// Index the report observations once, retaining only references into stores.
    pub fn new(ledgers: &[&'a LossLedger]) -> Self {
        let mut targets: BTreeMap<&str, ProjectionTargetLoss<'_>> = BTreeMap::new();
        let structural_code = format!("{RUNG_PREFIX}{STRUCTURAL_CODE}");
        let actual_code = format!("{RUNG_PREFIX}{ACTUAL_CODE}");
        for ledger in ledgers {
            for node in ledger.ledger.emit_sorted() {
                let structural = node.code == structural_code;
                if !structural && node.code != actual_code {
                    continue;
                }
                let Some(focus) = &node.source_ctx.focus else {
                    continue;
                };
                let target = targets.entry(focus.0.as_str()).or_default();
                for observation in &node.observations {
                    let note = observation.message.as_str();
                    if structural {
                        target.structural.insert(note);
                    } else {
                        target.actual.insert(note);
                        if let Some(source) = observation
                            .observed
                            .as_ref()
                            .filter(|slot| slot.datatype.as_deref() == Some(SOURCE_TERM_DATATYPE))
                        {
                            target.attributed.insert((source.lexical.as_str(), note));
                        }
                    }
                }
            }
        }
        Self { targets }
    }

    /// Structural notes followed by actual notes, preserving the report ordering.
    pub fn projection_drops_for(&self, target: &str) -> Vec<String> {
        let Some(loss) = self.targets.get(target) else {
            return Vec::new();
        };
        loss.structural
            .iter()
            .map(|note| (*note).to_owned())
            .chain(loss.actual.iter().map(|note| format!("actual: {note}")))
            .collect()
    }

    /// Every distinct note/source-term pair, with the existing source-first order.
    pub fn term_source_drops(&self, target: &str) -> Vec<(String, String)> {
        self.targets
            .get(target)
            .into_iter()
            .flat_map(|loss| &loss.attributed)
            .map(|(source, note)| ((*note).to_owned(), (*source).to_owned()))
            .collect()
    }
}

impl serde::Serialize for LossLedger {
    /// Encode every native witness field in canonical node order, borrowing nodes.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde::Serialize::serialize(&self.ledger.emit_sorted(), serializer)
    }
}

impl<'de> serde::Deserialize<'de> for LossLedger {
    /// Restore complete native witnesses through the same canonical ledger ingress.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        <Vec<DiagNode> as serde::Deserialize>::deserialize(deserializer).map(Self::from_nodes)
    }
}

impl LossLedger {
    /// A fresh, empty loss store.
    pub fn new() -> Self {
        Self {
            ledger: DiagLedger::new(),
        }
    }

    /// Fold another loss store's witnesses into this one — the substrate ledger's
    /// content-addressed CRDT union. Each producer that cannot reach the shared store at
    /// its call site (a `LangTarget::emit` emitter, a correspondence dialect lowering)
    /// builds its own [`LossLedger`] and the consumer merges it in here, so the merged
    /// read-back is byte-identical to a single fold (union is commutative/associative/
    /// idempotent, hash-consing shared `(code, focus)` witnesses).
    pub fn union(&mut self, other: &LossLedger) {
        self.ledger.union(&other.ledger);
    }

    /// Project the interned loss witnesses to a [`gmeow_errors::Report`] under `tool`,
    /// delegating to the substrate ledger. Each witness projects through
    /// `DiagLedger::to_finding`, so every finding carries its stable `finding_iri` /
    /// `anchor_iri` and — for an actual-drop witness — the wired antecedent DAG edge
    /// (the causing structural-limitation witness) as both a structured antecedent and a
    /// related location. This is what lets the diagnostic meta-fold join on a REAL
    /// provenance DAG and derive `gmeow:findingRootCause` on the shipped bundle, rather
    /// than the identity-less hand-built loss notes it could not join on.
    pub fn project_report(&self, tool: &str) -> gmeow_errors::Report {
        self.ledger.project_report(tool)
    }

    /// Every loss witness projected to a [`gmeow_errors::Finding`] under `tool`, in the
    /// ledger's deterministic order — the finding-list twin of
    /// [`project_report`](Self::project_report).
    pub fn findings(&self, tool: &str) -> Vec<gmeow_errors::Finding> {
        self.ledger.findings(tool)
    }

    /// The interned witnesses as owned nodes for callers that need independent ownership.
    /// Native stage publications share the ledger itself; its codec borrows sorted nodes.
    /// Round-trips through [`Self::from_nodes`].
    pub fn to_nodes(&self) -> Vec<DiagNode> {
        self.ledger.emit_sorted().into_iter().cloned().collect()
    }

    /// Reconstruct a loss store from the nodes emitted by [`Self::to_nodes`] — the inverse
    /// transport leg. Replays each pre-lowered node, so the reconstructed store's read-back
    /// is byte-identical to the original.
    pub fn from_nodes(nodes: Vec<DiagNode>) -> Self {
        let mut ledger = DiagLedger::new();
        ledger.replay(nodes);
        Self { ledger }
    }

    /// The number of per-run ACTUAL drops recorded for `target` — the realized-loss count
    /// the transcode hub attaches to a `<code>-projection` ledger row (the old
    /// `ProjectionResult::actual_drops.len()`). Sums the actual-note observations on the
    /// target's actual-coded witness (0 when the target recorded none).
    pub fn actual_drop_count(&self, target: &str) -> usize {
        let actual_code = format!("{RUNG_PREFIX}{ACTUAL_CODE}");
        self.ledger
            .emit_sorted()
            .into_iter()
            .filter(|node| {
                node.code == actual_code
                    && node
                        .source_ctx
                        .focus
                        .as_ref()
                        .is_some_and(|f| f.0 == target)
            })
            .map(|node| node.observations.len())
            .sum()
    }

    /// Intern one loss witness and return its arena handle. The finding is a
    /// non-gating `ProjectionLoss` note (matching the ingest-boundary loss lift in
    /// `gmeow_errors::rdf`); the fingerprint keys on `(code, category, focus)`, so
    /// witnesses that share a `(code, focus)` merge and accumulate their distinct notes
    /// as observations. `antecedents` are the causing witnesses this loss is derived
    /// from — the content-addressed DAG edges `to_finding` projects as related locations.
    #[allow(clippy::too_many_arguments)]
    fn intern(
        &mut self,
        producer: &str,
        code: &str,
        focus: String,
        tags: &[String],
        observed: Option<Slot>,
        note: &str,
        antecedents: &[DiagRef],
    ) -> DiagRef {
        let mut diag = Diag::new(
            register_code(&format!("{RUNG_PREFIX}{code}")),
            Grade::new(
                Severity::Note,
                FindingCategory::ProjectionLoss,
                Standpoint::Perspectival,
            ),
            note,
        )
        .with_focus(focus);
        if let Some(slot) = observed {
            diag = diag.with_observed(slot);
        }
        for tag in tags {
            diag = diag.with_tag(tag.clone());
        }
        if !antecedents.is_empty() {
            diag = diag.with_antecedents(antecedents.iter().copied());
        }
        self.ledger
            .attach(diag, StageId::new(format!("loss.{producer}")))
    }

    // ── Stage 1: transcode realized losses ──────────────────────────────────

    /// Record one realized transcode loss. **R1:** the `from␟to` codec pair is the
    /// focus, so a loss on one pair never collapses into the same-coded loss on
    /// another pair. `from`/`to` are also carried as tags.
    pub fn record_transcode_loss(
        &mut self,
        code: &str,
        from: &str,
        to: &str,
        note: &str,
        count: u64,
    ) {
        let focus = format!("{from}{PAIR_SEP}{to}");
        let tags = [from.to_owned(), to.to_owned()];
        self.intern(
            "transcode",
            code,
            focus,
            &tags,
            Some(Slot::new(count.to_string())),
            note,
            &[],
        );
    }

    /// The recorded transcode losses read back and sorted by `(from, to, code)` —
    /// exactly the order the `loss.json` artifact commits.
    pub fn transcode_rows(&self) -> Vec<TranscodeLossRow> {
        let mut rows: Vec<TranscodeLossRow> = self
            .ledger
            .emit_sorted()
            .into_iter()
            .map(|node| {
                let (from, to) = split_pair_focus(node);
                // Aggregate across ALL observations of the node, not just the
                // first: when two `record_transcode_loss` calls share a
                // `(code, from, to)` they hash-cons-merge onto one node with
                // multiple observations, and every observation past the first
                // would otherwise be silently dropped. The loss note is the
                // static per-code ledger explanation (invariant across a code's
                // observations), so one note per row is correct — take the first
                // non-empty. The count is the runtime item count, so SUM every
                // observation's count so no dropped item goes unaccounted.
                let note = node
                    .observations
                    .iter()
                    .map(|o| o.message.clone())
                    .find(|m| !m.is_empty())
                    .unwrap_or_default();
                let count = node
                    .observations
                    .iter()
                    .filter_map(|o| o.observed.as_ref())
                    .filter_map(|s| s.lexical.parse::<u64>().ok())
                    .sum();
                TranscodeLossRow {
                    code: strip_rung(&node.code),
                    from,
                    to,
                    note,
                    count,
                }
            })
            .collect();
        rows.sort_by(|a, b| {
            a.from
                .cmp(&b.from)
                .then(a.to.cmp(&b.to))
                .then(a.code.cmp(&b.code))
        });
        rows
    }

    // ── Stage 2: F2 projection-report per-target drops ──────────────────────

    /// Record actual cell-owned evidence after preservation admission. Human report
    /// messages project its lexical text; complete RDF literals remain in the native
    /// correspondence carrier. Owner, standpoint and evidence references stay attached
    /// to the same diagnostic witness, not inferred from the message or a target class.
    pub(crate) fn record_correspondence_drops(
        &mut self,
        target: &str,
        correspondence: &crate::ir::Correspondence,
        preservation: PreservationKind,
    ) {
        let mut tags = vec![format!("preservation:{}", preservation.as_str())];
        if let Some(standpoint) = &correspondence.according_to {
            tags.push(format!("according-to:{standpoint}"));
        }
        for source in &correspondence.axis_evidence.sources {
            tags.push(format!("evidence-source:{source}"));
        }
        for evidence in &correspondence.loss_evidence {
            self.intern(
                "projection",
                ACTUAL_CODE,
                target.to_owned(),
                &tags,
                Some(Slot::typed(
                    correspondence.iri.clone(),
                    SOURCE_TERM_DATATYPE,
                )),
                &evidence.lexical_form,
                &[],
            );
        }
    }

    /// Record one projection's drops: the structural (target-metadata) notes and
    /// the concrete per-run actual notes, both under the target focus (**R1**), the
    /// declared [`PreservationKind`] carried as a tag. Every actual drop is whole-program
    /// (no single source term); use [`Self::record_projection_drops_attributed`] to
    /// attribute a drop to the documented term it concerns.
    pub fn record_projection_drops(
        &mut self,
        target: &str,
        preservation: PreservationKind,
        lossy_drops: &[String],
        actual_drops: &[String],
    ) {
        let attributed: Vec<(String, Option<String>)> = actual_drops
            .iter()
            .map(|note| (note.clone(), None))
            .collect();
        self.record_projection_drops_attributed(target, preservation, lossy_drops, &attributed);
    }

    /// Record one projection's drops with per-actual-drop SOURCE-TERM ATTRIBUTION: each
    /// actual drop is `(note, source_term)` where `source_term` is the DOCUMENTED term the
    /// drop concerns (e.g. the `gmeow:` term whose alignment loses a distinction when
    /// projected DOWN to an external vocabulary), or `None` for a genuinely program-wide
    /// drop. The source term rides the actual observation's `observed` slot as a structured
    /// `xsd:anyURI` (NEVER scraped from the free-text note); the report serialization reads it
    /// back via [`Self::term_source_drops`] to attribute the drop to that term's page. The
    /// structural/actual interning and the antecedent DAG are identical to
    /// [`Self::record_projection_drops`], so the `gmeow:lossyDrop` note bytes are unchanged —
    /// the attribution is purely additive.
    pub fn record_projection_drops_attributed(
        &mut self,
        target: &str,
        preservation: PreservationKind,
        lossy_drops: &[String],
        actual_drops: &[(String, Option<String>)],
    ) {
        let pres_tag = [format!("preservation:{}", preservation.as_str())];
        // Intern the structural drops FIRST and capture the structural witness handle.
        // All structural notes of a target share `(STRUCTURAL_CODE, target-focus)`, so they
        // hash-cons into ONE witness — the target's declared structural limitation — and
        // every `attach` returns that same handle.
        let mut structural_ref: Option<DiagRef> = None;
        for note in lossy_drops {
            let r = self.intern(
                "projection",
                STRUCTURAL_CODE,
                target.to_owned(),
                &pres_tag,
                None,
                note,
                &[],
            );
            structural_ref = Some(r);
        }
        // Each concrete per-run drop is CAUSED BY the target's structural limitation: wire
        // the structural witness as its antecedent so the `to_finding` projection surfaces
        // the causing structural note as a related location (the U2 antecedent DAG). When a
        // target declares no structural drop, there is no cause to assert.
        let antecedents: &[DiagRef] = structural_ref.as_slice();
        for (note, source_term) in actual_drops {
            // A term-attributed drop carries the source term as a typed `observed` slot; all
            // actual drops of a target still hash-cons onto ONE witness (the fingerprint keys
            // on code/category/focus, NOT the observed value), so a target's differently-
            // attributed drops accumulate as distinct observations on that one witness.
            let observed = source_term
                .as_ref()
                .map(|iri| Slot::typed(iri.clone(), SOURCE_TERM_DATATYPE));
            self.intern(
                "projection",
                ACTUAL_CODE,
                target.to_owned(),
                &pres_tag,
                observed,
                note,
                antecedents,
            );
        }
    }

    /// The term-attributed actual drops on `target`, each as `(note, source_term_iri)` read
    /// off the actual witness's observations whose `observed` slot is the typed source-term
    /// IRI. Sorted by `(source_term, note)` for a deterministic report serialization. Empty
    /// when the target recorded no term-attributed drop (only whole-program notes).
    pub fn term_source_drops(&self, target: &str) -> Vec<(String, String)> {
        let actual_code = format!("{RUNG_PREFIX}{ACTUAL_CODE}");
        let mut out: Vec<(String, String)> = self
            .ledger
            .emit_sorted()
            .into_iter()
            .filter(|node| {
                node.code == actual_code
                    && node
                        .source_ctx
                        .focus
                        .as_ref()
                        .is_some_and(|f| f.0 == target)
            })
            .flat_map(|node| node.observations.iter())
            .filter_map(|obs| {
                obs.observed.as_ref().and_then(|slot| {
                    (slot.datatype.as_deref() == Some(SOURCE_TERM_DATATYPE))
                        .then(|| (obs.message.clone(), slot.lexical.clone()))
                })
            })
            .collect();
        out.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0)));
        out.dedup();
        out
    }

    /// The combined lossy-drop list for one target read back from the ledger — the
    /// single source of truth for what BOTH the Turtle `gmeow:lossyDrop` report and
    /// the JSON `preservation-ledger.json` emit: structural notes (sorted) followed
    /// by the actual notes (sorted, `actual: `-prefixed).
    pub fn projection_drops_for(&self, target: &str) -> Vec<String> {
        let structural_code = format!("{RUNG_PREFIX}{STRUCTURAL_CODE}");
        let actual_code = format!("{RUNG_PREFIX}{ACTUAL_CODE}");

        let mut structural = self.notes_for(target, &structural_code);
        structural.sort();
        let mut actual = self.notes_for(target, &actual_code);
        actual.sort();

        structural
            .into_iter()
            .chain(actual.into_iter().map(|a| format!("actual: {a}")))
            .collect()
    }

    /// The per-run actual-drop notes on `target` (sorted, as
    /// [`projection_drops_for`](Self::projection_drops_for) emits them) each paired with
    /// the stable finding IRI of the structural-limitation witness that CAUSED it, read
    /// from the actual witness's own **antecedent DAG edge** — so a consumer projecting
    /// the drop as a finding attaches the genuine cause as a related location. Empty when
    /// the target declared no structural cause (the actual witness carries no antecedent).
    pub fn actual_drop_causes(&self, target: &str) -> Vec<(String, String)> {
        let actual_code = format!("{RUNG_PREFIX}{ACTUAL_CODE}");
        let Some(node) = self.ledger.emit_sorted().into_iter().find(|node| {
            node.code == actual_code
                && node
                    .source_ctx
                    .focus
                    .as_ref()
                    .is_some_and(|f| f.0 == target)
        }) else {
            return Vec::new();
        };
        // The antecedent edges wired on the actual witness (never re-derived): the causing
        // structural-limitation witnesses, content-addressed by fingerprint.
        let causes: Vec<String> = node
            .antecedents
            .iter()
            .map(gmeow_errors::fingerprint_iri)
            .collect();
        if causes.is_empty() {
            return Vec::new();
        }
        let mut notes: Vec<String> = node
            .observations
            .iter()
            .map(|o| o.message.clone())
            .collect();
        notes.sort();
        notes
            .into_iter()
            .flat_map(|note| causes.iter().map(move |c| (note.clone(), c.clone())))
            .collect()
    }

    /// The observation notes on the witness for one `(target-focus, code)` pair.
    fn notes_for(&self, target: &str, code: &str) -> Vec<String> {
        self.ledger
            .emit_sorted()
            .into_iter()
            .filter(|node| {
                node.code == code
                    && node
                        .source_ctx
                        .focus
                        .as_ref()
                        .is_some_and(|f| f.0 == target)
            })
            .flat_map(|node| node.observations.iter().map(|o| o.message.clone()))
            .collect()
    }
}

/// Strip the `preservation.rung.` prefix from an interned loss code, recovering
/// the bare code the producer supplied.
fn strip_rung(code: &str) -> String {
    code.strip_prefix(RUNG_PREFIX).unwrap_or(code).to_owned()
}

/// Split a `from␟to` pair focus back into `(from, to)`.
fn split_pair_focus(node: &gmeow_errors::DiagNode) -> (String, String) {
    let focus = node
        .source_ctx
        .focus
        .as_ref()
        .map(|f| f.0.as_str())
        .unwrap_or("");
    match focus.split_once(PAIR_SEP) {
        Some((from, to)) => (from.to_owned(), to.to_owned()),
        None => (String::new(), String::new()),
    }
}

#[path = "loss_ledger.tests.rs"]
#[cfg(test)]
mod tests;
