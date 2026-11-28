// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The transition laws of the five membership axes run through the enactment gate.
//!
//! Every ordered pair of positions of an axis is recorded as one `logic:AxisMove` and judged
//! by the compiled laws. The expected edges below are spelled out from the enactment design
//! document's transition tables and are deliberately independent of the authored formulas, so
//! a widened or narrowed law makes a pair change verdict and fails here.

use std::collections::BTreeSet;

use super::{ViolationLowering, enactment_gate_markers_with_laws};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

/// One lifecycle axis: its governing constraint, every position, and the admitted edges.
struct Axis {
    constraint: &'static str,
    positions: &'static [&'static str],
    admitted: &'static [(&'static str, &'static str)],
}

const AXES: [Axis; 5] = [
    Axis {
        constraint: "EnactmentStateTransitionConstraint",
        positions: &[
            "EnactmentPending",
            "EnactmentRunning",
            "EnactmentPaused",
            "EnactmentCompleted",
            "EnactmentCancelled",
            "EnactmentFailed",
        ],
        admitted: &[
            ("EnactmentPending", "EnactmentRunning"),
            ("EnactmentPending", "EnactmentCancelled"),
            ("EnactmentRunning", "EnactmentPaused"),
            ("EnactmentRunning", "EnactmentCompleted"),
            ("EnactmentRunning", "EnactmentFailed"),
            ("EnactmentRunning", "EnactmentCancelled"),
            ("EnactmentPaused", "EnactmentRunning"),
            ("EnactmentPaused", "EnactmentCancelled"),
            ("EnactmentPaused", "EnactmentFailed"),
        ],
    },
    Axis {
        constraint: "StepStateTransitionConstraint",
        positions: &[
            "StepPending",
            "StepReady",
            "StepRunning",
            "StepWaiting",
            "StepCompleted",
            "StepSkipped",
            "StepCancelled",
            "StepFailed",
        ],
        admitted: &[
            ("StepPending", "StepReady"),
            ("StepPending", "StepSkipped"),
            ("StepPending", "StepCancelled"),
            ("StepReady", "StepRunning"),
            ("StepReady", "StepWaiting"),
            ("StepReady", "StepSkipped"),
            ("StepReady", "StepCancelled"),
            ("StepRunning", "StepWaiting"),
            ("StepRunning", "StepCompleted"),
            ("StepRunning", "StepFailed"),
            ("StepRunning", "StepCancelled"),
            ("StepWaiting", "StepReady"),
            ("StepWaiting", "StepCancelled"),
            ("StepWaiting", "StepFailed"),
        ],
    },
    Axis {
        constraint: "ApprovalStateTransitionConstraint",
        positions: &[
            "ApprovalNull",
            "ApprovalCreated",
            "ApprovalDetached",
            "ApprovalDischarged",
            "ApprovalExpired",
            "ApprovalViolated",
            "ApprovalCancelled",
            "ApprovalReleased",
        ],
        admitted: &[
            ("ApprovalNull", "ApprovalCreated"),
            ("ApprovalCreated", "ApprovalDetached"),
            ("ApprovalCreated", "ApprovalExpired"),
            ("ApprovalCreated", "ApprovalCancelled"),
            ("ApprovalCreated", "ApprovalReleased"),
            ("ApprovalDetached", "ApprovalDischarged"),
            ("ApprovalDetached", "ApprovalViolated"),
            ("ApprovalDetached", "ApprovalReleased"),
        ],
    },
    Axis {
        constraint: "EffectProtocolPositionTransitionConstraint",
        positions: &[
            "EffectIntended",
            "EffectAttempted",
            "EffectReceipted",
            "EffectReconciling",
            "EffectReconciled",
            "EffectCompensating",
            "EffectCompensated",
            "EffectAbandoned",
        ],
        admitted: &[
            ("EffectIntended", "EffectAttempted"),
            ("EffectIntended", "EffectAbandoned"),
            ("EffectAttempted", "EffectReceipted"),
            ("EffectAttempted", "EffectReconciling"),
            ("EffectAttempted", "EffectAbandoned"),
            ("EffectReceipted", "EffectCompensating"),
            ("EffectReconciling", "EffectReconciled"),
            ("EffectReconciling", "EffectAbandoned"),
            ("EffectReconciled", "EffectCompensating"),
            ("EffectCompensating", "EffectCompensated"),
            ("EffectCompensating", "EffectReconciling"),
        ],
    },
    Axis {
        constraint: "RecurrenceStateTransitionConstraint",
        positions: &[
            "RecurrenceScheduled",
            "RecurrenceActive",
            "RecurrenceSuspended",
            "RecurrenceSkipped",
            "RecurrenceEnded",
        ],
        admitted: &[
            ("RecurrenceScheduled", "RecurrenceActive"),
            ("RecurrenceScheduled", "RecurrenceSkipped"),
            ("RecurrenceScheduled", "RecurrenceSuspended"),
            ("RecurrenceScheduled", "RecurrenceEnded"),
            ("RecurrenceActive", "RecurrenceSuspended"),
            ("RecurrenceActive", "RecurrenceEnded"),
            ("RecurrenceSuspended", "RecurrenceScheduled"),
            ("RecurrenceSuspended", "RecurrenceActive"),
            ("RecurrenceSuspended", "RecurrenceSkipped"),
            ("RecurrenceSuspended", "RecurrenceEnded"),
        ],
    },
];

