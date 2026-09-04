// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Shape-grounding **certificates** — the durable, re-derived preservation judgment for
//! every `logic:formalizes` record on the projected constraint surfaces.
//!
//! A `logic:formalizes` record is a projected `sh:NodeShape` (on
//! `generated/shapes/constraint-shapes.ttl` or `generated/shapes/procedural-constraints.ttl`)
//! that names the canonical `logic:` source — or the legacy hand-authored shape — it
//! formalizes. The migration oracle proves the record's equivalence once, at migration
//! time; this module makes that proof a PERMANENT machine-checked fact: every regenerate
//! re-derives each record's preservation judgment from the record's own lowered shape by
//! re-running the real certify/oracle machinery, never by copying a stored verdict.
//!
//! Per record the derivation is:
//!
//! 1. **Read** the record's shape through the oracle's SHACL reader
//!    ([`read_shacl_shape`]) — the covered enforcement IR plus the explicit residue list.
//! 2. **Execute-parse** the record's own shape subgraph as a real SHACL document
//!    ([`purrdf::shapes::engine::parse_shapes`]) — a record that cannot run as a
//!    validator certifies nothing.
//! 3. **Certify** the OWL/RDFS-expressible core through the real lift/derive round-trip
//!    ([`certify`]): `derive_validation_shapes(lift(shape)) ≡ core(shape)` must hold.
//! 4. **Classify** the judgment from the machinery's own outputs (reusing the existing
//!    loss-ledger vocabulary, never new terms): a record whose whole enforcement lifts to
//!    OWL/RDFS and re-derives is [`PreservationKind::SoundUnder`]
//!    (`logic:SoundUnderApproximation`); a record whose enforcement rides closed-world
//!    constructs with no entailment form (`sh:sparql`, a raw `sh:SPARQLTarget`, …) is
//!    [`PreservationKind::ValidationOnly`] (`logic:ValidationOnly` — it validates but does
//!    not entail).
//!
//! A record whose judgment cannot be derived — an unreadable shape, a non-parsing SHACL
//! subgraph, a failed certify round-trip, or a record with no derivable enforcement at
//! all — is an **error**, never a skipped entry (hard-fail, no-optionality).
//!
//! The pipeline's mappings stage projects the certificates to the committed
//! `generated/logic/shape-grounding-ledger.ttl`; the dev-cli migration oracle
//! (`gmeow-dev shape-equivalence`) reads the record scan through the same
//! [`formalizes_records`] / [`record_failure_classes`] primitives, so both surfaces share
//! ONE implementation.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_errors::Diag;
use gmeow_logic_compile::ir::PreservationKind;
use gmeow_logic_compile::projections::lift::certify;
use gmeow_logic_compile::projections::subsumption::residue_normal_form;
use purrdf::{DatasetView, GraphMatch, RdfDataset, TermRef, TermValue};

use crate::shape_oracle::{read_shacl_shape, shape_subgraph_ttl};

/// `logic:formalizes` — the back-reference every projected constraint shape carries,
/// naming the canonical `logic:` source (or the legacy shape) it formalizes.
pub const LOGIC_FORMALIZES: &str = "https://blackcatinformatics.ca/logic/formalizes";
/// `gmeow:enforcesFailureClass` — the typed conformance-failure class a record raises.
pub const GMEOW_ENFORCES_FAILURE_CLASS: &str =
    "https://blackcatinformatics.ca/gmeow/enforcesFailureClass";

/// Build a `validate.parse` diagnostic (the same substrate the shape oracle hard-fails
/// on) from a shape-grounding derivation failure.
fn grounding_err(detail: String) -> Diag {
    Diag::of_kind(crate::error::Parse { detail })
}

/// The document-level prefixes the projected constraint surfaces declare
/// (`constraint-shapes.ttl` / `procedural-constraints.ttl`). A record's extracted
/// subgraph ([`shape_subgraph_ttl`]) carries absolute IRIs only, but its `sh:select`
/// bodies resolve CURIEs (`rdfs:subClassOf*`) against the DOCUMENT prefixes, so the
/// executable-SHACL parse re-supplies the same header the surfaces carry.
const SURFACE_PREFIXES: &str = "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
     @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
     @prefix sh:    <http://www.w3.org/ns/shacl#> .\n\
     @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n\n";

/// Every `logic:formalizes` record on a projected constraint surface: the record
/// subject IRI → the formalized IRIs it names (sorted, deduplicated). A non-IRI
/// subject or object never forms a record (a well-formed surface authors IRIs only).
pub fn formalizes_records(ds: &RdfDataset) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let Some(formalizes) = ds.term_id_by_value(&TermValue::iri(LOGIC_FORMALIZES)) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(formalizes), None, GraphMatch::Any) {
        if let (TermRef::Iri(s), TermRef::Iri(o)) = (ds.resolve(q.s), ds.resolve(q.o)) {
            out.entry(s.to_owned()).or_default().insert(o.to_owned());
        }
    }
    out
}

