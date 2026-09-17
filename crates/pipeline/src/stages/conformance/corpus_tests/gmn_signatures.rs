// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only admission of the selected GMN signature producer observations.

use std::sync::OnceLock;

use super::super::gmn_signatures::{CHANNEL, Observations};

fn observations() -> &'static Observations {
    static OBSERVATIONS: OnceLock<
        Result<super::source_artifact::Selected<Observations>, gmeow_errors::Diag>,
    > = OnceLock::new();
    super::source_artifact::get(&OBSERVATIONS, CHANNEL)
}

pub(super) fn rows(name: &str) -> usize {
    let case = observations()
        .cases
        .get(name)
        .unwrap_or_else(|| panic!("required GMN signature observation {name} is absent"));
    *case.rows.as_ref().unwrap_or_else(|error| {
        panic!(
            "GMN signature observation {name} ({}): {error}",
            case.query_path
        )
    })
}
