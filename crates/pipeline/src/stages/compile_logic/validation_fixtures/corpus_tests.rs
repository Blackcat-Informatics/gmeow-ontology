// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Read-only consumers of exact producer-selected validation outputs.

use super::{CHANNEL, ModuleProduct, ShapeProduct, ValidationFixtures};
use std::sync::OnceLock;

mod procedural_constraints;
mod shape_migration_equivalence;
mod substrate_reconciliation;

fn fixtures() -> &'static ValidationFixtures {
    static FIXTURES: OnceLock<ValidationFixtures> = OnceLock::new();
    FIXTURES.get_or_init(|| {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let bytes = crate::fixture::authenticated_artifact(&root, "stage-compile-logic", CHANNEL)
            .expect("authenticated producer-selected validation fixtures");
        serde_json::from_slice(&bytes).expect("hydrate complete validation fixture outputs")
    })
}

fn module(path: &str) -> &'static ModuleProduct {
    let module = fixtures()
        .modules
        .get(path)
        .unwrap_or_else(|| panic!("producer did not select {path}"))
        .as_ref()
        .unwrap_or_else(|error| panic!("{path}: {error}"));
    let malformed: Vec<_> = module
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == "MALFORMED_CONSTRAINT")
        .collect();
    assert!(
        malformed.is_empty(),
        "{path} has malformed constraints: {malformed:?}"
    );
    module
}

fn findings(module_path: &str, input: &str) -> Vec<(String, String)> {
    module(module_path);
    let observation = fixtures()
        .cases
        .iter()
        .find(|case| case.module == module_path && case.input == input)
        .unwrap_or_else(|| panic!("producer did not select {module_path} over {input}"));
    observation
        .findings
        .as_ref()
        .unwrap_or_else(|error| panic!("{module_path} over {input}: {error}"))
        .clone()
}

impl std::ops::Deref for ShapeProduct {
    type Target = gmeow_logic_compile::ir::ValidationShapeIr;
    fn deref(&self) -> &Self::Target {
        &self.ir
    }
}
