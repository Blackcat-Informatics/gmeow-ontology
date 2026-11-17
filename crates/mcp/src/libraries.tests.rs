// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! Concurrent persistence through the real workflows with explicit synthetic inputs.

use std::sync::{Mutex, mpsc};

use gmeow_bundle_view::native_query;
use gmeow_logic_compile::action_policy::SOURCE_SHA256;

use super::*;
use crate::storage::InMemorySegmentLibrary;

#[derive(Clone, Copy, Debug)]
enum Workflow {
    Conjecture,
    Candidate,
}

impl Workflow {
    fn schema(self) -> &'static str {
        match self {
            Self::Conjecture => MCP_PERSIST_CONJECTURE_SCHEMA,
            Self::Candidate => MCP_SUBMIT_CANDIDATE_SCHEMA,
        }
    }

    fn precondition(self) -> &'static str {
        match self {
            Self::Conjecture => MCP_CONJECTURE_VERDICT_PRESENTED,
            Self::Candidate => MCP_CANDIDATE_ADMISSIBLE,
        }
    }

    fn run(
        self,
        label: &str,
        library: Arc<dyn SegmentLibrary>,
        policy: &PreparedActionPolicy,
        medium: &StoreMedium,
    ) -> gmeow_errors::Result<StoredVerdict> {
        let formula = format!(
            "@prefix logic: <https://blackcatinformatics.ca/logic/> .
             @prefix ex: <https://example.org/concurrent/> .
             @prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .
             ex:formula a logic:Formula ; logic:relation rdf:type ;
               logic:argument [ logic:termIndex 0 ; logic:termIri ex:item ] ;
               logic:argument [ logic:termIndex 1 ; logic:termIri ex:{label} ] ."
        );
        let kb = format!("@prefix ex: <https://example.org/concurrent/> . ex:item a ex:{label} .");
        let standpoint = "https://example.org/concurrent/standpoint";
        match self {
            Self::Conjecture => {
                let out = run_conjecture_test_with_library(
                    &ConjectureRunInput {
                        formula_ttl: &formula,
                        kb_ttl: &kb,
                        standpoint,
                        math_conjecture: None,
                        dry_run: false,
                        max_steps: None,
                        max_answers: None,
                    },
                    medium,
                    policy,
                    || Ok(library),
                )?;
                Ok(StoredVerdict {
                    node: out.node_iri,
                    committed: out.committed,
                    receipt: out.receipt,
                })
            }
            Self::Candidate => {
                let out = run_submit_candidate_with_library(
                    &CandidateSubmitInput {
                        formula_ttl: &formula,
                        kb_ttl: &kb,
                        kb_format: "text/turtle",
                        standpoint,
                        math_conjecture: None,
                        for_slice: Some("https://example.org/concurrent/slice"),
                        for_packet: None,
                        dry_run: false,
                        max_steps: None,
                        max_answers: None,
                    },
                    medium,
                    policy,
                    || Ok(library),
                )?;
                Ok(StoredVerdict {
                    node: out.node_iri,
                    committed: out.committed,
                    receipt: out.receipt,
                })
            }
        }
    }
}

#[derive(Debug)]
struct StoredVerdict {
    node: String,
    committed: bool,
    receipt: TxReceipt,
}

fn synthetic_policy() -> PreparedActionPolicy {
    let mut source = String::new();
    for workflow in [Workflow::Conjecture, Workflow::Candidate] {
        let effect = format!("{}-effect", workflow.schema());
        source.push_str(&format!(
            "<{}> <{LOGIC_NAMESPACE}precondition> <{}> ;
              <{LOGIC_NAMESPACE}effect> <{effect}> .
             <{effect}> <{LOGIC_NAMESPACE}ins> <urn:synthetic:persisted> .\n",
            workflow.schema(),
            workflow.precondition(),
        ));
    }
    let dataset = purrdf::parse_dataset(source.as_bytes(), "text/turtle", None)
        .expect("the local synthetic action theory is valid RDF");
    PreparedActionPolicy::from_dataset(&dataset, SOURCE_SHA256)
        .expect("synthetic rows use the existing native policy admission pattern")
}

fn failure(message: &str) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(crate::error::Mcp::new(message.to_owned()))
}

enum Event {
    AtLock(usize),
    Finished(usize),
}

