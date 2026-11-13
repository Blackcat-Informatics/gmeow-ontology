// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Declared and recorded reads of one stage product by one consumer.
//!
//! A dependency edge of the build graph says more than "this stage runs after that one".
//! It says which lanes of the producer's product the consumer reads. The authored
//! pipeline description declares that read set on the edge; the scheduler hands the
//! consumer a view of the product that checks every access against the declaration and
//! records it; and the two are compared when the stage returns. A read the edge does not
//! declare and a declared read the stage never made are both failures, so the graph a
//! plan is derived from is the graph the stage bodies execute.
//!
//! The declaration also decides retention: a product keeps a lane exactly as long as a
//! consumer that has not run yet declares it.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use crate::node::GMEOW;

/// One readable lane of a stage product.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Read {
    /// The whole carrier through the raw bundle: dataset, typed handles,
    /// representation blobs, lookaside records and provenance.
    Carrier,
    /// The frozen dataset.
    Dataset,
    /// One typed handle, by its key.
    Handle(String),
    /// One representation blob, by its lane label.
    BlobRep(String),
    /// One byte artifact, by its exact logical path.
    Artifact(String),
    /// The whole byte-artifact lane.
    AllArtifacts,
}

impl Read {
    /// Whether this read needs the producer's live carrier. Byte artifacts survive the
    /// carrier's release (internal ones when a consumer declares them), so they do not.
    #[must_use]
    pub const fn needs_carrier(&self) -> bool {
        matches!(
            self,
            Self::Carrier | Self::Dataset | Self::Handle(_) | Self::BlobRep(_)
        )
    }

    /// Whether a declaration of `self` permits the access `other`.
    #[must_use]
    pub fn covers(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Carrier, access) => access.needs_carrier(),
            (Self::AllArtifacts, Self::Artifact(_) | Self::AllArtifacts) => true,
            (declared, access) => declared == access,
        }
    }

    /// The Turtle predicate-object pair that declares this read on a
    /// `gmeow:BuildDataFlow` edge.
    #[must_use]
    pub fn turtle(&self) -> String {
        match self {
            Self::Carrier => "gmeow:flowReadsLane gmeow:laneCarrier".to_owned(),
            Self::Dataset => "gmeow:flowReadsLane gmeow:laneDataset".to_owned(),
            Self::AllArtifacts => "gmeow:flowReadsLane gmeow:laneAllArtifacts".to_owned(),
            Self::Handle(key) => format!("gmeow:flowReadsHandle <{key}>"),
            Self::BlobRep(rep) => format!("gmeow:flowReadsBlobRep {rep:?}"),
            Self::Artifact(path) => format!("gmeow:flowReadsArtifact {path:?}"),
        }
    }

    /// Parse a `gmeow:flowReadsLane` individual.
    #[must_use]
    pub fn from_lane_iri(iri: &str) -> Option<Self> {
        match iri.strip_prefix(GMEOW)? {
            "laneCarrier" => Some(Self::Carrier),
            "laneDataset" => Some(Self::Dataset),
            "laneAllArtifacts" => Some(Self::AllArtifacts),
            _ => None,
        }
    }
}

impl std::fmt::Display for Read {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Carrier => f.write_str("the whole carrier"),
            Self::Dataset => f.write_str("the dataset"),
            Self::AllArtifacts => f.write_str("every byte artifact"),
            Self::Handle(key) => write!(f, "handle <{key}>"),
            Self::BlobRep(rep) => write!(f, "representation blob {rep:?}"),
            Self::Artifact(path) => write!(f, "artifact {path:?}"),
        }
    }
}

/// The reads one consumer declares over one producer's product.
pub type ReadSet = BTreeSet<Read>;

/// Whether `declared` permits `access`.
#[must_use]
pub fn permits(declared: &ReadSet, access: &Read) -> bool {
    declared.iter().any(|read| read.covers(access))
}

/// Whether any declared read needs the producer's live carrier.
#[must_use]
pub fn needs_carrier(declared: &ReadSet) -> bool {
    declared.iter().any(Read::needs_carrier)
}

