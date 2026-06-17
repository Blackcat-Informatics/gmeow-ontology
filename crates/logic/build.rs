// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use std::path::PathBuf;

fn main() {
    // ring (transitively required by scryer-prolog) ships C/asm objects that are
    // compiled as LTO bitcode. The default rust-lld linker on this toolchain
    // leaves their CPU-feature symbols (e.g. ring_core_*__avx2_available) as
    // unresolved in the gmeow-logic cdylib, which causes a load-time
    // ImportError. GNU ld (bfd) resolves them correctly, so force the C compiler
    // driver to use it for this crate.
    println!("cargo:rustc-link-arg=-fuse-ld=bfd");

    // Cargo does not propagate ring's own cargo:rustc-link-lib instruction to
    // this crate's cdylib link. Extract the two C object files that define the
    // CPU-feature symbols and link them directly so bfd retains them.
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR"));
    let build_dir = out_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("OUT_DIR should have two parent segments");
    if let Some(lib) = find_ring_core_static(build_dir) {
        let obj_dir = out_dir.join("ring_objs");
        std::fs::create_dir_all(&obj_dir).ok();
        extract_objs(&lib, &obj_dir, &["-crypto.o", "-cpu_intel.o"]);
        for entry in std::fs::read_dir(&obj_dir).unwrap().flatten() {
            println!("cargo:rustc-link-arg={}", entry.path().display());
        }
    }
}

fn find_ring_core_static(build_dir: &std::path::Path) -> Option<PathBuf> {
    if let Ok(entries) = std::fs::read_dir(build_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.starts_with("ring-") {
                        let out_dir = path.join("out");
                        if let Ok(out_entries) = std::fs::read_dir(&out_dir) {
                            for out_entry in out_entries.flatten() {
                                let fname = out_entry.file_name();
                                let fname_str = fname.to_string_lossy();
                                if fname_str.starts_with("libring_core_")
                                    && fname_str.ends_with("_.a")
                                    && !fname_str.contains("_test")
                                {
                                    return Some(out_entry.path());
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

fn extract_objs(archive: &std::path::Path, dest: &std::path::Path, suffixes: &[&str]) {
    let output = std::process::Command::new("ar")
        .arg("t")
        .arg(archive)
        .output()
        .expect("ar should be available");
    if !output.status.success() {
        return;
    }
    let listing = String::from_utf8_lossy(&output.stdout);
    for line in listing.lines() {
        if suffixes.iter().any(|s| line.ends_with(s)) {
            let _ = std::process::Command::new("ar")
                .arg("x")
                .arg(archive)
                .arg(line)
                .current_dir(dest)
                .status();
        }
    }
}
