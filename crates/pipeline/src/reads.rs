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
//! The declaration also decides retention. A product keeps its carrier until the last
//! consumer whose edge declares a carrier read has run. An internal `pipeline/` artifact
//! that any consumer in the run declares is kept through every release of its producer
//! until the run ends, whether or not that consumer has run yet; so is the source-span
//! table until its last declared reader has run.

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
    /// No lane: the edge orders the two stages and folds the producer's identity into
    /// the consumer's cache key. Declared explicitly, so an edge that carries no data is
    /// a visible decision and never an unread dependency.
    Identity,
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
    ///
    /// The whole byte-artifact lane covers COMMITTED artifacts only. An internal
    /// `pipeline/` artifact is permitted only by a declaration that names its exact path,
    /// because retention keeps an internal artifact only when it is named.
    #[must_use]
    pub fn covers(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Carrier, access) => access.needs_carrier(),
            (Self::AllArtifacts, Self::Artifact(path)) => !is_internal_artifact(path),
            (Self::AllArtifacts, Self::AllArtifacts) => true,
            (Self::Identity, _) | (_, Self::Identity) => false,
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
            Self::Identity => "gmeow:flowReadsLane gmeow:laneIdentity".to_owned(),
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
            "laneIdentity" => Some(Self::Identity),
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
            Self::Identity => f.write_str("its identity only"),
            Self::Handle(key) => write!(f, "handle <{key}>"),
            Self::BlobRep(rep) => write!(f, "representation blob {rep:?}"),
            Self::Artifact(path) => write!(f, "artifact {path:?}"),
        }
    }
}

/// Whether a logical artifact path is an internal dataflow artifact (not a committed output).
#[must_use]
pub fn is_internal_artifact(path: &str) -> bool {
    path.starts_with(crate::bundle::INTERNAL_ARTIFACT_PREFIX)
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

/// The internal `pipeline/` artifacts a read set names. A release drops every internal
/// artifact except those some consumer of the run declares; those are kept through every
/// release of the producer until the run ends.
pub fn internal_artifacts(declared: &ReadSet) -> impl Iterator<Item = &str> {
    declared.iter().filter_map(|read| match read {
        Read::Artifact(path) if is_internal_artifact(path) => Some(path.as_str()),
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

    /// The declared reads no access exercised. An identity-only declaration reads no
    /// lane, so there is nothing for an access to exercise.
    #[must_use]
    pub fn unread(&self) -> ReadSet {
        let observed = self.observed();
        self.declared
            .iter()
            .filter(|declared| **declared != Read::Identity)
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

/// The reads of one stage that its dataflow edges do not declare, one line each, each
/// naming the consumer, the producer, the lane and the declaration to add. A stage that
/// failed or panicked part-way is reported by this alone: the reads it never reached are
/// not differences.
#[must_use]
pub fn undeclared_reads(edges: &[Arc<EdgeAccess>]) -> Vec<String> {
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
    }
    lines
}

/// The declared-versus-recorded mismatches of one executed stage, one line each.
#[must_use]
pub fn mismatches(edges: &[Arc<EdgeAccess>]) -> Vec<String> {
    let mut lines = undeclared_reads(edges);
    for edge in edges {
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
        let observed = edge.observed();
        if observed.is_empty() {
            out.push_str(" ;\n    ");
            out.push_str(&Read::Identity.turtle());
        }
        for access in observed {
            out.push_str(" ;\n    ");
            out.push_str(&access.turtle());
        }
        out.push_str(" .\n\n");
    }
    out
}

/// Run the committed-scope pipeline once under [`ReadPolicy::Record`] and return the
/// Turtle that declares exactly what each stage body read through each edge.
///
/// This is how the read sets of the pipeline description are authored and checked
/// against a tree: nothing is refused, every consumed product is handed over whole, and
/// the output is the declaration the stage bodies need. A stage served from the cache
/// records nothing, so the run uses no stage cache. It writes no output of the pipeline.
///
/// # Errors
/// Any failure of the pipeline run itself.
pub fn record_run(root: &std::path::Path, jobs: usize) -> Result<String, gmeow_errors::Diag> {
    let spec =
        crate::run::spec_for_scope(crate::full_spec(), crate::run::RunOutputScope::Committed)?;
    let graph = spec.validate()?;
    let bound = crate::bind(&spec, &graph, &crate::default_registry())?;
    let mut ctx = crate::RunContext::open_uncached(root, jobs);
    ctx.read_policy = ReadPolicy::Record;
    let result = crate::scheduler::run(&graph, &bound, &mut ctx)?;
    let mut edges = result.recorded_reads;
    edges.sort_by(|a, b| (a.consumer(), a.producer()).cmp(&(b.consumer(), b.producer())));
    let mut out = observed_turtle(&edges);
    out.push_str(&observed_writes(&result.products)?);
    Ok(out)
}

/// What each stage's product carries, as Turtle comment lines: its byte artifacts, typed
/// handles and representation blobs. Read beside the recorded reads, it shows which
/// producer writes each lane a consumer reads.
fn observed_writes(
    products: &std::collections::BTreeMap<String, crate::node::StageProduct>,
) -> Result<String, gmeow_errors::Diag> {
    let mut out = String::from("# What each stage's product carries.\n");
    for (stage, product) in products {
        for path in product.artifact_refs()?.keys() {
            out.push_str(&format!("# {stage} artifact {path:?}\n"));
        }
        let bundle = product.raw_bundle();
        for key in bundle.handles().keys() {
            out.push_str(&format!("# {stage} handle <{key}>\n"));
        }
        let reps: BTreeSet<&str> = bundle
            .lookaside()
            .blobs
            .iter()
            .filter_map(|record| record.representation.as_deref())
            .collect();
        for rep in reps {
            out.push_str(&format!("# {stage} blob-rep {rep:?}\n"));
        }
    }
    Ok(out)
}

#[cfg(test)]
#[path = "reads.tests.rs"]
mod tests;
