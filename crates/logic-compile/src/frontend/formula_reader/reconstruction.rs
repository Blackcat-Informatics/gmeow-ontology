// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The production reconstruction observer has no storage or observable work.

#[derive(Default)]
pub(super) struct Observer {}

impl Observer {
    pub(super) fn record(&mut self) {}
}
