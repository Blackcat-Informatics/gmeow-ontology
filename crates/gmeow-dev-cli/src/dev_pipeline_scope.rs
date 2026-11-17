// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Explicit authenticated comparison of the authored scope query and executable plan.
//! Reads only the pipeline description and query; never schedules stage bodies.

use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use gmeow_errors::{Diag, Result};
use gmeow_pipeline::registry::registry_for;
use gmeow_pipeline::run::{BuildProfile, RunOutputScope, spec_for_scope};
use gmeow_pipeline::{PipelineSpec, bind, full_spec};
use purrdf::sparql::NativeSparqlEngine;
use purrdf::{RdfDataset, SparqlEngine, SparqlRequest, SparqlResult, TermValue, parse_dataset};

const MODULE: &str = "slices/core/pipeline/module.ttl";
const QUERY: &str = "slices/core/pipeline/queries/competency/output-scope-stages.rq";
const SCOPES: [(&str, RunOutputScope); 2] = [
    ("all", RunOutputScope::All),
    ("committed", RunOutputScope::Committed),
];

fn invalid(message: impl Into<String>) -> Diag {
    Diag::of_kind(gmeow_pipeline::error::InvalidDeclaration {
        message: message.into(),
    })
}

fn read_source(root: &Path, relative: &str) -> Result<String> {
    std::fs::read_to_string(root.join(relative))
        .map_err(|error| crate::error::source(format!("cannot read {relative}")).with_source(error))
}

fn query_rows(dataset: &Arc<RdfDataset>, query: &str) -> Result<BTreeSet<(String, String)>> {
    let result = NativeSparqlEngine::new()
        .query(
            dataset,
            SparqlRequest {
                query,
                base_iri: None,
                substitutions: &[],
            },
        )
        .map_err(|error| crate::error::rdf("execute pipeline scope query").with_source(error))?;
    let SparqlResult::Solutions {
        variables, rows, ..
    } = result
    else {
        return Err(invalid("pipeline scope query must return SELECT solutions"));
    };
    if variables.len() != 2 {
        return Err(invalid(format!(
            "pipeline scope query needs exactly scope/stage columns; got {variables:?}"
        )));
    }
    let column = |name: &str| {
        variables
            .iter()
            .position(|value| value.as_str() == name)
            .ok_or_else(|| invalid(format!("pipeline scope query is missing ?{name}")))
    };
    let scope = column("scope")?;
    let stage = column("stage")?;
    let mut selected = BTreeSet::new();
    for row in rows {
        if row.len() != variables.len() {
            return Err(invalid(format!(
                "pipeline scope query has malformed row {row:?}"
            )));
        }
        let Some(TermValue::Literal {
            lexical_form,
            datatype,
            language: None,
            direction: None,
        }) = &row[scope]
        else {
            return Err(invalid(format!(
                "pipeline scope query needs a bound untagged scope string; got {:?}",
                row[scope]
            )));
        };
        if datatype.as_str() != "http://www.w3.org/2001/XMLSchema#string"
            || !SCOPES
                .iter()
                .any(|(label, _)| *label == lexical_form.as_str())
        {
            return Err(invalid(format!(
                "pipeline scope query has unknown or ill-typed scope {:?}",
                row[scope]
            )));
        }
        let Some(TermValue::Iri(iri)) = &row[stage] else {
            return Err(invalid(format!(
                "pipeline scope query needs a bound stage IRI; got {:?}",
                row[stage]
            )));
        };
        let id = iri
            .strip_prefix(crate::dev_common::NAMESPACE)
            .filter(|id| !id.is_empty())
            .ok_or_else(|| {
                invalid(format!(
                    "pipeline scope query stage {iri} is outside the build namespace"
                ))
            })?;
        if !selected.insert((lexical_form.clone(), id.to_owned())) {
            return Err(invalid(format!(
                "pipeline scope query repeats ({lexical_form}, {iri}) despite DISTINCT"
            )));
        }
    }
    Ok(selected)
}

fn check(root: &Path) -> Result<Vec<(String, &'static str, String)>> {
    let source = read_source(root, MODULE)?;
    let dataset = parse_dataset(source.as_bytes(), "text/turtle", None)
        .map_err(|error| crate::error::rdf("parse pipeline description").with_source(error))?;
    let spec = PipelineSpec::from_dataset(&dataset)?;
    if spec != full_spec() {
        return Err(invalid(
            "authored pipeline description differs from the compiled executable spec",
        ));
    }
    let graph = spec.validate()?;
    let observed = query_rows(&dataset, &read_source(root, QUERY)?)?;
    let mut output = Vec::new();
    for profile in [BuildProfile::Ordinary, BuildProfile::Release] {
        let registry = registry_for(profile);
        let _bound = bind(&spec, &graph, &registry)?;
        for (label, scope) in SCOPES {
            let selected = spec_for_scope(spec.clone(), scope, &registry)?;
            selected.validate()?;
            let expected: BTreeSet<_> = selected
                .stages
                .into_iter()
                .map(|stage| (label.to_owned(), stage.id))
                .collect();
            let actual: BTreeSet<_> = observed
                .iter()
                .filter(|(row_scope, _)| row_scope.as_str() == label)
                .cloned()
                .collect();
            if actual != expected {
                let missing: Vec<_> = expected.difference(&actual).collect();
                let extra: Vec<_> = actual.difference(&expected).collect();
                return Err(invalid(format!(
                    "pipeline scope query disagrees for profile {} scope {label}: missing {missing:?}; extra {extra:?}",
                    profile.as_str()
                )));
            }
            output.extend(
                expected
                    .into_iter()
                    .map(|(_, stage)| (profile.as_str().to_owned(), label, stage)),
            );
        }
    }
    Ok(output)
}

/// Check every profile/scope before publishing deterministic tab-separated rows.
/// Producer admission is required by the command dispatcher before this function runs.
pub(crate) fn run() -> i32 {
    let outcome = check(&crate::dev_common::project_root()).and_then(|rows| {
        let mut stdout = std::io::stdout().lock();
        writeln!(stdout, "profile\tscope\tstage").map_err(|error| {
            crate::error::encoding("write pipeline scope header").with_source(error)
        })?;
        for (profile, scope, stage) in rows {
            writeln!(stdout, "{profile}\t{scope}\t{stage}").map_err(|error| {
                crate::error::encoding("write pipeline scope row").with_source(error)
            })?;
        }
        Ok(())
    });
    if let Err(error) = outcome {
        crate::dev_common::emit_report(&gmeow_cli_core::report_diag(error, "pipeline-scope-check"));
        return 1;
    }
    0
}
