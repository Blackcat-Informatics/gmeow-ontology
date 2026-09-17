// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Repo-free correspondence-calculus consumers over the embedded typed program.

use std::fmt::Write as _;
use std::path::Path;

use gmeow_cli_core::Reporter;
use gmeow_logic::correspondence_exec::physical_plan::{
    CompositionPhysicalPlan, CompositionPlanCertificate, CompositionPlanLimits,
    PreparedCompositionProgram, verify_composition_report,
};
use gmeow_logic::correspondence_exec::plan_projection::{
    PlanProjectionLimits, project_plan, recover_planned_schema_skeleton,
};
use gmeow_logic_compile::frontend::PreparedLogicSource;
use gmeow_logic_compile::projections::correspondence::{
    CorrespondenceProgram, parse_correspondence,
};
use serde::Serialize;

use super::{fail, gts_bytes, parse_rdf_file};
use crate::OutputFormat;

const CODE: &str = "gmeow-cli.correspondence";

struct LoadedProgram {
    program: CorrespondenceProgram,
    prepared: PreparedCompositionProgram,
}

fn load_program(reporter: &dyn Reporter, gts: Option<&Path>) -> Result<LoadedProgram, i32> {
    let bytes = gts_bytes(reporter, gts)?;
    let dataset = purrdf::gts::flattened_dataset_from_bytes(&bytes).map_err(|error| {
        fail(
            reporter,
            &format!("{CODE}.bundle"),
            format!("cannot fold correspondence bundle: {error}"),
        )
    })?;
    let program = parse_correspondence(&dataset).map_err(|error| {
        fail(
            reporter,
            &format!("{CODE}.program"),
            format!("cannot reconstruct the typed correspondence program: {error}"),
        )
    })?;
    if program.correspondences.is_empty() {
        return Err(fail(
            reporter,
            &format!("{CODE}.empty-program"),
            "the selected bundle carries no correspondence cells",
        ));
    }
    let prepared = PreparedCompositionProgram::prepare(&program, CompositionPlanLimits::default())
        .map_err(|error| {
            fail(
                reporter,
                &format!("{CODE}.planning"),
                format!("cannot prepare the typed correspondence program: {error}"),
            )
        })?;
    verify_composition_report(prepared.report(), &program).map_err(|error| {
        fail(
            reporter,
            &format!("{CODE}.certificate"),
            format!("correspondence physical-plan verification failed: {error}"),
        )
    })?;
    Ok(LoadedProgram { program, prepared })
}

fn selected_certificate<'a>(
    reporter: &dyn Reporter,
    loaded: &'a LoadedProgram,
    declaration: &str,
) -> Result<&'a CompositionPlanCertificate, i32> {
    loaded
        .prepared
        .report()
        .certificates
        .iter()
        .find(|certificate| certificate.declaration == declaration)
        .ok_or_else(|| {
            fail(
                reporter,
                &format!("{CODE}.unknown-composition"),
                format!("the embedded correspondence program has no composition <{declaration}>"),
            )
        })
}

fn print_json<T: Serialize>(reporter: &dyn Reporter, value: &T) -> i32 {
    match serde_json::to_string_pretty(value) {
        Ok(value) => {
            println!("{value}");
            0
        }
        Err(error) => fail(
            reporter,
            &format!("{CODE}.json"),
            format!("cannot serialize correspondence result: {error}"),
        ),
    }
}