/// The `gmeow:enforcesFailureClass` values carried by each `logic:formalizes` record
/// subject on a projected constraint surface (sorted, deduplicated). Typed conformance
/// diagnostics are part of the migration proof even though they do not alter which data
/// graph conforms.
pub fn record_failure_classes(ds: &RdfDataset) -> BTreeMap<String, BTreeSet<String>> {
    let mut out: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let (Some(formalizes), Some(enforces)) = (
        ds.term_id_by_value(&TermValue::iri(LOGIC_FORMALIZES)),
        ds.term_id_by_value(&TermValue::iri(GMEOW_ENFORCES_FAILURE_CLASS)),
    ) else {
        return out;
    };
    for q in ds.quads_for_pattern(None, Some(formalizes), None, GraphMatch::Any) {
        let TermRef::Iri(record) = ds.resolve(q.s) else {
            continue;
        };
        for fc in ds.quads_for_pattern(Some(q.s), Some(enforces), None, GraphMatch::Any) {
            if let TermRef::Iri(failure) = ds.resolve(fc.o) {
                out.entry(record.to_owned())
                    .or_default()
                    .insert(failure.to_owned());
            }
        }
    }
    out
}

/// One re-derived grounding certificate: the record subject, what it formalizes, the
/// typed failure classes it raises, and the preservation judgment the machinery derived
/// THIS run (never a stored copy), with a deterministic human basis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroundingCertificate {
    /// The record subject IRI (the projected constraint shape carrying `logic:formalizes`).
    pub record: String,
    /// The formalized IRIs (the canonical `logic:` sources / legacy shapes), sorted.
    pub formalizes: Vec<String>,
    /// The `gmeow:enforcesFailureClass` values on the record, sorted (possibly empty).
    pub failure_classes: Vec<String>,
    /// The re-derived preservation judgment (existing loss-ledger vocabulary only).
    pub preservation: PreservationKind,
    /// A deterministic explanation of HOW the judgment was derived.
    pub basis: String,
}

