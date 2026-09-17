// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! One finding projection, consumed as native RDF or as a terminal text surface.

use purrdf_core::{RdfDatasetBuilder, RdfLiteral, TermId};

use super::{GMEOW, LOGIC, RDF_TYPE, XSD_ANY_URI, XSD_NNI, finding_definition, finding_label};
use crate::model::{Location, Report};

trait Sink {
    fn iri(&mut self, subject: &str, predicate: &str, object: &str);
    fn literal(&mut self, subject: &str, predicate: &str, value: RdfLiteral);

    fn text(&mut self, subject: &str, local: &str, value: &str) {
        self.literal(
            subject,
            &format!("{GMEOW}{local}"),
            RdfLiteral::simple(value),
        );
    }

    fn location(&mut self, subject: &str, location: &Location) {
        for (local, value) in [
            ("gtsTermId", location.gts_term_id),
            ("gtsQuadIndex", location.gts_quad_index),
            ("gtsReifierId", location.gts_reifier_id),
            ("gtsFrameIndex", location.gts_frame_index),
            ("gtsSegmentIndex", location.gts_segment_index),
        ] {
            if let Some(value) = value {
                self.literal(
                    subject,
                    &format!("{GMEOW}{local}"),
                    RdfLiteral::typed(value.to_string(), XSD_NNI),
                );
            }
        }
        if let Some(path) = &location.path {
            self.text(subject, "findingLocationPath", path);
        }
        for (local, value) in [
            ("findingLocationLine", location.line),
            ("findingLocationColumn", location.column),
        ] {
            if let Some(value) = value {
                self.literal(
                    subject,
                    &format!("{GMEOW}{local}"),
                    RdfLiteral::typed(value.to_string(), XSD_NNI),
                );
            }
        }
    }
}

/// The presentation adapter retains the public renderer's established ordering.
struct TextSink<'a> {
    graph: &'a str,
    output: String,
}

impl Sink for TextSink<'_> {
    fn iri(&mut self, subject: &str, predicate: &str, object: &str) {
        use std::fmt::Write as _;
        writeln!(
            self.output,
            "<{subject}> <{predicate}> <{object}> <{}> .",
            self.graph
        )
        .expect("writing to a String is infallible");
    }

    fn literal(&mut self, subject: &str, predicate: &str, value: RdfLiteral) {
        use std::fmt::Write as _;
        write!(
            self.output,
            "<{subject}> <{predicate}> \"{}\"",
            super::nq_escape(&value.lexical_form)
        )
        .expect("writing to a String is infallible");
        // This projection authors plain, typed or carrier-language literals only.
        if let Some(language) = value.language {
            write!(self.output, "@{language}").expect("writing to a String is infallible");
        } else if let Some(datatype) = value.datatype {
            write!(self.output, "^^<{datatype}>").expect("writing to a String is infallible");
        }
        writeln!(self.output, " <{}> .", self.graph).expect("writing to a String is infallible");
    }
}

struct NativeSink<'a> {
    builder: &'a mut RdfDatasetBuilder,
    graph: TermId,
}

impl Sink for NativeSink<'_> {
    fn iri(&mut self, subject: &str, predicate: &str, object: &str) {
        let s = self.builder.intern_iri(subject);
        let p = self.builder.intern_iri(predicate);
        let o = self.builder.intern_iri(object);
        self.builder.push_quad(s, p, o, Some(self.graph));
    }

    fn literal(&mut self, subject: &str, predicate: &str, value: RdfLiteral) {
        let s = self.builder.intern_iri(subject);
        let p = self.builder.intern_iri(predicate);
        let o = self.builder.intern_literal(value);
        self.builder.push_quad(s, p, o, Some(self.graph));
    }
}

pub(super) fn to_text(report: &Report, graph: &str) -> String {
    let mut sink = TextSink {
        graph,
        output: String::new(),
    };
    emit(report, graph, &mut sink);
    sink.output
}

pub(super) fn append(report: &Report, graph: &str, builder: &mut RdfDatasetBuilder) {
    let graph_id = builder.intern_iri(graph);
    emit(
        report,
        graph,
        &mut NativeSink {
            builder,
            graph: graph_id,
        },
    );
}

