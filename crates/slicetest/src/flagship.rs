// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The flagship acceptance-manifest cross-check — the third gate surface, shared
//! across grounding layers.
//!
//! A grounding layer's depth bar is a `<ns>:FlagshipScenario` manifest gated on three
//! surfaces. SHACL (`<ns>:FlagshipScenarioShape`) and a structural ASK
//! (`ex:saFlagshipCoverage`) prove the scenarios are present and fully linked to a real
//! `<ns>:…ConformanceFailure` subclass — but neither can resolve the
//! `<ns>:demonstratedByCompetency` reference, because the `gmeow:CompetencyQuestion`
//! individuals live in `tests/competency.ttl`, which the module/examples-scoped
//! validators never load (the dataset split, documented in each manifest).
//!
//! [`validate_flagship_manifest`] is that missing surface. It unions the flagship manifest
//! with the competency corpus and asserts, for each scenario:
//!
//! * its competency reference resolves to a real `gmeow:CompetencyQuestion` that carries a
//!   `gmeow:cqExpectRow` expectation (a PINNED gate whose green execution the competency
//!   lane proves, not a dangling IRI), and its `gmeow:cqQueryFile` exists on disk;
//! * its worked example and its counter-example both exist on disk.
//!
//! It also pins the coverage set to EXACTLY the canonical scenarios passed in — no more,
//! no fewer. The flagship manifest vocabulary is HOISTED to the shared `gmeow:` namespace
//! (`gmeow:FlagshipScenario`, `gmeow:demonstratedBy*`, `gmeow:guardedByCounterExample`,
//! `gmeow:enforcesFailureClass`), so this one implementation binds every grounding slice
//! by its slice directory alone — a copy-paste mirror cannot silently re-assert the wrong
//! layer's coverage array, and there is no per-layer namespace parameter to get wrong.

use std::collections::BTreeSet;
use std::path::Path;

use gmeow_errors::{Diag, Result};

use crate::error::SpecCell;
use crate::native_query::{dataset_from_file, render_term, select, union};
use crate::paths::{example_file, query_file};

/// The shared `gmeow:` namespace the flagship acceptance manifest vocabulary lives under.
use gmeow_ns::GMEOW_NS;

fn fail(detail: impl Into<String>) -> Diag {
    Diag::of_kind(SpecCell {
        detail: detail.into(),
    })
}

/// Render one bound cell to its N-Triples lexical form, hard-failing on an unbound slot —
/// every column projected below is required, so an unbound cell is a bug in the manifest,
/// not an expected optional.
fn rendered(cell: &Option<purrdf::TermValue>) -> Result<String> {
    cell.as_ref()
        .map(render_term)
        .ok_or_else(|| fail("required flagship-manifest column was unbound"))
}

/// Strip the `<...>` of a rendered IRI, failing if the term is not an IRI.
fn as_iri(rendered: &str) -> Result<String> {
    rendered
        .strip_prefix('<')
        .and_then(|s| s.strip_suffix('>'))
        .map(str::to_owned)
        .ok_or_else(|| fail(format!("expected an IRI term, got {rendered}")))
}

/// Extract the lexical form of a rendered string literal (`"lexical"^^<datatype>`). The
/// manifest's path literals contain no quotes or escapes, so the first closing quote is
/// the lexical boundary.
fn as_literal(rendered: &str) -> Result<String> {
    let inner = rendered
        .strip_prefix('"')
        .ok_or_else(|| fail(format!("expected a literal term, got {rendered}")))?;
    inner
        .split_once('"')
        .map(|(lexical, _)| lexical.to_owned())
        .ok_or_else(|| fail(format!("literal term missing closing quote: {rendered}")))
}

