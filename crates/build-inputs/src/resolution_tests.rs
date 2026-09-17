// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn resolution_environment_tracks_selection_without_cargo_injected_test_noise() {
    let values = |target: &str, temporary: &str| {
        vec![
            ("CARGO_BUILD_TARGET".into(), target.into()),
            ("CARGO_TARGET_TMPDIR".into(), temporary.into()),
            ("CARGO_PKG_VERSION".into(), "1".into()),
        ]
    };
    let root = Path::new("/workspace");
    let first = resolution_environment(root, values("target-one", "first")).unwrap();
    assert_eq!(
        first,
        resolution_environment(root, values("target-one", "second")).unwrap()
    );
    assert_ne!(
        first,
        resolution_environment(root, values("target-two", "first")).unwrap()
    );
    assert!(
        resolution_environment(
            root,
            [("CARGO_SOURCE_VENDOR_DIRECTORY".into(), "/unowned".into())]
        )
        .is_err()
    );
    let left = resolution_environment(
        Path::new("/left"),
        [("RUSTC_WRAPPER".into(), "/left/wrapper".into())],
    )
    .unwrap();
    let right = resolution_environment(
        Path::new("/right"),
        [("RUSTC_WRAPPER".into(), "/right/wrapper".into())],
    )
    .unwrap();
    assert_eq!(left, right);
}
