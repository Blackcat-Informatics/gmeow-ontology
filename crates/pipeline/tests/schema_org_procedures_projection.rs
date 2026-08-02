// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The schema.org HowTo/Recipe consumer down-projection actually PROJECTS.
//!
//! `dsl/mappings/projections/schema-org-procedures.ttl` is the executable consumer
//! surface for the prescription / enactment spine: it lowers `logic:Plan`,
//! `logic:ActionSchema` and `logic:Enactment` onto schema.org's HowTo family. The
//! inventory gate (`gmeow_pipeline::projection_profiles`) proves the document exists
//! and still declares its cells; this proves the cells RUN — a profile that parses
//! but projects nothing would be the same loss in a new place.
//!
//! The query is compiled from the AUTHORED sources
//! (`compile_mappings`, the pure function of committed inputs the mappings stage
//! runs), then executed through the SAME
//! [`gmeow_pipeline::projections::project_graph`] the `gmeow project --profile
//! schema-org` consumer entry point runs its bundled CONSTRUCT through — so this is
//! the real executor over the real cells, not a re-implementation.

use std::path::{Path, PathBuf};

const SCHEMA: &str = "https://schema.org/";
const EX: &str = "https://example.org/kitchen/";

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("workspace root")
}

/// A worked prescription A-Box exercising all eight restored cells.
fn worked_instance_nt() -> String {
    let logic = "https://blackcatinformatics.ca/logic/";
    let gmeow = "https://blackcatinformatics.ca/gmeow/";
    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    [
        format!("<{EX}bakeBread> <{rdf_type}> <{logic}Plan> ."),
        format!("<{EX}bakeBread> <{logic}prescriptionKind> <{logic}prescriptionKindRecipe> ."),
        format!("<{EX}bakeBread> <{logic}planBody> <{EX}knead> ."),
        format!("<{EX}knead> <{rdf_type}> <{logic}ActionSchema> ."),
        format!("<{EX}knead> <{logic}precondition> <{EX}flour> ."),
        format!("<{EX}knead> <{logic}precondition> <{EX}mixer> ."),
        format!("<{EX}mixer> <{rdf_type}> <{gmeow}PhysicalObject> ."),
        format!("<{EX}knead> <{logic}effect> <{EX}dough> ."),
        format!("<{EX}mondayBake> <{rdf_type}> <{logic}Enactment> ."),
    ]
    .join("\n")
}

#[test]
fn the_schema_org_howto_profile_projects_the_prescription_spine() {
    let root = repo_root();
    let compiled =
        gmeow_pipeline::stages::mappings::compile_mappings(&root).expect("compile mappings");
    let rq_path = format!(
        "{}/schema-org.rq",
        gmeow_pipeline::stages::mappings::QUERIES_DIR
    );
    let query = compiled
        .artifacts
        .get(&rq_path)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_else(|| panic!("compiled mappings missing {rq_path}"));

    // The restored cells reached the compiled consumer query.
    for atom in [
        "schema:HowTo",
        "schema:Recipe",
        "schema:HowToStep",
        "schema:step",
        "schema:supply",
        "schema:tool",
        "schema:result",
        "schema:Action",
    ] {
        assert!(
            query.contains(atom),
            "the compiled schema-org CONSTRUCT is missing the restored `{atom}` branch"
        );
    }

    let projected = gmeow_pipeline::projections::project_graph(
        &worked_instance_nt(),
        &query,
        &Default::default(),
    )
    .expect("the schema-org profile projects the worked prescription instance");

    let rdf_type = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
    let expected = [
        format!("<{EX}bakeBread> <{rdf_type}> <{SCHEMA}HowTo> ."),
        format!("<{EX}bakeBread> <{rdf_type}> <{SCHEMA}Recipe> ."),
        format!("<{EX}knead> <{rdf_type}> <{SCHEMA}HowToStep> ."),
        format!("<{EX}bakeBread> <{SCHEMA}step> <{EX}knead> ."),
        format!("<{EX}knead> <{SCHEMA}supply> <{EX}flour> ."),
        format!("<{EX}knead> <{SCHEMA}supply> <{EX}mixer> ."),
        format!("<{EX}knead> <{SCHEMA}tool> <{EX}mixer> ."),
        format!("<{EX}knead> <{SCHEMA}result> <{EX}dough> ."),
        format!("<{EX}mondayBake> <{rdf_type}> <{SCHEMA}Action> ."),
    ];
    for triple in &expected {
        assert!(
            projected.lines().any(|line| line.trim() == triple),
            "the projection dropped `{triple}`\nprojected:\n{projected}"
        );
    }
    // Print the real consumer output so the projection is inspectable, not merely
    // asserted.
    println!("--- gmeow project --profile schema-org (restored HowTo cells) ---");
    let mut rows: Vec<&str> = projected
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    rows.sort_unstable();
    for row in rows {
        println!("{row}");
    }
}
