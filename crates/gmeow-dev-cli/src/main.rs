// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gmeow-dev` binary entrypoint.

fn main() {
    gmeow_cli_core::init_tracing();
    std::process::exit(gmeow_dev_cli::run());
}