fn emit(report: &Report, graph: &str, sink: &mut impl Sink) {
    let normalized = report.normalized();
    let rules = super::rule_map(&normalized);
    for (index, finding) in normalized.findings.iter().enumerate() {
        // Keep ledger identities so antecedent and quad-provenance edges close.
        let subject = finding.finding_iri.clone().unwrap_or_else(|| {
            format!(
                "{GMEOW}diagnostics/finding/{}-{index}",
                super::stable_fingerprint(finding)
            )
        });
        sink.iri(&subject, RDF_TYPE, &format!("{GMEOW}Finding"));
        for (predicate, object) in crate::abox::abox_annotations(
            &subject,
            &finding_label(&finding.code, &finding.message),
            &finding_definition(finding),
            graph,
        ) {
            match object {
                crate::abox::AboxObject::Iri(iri) => sink.iri(&subject, predicate, &iri),
                crate::abox::AboxObject::CarrierLiteral(value) => sink.literal(
                    &subject,
                    predicate,
                    RdfLiteral::language_tagged(value, crate::abox::X_GMEOW_ENGLISH),
                ),
            }
        }
        sink.iri(
            &subject,
            &format!("{GMEOW}findingSeverity"),
            &super::severity_individual(finding.severity),
        );
        sink.text(&subject, "findingCode", &finding.code);
        sink.text(&subject, "findingMessage", &finding.message);
        if let Some(tool) = &finding.tool {
            sink.text(&subject, "findingTool", tool);
        }
        if let Some(category) = finding.category {
            sink.iri(
                &subject,
                &format!("{GMEOW}findingCategory"),
                &format!("{LOGIC}{}", category.iri_local()),
            );
        }
        if let Some(standpoint) = finding.standpoint {
            sink.iri(
                &subject,
                &format!("{GMEOW}findingStandpoint"),
                &format!("{GMEOW}{}", standpoint.iri_local()),
            );
        }
        if let Some(class) = &finding.failure_class {
            sink.iri(&subject, &format!("{GMEOW}findingFailureClass"), class);
        }
        for antecedent in &finding.antecedents {
            sink.iri(&subject, &format!("{GMEOW}findingAntecedent"), antecedent);
        }
        if let Some(anchor) = &finding.anchor_iri {
            sink.iri(&subject, &format!("{GMEOW}findingAnchor"), anchor);
            if finding.anchor_non_trivial {
                sink.iri(anchor, RDF_TYPE, &format!("{GMEOW}NonTrivialAnchor"));
            }
        }
        for remediation in &finding.remediation {
            sink.text(&subject, "findingRemediation", &remediation.text);
        }
        for guidance in &finding.guidance {
            sink.text(
                &subject,
                guidance.modality.predicate_local(),
                &guidance.text,
            );
        }
        for quad in &finding.derived_from_quads {
            sink.iri(&subject, &format!("{GMEOW}findingDerivedFromQuad"), quad);
        }
        // Only grade coordinates are projected. Authored native rules own the
        // gate verdict; this renderer never asserts their conclusion.
        for suggestion in &finding.suggestions {
            sink.text(&subject, "findingSuggestion", suggestion);
        }
        if let Some(rule) = rules.get(finding.code.as_str())
            && let Some(uri) = &rule.help_uri
        {
            sink.literal(
                &subject,
                &format!("{GMEOW}findingHelpUri"),
                RdfLiteral::typed(uri, XSD_ANY_URI),
            );
        }
        for (index, location) in finding.locations.iter().enumerate() {
            let node = format!("{subject}/location/{index}");
            sink.iri(&subject, &format!("{GMEOW}findingLocation"), &node);
            sink.location(&node, location);
        }
        for (index, label) in finding.related_labels.iter().enumerate() {
            let node = format!("{subject}/relatedLabel/{index}");
            sink.iri(&subject, &format!("{GMEOW}relatedLabel"), &node);
            sink.text(&node, "labelMessage", &label.message);
            sink.location(&node, &label.location);
        }
    }
}
