// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The serialization boundary: lower a live [`Diag`] to a [`DiagNode`] exactly
//! ONCE.
//!
//! In-process a [`Diag`] carries a live `dyn Error` source chain and
//! `&'static Location` emit sites — neither is serializable, and neither is ever
//! pre-flattened while the diagnostic is still propagating. This module is the
//! single place that flattening happens: at the cache/carrier edge, when a
//! diagnostic is attached to (or replayed into) the ledger. Because lowering is a
//! pure, deterministic function of the diagnostic's content, a freshly-lowered
//! node and a cache-replayed node are byte-identical.

use crate::code;
use crate::diag::{Diag, DiagRef, StageId};
use crate::ledger::{DiagFingerprint, DiagNode, Observation, SerFrame, SerLocation};

/// Lower a live [`Diag`] into a serializable [`DiagNode`], stamping the producing
/// stage. `resolve` maps each in-process antecedent handle to its
/// content-addressed fingerprint (the edges are content-addressed — a raw
/// [`DiagRef`] is never serialized).
pub(crate) fn lower(
    diag: &Diag,
    stage: StageId,
    resolve: impl Fn(DiagRef) -> DiagFingerprint,
) -> DiagNode {
    let inner = diag.inner();

    // The live source chain is flattened here, exactly once.
    let mut frames: Vec<SerFrame> = Vec::new();
    // Context frames (innermost first), stamped with their Rust call site.
    for frame in &inner.context {
        frames.push(SerFrame {
            message: frame.label.clone(),
            at: Some(SerLocation::from_caller(frame.at)),
        });
    }
    // The live `dyn Error` source chain.
    let mut source = inner
        .source
        .as_ref()
        .map(|b| b.as_ref() as &dyn std::error::Error);
    while let Some(err) = source {
        frames.push(SerFrame {
            message: err.to_string(),
            at: None,
        });
        source = err.source();
    }

    let antecedents: Vec<DiagFingerprint> = inner.antecedents.iter().map(|r| resolve(*r)).collect();

    let observation = Observation {
        message: inner.message.clone(),
        observed: inner.observed.clone(),
        expected: inner.expected.clone(),
    };

    let fingerprint = DiagFingerprint::compute(
        code::code_str(inner.code),
        inner.grade.category,
        &inner.source_ctx,
    );

    DiagNode {
        fingerprint,
        stage,
        grade: inner.grade,
        code: code::code_str(inner.code).to_owned(),
        observations: vec![observation],
        frames,
        antecedents: antecedents.into_boxed_slice(),
        source_ctx: inner.source_ctx.clone(),
        attributions: inner.attributions.clone(),
        advice: inner.advice.clone(),
        remediation: inner.remediation.clone(),
        guidance: inner.guidance.clone(),
        derived_from_quads: inner.derived_from_quads.clone(),
        labels: inner.labels.clone(),
        tags: inner.tags.clone(),
        documented_terms: inner.documented_terms.clone(),
        failure_class: inner.failure_class.clone(),
        knowledge: inner.grade.category.polarity(),
        emitted_at: SerLocation::from_caller(inner.locus.emitted_at),
        locus_stage: inner.locus.stage.as_ref().map(|s| s.0.clone()),
    }
}
