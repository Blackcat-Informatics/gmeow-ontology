// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The explicit display projection of a native refinement report.
//! All roster and proof fields survive unchanged. Outcome explanations retain their
//! original text; this view cannot replace an operation receipt or authorize a rewrite.

use serde::{Deserialize, Serialize};

use crate::reason::enactment::refine::{RefineCandidate, RefinePin, RefineRejection, RefineReport};
use crate::runtime::OperationOutcome;

/// The exact refinement fields consumed by the public operator surface.
#[derive(Debug, Serialize, Deserialize)]
pub struct RefinementRecord {
    pub task: String,
    pub fragment: String,
    pub candidates: Vec<RefineCandidate>,
    pub rejections: Vec<RefineRejection>,
    pub reached: Vec<String>,
    pub pins: Vec<RefinePin>,
    pub cycles: Vec<String>,
    pub outcome: RefinementOutcome,
}

/// A display disposition, never a resumable native session or coverage certificate.
#[derive(Debug, Serialize, Deserialize)]
pub enum RefinementOutcome {
    Applied {
        consumed_steps: u64,
        new_state_hash: String,
    },
    Incomplete {
        status: crate::seam::BudgetStatus,
        cause: String,
    },
    UnsupportedFragment {
        kind: String,
    },
    Invalid {
        fault: String,
    },
    EngineFailure {
        diagnostic: String,
    },
    Unsettled {
        detail: String,
    },
}

impl From<&RefineReport> for RefinementRecord {
    fn from(report: &RefineReport) -> Self {
        let outcome = match &report.outcome {
            OperationOutcome::Applied {
                run,
                new_state_hash,
            } => RefinementOutcome::Applied {
                consumed_steps: run.consumed_steps,
                new_state_hash: new_state_hash.clone(),
            },
            OperationOutcome::Incomplete { status, cause } => RefinementOutcome::Incomplete {
                status: *status,
                cause: format!("{cause:?}"),
            },
            OperationOutcome::UnsupportedFragment { kind } => {
                RefinementOutcome::UnsupportedFragment {
                    kind: format!("{kind:?}"),
                }
            }
            OperationOutcome::Invalid { fault } => RefinementOutcome::Invalid {
                fault: format!("{fault:?}"),
            },
            OperationOutcome::EngineFailure { diagnostic } => RefinementOutcome::EngineFailure {
                diagnostic: format!("{diagnostic:?}"),
            },
            other => RefinementOutcome::Unsettled {
                detail: format!("{other:?}"),
            },
        };
        Self {
            task: report.task.clone(),
            fragment: report.fragment.clone(),
            candidates: report.candidates.clone(),
            rejections: report.rejections.clone(),
            reached: report.reached.clone(),
            pins: report.pins.clone(),
            cycles: report.cycles.clone(),
            outcome,
        }
    }
}
