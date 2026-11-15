// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gmeow` binary entrypoint.

fn main() {
    if let Err(error) = gmeow_cli_core::init_tracing() {
        std::process::exit(gmeow_cli::report_stage_memory_log_failure(&error));
    }
    std::process::exit(gmeow_cli::run());
}
