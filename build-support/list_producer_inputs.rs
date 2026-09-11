// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! List the exact local files capable of changing one source-built producer binary.
//!
//! This is compiled directly with `rustc` by the CI fingerprint script, before Cargo
//! cache admission. It deliberately has no package or third-party dependency.

use std::io::Write;
use std::path::PathBuf;

mod producer_inputs;

/// Write workspace-relative producer input paths as a NUL-delimited stream.
///
/// Read the root crate from the first CLI argument. Inventory, path encoding, and
/// output failures abort the fingerprint helper rather than emitting a partial identity.
fn main() {
    let root_crate = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: list_producer_inputs CRATE_DIR");
    let workspace = std::env::current_dir().expect("current workspace directory");
    let root_crate = workspace.join(root_crate);
    let inputs = producer_inputs::paths(&workspace, &root_crate);

    let mut stdout = std::io::stdout().lock();
    for path in inputs {
        let relative = path.strip_prefix(&workspace).unwrap_or(&path);
        stdout
            .write_all(relative.as_os_str().as_encoded_bytes())
            .expect("write producer input path");
        stdout.write_all(&[0]).expect("write NUL separator");
    }
}