/// Judge `moves`, given as Turtle statements over the `logic:` prefix, and return each
/// condemned `(subject local name, law local name)`.
fn condemned(laws: &ViolationLowering, moves: &str) -> BTreeSet<(String, String)> {
    let source = format!("@prefix logic: <{LOGIC}> . @prefix ex: <https://example.org/> . {moves}");
    let edb = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None)
        .expect("generated moves are valid Turtle");
    enactment_gate_markers_with_laws(&edb, &[], laws)
        .expect("the compiled laws run over the generated moves")
        .into_iter()
        .map(|violation| {
            (
                violation
                    .subject
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
                violation
                    .law
                    .rsplit('/')
                    .next()
                    .unwrap_or_default()
                    .to_owned(),
            )
        })
        .collect()
}

/// A move record from `from` to `to`, named after both positions.
fn move_record(from: &str, to: &str) -> String {
    format!(
        "ex:{from}_{to} a logic:AxisMove ; logic:moveFromPosition logic:{from} ; \
         logic:moveToPosition logic:{to} .\n"
    )
}

/// Every ordered pair of positions of axes 1, 2, 3, 4 and 7 is admitted exactly when the
/// design's transition table lists it, and a refused pair is condemned under its own axis
/// law alone.
///
/// Protects the contract that the transition formulas are executed rather than described: a
/// move out of a quiescent position, a self move, a skipped edge and the omitted
/// `ApprovalDetached` to `ApprovalCancelled` edge are refused, while every listed edge
/// passes untouched.
pub(super) fn assert_axis_transition_laws_decide_every_pair(laws: &ViolationLowering) {
    for axis in &AXES {
        let mut moves = String::new();
        let mut expected = BTreeSet::new();
        for from in axis.positions {
            for to in axis.positions {
                moves.push_str(&move_record(from, to));
                if !axis.admitted.contains(&(*from, *to)) {
                    expected.insert((format!("{from}_{to}"), axis.constraint.to_owned()));
                }
            }
        }
        assert_eq!(
            condemned(laws, &moves),
            expected,
            "the {} verdicts must match the design's transition table pair for pair",
            axis.constraint
        );
    }
}

/// A move whose ends belong to different axes, or that lacks an end, is refused.
///
/// Protects the boundary of the per-axis laws: a pair spanning two axes is judged by the law
/// of its origin and refused, and a move naming only one end is condemned by the completeness
/// law instead of passing every transition law unjudged.
pub(super) fn assert_cross_axis_and_incomplete_moves_are_refused(laws: &ViolationLowering) {
    let moves = format!(
        "{}{}{}",
        move_record("EnactmentPending", "StepReady"),
        "ex:noDestination a logic:AxisMove ; logic:moveFromPosition logic:StepPending .\n",
        "ex:noOrigin a logic:AxisMove ; logic:moveToPosition logic:StepReady .\n",
    );
    assert_eq!(
        condemned(laws, &moves),
        BTreeSet::from([
            (
                "EnactmentPending_StepReady".to_owned(),
                "EnactmentStateTransitionConstraint".to_owned()
            ),
            (
                "noDestination".to_owned(),
                "AxisMoveNamesBothPositionsConstraint".to_owned()
            ),
            (
                "noOrigin".to_owned(),
                "AxisMoveNamesBothPositionsConstraint".to_owned()
            ),
        ])
    );
}

/// The shipped laws, hydrated from the producer-selected native law inventory, decide every
/// pair of the five axes as the design's tables state.
#[test]
fn shipped_axis_laws_decide_every_pair() {
    assert_axis_transition_laws_decide_every_pair(super::compiled_law_report());
}

/// The shipped laws refuse cross-axis and incomplete moves.
#[test]
fn shipped_axis_laws_refuse_cross_axis_and_incomplete_moves() {
    assert_cross_axis_and_incomplete_moves_are_refused(super::compiled_law_report());
}
