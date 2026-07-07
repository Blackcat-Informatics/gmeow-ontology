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

use std::collections::BTreeSet;

use gmeow_errors::{
    Diag, DiagLedger, FindingCategory, Grade, Severity, Slot, StageId, Standpoint, register_code,
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
#[derive(Debug, Default)]
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

    /// Intern one loss witness. The finding is a non-gating `ProjectionLoss` note
    /// (matching the ingest-boundary loss lift in `gmeow_errors::rdf`); the
    /// fingerprint keys on `(code, category, focus)`, so witnesses that share a
    /// `(code, focus)` merge and accumulate their distinct notes as observations.
    fn intern(
        &mut self,
        producer: &str,
        code: &str,
        focus: String,
        tags: &[String],
        observed: Option<u64>,
        note: &str,
    ) {
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
        self.ledger
            .attach(diag, StageId::new(format!("loss.{producer}")));
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
        self.intern("transcode", code, focus, &tags, Some(count), note);
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

    // ── Stage 2: coherence certificate projection-loss codes ────────────────

    /// Build a loss store from the caller-supplied coherence loss codes. Each code
    /// is its own focus (they are already distinct ledger codes), so read-back
    /// reproduces the exact set the content hash folds over.
    pub fn from_certificate_codes<'a>(codes: impl IntoIterator<Item = &'a str>) -> Self {
        let mut store = Self::new();
        for code in codes {
            store.intern("certificate", code, code.to_owned(), &[], None, code);
        }
        store
    }

    /// The recorded coherence loss codes, read back as the sorted set the
    /// `CoherencePayload.projection_losses` field carries (and hashes).
    pub fn certificate_codes(&self) -> BTreeSet<String> {
        self.ledger
            .emit_sorted()
            .into_iter()
            .map(|node| strip_rung(&node.code))
            .collect()
    }

    // ── Stage 3: F2 projection-report per-target drops ──────────────────────

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
        for note in lossy_drops {
            self.intern(
                "projection",
                STRUCTURAL_CODE,
                target.to_owned(),
                &pres_tag,
                None,
                note,
            );
        }
        for note in actual_drops {
            self.intern(
                "projection",
                ACTUAL_CODE,
                target.to_owned(),
                &pres_tag,
                None,
                note,
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
    fn certificate_codes_round_trip_sorted_set() {
        let input: BTreeSet<String> = ["named-graph-dropped", "rdf12-star-jsonld-rejected"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let store = LossLedger::from_certificate_codes(input.iter().map(String::as_str));
        assert_eq!(store.certificate_codes(), input);
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
