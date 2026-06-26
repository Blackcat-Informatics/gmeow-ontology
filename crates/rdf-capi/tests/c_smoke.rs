// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Drives the C smoke test (`tests/smoke.c`): it compiles the C program against
//! the committed `include/purrdf.h`, links it against the freshly built
//! `libpurrdf` shared library, runs it, and asserts it exits zero. This proves
//! the REAL C-ABI (header + linkage), not just Rust calling Rust.

#![cfg(not(miri))]

use std::path::PathBuf;
use std::process::Command;

#[test]
fn c_abi_smoke() {
    let manifest = env!("CARGO_MANIFEST_DIR");
    let smoke_c = format!("{manifest}/tests/smoke.c");
    let header_dir = format!("{manifest}/include");

    // The integration-test binary lives at `<target>/<profile>/deps/<name>-<hash>`,
    // so its grandparent is the profile dir where the cdylib is emitted. This is
    // robust to a custom `CARGO_TARGET_DIR`.
    let test_exe = std::env::current_exe().expect("current_exe");
    let profile_dir: PathBuf = test_exe
        .parent()
        .and_then(|deps| deps.parent())
        .expect("profile dir")
        .to_path_buf();

    let lib = profile_dir.join("libpurrdf.so");
    assert!(
        lib.exists(),
        "libpurrdf.so not found at {} — the cdylib should be built with the crate",
        lib.display()
    );

    let cc = std::env::var("CC").unwrap_or_else(|_| "cc".to_string());
    let bin = profile_dir.join("purrdf_c_smoke");

    let compile = Command::new(&cc)
        .arg(&smoke_c)
        .arg("-std=c11")
        .arg(format!("-I{header_dir}"))
        .arg(format!("-L{}", profile_dir.display()))
        .arg("-lpurrdf")
        .arg("-o")
        .arg(&bin)
        .status()
        .expect("failed to invoke the C compiler");
    assert!(compile.success(), "C smoke failed to compile/link");

    let run = Command::new(&bin)
        .env("LD_LIBRARY_PATH", &profile_dir)
        .status()
        .expect("failed to run the C smoke binary");
    assert!(run.success(), "C smoke binary returned a failure exit code");
}