/// Derive the grounding certificate for every `logic:formalizes` record across the
/// projected constraint surfaces. Deterministic: certificates are returned sorted by
/// record IRI.
///
/// # Errors
///
/// * a record subject appears on more than one surface (an ambiguous certificate);
/// * a record's shape fails the oracle reader ([`read_shacl_shape`]);
/// * a record's shape subgraph does not parse as an executable SHACL document;
/// * the lift/certify round-trip over the record's OWL/RDFS-expressible core fails;
/// * a record carries no derivable enforcement at all (nothing to certify).
///
/// Any of these is a HARD failure of the whole derivation — a formalizes record whose
/// judgment cannot be derived is never a skipped entry.
pub fn derive_grounding_certificates(
    surfaces: &[std::sync::Arc<RdfDataset>],
) -> gmeow_errors::Result<Vec<GroundingCertificate>> {
    let mut out: Vec<GroundingCertificate> = Vec::new();
    let mut seen: BTreeSet<String> = BTreeSet::new();
    for (surface_idx, ds) in surfaces.iter().enumerate() {
        let failure_classes = record_failure_classes(ds);
        for (record, formalized) in formalizes_records(ds) {
            if !seen.insert(record.clone()) {
                return Err(grounding_err(format!(
                    "shape-grounding: record <{record}> appears on more than one projected \
                     constraint surface; a certificate must have exactly one defining surface"
                )));
            }
            // 1. Read the record's lowered shape through the oracle reader.
            let read = read_shacl_shape(ds, &record).map_err(|e| {
                grounding_err(format!(
                    "shape-grounding: record <{record}> is not a readable sh:NodeShape \
                     (judgment underivable): {e}"
                ))
            })?;
            // 2. The record's own subgraph must parse as an executable SHACL document —
            //    a record that cannot run as a validator certifies nothing.
            let subgraph = shape_subgraph_ttl(
                ds,
                std::slice::from_ref(&record),
                &format!("g{surface_idx}"),
            );
            if subgraph.trim().is_empty() {
                return Err(grounding_err(format!(
                    "shape-grounding: record <{record}> has an empty shape subgraph \
                     (judgment underivable)"
                )));
            }
            purrdf::shapes::engine::parse_shapes(&format!("{SURFACE_PREFIXES}{subgraph}"), None)
                .map_err(|e| {
                    grounding_err(format!(
                        "shape-grounding: record <{record}> does not parse as an executable \
                     SHACL shape (judgment underivable): {e}"
                    ))
                })?;
            // 3. Re-run the lift/certify round-trip over the OWL/RDFS-expressible core:
            //    derive_validation_shapes(lift(shape)) ≡ core(shape), against the REAL
            //    forward derive. This is the per-regenerate re-derivation, not a copy.
            certify(&read.ir).map_err(|e| {
                grounding_err(format!(
                    "shape-grounding: record <{record}> failed the lift/certify \
                     round-trip (judgment underivable): {e}"
                ))
            })?;
            // 4. Classify from the machinery's own outputs. The `logic:formalizes`
            //    back-reference is record METADATA (it carries no enforcement), so it is
            //    excluded from the enforcement residue the judgment keys on.
            let mut residue: Vec<String> = read
                .unsupported
                .iter()
                .filter(|p| p.as_str() != LOGIC_FORMALIZES)
                .cloned()
                .collect();
            residue.extend(residue_normal_form(&read.ir));
            residue.sort();
            residue.dedup();
            let covered = !read.ir.properties.is_empty() || !read.ir.node_components.is_empty();
            let (preservation, basis) = if residue.is_empty() {
                if !covered {
                    return Err(grounding_err(format!(
                        "shape-grounding: record <{record}> carries no derivable \
                         enforcement (no covered components, no residue constructs); \
                         nothing to certify"
                    )));
                }
                (
                    PreservationKind::SoundUnder,
                    "re-derived this regenerate: the lift/certify round-trip re-derived the \
                     record's full enforcement from its OWL/RDFS axiom antecedents \
                     (derive_validation_shapes(lift(shape)) is enforcement-equivalent to the \
                     shape; no closed-world residue), so the projection is a sound \
                     approximation of its canonical logic: source"
                        .to_owned(),
                )
            } else {
                (
                    PreservationKind::ValidationOnly,
                    format!(
                        "re-derived this regenerate: the record parses as an executable SHACL \
                         validator and its OWL/RDFS-expressible core passed the lift/certify \
                         round-trip, but its enforcement is carried by closed-world constructs \
                         with no entailment form ({}), so the projection validates without \
                         entailing",
                        residue.join(", ")
                    ),
                )
            };
            let record_failures: Vec<String> = failure_classes
                .get(&record)
                .map(|s| s.iter().cloned().collect())
                .unwrap_or_default();
            out.push(GroundingCertificate {
                record,
                formalizes: formalized.into_iter().collect(),
                failure_classes: record_failures,
                preservation,
                basis,
            });
        }
    }
    out.sort_by(|a, b| a.record.cmp(&b.record));
    Ok(out)
}

