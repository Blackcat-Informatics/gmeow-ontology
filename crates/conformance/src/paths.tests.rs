// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn case_dir_is_the_sentinel_parent() {
    let sentinel = Path::new("/repo/conformance/logic/cases/foundation/free-role/profile.json");
    assert_eq!(
        case_dir(sentinel),
        Path::new("/repo/conformance/logic/cases/foundation/free-role")
    );
}

#[test]
fn case_id_is_category_slash_case() {
    let dir = Path::new("/repo/conformance/logic/cases/foundation/free-role");
    assert_eq!(case_id(dir), "foundation/free-role");
}

#[test]
fn case_id_preserves_external_prefix_for_three_level_cases() {
    // A vendored external corpus case keeps its `external/` prefix.
    let dir = Path::new("/repo/conformance/logic/cases/external/w3c-mini/clash");
    assert_eq!(case_id(dir), "external/w3c-mini/clash");
}

#[test]
fn case_id_works_on_relative_harness_paths() {
    // The datatest harness passes paths relative to the crate dir.
    let dir = Path::new("../../conformance/logic/cases/foundation/free-role");
    assert_eq!(case_id(dir), "foundation/free-role");
}
