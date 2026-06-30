// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The dogfooded `graph/provenance` projection (#1132 C9).
//!
//! The pipeline's occurrence-based provenance sidecar ([`gmeow_rdf::provenance::DatasetProvenance`]) is
//! projected — THROUGH THE PUBLIC-IRI BOUNDARY (`public_projection`) — into a
//! net-new named graph the bundle carries, so a repo-free consumer reads the full
//! compilation-unit + per-lane carrier manifest WITHOUT re-running the build:
//!
//! * each compilation unit becomes a `gmeow:Procedure`-step input describing its
//!   public name/IRI and its [`OriginKind`](gmeow_rdf::provenance::OriginKind), and
//! * each carrier lane (dataset / logic / reasoning / relational-core / the report
//!   lanes) becomes a `gmeow:ProcedureStep` carrying a `logic:loadBearing` bit
//!   (true = a trim drops correctness, false = a droppable annotation/report lane).
//! * the pipeline itself is one `gmeow:Procedure` enacted by one `gmeow:Execution`.
//!
//! ## S0.5 — no runtime ids
//!
//! NOTHING in this graph derives from a runtime `UnitId` / `ArtifactId` /
//! `OriginSetId`. Every node is built from the PUBLIC strings (`unit name`,
//! `artifact path`, `kind`) the projection exposes, or a fixed lane label — so the
//! emitted bytes never contain `unit#N` / `artifact#N` / `origin-set#N`. The
//! this module's tests prove this on the real ontology.
//!
//! ## Determinism
//!
//! Every collection is sorted before emission and the projection rows are already
//! sorted + deduped by `public_projection`, so the bytes are byte-stable across
//! runs (no timestamps, no iteration-order leakage).

use std::collections::BTreeSet;
use std::fmt::Write as _;

/// The net-new named-graph IRI carrying the dogfooded provenance projection.
pub const GRAPH_PROVENANCE: &str = "https://blackcatinformatics.ca/gmeow/graph/provenance";

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";
const LOGIC: &str = "https://blackcatinformatics.ca/logic/";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const XSD_BOOLEAN: &str = "http://www.w3.org/2001/XMLSchema#boolean";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const RDFS_IS_DEFINED_BY: &str = "http://www.w3.org/2000/01/rdf-schema#isDefinedBy";
const SKOS_DEFINITION: &str = "http://www.w3.org/2004/02/skos/core#definition";

/// The assertional `gmeow:graphBoxRole` value every materialized A-Box individual
/// in this projection carries.
const BOX_ABOX: &str = "https://blackcatinformatics.ca/gmeow/boxABox";

/// The pipeline procedure's success criterion. The canonical build's goal is the
/// shippable bundle and its success mode is deterministic (every execution reaches
/// it), mirroring `gmeow:pipeline-build` authored in `slices/core/pipeline`.
const GOAL_SHIPPABLE_BUNDLE: &str = "https://blackcatinformatics.ca/gmeow/goalShippableBundle";
const STRONG_PLAN_SUCCESS: &str = "https://blackcatinformatics.ca/logic/StrongPlanSuccess";

/// One carrier lane the bundle threads, with its load-bearing bit.
///
/// `load_bearing == true`: trimming the lane drops correctness (the dataset and the
/// three typed-handle graphs — a consumer that drops them loses the answer). `false`:
/// the lane is a droppable report/annotation surface (diagnostics, projection-ledger,
/// conformance) — dropping it only pessimizes, never changes the answer.
struct Lane {
    /// The lane's stable public slug (also its node-IRI local part).
    slug: &'static str,
    /// The backing named-graph IRI the lane folds into (its content surface).
    graph: &'static str,
    /// Whether the lane is load-bearing (a trim drops correctness).
    load_bearing: bool,
}

