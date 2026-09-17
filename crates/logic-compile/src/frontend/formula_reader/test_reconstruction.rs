// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Per-reader reconstruction counting for the memoization controls.

#[derive(Default)]
pub(super) struct Observer {
    pub(super) reconstructions: usize,
}

impl Observer {
    pub(super) fn record(&mut self) {
        self.reconstructions += 1;
    }
}
