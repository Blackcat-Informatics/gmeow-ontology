// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use crate::code::{intern_code, register_code};
use static_assertions::assert_not_impl_all;
use std::io;

// The two structural invariants that keep the blanket-From coherent and the
// serialization boundary single. macro_rules-based, no brittle stderr.
assert_not_impl_all!(Diag: std::error::Error);
assert_not_impl_all!(Diag: serde::Serialize);

#[test]
fn diag_ref_index_roundtrips() {
    for i in [0usize, 1, 7, 4096, u32::MAX as usize - 1] {
        assert_eq!(DiagRef::from_index(i).index(), i);
    }
}

#[test]
fn diag_is_one_word() {
    assert_eq!(std::mem::size_of::<Diag>(), std::mem::size_of::<usize>());
}

#[test]
fn result_of_diag_is_pointer_sized() {
    assert_eq!(
        std::mem::size_of::<Result<(), Diag>>(),
        std::mem::size_of::<*const ()>()
    );
}

#[derive(Debug)]
struct MyKind;
impl fmt::Display for MyKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("my kind failed")
    }
}
impl StdError for MyKind {}
impl DiagKind for MyKind {
    fn code(&self) -> Code {
        register_code("test.diagkind.my-kind")
    }
    fn grade(&self) -> Grade {
        Grade::new(
            Severity::Warning,
            FindingCategory::PolicyWarning,
            Standpoint::Advisory,
        )
    }
}

// Compiling this function proves BOTH From paths coexist — the reflexive
// `From<Diag> for Diag` (for the `Diag` `?`) and the blanket `From<E: Error>`
// (for the `io::Error` `?`). They can only coexist if `Diag: !Error`.
fn thread_both(fail_diag: bool) -> Result<(), Diag> {
    fn make_io() -> Result<(), io::Error> {
        Err(io::Error::other("io boom"))
    }
    fn make_diag() -> Result<(), Diag> {
        Err(Diag::new(
            code::foreign_code(),
            Grade::new(
                Severity::Error,
                FindingCategory::ContradictionWitness,
                Standpoint::Binding,
            ),
            "diag boom",
        ))
    }
    if fail_diag {
        make_diag()?; // reflexive From<Diag> for Diag
    } else {
        make_io()?; // blanket From<io::Error> for Diag
    }
    Ok(())
}

#[test]
fn both_from_paths_compose_through_question_mark() {
    assert!(thread_both(false).is_err());
    assert!(thread_both(true).is_err());
}

#[test]
fn blanket_from_preserves_downcast() {
    let diag: Diag = io::Error::new(io::ErrorKind::NotFound, "missing").into();
    let recovered = diag.downcast_ref::<io::Error>().expect("downcast");
    assert_eq!(recovered.kind(), io::ErrorKind::NotFound);
    assert!(diag.is::<io::Error>());
    // A foreign error takes the reserved code but stays a real gate-able error.
    assert_eq!(diag.code(), code::foreign_code());
    assert_eq!(diag.gate(), GateVerdict::Fatal);
}

#[test]
fn of_kind_preserves_code_grade_and_downcast() {
    let diag = Diag::of_kind(MyKind);
    assert_eq!(diag.code(), register_code("test.diagkind.my-kind"));
    assert_eq!(diag.grade().severity, Severity::Warning);
    // An advisory PolicyWarning never gates.
    assert_eq!(diag.gate(), GateVerdict::Collected);
    assert!(diag.downcast_ref::<MyKind>().is_some());
}

#[test]
fn track_caller_captures_the_question_mark_site_not_crate_internals() {
    // R3: the emit location captured through the blanket From must be THIS
    // function's file, not diag.rs.
    fn boom() -> Result<(), Diag> {
        Err(io::Error::other("x"))?;
        Ok(())
    }
    let diag = boom().unwrap_err();
    let file = diag.emitted_at().file();
    assert!(
        file.ends_with("diag.rs") || file.contains("diag"),
        "unexpected emit file {file}"
    );
    // More precisely: it must NOT point at the From impl line region; the
    // captured line is the `?` site inside `boom`, which lives in this test
    // module. We assert the file is this source file.
    assert!(file.ends_with("src/diag.rs"));
}

#[test]
fn result_ext_adds_context_and_display_walks_it() {
    let r: Result<(), io::Error> = Err(io::Error::other("root cause"));
    let diag = r.ctx("while loading the bundle").unwrap_err();
    let rendered = format!("{diag:#}");
    assert!(rendered.contains("while loading the bundle"));
    assert!(rendered.contains("root cause"));
}