/// Notify even when evaluation or transaction execution returns an error or panics.
struct Completion {
    writer: usize,
    events: mpsc::Sender<Event>,
}

impl Drop for Completion {
    fn drop(&mut self) {
        // The controller may already have cancelled and stopped receiving.
        let _ = self.events.send(Event::Finished(self.writer));
    }
}

struct InterleavedLibrary {
    inner: Arc<InMemorySegmentLibrary>,
    writer: usize,
    refuse_replace: bool,
    events: mpsc::Sender<Event>,
    permit: Mutex<mpsc::Receiver<()>>,
    successful_image: Arc<Mutex<Option<Vec<u8>>>>,
}

impl SegmentLibrary for InterleavedLibrary {
    fn read_bytes(&self) -> gmeow_errors::Result<Vec<u8>> {
        self.inner.read_bytes()
    }

    fn replace_bytes(&self, bytes: &[u8]) -> gmeow_errors::Result<()> {
        if self.refuse_replace {
            return Err(failure("synthetic library replacement refused"));
        }
        self.inner.replace_bytes(bytes)?;
        *self
            .successful_image
            .lock()
            .expect("image observation lock") = Some(bytes.to_vec());
        Ok(())
    }

    fn lock(&self) -> gmeow_errors::Result<Box<dyn LibraryLock + '_>> {
        self.events
            .send(Event::AtLock(self.writer))
            .map_err(|_| failure("lock rendezvous controller disconnected"))?;
        self.permit
            .lock()
            .map_err(|_| failure("writer permit lock was poisoned"))?
            .recv()
            .map_err(|_| failure("writer permit was cancelled"))?;
        self.inner.lock()
    }
}

/// Release every waiter on success, upstream refusal or panic before joining workers.
fn coordinate(
    events: mpsc::Receiver<Event>,
    permits: &[mpsc::Sender<()>; 2],
) -> gmeow_errors::Result<()> {
    let result = (|| {
        let mut arrived = BTreeSet::new();
        while arrived.len() != 2 {
            match events.recv().map_err(|_| failure("writers disconnected"))? {
                Event::AtLock(writer) => {
                    if !arrived.insert(writer) {
                        return Err(failure("one writer entered the rendezvous twice"));
                    }
                }
                Event::Finished(_) => {
                    return Err(failure("a writer finished before both reached persistence"));
                }
            }
        }
        permits[0]
            .send(())
            .map_err(|_| failure("first writer disconnected before its permit"))?;
        if !matches!(events.recv(), Ok(Event::Finished(0))) {
            return Err(failure("first writer did not finish after its permit"));
        }
        permits[1]
            .send(())
            .map_err(|_| failure("second writer disconnected before its permit"))?;
        Ok(())
    })();
    // Sending after a worker completed is harmless; closed receivers mean it has exited.
    for permit in permits {
        let _ = permit.send(());
    }
    result
}

fn complete_audits(dataset: &Arc<purrdf::RdfDataset>, workflow: Workflow, expected: usize) {
    let query = format!(
        "SELECT ?call ?turn ?start ?at WHERE {{
           ?call <{LOGIC_INSTANTIATES_SCHEMA}> <{}> .
           OPTIONAL {{
             ?call <{LOGIC_PROPER_PART_OF}> ?turn ;
               <{GMEOW_AT_TIME}> ?at ;
               <{GMEOW_EVENT_TEMPORAL_FRAME}> <{GMEOW_TEMPORAL_FRAME_UTC_GREGORIAN}> .
             ?turn <{LOGIC_TRANSITION_FROM_STATE}> ?start .
             ?start <{LOGIC_SITUATION_OBTAINS}> <{}> .
           }}
         }}",
        workflow.schema(),
        workflow.precondition(),
    );
    let audits = native_query::select(dataset, &query).expect("read the committed audit graph");
    assert_eq!(
        audits.rows.len(),
        expected,
        "every successful verdict has an audit"
    );
    assert!(
        audits
            .rows
            .iter()
            .all(|row| row.iter().all(Option::is_some))
    );
    let call = audits.col("call").expect("query projects call identity");
    let identities: BTreeSet<_> = audits
        .rows
        .iter()
        .map(|row| native_query::term_iri(row[call].as_ref()).expect("audit calls have IRIs"))
        .collect();
    assert_eq!(
        identities.len(),
        expected,
        "audit identities must stay distinct"
    );
}

