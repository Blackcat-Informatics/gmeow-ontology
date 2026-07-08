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

    /// The interned witnesses as owned, serializable nodes — the transport form that
    /// carries the store across a stage boundary (the compile-logic → mappings channel is
    /// JSON, so the live ledger cannot cross it). Round-trips through [`Self::from_nodes`].
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
        observed: Option<u64>,
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
        if let Some(count) = observed {
            diag = diag.with_observed(Slot::new(count.to_string()));
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
        self.intern("transcode", code, focus, &tags, Some(count), note, &[]);
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

    /// Record one projection's drops: the structural (target-metadata) notes and
    /// the concrete per-run actual notes, both under the target focus (**R1**), the
    /// declared [`PreservationKind`] carried as a tag.
    pub fn record_projection_drops(
        &mut self,
        target: &str,
        preservation: PreservationKind,
        lossy_drops: &[String],
        actual_drops: &[String],
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
        for note in actual_drops {
            self.intern(
                "projection",
                ACTUAL_CODE,
                target.to_owned(),
                &pres_tag,
                None,
                note,
                antecedents,
            );
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transcode_rows_round_trip_and_sort() {
        let mut store = LossLedger::new();
        // Deliberately out of (from,to,code) order and across two pairs (R1).
        store.record_transcode_loss("named-graph-dropped", "trig", "turtle", "graphs go", 1);
        store.record_transcode_loss("rdf12-star-unrepresentable", "trig", "turtle", "star go", 3);
        store.record_transcode_loss("owl-dl-projection", "turtle", "owl-dl", "dl drop", 2);

        let rows = store.transcode_rows();
        // Two distinct pairs never collapsed into one witness (R1).
        assert_eq!(rows.len(), 3);
        // Sorted by (from, to, code): trig<turtle pair first (named<rdf12), then turtle→owl-dl.
        assert_eq!(rows[0].from, "trig");
        assert_eq!(rows[0].code, "named-graph-dropped");
        assert_eq!(rows[0].count, 1);
        assert_eq!(rows[1].code, "rdf12-star-unrepresentable");
        assert_eq!(rows[1].count, 3);
        assert_eq!(rows[2].from, "turtle");
        assert_eq!(rows[2].to, "owl-dl");
        assert_eq!(rows[2].count, 2);
    }

    #[test]
    fn transcode_rows_aggregate_multiple_observations_of_one_node() {
        // Two records sharing the SAME (code, from, to) hash-cons-merge into one
        // DiagNode carrying two observations. The read-back must aggregate ALL of
        // them — reading only the first would silently drop the second's count.
        let mut store = LossLedger::new();
        store.record_transcode_loss("named-graph-dropped", "trig", "turtle", "graphs go", 2);
        store.record_transcode_loss("named-graph-dropped", "trig", "turtle", "graphs go", 3);

        let rows = store.transcode_rows();
        // Still ONE row per (from, to, code) — the observations merged, not the rows.
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].from, "trig");
        assert_eq!(rows[0].to, "turtle");
        assert_eq!(rows[0].code, "named-graph-dropped");
        // Count is the SUM of every observation (2 + 3), not just the first (2).
        assert_eq!(rows[0].count, 5);
        // The static per-code note is preserved, never dropped.
        assert_eq!(rows[0].note, "graphs go");
    }

    #[test]
    fn actual_drop_carries_structural_limitation_as_antecedent() {
        // U2 producer-side antecedent DAG: a concrete per-run drop is CAUSED BY the
        // target's declared structural limitation. `actual_drop_causes` reads that edge off
        // the actual witness's own antecedents and pairs each drop note with the stable
        // finding IRI of its cause — the provenance a consumer attaches as a related location.
        let mut store = LossLedger::new();
        store.record_projection_drops(
            "owl-dl",
            PreservationKind::SoundUnder,
            &["OWL-DL cannot carry full first-order formulas".to_owned()],
            &["logic:Formula #3 dropped as unsupported residue".to_owned()],
        );
        let causes = store.actual_drop_causes("owl-dl");
        assert_eq!(causes.len(), 1, "one (drop, cause) pair: {causes:?}");
        assert_eq!(
            causes[0].0,
            "logic:Formula #3 dropped as unsupported residue"
        );
        assert!(
            causes[0].1.starts_with("https://"),
            "the cause is the structural witness's stable finding IRI: {}",
            causes[0].1
        );

        // No fabrication: a target with NO declared structural limitation has no antecedent
        // edge, so no cause is asserted (epistemic-shape preservation).
        let mut bare = LossLedger::new();
        bare.record_projection_drops(
            "canonical-rdf12",
            PreservationKind::SoundUnder,
            &[],
            &["a per-run drop with no structural cause".to_owned()],
        );
        assert!(
            bare.actual_drop_causes("canonical-rdf12").is_empty(),
            "with no structural limitation there is no genuine cause — no fabricated antecedent"
        );

        // And the antecedent edge is genuinely ON the witness (not re-derived): the actual
        // node's `to_finding` projection surfaces the cause as a related location too.
        let finding = store
            .ledger
            .findings("logic-compile")
            .into_iter()
            .find(|f| f.code.contains("actual"))
            .expect("an actual-drop finding");
        assert!(
            !finding.related_locations.is_empty(),
            "to_finding must project the wired antecedent as a related location: {finding:?}"
        );
    }

    #[test]
    fn projection_drops_match_structural_then_prefixed_actual() {
        let mut store = LossLedger::new();
        let structural = vec!["z structural".to_owned(), "a structural".to_owned()];
        let actual = vec!["y actual".to_owned(), "b actual".to_owned()];
        store.record_projection_drops("owl-dl", PreservationKind::SoundUnder, &structural, &actual);

        let drops = store.projection_drops_for("owl-dl");
        assert_eq!(
            drops,
            vec![
                "a structural".to_owned(),
                "z structural".to_owned(),
                "actual: b actual".to_owned(),
                "actual: y actual".to_owned(),
            ]
        );
        // A different target is isolated by focus (R1): no cross-target bleed.
        assert!(store.projection_drops_for("gufo").is_empty());
    }
}