/// The fixed carrier-lane manifest (C6–C9). Order is irrelevant — the emitter
/// sorts — but kept declaration-stable for readability. The graph IRIs mirror the
/// snapshot's named-graph constants; the load-bearing classification follows the
/// `logic:loadBearing` doctrine (carrier graphs bear, report graphs are droppable).
const LANES: &[Lane] = &[
    Lane {
        slug: "dataset",
        graph: "https://blackcatinformatics.ca/gmeow/graph/base",
        load_bearing: true,
    },
    Lane {
        slug: "logic",
        graph: "https://blackcatinformatics.ca/gmeow/graph/logic",
        load_bearing: true,
    },
    Lane {
        slug: "reasoning",
        graph: "https://blackcatinformatics.ca/gmeow/graph/reasoning",
        load_bearing: true,
    },
    Lane {
        slug: "relational-core",
        graph: "https://blackcatinformatics.ca/gmeow/graph/relational-core",
        load_bearing: true,
    },
    Lane {
        slug: "correspondence",
        graph: "https://blackcatinformatics.ca/gmeow/graph/correspondence",
        load_bearing: true,
    },
    Lane {
        slug: "diagnostics",
        graph: "https://blackcatinformatics.ca/gmeow/graph/diagnostics",
        load_bearing: false,
    },
    Lane {
        slug: "projection-ledger",
        graph: "https://blackcatinformatics.ca/gmeow/graph/projection-ledger",
        load_bearing: false,
    },
    Lane {
        slug: "conformance",
        graph: "https://blackcatinformatics.ca/gmeow/graph/conformance",
        load_bearing: false,
    },
    Lane {
        slug: "provenance",
        graph: GRAPH_PROVENANCE,
        load_bearing: false,
    },
];

/// The `gmeow:Procedure` node for the pipeline DAG.
const PROCEDURE_IRI: &str = "https://blackcatinformatics.ca/gmeow/provenance/pipeline";
/// The `gmeow:Execution` node enacting the procedure for this compilation.
const EXECUTION_IRI: &str = "https://blackcatinformatics.ca/gmeow/provenance/pipeline-execution";

/// Render one `gmeow:`-local IRI node from a stable slug, percent-safe (slugs are
/// already URL-safe lowercase-with-dashes).
fn unit_iri(name: &str) -> String {
    // The unit name is a repo-relative path (`slices/core/foo/module.ttl`); slugify
    // by replacing path separators and dots so it forms ONE IRI local segment. The
    // slug is a pure function of the public name — no runtime id ever enters it.
    let slug = name.replace(['/', '.'], "-");
    format!("{GMEOW}provenance/unit/{slug}")
}

fn lane_iri(slug: &str) -> String {
    format!("{GMEOW}provenance/lane/{slug}")
}

