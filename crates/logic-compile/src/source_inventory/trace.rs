// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Versioned compiler trace identity, streamed without copying the source inventory.

use serde::Serialize;
use serde::ser::SerializeSeq;

use crate::frontend::{
    AxiomSource, CompiledTheory, FormulaDisposition, OwnerDisposition, OwnerFamily,
};
use purrdf::RdfDataset;

use super::identity::{node_anchor, term_anchor};

const COMPILER_SOURCE_TRACE_DOMAIN: purrdf_hash::Domain =
    purrdf_hash::Domain::new(b"gmeow-compiler-source-trace-v1\0");

#[derive(Serialize)]
struct TraceV1<'a> {
    schema_version: u32,
    axioms: Axioms<'a>,
    owners: Owners<'a>,
    formulas: Formulas<'a>,
    diagnostics: Diagnostics<'a>,
}

struct Axioms<'a>(&'a CompiledTheory);
struct Origins<'a> {
    dataset: &'a RdfDataset,
    sources: &'a [AxiomSource],
}
struct Owners<'a>(&'a CompiledTheory);
struct Formulas<'a>(&'a CompiledTheory);
struct Diagnostics<'a>(&'a CompiledTheory);

#[derive(Serialize)]
enum Origin {
    Statement {
        subject: String,
        predicate: String,
        object: String,
    },
    ClassExpression {
        source: String,
    },
    Reification {
        source: String,
    },
    Formula {
        source: String,
    },
}

impl Serialize for Origins<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.sources.len()))?;
        for source in self.sources {
            let origin = match source {
                AxiomSource::Statement {
                    subject,
                    predicate,
                    object,
                } => Origin::Statement {
                    subject: node_anchor(self.dataset, *subject),
                    predicate: term_anchor(self.dataset, self.dataset.resolve(*predicate)),
                    object: term_anchor(self.dataset, self.dataset.resolve(*object)),
                },
                AxiomSource::ClassExpression(node) => Origin::ClassExpression {
                    source: node_anchor(self.dataset, *node),
                },
                AxiomSource::Reification(node) => Origin::Reification {
                    source: node_anchor(self.dataset, *node),
                },
                AxiomSource::Formula(node) => Origin::Formula {
                    source: node_anchor(self.dataset, *node),
                },
            };
            sequence.serialize_element(&origin)?;
        }
        sequence.end()
    }
}

impl Serialize for Axioms<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.axiom_sources().len()))?;
        for sources in self.0.axiom_sources() {
            sequence.serialize_element(&Origins {
                dataset: self.0.source().dataset(),
                sources,
            })?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct Owner<'a> {
    source: String,
    family: OwnerFamily,
    route: &'a OwnerDisposition,
    diagnostics: &'a [usize],
}

impl Serialize for Owners<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.owner_lowerings().len()))?;
        for owner in self.0.owner_lowerings() {
            sequence.serialize_element(&Owner {
                source: node_anchor(self.0.source().dataset(), owner.source),
                family: owner.family,
                route: &owner.disposition,
                diagnostics: &owner.diagnostics,
            })?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct Formula<'a> {
    source: String,
    route: &'a FormulaDisposition,
}

impl Serialize for Formulas<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.formula_lowerings().len()))?;
        for formula in self.0.formula_lowerings() {
            sequence.serialize_element(&Formula {
                source: node_anchor(self.0.source().dataset(), formula.source),
                route: &formula.disposition,
            })?;
        }
        sequence.end()
    }
}

#[derive(Serialize)]
struct Diagnostic<'a> {
    severity: &'a str,
    code: &'a str,
    message: &'a str,
    subject: &'a Option<String>,
}

impl Serialize for Diagnostics<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.diagnostics().len()))?;
        for diagnostic in self.0.diagnostics() {
            sequence.serialize_element(&Diagnostic {
                severity: diagnostic.severity.as_str(),
                code: &diagnostic.code,
                message: &diagnostic.message,
                subject: &diagnostic.subject,
            })?;
        }
        sequence.end()
    }
}

pub(super) fn digest(theory: &CompiledTheory) -> String {
    let mut hasher = purrdf_hash::blake3::Hasher::new();
    hasher.update(COMPILER_SOURCE_TRACE_DOMAIN.as_bytes());
    serde_json::to_writer(
        &mut hasher,
        &TraceV1 {
            schema_version: 1,
            axioms: Axioms(theory),
            owners: Owners(theory),
            formulas: Formulas(theory),
            diagnostics: Diagnostics(theory),
        },
    )
    .expect("typed compiler traces serialize to an infallible digest sink");
    purrdf_hash::hex::encode(hasher.finalize().as_bytes())
}
