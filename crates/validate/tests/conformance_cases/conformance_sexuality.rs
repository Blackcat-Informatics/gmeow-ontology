// SPDX-License-Identifier: AGPL-3.0-only

//! Conformance twin migrated from tests/test_sexuality.py
//!
//! Ports the single retained dynamic guard: the `orientation-values.rq`
//! competency SELECT enumerates the recognised sexual AND romantic orientation
//! value individuals as SEPARATE axes (the split-attraction model). The query
//! `UNION`s the two axis branches, tags each with `BIND("sexual"/"romantic" AS
//! ?axis)`, and carries an `OPTIONAL rdfs:label` cell.
//!
//! The asserted-TBox structural invariants those originals shared with
//! module-scoped slicetest cells stayed in `slices/core/sexuality/tests/structural.ttl`.

use crate::conformance_support::*;

const GMEOW: &str = "https://blackcatinformatics.ca/gmeow/";

fn gm(local: &str) -> String {
    format!("{GMEOW}{local}")
}

/// Twin of `test_competency_orientation_values_query`: over the merged ontology,
/// the split-attraction UNION enumerates both `gmeow:SexualOrientationValue` and
/// `gmeow:RomanticOrientationValue` individuals. The Python "named individuals
/// present + `len(values) >= 16`" becomes a `column_superset` over `?orientation`
/// (two sexual + two romantic seeds) plus `select_count_at_least(16)`.
#[gmeow_test_batch_macros::batch_test]
fn competency_orientation_values_query() {
    QueryCase::new(
        "sexuality/orientation-values",
        &[Feature::Union, Feature::Bind],
    )
    .over_ontology()
    .query_file("orientation-values.rq")
    .column_superset(
        "orientation",
        vec![
            iri(&gm("orientAsexual")),
            iri(&gm("orientBisexual")),
            iri(&gm("romanticAromantic")),
            iri(&gm("romanticBiromantic")),
        ],
    )
    .select_count_at_least(16)
    .run();
}