pub(crate) fn correspondence_inspect(
    reporter: &dyn Reporter,
    gts: Option<&Path>,
    format: OutputFormat,
) -> i32 {
    let loaded = match load_program(reporter, gts) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    if matches!(format, OutputFormat::Json) {
        #[derive(Serialize)]
        struct Inspection<'a> {
            program: &'a CorrespondenceProgram,
            physical_plans:
                &'a gmeow_logic::correspondence_exec::physical_plan::CompositionPlanReport,
        }
        return print_json(
            reporter,
            &Inspection {
                program: &loaded.program,
                physical_plans: loaded.prepared.report(),
            },
        );
    }
    println!("preservation: {}", loaded.program.preservation.as_str());
    println!("correspondences: {}", loaded.program.correspondences.len());
    for correspondence in &loaded.program.correspondences {
        println!(
            "cell <{}> relation={} class={} source={} target={} get={} put={}",
            correspondence.iri,
            correspondence.relation.as_str(),
            correspondence.morphism_class.as_str(),
            correspondence.source_endpoint.as_deref().unwrap_or("-"),
            correspondence.target_endpoint.as_deref().unwrap_or("-"),
            correspondence.get_leg.as_deref().unwrap_or("-"),
            correspondence.put_leg.as_deref().unwrap_or("-"),
        );
    }
    println!("compositions: {}", loaded.program.compositions.len());
    for certificate in &loaded.prepared.report().certificates {
        println!(
            "composition <{}> plan={} certificate={}",
            certificate.declaration,
            physical_plan_name(&certificate.selected),
            certificate.certificate_digest,
        );
    }
    0
}

pub(crate) fn correspondence_compose(
    reporter: &dyn Reporter,
    declaration: &str,
    gts: Option<&Path>,
    format: OutputFormat,
) -> i32 {
    let loaded = match load_program(reporter, gts) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let certificate = match selected_certificate(reporter, &loaded, declaration) {
        Ok(certificate) => certificate,
        Err(code) => return code,
    };
    if matches!(format, OutputFormat::Json) {
        return print_json(reporter, certificate);
    }
    println!("composition: <{}>", certificate.declaration);
    println!(
        "physical-plan: {}",
        physical_plan_name(&certificate.selected)
    );
    println!("certificate: {}", certificate.certificate_digest);
    println!("program: {}", certificate.scope.program_digest);
    println!(
        "cost: queries {} -> {}; intermediates {} -> {}",
        certificate.original_cost.native_queries,
        certificate.selected_cost.native_queries,
        certificate
            .original_cost
            .materialized_intermediate_relations,
        certificate
            .selected_cost
            .materialized_intermediate_relations,
    );
    0
}

pub(crate) fn correspondence_execute(
    reporter: &dyn Reporter,
    declaration: &str,
    input: &Path,
    gts: Option<&Path>,
    format: OutputFormat,
) -> i32 {
    let loaded = match load_program(reporter, gts) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    if let Err(code) = selected_certificate(reporter, &loaded, declaration) {
        return code;
    }
    let dataset = match parse_rdf_file(reporter, CODE, input) {
        Ok(dataset) => dataset,
        Err(code) => return code,
    };
    let execution = match loaded.prepared.execute(declaration, &dataset) {
        Ok(execution) => execution,
        Err(error) => {
            return fail(
                reporter,
                &format!("{CODE}.execution"),
                format!(
                    "cannot execute <{declaration}> over {}: {error}",
                    input.display()
                ),
            );
        }
    };
    if matches!(format, OutputFormat::Json) {
        return print_json(reporter, &execution);
    }
    for row in &execution.rows {
        println!("row {} {} {}", row.source, row.middle, row.target);
    }
    println!("certificate: {}", execution.receipt.certificate_digest);
    println!("input: {}", execution.receipt.input_digest);
    println!(
        "physical-plan: {}",
        physical_plan_name(&execution.receipt.mode)
    );
    println!("rows: {}", execution.receipt.result_rows);
    println!(
        "materialized-intermediate-rows: {}",
        execution.receipt.materialized_intermediate_rows
    );
    0
}