/// A Turtle string-literal escape (`\`, `"`, and the C0 whitespace).
fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Render the certificates as a Turtle document — one entry per record, in the
/// certificates' (sorted) order, using ONLY existing vocabulary: `logic:formalizes`
/// (one line per formalized IRI, so the entry count is line-countable against the
/// surfaces), `gmeow:enforcesFailureClass`, `logic:preservationKind`, and an
/// `rdfs:comment` derivation basis. Deterministic: a pure fold of its input.
pub fn render_grounding_ledger(certs: &[GroundingCertificate]) -> String {
    let mut out = String::from(
        "@prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix rdfs:  <http://www.w3.org/2000/01/rdf-schema#> .\n",
    );
    for c in certs {
        out.push('\n');
        out.push_str(&format!("<{}>\n", c.record));
        for f in &c.formalizes {
            out.push_str(&format!("    logic:formalizes <{f}> ;\n"));
        }
        for f in &c.failure_classes {
            out.push_str(&format!("    gmeow:enforcesFailureClass <{f}> ;\n"));
        }
        out.push_str(&format!(
            "    logic:preservationKind logic:{} ;\n",
            c.preservation.as_str()
        ));
        out.push_str(&format!("    rdfs:comment \"{}\" .\n", esc(&c.basis)));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use purrdf::parse_dataset;

    const PREFIXES: &str = "@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
         @prefix logic: <https://blackcatinformatics.ca/logic/> .\n\
         @prefix gmeow: <https://blackcatinformatics.ca/gmeow/> .\n\
         @prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n\n";

    fn surface(ttl: &str) -> std::sync::Arc<RdfDataset> {
        parse_dataset(format!("{PREFIXES}{ttl}").as_bytes(), "text/turtle", None)
            .expect("fixture surface parses")
    }

    /// A `sh:sparql` procedural record (the production shape of both constraint
    /// surfaces): the judgment re-derives to `logic:ValidationOnly` and the entry
    /// carries the record subject, the formalized IRI, and the failure class.
    #[test]
    fn sparql_record_derives_validation_only() {
        let ds = surface(
            "gmeow:DemoConstraintShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:DemoLegacyShape ;\n\
                 gmeow:enforcesFailureClass gmeow:MissingRequiredProperty ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"demo\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/p> $this . }\"\"\" ] .\n",
        );
        let certs = derive_grounding_certificates(&[ds]).expect("derivable");
        assert_eq!(certs.len(), 1);
        let c = &certs[0];
        assert_eq!(
            c.record,
            "https://blackcatinformatics.ca/gmeow/DemoConstraintShape"
        );
        assert_eq!(
            c.formalizes,
            vec!["https://blackcatinformatics.ca/gmeow/DemoLegacyShape".to_owned()]
        );
        assert_eq!(
            c.failure_classes,
            vec!["https://blackcatinformatics.ca/gmeow/MissingRequiredProperty".to_owned()]
        );
        assert_eq!(c.preservation, PreservationKind::ValidationOnly);
        assert!(
            c.basis.contains("shacl#sparql"),
            "the basis names the closed-world construct: {}",
            c.basis
        );
    }

    /// A fully-declarative record (cardinality + `sh:class`): the lift/certify
    /// round-trip re-derives its whole enforcement, so the judgment is
    /// `logic:SoundUnderApproximation`.
    #[test]
    fn declarative_record_certifies_sound_under() {
        let ds = surface(
            "gmeow:DemoDeclShape a sh:NodeShape ;\n\
                 logic:formalizes logic:demoAxiom ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:property [ sh:path gmeow:p ; sh:minCount 1 ; sh:class gmeow:Other ] .\n",
        );
        let certs = derive_grounding_certificates(&[ds]).expect("derivable");
        assert_eq!(certs.len(), 1);
        assert_eq!(certs[0].preservation, PreservationKind::SoundUnder);
        assert!(
            certs[0].basis.contains("lift/certify"),
            "the basis records the certify round-trip: {}",
            certs[0].basis
        );
    }

    /// A record whose subject is NOT a readable `sh:NodeShape` is a hard error —
    /// an underivable judgment is never a skipped entry.
    #[test]
    fn unreadable_record_is_an_error_not_a_skip() {
        let ds = surface("gmeow:NotAShape logic:formalizes gmeow:DemoLegacyShape .\n");
        let err = derive_grounding_certificates(&[ds]).expect_err("underivable");
        assert!(
            err.to_string().contains("NotAShape"),
            "the error names the underivable record: {err}"
        );
    }

    /// The same record subject on two surfaces is ambiguous — a hard error.
    #[test]
    fn duplicate_record_across_surfaces_is_an_error() {
        let block = "gmeow:DupShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:DemoLegacyShape ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"demo\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/p> $this . }\"\"\" ] .\n";
        let err = derive_grounding_certificates(&[surface(block), surface(block)])
            .expect_err("ambiguous record");
        assert!(
            err.to_string()
                .contains("more than one projected constraint surface"),
            "unexpected error: {err}"
        );
    }

    /// The renderer is a deterministic pure fold: entries ride in sorted record order,
    /// one `logic:formalizes` line per formalized IRI, and re-rendering is byte-equal.
    #[test]
    fn renderer_is_deterministic_and_line_countable() {
        let ds = surface(
            "gmeow:BShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:LegacyB ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"b\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/p> $this . }\"\"\" ] .\n\
             gmeow:AShape a sh:NodeShape ;\n\
                 logic:formalizes gmeow:LegacyA ;\n\
                 sh:targetClass gmeow:Demo ;\n\
                 sh:sparql [ a sh:SPARQLConstraint ; sh:severity sh:Violation ;\n\
                     sh:message \"a\" ;\n\
                     sh:select \"\"\"SELECT $this WHERE { $this <https://blackcatinformatics.ca/gmeow/q> $this . }\"\"\" ] .\n",
        );
        let certs = derive_grounding_certificates(&[ds]).expect("derivable");
        assert_eq!(certs.len(), 2);
        assert!(
            certs[0].record < certs[1].record,
            "certificates are sorted by record IRI"
        );
        let a = render_grounding_ledger(&certs);
        let b = render_grounding_ledger(&certs);
        assert_eq!(a, b, "re-rendering is byte-identical");
        assert_eq!(
            a.matches("    logic:formalizes <").count(),
            2,
            "one logic:formalizes line per formalized IRI (count-consistent with the surfaces)"
        );
        assert_eq!(
            a.matches("logic:preservationKind").count(),
            2,
            "one judgment per record"
        );
        // The rendered document parses as Turtle.
        parse_dataset(a.as_bytes(), "text/turtle", None).expect("ledger parses");
    }
}
