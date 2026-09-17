// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Consume-path outputs over the selected demonstrator and shared language analyses.
//! These observations describe those inputs; they do not authorize calculus rewrites.

use std::collections::{BTreeMap, BTreeSet};

use gmeow_lang_bridge::{ConsumeProjection, Gmn0Model, GmnConsumeError, consume_project};
use purrdf::{RdfQuad, RdfTerm};
use serde::{Deserialize, Serialize};

use crate::stages::parse_sources::SourceCatalog;

pub(super) const SOURCE: &str = "slices/grounding/lang/examples/gmn-ring-consume.ttl";
pub(super) const CORE: &str = "https://blackcatinformatics.ca/gmeow/gmnRingCore";
pub(super) const TRUSTED: &str = "https://blackcatinformatics.ca/gmeow/gmnRingTrusted";
pub(super) const RESTRICTED: &str = "https://blackcatinformatics.ca/gmeow/gmnRingRestricted";
pub(super) const NATO: &str = "https://blackcatinformatics.ca/gmeow/gmnRingNato";
const UNKNOWN: &str = "https://example.org/notARing";
const EX: &str = "https://blackcatinformatics.ca/gmeow/examples/lang/";
const CONTENT_RING: &str = "https://blackcatinformatics.ca/gmeow/gmnContentRing";

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Observation {
    pub rings: BTreeSet<String>,
    pub within: BTreeMap<String, BTreeMap<String, Option<bool>>>,
    pub trusted: Result<ConsumeProjection, GmnConsumeError>,
    pub nato: Result<ConsumeProjection, GmnConsumeError>,
    pub core: Result<ConsumeProjection, GmnConsumeError>,
    pub budgeted: Option<Result<ConsumeProjection, GmnConsumeError>>,
    pub unclassified: Result<ConsumeProjection, GmnConsumeError>,
    pub reference_leak: Result<ConsumeProjection, GmnConsumeError>,
    pub unknown_target: Result<ConsumeProjection, GmnConsumeError>,
}

pub(super) fn observe(
    model: &Gmn0Model,
    catalog: &SourceCatalog,
) -> gmeow_errors::Result<Observation> {
    let language = catalog.language()?;
    let lattice = &language.lattice;
    let dictionary = &language.dictionary;
    let names = [CORE, TRUSTED, RESTRICTED, NATO];
    let rings = names
        .iter()
        .filter(|name| lattice.contains(name))
        .map(|name| (*name).to_owned())
        .collect();
    let within = names
        .into_iter()
        .chain([UNKNOWN])
        .map(|content| {
            let targets = names
                .iter()
                .map(|target| ((*target).to_owned(), lattice.within(content, target)))
                .collect();
            (content.to_owned(), targets)
        })
        .collect();
    let project = |model: &Gmn0Model, target, budget| {
        consume_project(model, lattice, target, budget, dictionary)
    };
    let trusted = project(model, TRUSTED, None);
    // The budget probe requires a successful, nonempty baseline. Its absence is
    // visible evidence that the consumer must reject, never an unbudgeted fallback.
    let budgeted = trusted
        .as_ref()
        .ok()
        .and_then(|full| full.tokens.checked_sub(1))
        .map(|budget| project(model, TRUSTED, Some(budget)));
    let iri = |local: &str| RdfTerm::iri(format!("{EX}{local}"));
    let unclassified = Gmn0Model {
        quads: vec![RdfQuad::new(
            iri("ringDemoOrphan"),
            format!("{EX}ringDemoField"),
            iri("ringDemoOrphanDatum"),
        )],
    };
    let reference_leak = Gmn0Model {
        quads: vec![
            RdfQuad::new(iri("ringDemoCore"), CONTENT_RING, RdfTerm::iri(CORE)),
            RdfQuad::new(
                iri("ringDemoCore"),
                format!("{EX}ringDemoRefers"),
                iri("ringDemoRestricted"),
            ),
            RdfQuad::new(
                iri("ringDemoRestricted"),
                CONTENT_RING,
                RdfTerm::iri(RESTRICTED),
            ),
            RdfQuad::new(
                iri("ringDemoRestricted"),
                format!("{EX}ringDemoField"),
                iri("ringDemoRestrictedDatum"),
            ),
        ],
    };
    Ok(Observation {
        rings,
        within,
        trusted,
        nato: project(model, NATO, None),
        core: project(model, CORE, None),
        budgeted,
        unclassified: project(&unclassified, TRUSTED, None),
        reference_leak: project(&reference_leak, TRUSTED, None),
        unknown_target: project(model, UNKNOWN, None),
    })
}