/// The internal `pipeline/` artifacts a read set names. They are dropped when a carrier
/// is released unless a consumer that has not run yet declares them.
pub fn internal_artifacts(declared: &ReadSet) -> impl Iterator<Item = &str> {
    declared.iter().filter_map(|read| match read {
        Read::Artifact(path) if path.starts_with(crate::bundle::INTERNAL_ARTIFACT_PREFIX) => {
            Some(path.as_str())
        }
        _ => None,
    })
}

/// The declared reads of one edge, and what the consumer actually read through it.
#[derive(Debug)]
pub struct EdgeAccess {
    consumer: String,
    producer: String,
    declared: ReadSet,
    observed: Mutex<ReadSet>,
}

impl EdgeAccess {
    /// The access record of `consumer` reading `producer` under `declared`.
    #[must_use]
    pub fn new(consumer: &str, producer: &str, declared: ReadSet) -> Arc<Self> {
        Arc::new(Self {
            consumer: consumer.to_owned(),
            producer: producer.to_owned(),
            declared,
            observed: Mutex::new(ReadSet::new()),
        })
    }

    /// Record one access. The access is always recorded and never refused here: the
    /// stage finishes, and the scheduler then reports every undeclared and every unmade
    /// read of the stage together instead of the first one a run happens to reach.
    pub fn record(&self, access: Read) {
        self.observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(access);
    }

    /// Whether the declaration permits `access`.
    #[must_use]
    pub fn permits(&self, access: &Read) -> bool {
        permits(&self.declared, access)
    }

    /// The consuming stage.
    #[must_use]
    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    /// The producing stage.
    #[must_use]
    pub fn producer(&self) -> &str {
        &self.producer
    }

    /// The declared read set.
    #[must_use]
    pub const fn declared(&self) -> &ReadSet {
        &self.declared
    }

    /// Everything read through this edge so far.
    #[must_use]
    pub fn observed(&self) -> ReadSet {
        self.observed
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }

    /// The accesses made that the declaration does not permit.
    #[must_use]
    pub fn undeclared(&self) -> ReadSet {
        self.observed()
            .into_iter()
            .filter(|access| !self.permits(access))
            .collect()
    }

    /// The declared reads no access exercised.
    #[must_use]
    pub fn unread(&self) -> ReadSet {
        let observed = self.observed();
        self.declared
            .iter()
            .filter(|declared| !observed.iter().any(|access| declared.covers(access)))
            .cloned()
            .collect()
    }
}

/// What a run does with a mismatch between declared and recorded reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadPolicy {
    /// A stage that reads what its edges do not declare, or does not read what they
    /// declare, fails and publishes nothing. Every production run.
    Enforce,
    /// Reads are recorded and reported, and nothing is refused. Only the maintainer
    /// command that prints the read sets a tree's stage bodies make.
    Record,
}

/// The declared-versus-recorded mismatches of one executed stage, one line each.
#[must_use]
pub fn mismatches(edges: &[Arc<EdgeAccess>]) -> Vec<String> {
    let mut lines = Vec::new();
    for edge in edges {
        for access in edge.undeclared() {
            lines.push(format!(
                "{} read {access} of {} but its dataflow edge does not declare it; declare `{}`",
                edge.consumer,
                edge.producer,
                access.turtle()
            ));
        }
        for declared in edge.unread() {
            lines.push(format!(
                "{} declares it reads {declared} of {} and never did; remove `{}` or the edge",
                edge.consumer,
                edge.producer,
                declared.turtle()
            ));
        }
    }
    lines
}

/// The Turtle that declares exactly what was recorded over `edges`, for authoring.
#[must_use]
pub fn observed_turtle(edges: &[Arc<EdgeAccess>]) -> String {
    let mut out = String::new();
    for edge in edges {
        let short = |stage: &str| stage.strip_prefix("stage-").unwrap_or(stage).to_owned();
        out.push_str(&format!(
            "gmeow:pipeline-dataflow-{}-{}\n    a gmeow:BuildDataFlow ;\n    gmeow:buildFlowFrom gmeow:{} ;\n    gmeow:buildFlowTo gmeow:{}",
            short(&edge.consumer),
            short(&edge.producer),
            edge.producer,
            edge.consumer
        ));
        for access in edge.observed() {
            out.push_str(" ;\n    ");
            out.push_str(&access.turtle());
        }
        out.push_str(" .\n\n");
    }
    out
}

#[cfg(test)]
#[path = "reads.tests.rs"]
mod tests;
