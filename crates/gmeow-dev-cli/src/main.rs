// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The `gmeow-dev` binary entrypoint.

fn main() {
    if let Err(error) = gmeow_cli_core::init_tracing() {
        gmeow_dev_cli::report_stage_memory_log_failure(&error);
        std::process::exit(1);
    }
    let observer = match gmeow_pipeline::allocator_observation::AllocatorObserver::start_from_env()
    {
        Ok(observer) => observer,
        Err(error) => {
            gmeow_dev_cli::report_allocator_observation_failure(&error);
            std::process::exit(1);
        }
    };
    let mut status = gmeow_dev_cli::run();
    if let Some(observer) = observer
        && let Err(error) = observer.finish()
    {
        gmeow_dev_cli::report_allocator_observation_failure(&error);
        if status == 0 {
            status = 1;
        }
    }
    std::process::exit(status);
}