pub(crate) fn correspondence_explain(
    reporter: &dyn Reporter,
    declaration: &str,
    gts: Option<&Path>,
    format: OutputFormat,
) -> i32 {
    let loaded = match load_program(reporter, gts) {
        Ok(loaded) => loaded,
        Err(code) => return code,
    };
    let certificate = match selected_certificate(reporter, &loaded, declaration) {
        Ok(certificate) => certificate,
        Err(code) => return code,
    };
    let cells: Vec<_> = [
        &certificate.first.correspondence,
        &certificate.second.correspondence,
        &certificate.composite.correspondence,
    ]
    .into_iter()
    .filter_map(|iri| {
        loaded
            .program
            .correspondences
            .iter()
            .find(|cell| &cell.iri == iri)
    })
    .collect();
    if cells.len() != 3 {
        return fail(
            reporter,
            &format!("{CODE}.explanation"),
            "the certified composition does not resolve all three logical stages",
        );
    }
    if matches!(format, OutputFormat::Json) {
        return print_json(
            reporter,
            &serde_json::json!({
                "certificate": certificate,
                "logical_cells": cells,
                "bounded_corpus_authorizes_rewrite": false,
            }),
        );
    }
    println!("composition: <{}>", certificate.declaration);
    println!("theorem-rule: {}", certificate.applied_rules.join(","));
    println!("output-contract: {}", certificate.output_contract);
    for (position, (stage, cell)) in [
        ("first", (&certificate.first, cells[0])),
        ("second", (&certificate.second, cells[1])),
        ("composite", (&certificate.composite, cells[2])),
    ] {
        println!(
            "{position}: <{}> {} -> {} path={} standpoint={}",
            stage.correspondence,
            stage.source_endpoint,
            stage.target_endpoint,
            stage.path,
            stage.standpoint.as_deref().unwrap_or("unspecified"),
        );
        println!("  checks: {:?}", stage.checks);
        for loss in &cell.loss_evidence {
            println!("  loss: {}", loss.lexical_form);
        }
        for caveat in &cell.caveats {
            for comment in &caveat.comments {
                println!("  caveat: {}", comment.lexical_form);
            }
        }
    }
    println!("bounded-corpus-authorizes-rewrite: false");
    0
}

pub(crate) fn correspondence_merge(
    reporter: &dyn Reporter,
    input: &Path,
    selected: Option<&str>,
    format: OutputFormat,
) -> i32 {
    let dataset = match parse_rdf_file(reporter, CODE, input) {
        Ok(dataset) => dataset,
        Err(code) => return code,
    };
    let theory = match PreparedLogicSource::new(&dataset)
        .and_then(|source| source.into_compiled(Some(input.display().to_string())))
    {
        Ok(theory) => std::sync::Arc::new(theory),
        Err(error) => {
            return fail(
                reporter,
                &format!("{CODE}.merge-source"),
                format!(
                    "cannot compile {} as canonical logic source: {error}",
                    input.display()
                ),
            );
        }
    };
    let merges = match gmeow_logic::correspondence_exec::presentation::source::execute(
        theory,
        gmeow_logic::correspondence_exec::presentation::PresentationLimits::default(),
    ) {
        Ok(merges) => merges,
        Err(error) => {
            return fail(
                reporter,
                &format!("{CODE}.merge"),
                format!("checked presentation merge failed: {error}"),
            );
        }
    };
    if merges.merges().is_empty() {
        return fail(
            reporter,
            &format!("{CODE}.no-merges"),
            format!(
                "{} carries no presentation-merge declaration",
                input.display()
            ),
        );
    }
    if let Some(selected) = selected
        && !merges.merges().contains_key(selected)
    {
        return fail(
            reporter,
            &format!("{CODE}.unknown-merge"),
            format!(
                "{} carries no presentation merge <{selected}>",
                input.display()
            ),
        );
    }
    let bytes = match merges.report() {
        Ok(bytes) => bytes,
        Err(error) => {
            return fail(
                reporter,
                &format!("{CODE}.merge-report"),
                format!("cannot render checked merge report: {error}"),
            );
        }
    };
    if matches!(format, OutputFormat::Json) {
        println!("{}", String::from_utf8_lossy(&bytes));
        return 0;
    }
    let mut output = String::new();
    for (iri, merge) in merges.merges() {
        if selected.is_some_and(|selected| selected != iri) {
            continue;
        }
        writeln!(
            output,
            "merge <{iri}> square=checked contexts={} symbols={} sentences={} engine={}",
            merge.output().contexts().len(),
            merge.output().symbols().len(),
            merge.output().sentences().len(),
            merge.engine_descriptor(),
        )
        .expect("writing to String is infallible");
    }
    print!("{output}");
    0
}

