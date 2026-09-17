// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use purrdf::PageId;

fn stopped(cause: StopCause) -> PagedQueryError {
    PagedQueryError::Stopped {
        page: PageId(0),
        cause,
        message: String::from("test stop"),
    }
}

// The governor merge unified cancellation and deadlines into one `Stopped` variant
// distinguished only by its `StopCause`. `map_paged_error` must keep the two outcomes
// separate: a cancellation and a deadline are different incompleteness causes and a
// caller reads them off `IncompleteCause`. Both arms are exercised here so neither can
// silently collapse into the other under a future dependency bump.
#[test]
fn stopped_cancelled_maps_to_incomplete_cancelled() {
    assert!(matches!(
        map_paged_error(&stopped(StopCause::Cancelled)),
        OperationOutcome::Incomplete {
            status: BudgetStatus::Partial,
            cause: IncompleteCause::Cancelled,
        }
    ));
}

#[test]
fn stopped_deadline_maps_to_incomplete_deadline() {
    assert!(matches!(
        map_paged_error(&stopped(StopCause::Deadline)),
        OperationOutcome::Incomplete {
            status: BudgetStatus::Partial,
            cause: IncompleteCause::Deadline,
        }
    ));
}