/// Project the provenance sidecar's PUBLIC projection (#1132 C9) plus the fixed
/// carrier-lane manifest into deterministic N-Triples for the `graph/provenance`
/// named graph. The rows arrive sorted + deduped (`public_projection`); this only
/// re-derives the distinct `(unit_name, kind)` set, sorts everything, and emits.
///
/// The output is plain N-Triples (default graph); the snapshot's `add_named` routes
/// every triple into [`GRAPH_PROVENANCE`] and re-canonicalizes — the SAME fold path
/// every other named graph flows through (C6/C7/C8).
#[must_use]
pub fn project_provenance_graph(
    projection: &[(usize, String, String, String, Option<String>)],
) -> String {
    // The distinct compilation units, by their PUBLIC `(name, kind)` — never a runtime
    // id. A unit may appear in many occurrence rows; collapse to one node.
    let mut units: BTreeSet<(String, String)> = BTreeSet::new();
    // The distinct `(unit_name, artifact_path)` carriage edges.
    let mut carries: BTreeSet<(String, String)> = BTreeSet::new();
    for (_quad_index, unit_name, kind, artifact_path, _location) in projection {
        units.insert((unit_name.clone(), kind.clone()));
        carries.insert((unit_name.clone(), artifact_path.clone()));
    }

    let mut out = String::new();

    // ── the Procedure + its Execution (the realized process vocab) ───────────────
    triple_iri(
        &mut out,
        PROCEDURE_IRI,
        RDF_TYPE,
        &format!("{GMEOW}Procedure"),
    );
    // A `gmeow:Procedure` is a `logic:Plan`, so it MUST declare its success
    // criterion — `logic:planGoal` (the gmeow:Goal the steps reach) and
    // `logic:planSuccessMode` (the quantification over outcomes), per logic:PlanShape.
    triple_iri(
        &mut out,
        PROCEDURE_IRI,
        &format!("{LOGIC}planGoal"),
        GOAL_SHIPPABLE_BUNDLE,
    );
    triple_iri(
        &mut out,
        PROCEDURE_IRI,
        &format!("{LOGIC}planSuccessMode"),
        STRONG_PLAN_SUCCESS,
    );
    annotate_abox(
        &mut out,
        PROCEDURE_IRI,
        "GMEOW pipeline procedure",
        "The dogfooded gmeow:Procedure node for the regeneration pipeline DAG: the \
         plan whose steps are the carrier lanes that produce gmeow.gts in one pass.",
    );
    triple_iri(
        &mut out,
        EXECUTION_IRI,
        RDF_TYPE,
        &format!("{GMEOW}Execution"),
    );
    annotate_abox(
        &mut out,
        EXECUTION_IRI,
        "GMEOW pipeline execution",
        "The gmeow:Execution that enacts the regeneration procedure for this \
         compilation, running every carrier-lane step.",
    );
    triple_iri(
        &mut out,
        EXECUTION_IRI,
        &format!("{GMEOW}executesProcedure"),
        PROCEDURE_IRI,
    );

    // ── each compilation unit (public name + kind), linked to the procedure ──────
    for (name, kind) in &units {
        let iri = unit_iri(name);
        triple_iri(&mut out, &iri, RDF_TYPE, &format!("{GMEOW}CompilationUnit"));
        triple_lit(&mut out, &iri, &format!("{GMEOW}unitName"), name);
        triple_lit(&mut out, &iri, &format!("{GMEOW}originKind"), kind);
        annotate_abox(
            &mut out,
            &iri,
            &format!("Compilation unit: {name}"),
            &format!("A {kind} compilation unit ({name}) folded into the gmeow.gts bundle."),
        );
    }

    // ── each carriage edge (unit → artifact path) ────────────────────────────────
    for (name, artifact) in &carries {
        let iri = unit_iri(name);
        triple_lit(&mut out, &iri, &format!("{GMEOW}carriesArtifact"), artifact);
    }

    // ── each carrier lane: a ProcedureStep with its loadBearing bit ──────────────
    for lane in LANES {
        let iri = lane_iri(lane.slug);
        triple_iri(&mut out, &iri, RDF_TYPE, &format!("{GMEOW}ProcedureStep"));
        triple_iri(
            &mut out,
            PROCEDURE_IRI,
            &format!("{GMEOW}hasProcedureStep"),
            &iri,
        );
        triple_iri(
            &mut out,
            EXECUTION_IRI,
            &format!("{GMEOW}executesStep"),
            &iri,
        );
        triple_lit(&mut out, &iri, &format!("{GMEOW}laneSlug"), lane.slug);
        triple_iri(&mut out, &iri, &format!("{GMEOW}carrierGraph"), lane.graph);
        triple_bool(
            &mut out,
            &iri,
            &format!("{LOGIC}loadBearing"),
            lane.load_bearing,
        );
        annotate_abox(
            &mut out,
            &iri,
            &format!("Carrier lane: {}", lane.slug),
            &format!(
                "The {} carrier lane — a gmeow:ProcedureStep folding into {} ({}).",
                lane.slug,
                lane.graph,
                if lane.load_bearing {
                    "load-bearing"
                } else {
                    "droppable report surface"
                },
            ),
        );
    }

    // Sort every line so the projection is byte-stable independent of emission order.
    let mut lines: Vec<&str> = out.lines().collect();
    lines.sort_unstable();
    lines.dedup();
    let mut sorted = lines.join("\n");
    sorted.push('\n');
    sorted
}

