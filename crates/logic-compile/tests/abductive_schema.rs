//! The abductive-schema vocabulary (`logic:AbductiveSchema` + its four
//! `logic:completenessFormula` trees) in `slices/grounding/logic/module.ttl` must:
//!   1. parse cleanly (no `MALFORMED_FORMULA` / `MALFORMED_CONSTRAINT`), and
//!   2. keep every completeness formula OUT of the top-level formula set while remaining
//!      reconstructable — so authoring a completeness condition never asserts it as an
//!      always-true axiom (which would corrupt the reasoned core and auto-assert the very
//!      structure the abductive advice only RECOMMENDS adding).

use std::path::PathBuf;

use gmeow_logic_compile::frontend::{parse_logic_str, reconstruct_formula};

const LOGIC: &str = "https://blackcatinformatics.ca/logic/";

/// Every schema's `logic:completenessFormula` root (local name). `besForall` is the sortal
/// schema's reused constraint-integrity disjunction; the other three are schema-owned.
const COMPLETENESS_ROOTS: &[&str] = &[
    "relatorMediationComplete",
    "referenceFrameComplete",
    "wemiChainComplete",
    "besForall",
];

fn repo_root() -> PathBuf {
    // crates/logic-compile/tests -> crates/logic-compile -> crates -> repo root
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root")
        .to_path_buf()
}

fn module_text() -> String {
    std::fs::read_to_string(repo_root().join("slices/grounding/logic/module.ttl"))
        .expect("read logic module.ttl")
}

#[test]
fn abductive_vocabulary_parses_without_malformed_nodes() {
    let src = module_text();
    let (_program, diags) = parse_logic_str(&src, None).expect("logic module parses");
    let malformed: Vec<String> = diags
        .iter()
        .filter(|d| d.code == "MALFORMED_FORMULA" || d.code == "MALFORMED_CONSTRAINT")
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    assert!(
        malformed.is_empty(),
        "abductive vocabulary introduced malformed nodes: {malformed:?}"
    );
}

#[test]
fn completeness_formulas_reconstruct_but_never_become_axioms() {
    let src = module_text();
    let dataset =
        purrdf::parse_dataset(src.as_bytes(), "text/turtle", None).expect("dataset parses");
    let (program, _diags) = parse_logic_str(&src, None).expect("logic module parses");

    for root in COMPLETENESS_ROOTS {
        let iri = format!("{LOGIC}{root}");
        // (2a) Reconstructable as a well-formed first-order formula through the public entry.
        let formula = reconstruct_formula(dataset.as_ref(), &iri)
            .unwrap_or_else(|e| panic!("completeness root {iri} must reconstruct: {}", e.message()));
        // (2b) NOT asserted as a free-standing top-level formula.
        assert!(
            !program.formulas.contains(&formula),
            "completeness formula {iri} leaked into the top-level formula set (would be asserted \
             as an always-true axiom)"
        );
    }

    // The schema's structural edges never leak into the domain axiom set.
    for pred_local in ["completenessFormula", "repairStrategy", "repairsDiscipline"] {
        let pred = format!("{LOGIC}{pred_local}");
        assert!(
            !program.axioms.iter().any(|ax| ax.predicate == pred),
            "logic:{pred_local} leaked into prog.axioms"
        );
    }
}