pub(crate) fn correspondence_project_plan(
    reporter: &dyn Reporter,
    input: &Path,
    plan: &str,
    graph: Option<&str>,
    max_unroll: usize,
    format: OutputFormat,
) -> i32 {
    let dataset = match parse_rdf_file(reporter, CODE, input) {
        Ok(dataset) => dataset,
        Err(code) => return code,
    };
    let projection = match project_plan(
        &dataset,
        plan,
        graph,
        PlanProjectionLimits {
            max_unroll,
            ..PlanProjectionLimits::default()
        },
    ) {
        Ok(projection) => projection,
        Err(error) => {
            return fail(
                reporter,
                &format!("{CODE}.plan-projection"),
                format!(
                    "cannot project plan <{plan}> from {}: {error}",
                    input.display()
                ),
            );
        }
    };
    if matches!(format, OutputFormat::Json) {
        return print_json(reporter, &projection);
    }
    println!("plan: <{}>", projection.plan);
    println!(
        "source-graph: {}",
        projection.source_graph.as_deref().unwrap_or("default")
    );
    println!(
        "standpoint: {}",
        projection.standpoint.as_deref().unwrap_or("unspecified")
    );
    println!("bounded-unrolling: {}", projection.loop_count);
    println!("planned-steps: {}", projection.steps.len());
    println!("control-edges: {}", projection.edges.len());
    println!("action-schemas: {}", projection.actions.len());
    println!("guards: {}", projection.guards.len());
    println!("freshness-requirements: {}", projection.freshness.len());
    for loss in &projection.loss_evidence {
        println!("loss: {loss}");
    }
    println!("source-digest: {}", projection.certificate.source_digest);
    println!("certificate: {}", projection.certificate.certificate_digest);
    0
}

pub(crate) fn correspondence_recover_plan(
    reporter: &dyn Reporter,
    plan_source: &Path,
    observations: &Path,
    plan: &str,
    graph: Option<&str>,
    max_unroll: usize,
    format: OutputFormat,
) -> i32 {
    let source = match parse_rdf_file(reporter, CODE, plan_source) {
        Ok(dataset) => dataset,
        Err(code) => return code,
    };
    let observed = match parse_rdf_file(reporter, CODE, observations) {
        Ok(dataset) => dataset,
        Err(code) => return code,
    };
    let projection = match project_plan(
        &source,
        plan,
        graph,
        PlanProjectionLimits {
            max_unroll,
            ..PlanProjectionLimits::default()
        },
    ) {
        Ok(projection) => projection,
        Err(error) => {
            return fail(
                reporter,
                &format!("{CODE}.plan-source"),
                format!(
                    "cannot admit plan <{plan}> from {}: {error}",
                    plan_source.display()
                ),
            );
        }
    };
    let recovery = match recover_planned_schema_skeleton(&projection, &observed) {
        Ok(recovery) => recovery,
        Err(error) => {
            return fail(
                reporter,
                &format!("{CODE}.plan-recovery"),
                format!(
                    "cannot recover the planned skeleton from {}: {error}",
                    observations.display()
                ),
            );
        }
    };
    if matches!(format, OutputFormat::Json) {
        return print_json(reporter, &recovery);
    }
    println!("plan: <{}>", recovery.plan);
    println!("verdict: planned-skeleton-recovered");
    println!("occurrences: {}", recovery.occurrences.len());
    for occurrence in &recovery.occurrences {
        println!(
            "occurrence <{}> schema=<{}> graph={}",
            occurrence.occurrence,
            occurrence.schema,
            occurrence.graph.as_deref().unwrap_or("default")
        );
    }
    for schema in &recovery.unobserved_planned_schemas {
        println!("unobserved-planned-schema: <{schema}>");
    }
    for occurrence in &recovery.off_plan_occurrences {
        println!("off-plan-occurrence: <{occurrence}>");
    }
    for loss in &recovery.loss_evidence {
        println!("loss: {loss}");
    }
    println!("observed-input-digest: {}", recovery.observed_input_digest);
    println!(
        "projection-certificate: {}",
        recovery.projection_certificate_digest
    );
    println!("recovery-receipt: {}", recovery.recovery_digest);
    0
}

fn physical_plan_name(plan: &CompositionPhysicalPlan) -> &'static str {
    match plan {
        CompositionPhysicalPlan::Original { .. } => "original",
        CompositionPhysicalPlan::FusedWitnessJoin => "fused-witness-join",
    }
}
