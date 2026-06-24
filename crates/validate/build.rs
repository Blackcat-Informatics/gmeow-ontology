// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Link the Python runtime for test binaries.
//!
//! The production `gmeow_native` cdylib enables the `extension-module` feature,
//! so PyO3 does not link libpython (the symbols are supplied by the Python
//! executable that loads the extension). Unit tests, however, build the crate
//! without that feature and exercise the PyO3-bound public entry points
//! directly, so the test binary must resolve Python symbols itself.
//!
//! This script only runs when `extension-module` is disabled. It discovers the
//! interpreter's libdir/version via the same Python that PyO3 would use, then
//! emits a late, `--no-as-needed` link of the versioned libpython so the test
//! executable can initialize the interpreter (#937, ETHOS §22).

use std::process::Command;

fn main() {
    if cfg!(feature = "extension-module") {
        return;
    }

    let python = std::env::var("PYO3_PYTHON").unwrap_or_else(|_| "python3".to_string());
    println!("cargo:rerun-if-env-changed=PYO3_PYTHON");

    let libdir = python_stdout(
        &python,
        &[
            "-c",
            "import sysconfig, sys; sys.stdout.write(sysconfig.get_config_var('LIBDIR') or '')",
        ],
    );
    let version = python_stdout(
        &python,
        &[
            "-c",
            "import sys; sys.stdout.write(f'{sys.version_info.major}.{sys.version_info.minor}')",
        ],
    );

    if libdir.is_empty() || version.is_empty() {
        // No usable shared Python at build time; leave the link step to the
        // ambient environment (e.g., CI with a shared interpreter).
        return;
    }

    println!("cargo:rustc-link-search=native={libdir}");
    println!("cargo:rustc-link-arg=-Wl,--no-as-needed");
    println!("cargo:rustc-link-arg=-lpython{version}");
    println!("cargo:rustc-link-arg=-Wl,--as-needed");
}

fn python_stdout(cmd: &str, args: &[&str]) -> String {
    match Command::new(cmd).args(args).output() {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => String::new(),
    }
}
