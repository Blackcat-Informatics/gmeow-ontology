// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Complete directed meta evidence, including internal trace and root edges.

use super::MetaDerivation;
use purrdf_lex::json::{
    Value,
    record::{DecodeError, FromJson, Record, items_with},
};
use std::collections::BTreeSet;

::purrdf_lex::json_record!(impl ToJson for MetaDerivation {
    "traces" => traces,
    "root_cause" => root_cause,
    "cluster" => cluster,
    "cluster_root" => cluster_root,
    "cluster_root_edges" => cluster_root_edges,
    "cluster_typed" => cluster_typed,
    "root_finding_typed" => root_finding_typed,
    "glut" => glut,
});

fn edges(value: &Value) -> Result<BTreeSet<(String, String)>, DecodeError> {
    items_with(value, |row| {
        let [source, target] = gmeow_errors::json::read_string_row(row)?;
        Ok((source, target))
    })
    .map(|rows| rows.into_iter().collect())
}

impl FromJson for MetaDerivation {
    fn from_json(value: &Value) -> Result<Self, DecodeError> {
        let mut fields = Record::new(value, "struct MetaDerivation")?;
        Ok(Self {
            traces: fields.required_with("traces", edges)?,
            root_cause: fields.required_with("root_cause", edges)?,
            cluster: fields.required_with("cluster", edges)?,
            cluster_root: fields.required("cluster_root")?,
            cluster_root_edges: fields.required_with("cluster_root_edges", edges)?,
            cluster_typed: fields.required("cluster_typed")?,
            root_finding_typed: fields.required("root_finding_typed")?,
            glut: fields.required_with("glut", edges)?,
        })
    }
}