/// Emit the assertional-tier annotation quartet for one materialized A-Box
/// individual: a human `rdfs:label`, a `skos:definition`, the `rdfs:isDefinedBy`
/// provenance anchor pointing at this projection's named graph, and the
/// `gmeow:graphBoxRole gmeow:boxABox` assertional role. This is the same contract
/// the documentation projection (`crates/docs/src/rdf.rs`) attaches to every
/// generated subject so the folded bundle satisfies the typed-instance validation
/// contract (#1104) without suppressing the structural lint.
fn annotate_abox(out: &mut String, subject: &str, label: &str, definition: &str) {
    triple_lit(out, subject, RDFS_LABEL, label);
    triple_lit(out, subject, SKOS_DEFINITION, definition);
    triple_iri(out, subject, RDFS_IS_DEFINED_BY, GRAPH_PROVENANCE);
    triple_iri(out, subject, &format!("{GMEOW}graphBoxRole"), BOX_ABOX);
}

fn triple_iri(out: &mut String, s: &str, p: &str, o: &str) {
    writeln!(out, "<{s}> <{p}> <{o}> .").expect("write to String");
}

fn triple_lit(out: &mut String, s: &str, p: &str, lit: &str) {
    writeln!(out, "<{s}> <{p}> {} .", escape_literal(lit)).expect("write to String");
}

fn triple_bool(out: &mut String, s: &str, p: &str, value: bool) {
    writeln!(
        out,
        "<{s}> <{p}> \"{}\"^^<{XSD_BOOLEAN}> .",
        if value { "true" } else { "false" }
    )
    .expect("write to String");
}