/// Concurrent real workflows cannot reuse an earlier GTS head/term base; a refused
/// replacement preserves the successful prefix, including its complete audit.
#[test]
fn verdict_workflows_commit_against_the_live_tail_with_atomic_audits() {
    // gmeow-test-input: synthetic-only
    let policy = synthetic_policy();
    let medium = StoreMedium {
        dictionary: "test-mcp-continuations".to_owned(),
        bytes: purrdf::gts::fixture::shipped_dictionary(),
    };
    for workflow in [Workflow::Conjecture, Workflow::Candidate] {
        for refuse_second in [false, true] {
            let inner = Arc::new(InMemorySegmentLibrary::default());
            let seed = workflow
                .run("Seed", inner.clone(), &policy, &medium)
                .expect("seed through the real workflow before arming the rendezvous");
            assert!(seed.committed);
            let prefix = inner.read_bytes().expect("seeded populated library");
            let successful_image = Arc::new(Mutex::new(None));
            let (events_tx, events_rx) = mpsc::channel();
            let (permit0_tx, permit0_rx) = mpsc::channel();
            let (permit1_tx, permit1_rx) = mpsc::channel();
            let libraries = [permit0_rx, permit1_rx].map(|permit| {
                (
                    permit,
                    inner.clone(),
                    successful_image.clone(),
                    events_tx.clone(),
                )
            });
            let (joined, progress) = std::thread::scope(|scope| {
                let handles: Vec<_> = libraries
                    .into_iter()
                    .enumerate()
                    .map(|(writer, (permit, inner, image, events))| {
                        let library = Arc::new(InterleavedLibrary {
                            inner,
                            writer,
                            refuse_replace: refuse_second && writer == 1,
                            events: events.clone(),
                            permit: Mutex::new(permit),
                            successful_image: image,
                        });
                        let policy = &policy;
                        let medium = &medium;
                        scope.spawn(move || {
                            let _completion = Completion { writer, events };
                            workflow.run(
                                if writer == 0 { "First" } else { "Second" },
                                library,
                                policy,
                                medium,
                            )
                        })
                    })
                    .collect();
                drop(events_tx);
                let progress = coordinate(events_rx, &[permit0_tx, permit1_tx]);
                let joined: Vec<_> = handles.into_iter().map(|handle| handle.join()).collect();
                (joined, progress)
            });
            // All workers have joined before any assertion can unwind.
            progress.expect("both real workflows reached their lock-entry rendezvous");
            let mut results = joined
                .into_iter()
                .map(|result| result.expect("real workflow worker must not panic"));
            let first = results
                .next()
                .expect("first worker")
                .expect("first commit succeeds");
            assert!(first.committed);
            assert!(matches!(first.receipt, TxReceipt::CommittedSuccess { .. }));
            let second = results.next().expect("second worker");
            let mut expected = BTreeSet::from([seed.node, first.node]);
            assert_eq!(
                expected.len(),
                2,
                "seed and first verdict identities differ"
            );
            if refuse_second {
                assert!(
                    second
                        .expect_err("forced replacement refusal must propagate")
                        .to_string()
                        .contains("synthetic library replacement refused")
                );
            } else {
                let second = second.expect("second commit succeeds against the newer tail");
                assert!(second.committed);
                assert!(matches!(second.receipt, TxReceipt::CommittedSuccess { .. }));
                assert!(
                    expected.insert(second.node),
                    "successful verdict identities differ"
                );
            }
            let bytes = inner.read_bytes().expect("final library bytes");
            assert!(
                bytes.starts_with(&prefix),
                "the seed remains an exact prefix"
            );
            if refuse_second {
                assert_eq!(
                    Some(bytes.clone()),
                    *successful_image
                        .lock()
                        .expect("successful image observation"),
                    "refusing replacement leaves the preceding successful commit byte-exact"
                );
            }
            gmeow_gts_profile::validate_mandated_frames(&bytes).expect("mandatory store profile");
            let imported = purrdf::import_gts_events(&bytes)
                .expect("strict native admission authenticates chain and term references");
            let recovered: BTreeSet<_> = read_library(inner.as_ref())
                .expect("all committed lifecycle records read back")
                .into_keys()
                .collect();
            assert_eq!(recovered, expected);
            complete_audits(&imported.dataset, workflow, expected.len());
        }
    }
}
