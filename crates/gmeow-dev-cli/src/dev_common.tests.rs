// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

/// Resolve nested invocations locally and never replace an unrelated selected directory.
#[test]
fn checkout_discovery_preserves_the_selected_tree_without_a_compiled_fallback() {
    let directory = tempfile::tempdir().expect("isolated selection");
    let selected = directory.path().join("checkout");
    let nested = selected.join("crates/example/src");
    std::fs::create_dir_all(&nested).expect("nested invocation directory");
    std::fs::create_dir(selected.join("slices")).expect("source tree");
    std::fs::write(selected.join("Cargo.toml"), b"[workspace]\n").expect("marker");
    assert_eq!(enclosing_checkout(&nested), selected);
    assert_eq!(enclosing_checkout(&selected), selected);
    let unrelated = directory.path().join("unrelated");
    std::fs::create_dir(&unrelated).expect("unrelated invocation directory");
    assert_eq!(enclosing_checkout(&unrelated), unrelated);
}