/// Escape a string literal to a valid N-Triples quoted literal.
fn escape_literal(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_projection() -> Vec<(usize, String, String, String, Option<String>)> {
        vec![
            (
                0,
                "ontology/gmeow.ttl".to_string(),
                "root-ontology".to_string(),
                "ontology/gmeow.ttl".to_string(),
                None,
            ),
            (
                1,
                "slices/core/epistemics/module.ttl".to_string(),
                "source".to_string(),
                "slices/core/epistemics/module.ttl".to_string(),
                None,
            ),
            (
                2,
                "imports/prov.ttl".to_string(),
                "import".to_string(),
                "imports/prov.ttl".to_string(),
                None,
            ),
        ]
    }

    #[test]
    fn projection_carries_procedure_and_execution_nodes() {
        let nt = project_provenance_graph(&sample_projection());
        assert!(nt.contains(&format!(
            "<{PROCEDURE_IRI}> <{RDF_TYPE}> <{GMEOW}Procedure> ."
        )));
        assert!(nt.contains(&format!(
            "<{EXECUTION_IRI}> <{RDF_TYPE}> <{GMEOW}Execution> ."
        )));
        assert!(nt.contains(&format!(
            "<{EXECUTION_IRI}> <{GMEOW}executesProcedure> <{PROCEDURE_IRI}> ."
        )));
    }

    #[test]
    fn every_lane_is_a_step_with_a_loadbearing_bit() {
        let nt = project_provenance_graph(&sample_projection());
        for lane in LANES {
            let iri = lane_iri(lane.slug);
            assert!(
                nt.contains(&format!("<{iri}> <{RDF_TYPE}> <{GMEOW}ProcedureStep> .")),
                "lane {} must be a ProcedureStep",
                lane.slug
            );
            assert!(
                nt.contains(&format!(
                    "<{PROCEDURE_IRI}> <{GMEOW}hasProcedureStep> <{iri}> ."
                )),
                "lane {} must link to the procedure",
                lane.slug
            );
            let expect = format!(
                "<{iri}> <{LOGIC}loadBearing> \"{}\"^^<{XSD_BOOLEAN}> .",
                lane.load_bearing
            );
            assert!(
                nt.contains(&expect),
                "lane {} must carry its loadBearing bit ({})",
                lane.slug,
                lane.load_bearing
            );
        }
    }

    #[test]
    fn every_materialized_subject_carries_the_assertional_contract() {
        // Regression guard for validate --gts (#1155 / #1104): every materialized
        // A-Box individual this projection folds into the bundle MUST carry the
        // assertional-tier quartet — an rdfs:label, a skos:definition, an
        // rdfs:isDefinedBy anchored at graph/provenance, and gmeow:graphBoxRole
        // gmeow:boxABox — or the bundle structural lint flags it as a
        // vocabulary-surface individual missing its annotations.
        let nt = project_provenance_graph(&sample_projection());
        let mut subjects: Vec<String> = vec![
            PROCEDURE_IRI.to_owned(),
            EXECUTION_IRI.to_owned(),
            unit_iri("ontology/gmeow.ttl"),
        ];
        for lane in LANES {
            subjects.push(lane_iri(lane.slug));
        }
        for s in subjects {
            for (pred, what) in [
                (RDFS_LABEL, "rdfs:label"),
                (SKOS_DEFINITION, "skos:definition"),
                (&format!("{GMEOW}graphBoxRole")[..], "gmeow:graphBoxRole"),
            ] {
                assert!(
                    nt.contains(&format!("<{s}> <{pred}> ")),
                    "subject {s} must carry {what}"
                );
            }
            assert!(
                nt.contains(&format!(
                    "<{s}> <{RDFS_IS_DEFINED_BY}> <{GRAPH_PROVENANCE}> ."
                )),
                "subject {s} must anchor rdfs:isDefinedBy at graph/provenance"
            );
            assert!(
                nt.contains(&format!("<{s}> <{GMEOW}graphBoxRole> <{BOX_ABOX}> .")),
                "subject {s} must declare gmeow:graphBoxRole gmeow:boxABox"
            );
        }
    }

    #[test]
    fn pipeline_procedure_declares_its_plan_success_criterion() {
        // A gmeow:Procedure is a logic:Plan, so logic:PlanShape demands exactly one
        // logic:planGoal and one logic:planSuccessMode — the emitter must supply them.
        let nt = project_provenance_graph(&sample_projection());
        assert!(nt.contains(&format!(
            "<{PROCEDURE_IRI}> <{LOGIC}planGoal> <{GOAL_SHIPPABLE_BUNDLE}> ."
        )));
        assert!(nt.contains(&format!(
            "<{PROCEDURE_IRI}> <{LOGIC}planSuccessMode> <{STRONG_PLAN_SUCCESS}> ."
        )));
    }

    #[test]
    fn projection_carries_no_runtime_ids() {
        // S0.5: the public projection must NEVER leak a runtime id.
        let nt = project_provenance_graph(&sample_projection());
        assert!(!nt.contains("unit#"), "no runtime UnitId in the graph");
        assert!(
            !nt.contains("artifact#"),
            "no runtime ArtifactId in the graph"
        );
        assert!(
            !nt.contains("origin-set#"),
            "no runtime OriginSetId in the graph"
        );
    }

    #[test]
    fn projection_is_byte_deterministic() {
        let a = project_provenance_graph(&sample_projection());
        // Re-project from a row-shuffled input — the emitter sorts, so the bytes match.
        let mut shuffled = sample_projection();
        shuffled.reverse();
        let b = project_provenance_graph(&shuffled);
        assert_eq!(
            a, b,
            "the projection must be byte-stable across input order"
        );
    }

    #[test]
    fn each_unit_carries_name_and_kind() {
        let nt = project_provenance_graph(&sample_projection());
        let root = unit_iri("ontology/gmeow.ttl");
        assert!(nt.contains(&format!("<{root}> <{RDF_TYPE}> <{GMEOW}CompilationUnit> .")));
        assert!(nt.contains(&format!("<{root}> <{GMEOW}originKind> \"root-ontology\" .")));
    }
}
