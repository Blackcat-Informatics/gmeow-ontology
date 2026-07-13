// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The compositional-lowering corpus producer: run the flagship quantified subject–verb–object
//! sentence through the Montagovian lowering and fold the resulting first-order formula into the
//! bundle as its own queryable named graph.
//!
//! This is the production wiring of the "a sentence to a formula, compositionally" flagship. The
//! authored flagship sentence "every cat chases a mouse"
//! ([`gmeow_lang_bridge::lower::flagship_svo_sentence`]) is lowered — one declared stage at a
//! time — to the full first-order [`Formula`](gmeow_logic_compile::ir::Formula)
//! `∀x(cat(x) → ∃y(mouse(y) ∧ chase(x, y)))` by
//! [`gmeow_lang_bridge::lower::lower_svo`], and the lowering is emitted as a deterministic
//! (sorted, deduped) `lang:CompositionalLowering` N-Triples graph carrying its formula content
//! key plus one `lang:LoweringStage` per lowering step (each with its `logic:preservationKind`).
//!
//! A sentence outside the modeled quantified-SVO fragment is a HARD FAILURE (a
//! [`gmeow_errors::Diag`] carrying
//! the offending construct) — never a plausible-but-wrong formula, never a silent fallback. The
//! modeled fragment lowers exactly, so every stage discharges [`PreservationKind::Exact`] and the
//! corpus folds ONE honest, vacuously-exact loss-ledger row (nothing dropped) exactly as the
//! sibling `lang:` producers fold theirs.

use gmeow_lang_bridge::lower::{flagship_svo_sentence, lower_svo};
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::loss_ledger::LossLedger;
use gmeow_logic_compile::projections::ProjectionResult;

/// The stable, deterministic subject IRI the flagship lowering is rooted at. Content-free (a
/// fixed authored example IRI under the `gmeow:` example base), so the emitted N-Triples are
/// byte-reproducible run to run.
pub const FLAGSHIP_LOWERING_IRI: &str =
    "https://blackcatinformatics.ca/gmeow/examples/lang/flagship/sentence-to-formula";

/// The loss-ledger target focus the one honest corpus row is keyed under.
const LEDGER_TARGET: &str = "lang-lowering:sentence-to-formula";

/// The assembled compositional-lowering corpus: the sorted, byte-stable N-Triples graph
/// (`graph/lang-lowering-corpus`), the single exact loss-ledger row, and the loss store its
/// (empty) drops are interned into.
pub struct LangLoweringCorpus {
    /// The deterministic, sorted N-Triples graph of the `lang:CompositionalLowering` and its
    /// per-stage `lang:LoweringStage` preservation records.
    pub ntriples: Vec<u8>,
    /// The single honest ledger row for the lowering: vacuously exact (the modeled fragment's
    /// compositional formula captures the sentence's truth conditions exactly, so nothing is
    /// dropped). Its (empty) drops live in [`loss`](Self::loss).
    pub ledger: Vec<ProjectionResult>,
    /// The loss store the row's (empty) drops are interned into, keyed by target focus. The
    /// mappings stage unions it into the single report loss store.
    pub loss: LossLedger,
}

/// Build the compositional-lowering corpus: lower the authored flagship SVO sentence to its
/// first-order formula and emit it as the deterministic `lang:CompositionalLowering` graph.
///
/// Hard-fails (never a fallback) if the flagship sentence does not lower, or if the lowering
/// records an undeclared stage — both are contract violations of the modeled fragment.
pub fn build_corpus() -> Result<LangLoweringCorpus, gmeow_errors::Diag> {
    let sentence = flagship_svo_sentence();
    let lowering = lower_svo(&sentence).map_err(|e| {
        stage_err(format!(
            "the flagship quantified-SVO sentence must lower to a compositional formula, but the \
             lowering hard-failed on construct: {}",
            e.construct
        ))
    })?;
    // Every lowering step is declared, in order — a silent step is a hard fail, not a shipped
    // corpus with an unaccounted stage.
    lowering.assert_all_stages_declared().map_err(|e| {
        stage_err(format!(
            "the flagship lowering must declare every stage in order: {}",
            e.construct
        ))
    })?;

    let ntriples = lowering.to_ntriples(FLAGSHIP_LOWERING_IRI);

    // One honest ledger row: the modeled fragment lowers exactly, so the row is vacuously exact
    // (nothing projected away, nothing dropped) — the overclaim floor accepts it, mirroring how
    // the sibling `lang:` producers fold their exact rows.
    let mut loss = LossLedger::new();
    loss.record_projection_drops(LEDGER_TARGET, PreservationKind::Exact, &[], &[]);
    let ledger = vec![ProjectionResult {
        target: LEDGER_TARGET.to_owned(),
        content: format!(
            "compositional lowering of the flagship quantified-SVO sentence to \
             {}: every stage exact",
            lowering.formula.content_key()
        ),
        is_rdf: true,
        preservation: PreservationKind::Exact,
        complexity: "n/a".to_owned(),
    }];

    Ok(LangLoweringCorpus {
        ntriples,
        ledger,
        loss,
    })
}

/// A `stage-mappings` hard-fail diagnostic (byte-identical shape to the sibling `lang:`
/// producers, so a lowering failure surfaces as a mappings-stage failure).
fn stage_err(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::StageFailed {
        stage: "stage-mappings".to_string(),
        message: message.into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn corpus_carries_the_compositional_lowering() {
        let corpus = build_corpus().expect("the flagship sentence lowers");
        let nt = String::from_utf8(corpus.ntriples).expect("UTF-8 N-Triples");
        assert!(!nt.trim().is_empty(), "the lowering corpus is non-empty");
        // The compositional formula's `chase` relation and the lowering typing ride the corpus.
        assert!(nt.contains("CompositionalLowering"), "{nt}");
        assert!(nt.contains("LoweringStage"), "{nt}");
        assert!(
            nt.contains("chase"),
            "the emitted lowering carries the `chase` relation: {nt}"
        );
        // Every stage lowers exactly (the modeled fragment's compositional formula is exact).
        let exact = PreservationKind::Exact.iri();
        let stage_exact = nt
            .lines()
            .filter(|l| l.contains("preservationKind") && l.contains(&exact))
            .count();
        assert_eq!(
            stage_exact,
            gmeow_lang_bridge::lower::REQUIRED_STAGES.len(),
            "every lowering stage records an exact preservation: {nt}"
        );
    }

    #[test]
    fn corpus_folds_one_honest_exact_row() {
        let corpus = build_corpus().expect("build corpus");
        assert_eq!(corpus.ledger.len(), 1, "one honest ledger row");
        let row = &corpus.ledger[0];
        assert_eq!(row.preservation, PreservationKind::Exact);
        // Vacuously exact: the loss store interns no drops for the row's target.
        assert!(
            corpus.loss.projection_drops_for(LEDGER_TARGET).is_empty(),
            "an exact lowering drops nothing"
        );
    }

    #[test]
    fn corpus_is_byte_reproducible() {
        let a = build_corpus().expect("a").ntriples;
        let b = build_corpus().expect("b").ntriples;
        assert_eq!(a, b, "the lowering corpus must be deterministic");
    }
}
