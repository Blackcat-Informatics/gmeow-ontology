// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The axis→producer binding gate and the projection-target completeness gate,
//! run against the real committed rubric — with the binding gate resolving each
//! producer through the real constitution-gate AST resolver
//! (`gmeow_validate::constitution::rust_item_names`) over this crate's source, so
//! the tests prove real symbol resolution rather than list membership.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use gmeow_slice_quality::gate::{binding_gate, completeness_gate};
use gmeow_slice_quality::model::{
    Axis, ContextScope, GovernanceFloors, MeasurementStandard, Rubric,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn rubric() -> Rubric {
    gmeow_slice_quality::load_repo_rubric(&repo_root()).unwrap()
}

/// Walk `.rs` files under `dir`, calling `f` with each file's text.
fn scan_rs(dir: &Path, f: &mut impl FnMut(&str)) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            if p.file_name().is_some_and(|n| n == "target") {
                continue;
            }
            scan_rs(&p, f);
        } else if p.extension().is_some_and(|x| x == "rs")
            && let Ok(text) = std::fs::read_to_string(&p)
        {
            f(&text);
        }
    }
}

/// The set of every Rust item name defined in this crate's `src/` — the real
/// primitive set the binding gate resolves producers against.
fn primitive_symbols() -> HashSet<String> {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut names = HashSet::new();
    scan_rs(&src, &mut |text| {
        names.extend(gmeow_validate::constitution::rust_item_names(text));
    });
    names
}

/// A minimal axis binding the given producer.
fn mk_axis(producer: &str) -> Axis {
    Axis {
        iri: format!("ex:{producer}"),
        label: String::new(),
        producer: producer.to_owned(),
        dimension_iri: "ex:d".to_owned(),
        thresholds: vec![],
        weight: 1.0,
        scope: ContextScope::SliceLocal,
        advice: String::new(),
    }
}

#[test]
fn axis_producers_bind_bijectively_to_implemented_primitives() {
    // (c) The real, committed rubric: all fourteen producers must resolve to real Rust
    // items in this crate's source AND be in bijection with IMPLEMENTED.
    let symbols = primitive_symbols();
    let errs = binding_gate(&rubric(), |s| symbols.contains(s));
    assert!(
        errs.is_empty(),
        "axis↔producer binding must be a bijection resolving to real items: {errs:#?}"
    );
    // Sanity: the resolver actually found the real primitives (not an empty set).
    assert!(
        symbols.contains("grounding_axis") && symbols.contains("reasoner_axis"),
        "the AST resolver must see the real primitive fns"
    );
}

#[test]
fn binding_gate_reds_on_prefix_producer_against_real_source() {
    // (a) A producer that is a strict PREFIX of a real primitive item
    // (`grounding_ax` ⊂ `grounding_axis`) must red when resolved against the real
    // crate source — proving the substring/prefix false positive is gone. A naive
    // `text.contains("fn grounding_ax")` would have matched `fn grounding_axis`.
    let symbols = primitive_symbols();
    assert!(
        !symbols.contains("grounding_ax"),
        "the prefix is not itself a defined item"
    );
    let rubric = Rubric {
        standard: MeasurementStandard {
            tiers: vec![],
            axes: vec![mk_axis("grounding_ax")],
        },
        floors: GovernanceFloors::default(),
    };
    let errs = binding_gate(&rubric, |s| symbols.contains(s));
    assert!(
        errs.iter().any(|e| e.contains("grounding_ax")
            && e.contains("resolves to no Rust primitive item")),
        "a strict-prefix producer must red against real resolution: {errs:#?}"
    );
}

#[test]
fn binding_gate_reds_on_producer_with_no_axes_item() {
    // (b) A rubric producer that names no item in the crate source must red the
    // binding gate, even against the real resolver.
    let symbols = primitive_symbols();
    let rubric = Rubric {
        standard: MeasurementStandard {
            tiers: vec![],
            axes: vec![mk_axis("no_such_primitive_symbol")],
        },
        floors: GovernanceFloors::default(),
    };
    let errs = binding_gate(&rubric, |s| symbols.contains(s));
    assert!(
        errs.iter().any(|e| e.contains("no_such_primitive_symbol")
            && e.contains("resolves to no Rust primitive item")),
        "an unresolvable producer must red: {errs:#?}"
    );
}

#[test]
fn every_projection_surface_is_covered_by_an_axis_or_a_dated_exemption() {
    let errs = completeness_gate(&rubric());
    assert!(errs.is_empty(), "projection completeness: {errs:#?}");
}
