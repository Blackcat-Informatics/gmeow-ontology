// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[derive(Debug, PartialEq, Eq)]
struct Refusal {
    predicate: String,
}

#[derive(Default)]
struct RefusingLiteral {
    visited: Vec<&'static str>,
}

impl Sink for RefusingLiteral {
    type Error = Refusal;

    fn iri(&mut self, _: &str, _: &str, _: &str) -> Result<(), Self::Error> {
        self.visited.push("iri");
        Ok(())
    }

    fn literal(&mut self, _: &str, predicate: &str, _: RdfLiteral) -> Result<(), Self::Error> {
        self.visited.push("literal");
        Err(Refusal {
            predicate: predicate.to_owned(),
        })
    }
}

#[test]
fn finding_projection_preserves_the_first_sink_refusal_and_stops() {
    let mut report = Report::new("synthetic");
    report.add_finding(crate::model::Finding::new(
        crate::model::Severity::Error,
        "synthetic.source",
        "original finding",
    ));
    let mut sink = RefusingLiteral::default();
    assert_eq!(
        emit(&report, "urn:diagnostics", &mut sink),
        Err(Refusal {
            predicate: crate::abox::RDFS_LABEL.to_owned(),
        })
    );
    assert_eq!(sink.visited, ["iri", "literal"]);
}