/// Cross-check the flagship acceptance manifest of the slice at `slice_dir`, whose
/// scenarios are typed `gmeow:FlagshipScenario` (the shared, hoisted vocabulary) and whose
/// coverage set is EXACTLY `canonical`.
///
/// Unions `examples/flagship-acceptance.ttl` with `tests/competency.ttl` and asserts, per
/// scenario, that its competency resolves to a pinned green `gmeow:CompetencyQuestion`, its
/// query file exists, its worked example and counter-example exist on disk, and it names a
/// producer (`gmeow:demonstratedByProducer`, a REQUIRED column — a scenario missing it is
/// not bound by the SELECT and so drops out of the coverage set, failing the pin below).
/// Then pins the coverage set to `canonical` — a 6th or dropped scenario fails the
/// assertion.
pub fn validate_flagship_manifest(slice_dir: &Path, canonical: &[&str]) -> Result<()> {
    let manifest_path = slice_dir.join("examples").join("flagship-acceptance.ttl");
    let competency_path = slice_dir.join("tests").join("competency.ttl");

    let manifest = dataset_from_file(&manifest_path)
        .map_err(|error| fail(format!("parse {}: {error}", manifest_path.display())))?;
    let competency = dataset_from_file(&competency_path)
        .map_err(|error| fail(format!("parse {}: {error}", competency_path.display())))?;
    let dataset = union(&[manifest, competency]);

    // Every scenario and its five realizing/enforcing links, in one pass. The producer is a
    // REQUIRED column: a scenario missing gmeow:demonstratedByProducer will not bind and so
    // never enters `seen`, failing the exact coverage-set pin — the acceptance bar is
    // discharged by a runnable producer, not by an unrun label.
    let scenarios = select(
        &dataset,
        &format!(
            r"
            SELECT ?s ?ex ?cq ?prod ?ce ?fc WHERE {{
                ?s a <{GMEOW_NS}FlagshipScenario> ;
                   <{GMEOW_NS}demonstratedByExample>    ?ex ;
                   <{GMEOW_NS}demonstratedByCompetency> ?cq ;
                   <{GMEOW_NS}demonstratedByProducer>   ?prod ;
                   <{GMEOW_NS}guardedByCounterExample>  ?ce ;
                   <{GMEOW_NS}enforcesFailureClass>     ?fc .
            }}"
        ),
    )
    .map_err(|error| {
        fail(format!(
            "query flagship scenarios in {}: {error}",
            manifest_path.display()
        ))
    })?;

    let mut seen = BTreeSet::new();
    for row in &scenarios.rows {
        let scenario = as_iri(&rendered(&row[0])?)?;
        let example = as_literal(&rendered(&row[1])?)?;
        let competency_iri = as_iri(&rendered(&row[2])?)?;
        // ?prod must be a bound literal. Its production stage is authenticated separately.
        let _producer = as_literal(&rendered(&row[3])?)?;
        let counter_example = as_literal(&rendered(&row[4])?);
        // ?fc must be an IRI; the subclass check is the structural/SHACL gate's job.
        let counter_example = counter_example?;
        let _failure_class = as_iri(&rendered(&row[5])?)?;

        // The worked example and the counter-example must exist on disk.
        let example_abs = example_file(slice_dir, &example);
        if !example_abs.exists() {
            return Err(fail(format!(
                "{scenario}: gmeow:demonstratedByExample {example} does not exist at {}",
                example_abs.display()
            )));
        }
        let counter_abs = example_file(slice_dir, &counter_example);
        if !counter_abs.exists() {
            return Err(fail(format!(
                "{scenario}: gmeow:guardedByCounterExample {counter_example} does not exist at {}",
                counter_abs.display()
            )));
        }

        // The competency reference must resolve to a real, pinned (cqExpectRow) competency
        // question whose query file exists.
        validate_competency_is_green(&dataset, &scenario, &competency_iri)?;

        if !seen.insert(scenario.clone()) {
            return Err(fail(format!("duplicate scenario {scenario}")));
        }
    }

    let expected: BTreeSet<String> = canonical.iter().map(|s| (*s).to_owned()).collect();
    if seen != expected {
        return Err(fail(format!(
            "the flagship coverage set must be EXACTLY the canonical scenarios; expected {expected:?}, got {seen:?}"
        )));
    }
    Ok(())
}

/// Assert `competency_iri` names a `gmeow:CompetencyQuestion` carrying at least one
/// `gmeow:cqExpectRow` and a `gmeow:cqQueryFile` that exists on disk.
fn validate_competency_is_green(
    dataset: &std::sync::Arc<purrdf::RdfDataset>,
    scenario: &str,
    competency_iri: &str,
) -> Result<()> {
    let q = format!(
        r"
        PREFIX gmeow: <https://blackcatinformatics.ca/gmeow/>
        SELECT ?qf ?row WHERE {{
            <{competency_iri}> a gmeow:CompetencyQuestion ;
                gmeow:cqQueryFile ?qf ;
                gmeow:cqExpectRow ?row .
        }}"
    );
    let hits = select(dataset, &q).map_err(|error| {
        fail(format!(
            "{scenario}: resolve demonstrated competency {competency_iri}: {error}"
        ))
    })?;
    if hits.rows.is_empty() {
        return Err(fail(format!(
            "{scenario}: demonstratedByCompetency {competency_iri} does not resolve to a \
             gmeow:CompetencyQuestion carrying gmeow:cqQueryFile + gmeow:cqExpectRow \
             (a dangling or unpinned competency reference)"
        )));
    }
    // Every row shares the same cqQueryFile; check the first exists on disk.
    let query_rel = as_literal(&rendered(&hits.rows[0][0])?)?;
    let query_abs = query_file(&query_rel);
    if !query_abs.exists() {
        return Err(fail(format!(
            "{scenario}: cqQueryFile {query_rel} for {competency_iri} does not exist at {}",
            query_abs.display()
        )));
    }
    Ok(())
}
