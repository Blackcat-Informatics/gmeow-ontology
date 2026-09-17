// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .unwrap()
}

#[test]
fn pydantic_stage_emits_all_authenticated_artifacts() {
    let stage = PydanticStage::new();
    assert_eq!(stage.id(), "stage-export-pydantic");
    let root = repo_root();
    let first = crate::fixture::stage_artifacts(&root, 1, "stage-export-pydantic")
        .expect("authenticated Pydantic package projection");

    for member in [
        "gmeow_models/models.py",
        "gmeow_models/_base.py",
        "gmeow_models/__init__.py",
        "gmeow_models/py.typed",
        "gmeow_models/__about__.py",
    ] {
        let path = format!("{PACKAGE_DISK_PREFIX}{member}");
        assert!(first.contains_key(&path), "missing {path}");
    }

    let models_py =
        String::from_utf8(first[&format!("{PACKAGE_DISK_PREFIX}gmeow_models/models.py")].clone())
            .expect("models.py utf8");
    assert!(models_py.contains("class "), "models.py has no class");
    assert!(
        models_py.contains("PurrdfBaseModel"),
        "models.py does not use the purrdf base class"
    );

    let about_py = String::from_utf8(
        first[&format!("{PACKAGE_DISK_PREFIX}gmeow_models/__about__.py")].clone(),
    )
    .expect("about.py utf8");
    assert!(
        about_py.contains("__version__"),
        "__about__.py does not carry a version binding:\n{about_py}"
    );
}