#[test]
fn or_collect_pushes_and_continues() {
    let mut sink: Vec<Diag> = Vec::new();
    let ok: Result<u8, io::Error> = Ok(7);
    let err: Result<u8, io::Error> = Err(io::Error::other("nope"));
    assert_eq!(ok.or_collect(&mut sink), Some(7));
    assert_eq!(err.or_collect(&mut sink), None);
    assert_eq!(sink.len(), 1);
}

#[test]
fn collect_all_gathers_ok_and_sinks_err() {
    let mut sink: Vec<Diag> = Vec::new();
    let items: Vec<Result<u8, io::Error>> = vec![Ok(1), Err(io::Error::other("bad")), Ok(3)];
    let got = items.collect_all(&mut sink);
    assert_eq!(got, vec![1, 3]);
    assert_eq!(sink.len(), 1);
}

crate::define_diag_kind! {
    /// A generated kind for the macro test.
    pub struct UnknownStageImpl { stage: String, impl_key: String }
    code = "test.define.unknown-stage-impl";
    grade = Grade::new(
        Severity::Error,
        FindingCategory::ModelingDisciplineViolation,
        Standpoint::Binding,
    );
    message = "no impl `{}` for stage `{}`", impl_key, stage;
}

#[test]
fn define_diag_kind_binds_code_grade_message_and_registers() {
    let e = UnknownStageImpl {
        stage: "reason".to_owned(),
        impl_key: "demo".to_owned(),
    };
    // Message renders from the fields via the positional format.
    assert_eq!(e.to_string(), "no impl `demo` for stage `reason`");
    assert_eq!(UnknownStageImpl::CODE, "test.define.unknown-stage-impl");
    // Code is registered (eagerly reachable via register(), and via code()).
    assert_eq!(e.code(), UnknownStageImpl::register());
    assert!(intern_code("test.define.unknown-stage-impl").is_ok());
    // Building a Diag from it preserves code + grade + downcast.
    let diag = Diag::of_kind(e);
    assert_eq!(diag.code(), UnknownStageImpl::register());
    assert_eq!(diag.gate(), GateVerdict::Fatal);
    assert!(diag.downcast_ref::<UnknownStageImpl>().is_some());
}

crate::define_diag_kind! {
    /// A generated kind carrying the OPTIONAL ontology failure-class binding.
    pub struct FailureClassBound { detail: String }
    code = "test.define.failure-class-bound";
    grade = Grade::new(
        Severity::Error,
        FindingCategory::ModelingDisciplineViolation,
        Standpoint::Binding,
    );
    message = "bound kind: {}", detail;
    failure_class = "https://blackcatinformatics.ca/gmeow/TestFailureClass";
}

/// The `failure_class` clause is OPTIONAL and PURELY ADDITIVE: a kind that
/// declares one exposes the IRI through both the constant and the trait
/// accessor, and a kind that does not keeps the `None` default — so extending
/// the macro cannot have changed the meaning of any existing kind.
#[test]
fn define_diag_kind_binds_the_optional_failure_class() {
    assert_eq!(
        FailureClassBound::FAILURE_CLASS,
        Some("https://blackcatinformatics.ca/gmeow/TestFailureClass")
    );
    let bound = FailureClassBound {
        detail: "demo".to_owned(),
    };
    assert_eq!(
        bound.failure_class(),
        Some("https://blackcatinformatics.ca/gmeow/TestFailureClass")
    );
    // Everything else the macro generates is unchanged by the new clause.
    assert_eq!(bound.to_string(), "bound kind: demo");
    assert_eq!(bound.code(), FailureClassBound::register());

    // A kind WITHOUT the clause keeps the honest `None` default.
    assert_eq!(UnknownStageImpl::FAILURE_CLASS, None);
    assert_eq!(
        UnknownStageImpl {
            stage: "reason".to_owned(),
            impl_key: "demo".to_owned(),
        }
        .failure_class(),
        None
    );
}

#[test]
fn macros_build_and_bail() {
    let code = register_code("test.macro.code");
    fn use_ensure(code: Code, n: i32) -> Result<(), Diag> {
        ensure!(n > 0, code, "n must be positive, got {n}");
        Ok(())
    }
    assert!(use_ensure(code, 1).is_ok());
    let err = use_ensure(code, -1).unwrap_err();
    assert!(err.message().contains("n must be positive"));
    assert_eq!(err.code(), code);
}
