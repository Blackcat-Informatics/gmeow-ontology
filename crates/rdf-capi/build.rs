// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Generate the `purrdf.h` C header from the crate's `extern "C"` surface.
//!
//! The header is generated to `$OUT_DIR/purrdf.h` on every build so the C smoke
//! test (`tests/c_smoke.rs`) always links against a header matching the compiled
//! library. When `PURRDF_WRITE_HEADER=1` is set (the `make capi-header` path),
//! the canonical committed header at `include/purrdf.h` — the SemVer-frozen ABI
//! contract — is refreshed too. cargo-c regenerates its own copy during
//! `cargo cbuild`; the drift gate diffs that against the committed header.

use std::path::Path;

fn main() {
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR");

    let config = cbindgen::Config::from_root_or_default(Path::new(&crate_dir));
    let bindings = cbindgen::Builder::new()
        .with_crate(&crate_dir)
        .with_config(config)
        .generate()
        .expect("cbindgen failed to generate the purrdf C header");

    // Always emit to OUT_DIR for the C smoke test.
    let out_header = Path::new(&out_dir).join("purrdf.h");
    bindings.write_to_file(&out_header);

    // Refresh the committed contract only when explicitly asked (header-regen path).
    if std::env::var("PURRDF_WRITE_HEADER").as_deref() == Ok("1") {
        let committed = Path::new(&crate_dir).join("include").join("purrdf.h");
        if let Some(parent) = committed.parent() {
            std::fs::create_dir_all(parent).expect("create include/ dir");
        }
        bindings.write_to_file(&committed);
    }

    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=cbindgen.toml");
    println!("cargo:rerun-if-env-changed=PURRDF_WRITE_HEADER");
    // Export the generated header dir so the C smoke test can find it.
    println!("cargo:include={out_dir}");
}
