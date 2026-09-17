// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn option_code_is_one_word() {
    assert_eq!(
        std::mem::size_of::<Option<Code>>(),
        std::mem::size_of::<u32>()
    );
}

#[test]
fn register_then_intern_round_trips() {
    let mut reg = CodeRegistry::new();
    let c = reg.register("shacl.nonconforming");
    assert_eq!(reg.intern("shacl.nonconforming").unwrap(), c);
    assert_eq!(reg.as_str(c), "shacl.nonconforming");
}

#[test]
fn register_is_idempotent() {
    let mut reg = CodeRegistry::new();
    let a = reg.register("discipline/stereotype");
    let b = reg.register("discipline/stereotype");
    assert_eq!(a, b);
    assert_eq!(reg.len(), 1);
}

#[test]
fn unregistered_code_is_a_hard_fail() {
    let reg = CodeRegistry::new();
    assert_eq!(
        reg.intern("bogus.unregistered"),
        Err(UnknownCode("bogus.unregistered".to_owned()))
    );
}

#[test]
fn seed_registers_a_batch() {
    let mut reg = CodeRegistry::new();
    reg.seed(&["a.one", "b.two", "c.three"]);
    assert_eq!(reg.len(), 3);
    assert!(reg.contains("b.two"));
}

#[test]
fn open_space_admits_arbitrary_static_codes() {
    // Phase-4 preservation rungs (or any new family) fit without a closed enum.
    let mut reg = CodeRegistry::new();
    let rung = reg.register("preservation.rung.section-retraction");
    assert_eq!(reg.as_str(rung), "preservation.rung.section-retraction");
}

#[test]
fn global_registry_interns_after_registration() {
    // A code unique to this test so it cannot collide with other tests that
    // share the process-global registry.
    register_code("test.code.global-roundtrip");
    assert!(intern_code("test.code.global-roundtrip").is_ok());
    assert!(intern_code("test.code.never-registered-xyz").is_err());
}
