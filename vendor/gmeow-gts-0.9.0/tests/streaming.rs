// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use ciborium::value::Value;
use gmeow_gts::model::{
    Diagnostic, OpaqueNode, Quad, Signature, StreamableInfo, Suppression, Term, Triple3,
};
use gmeow_gts::reader::{read, read_file_segments, read_to_sink, StreamingSink};

#[derive(Default)]
struct RecordingSink {
    terms: Vec<(usize, usize, Term)>,
    quads: Vec<(usize, Quad)>,
    reifiers: Vec<(usize, usize, Triple3)>,
    annotations: Vec<(usize, Triple3)>,
    suppressions: Vec<(usize, Suppression)>,
    blobs: Vec<(usize, String, bool)>,
    opaque: Vec<(usize, String, String)>,
    signatures: Vec<(usize, Vec<u8>, String)>,
    diagnostics: Vec<Diagnostic>,
    segment_heads: Vec<(usize, Vec<u8>)>,
    streamable: Vec<(usize, StreamableInfo)>,
}

impl StreamingSink for RecordingSink {
    fn term(&mut self, segment_index: usize, term_id: usize, term: &Term) {
        self.terms.push((segment_index, term_id, term.clone()));
    }

    fn quad(&mut self, segment_index: usize, quad: Quad) {
        self.quads.push((segment_index, quad));
    }

    fn reifier(&mut self, segment_index: usize, reifier: usize, triple: Triple3) {
        self.reifiers.push((segment_index, reifier, triple));
    }

    fn annotation(&mut self, segment_index: usize, annotation: Triple3) {
        self.annotations.push((segment_index, annotation));
    }

    fn suppression(&mut self, segment_index: usize, suppression: &Suppression) {
        self.suppressions.push((segment_index, suppression.clone()));
    }

    fn blob(&mut self, segment_index: usize, digest: &str, meta: Option<&Value>) {
        self.blobs
            .push((segment_index, digest.to_string(), meta.is_some()));
    }

    fn opaque(&mut self, segment_index: usize, opaque: &OpaqueNode) {
        self.opaque.push((
            segment_index,
            opaque.frame_type.clone(),
            opaque.reason.clone(),
        ));
    }

    fn signature(&mut self, segment_index: usize, signature: &Signature) {
        self.signatures.push((
            segment_index,
            signature.frame_id.clone(),
            signature.status.clone(),
        ));
    }

    fn diagnostic(&mut self, diagnostic: &Diagnostic) {
        self.diagnostics.push(diagnostic.clone());
    }

    fn segment_head(&mut self, segment_index: usize, head: &[u8]) {
        self.segment_heads.push((segment_index, head.to_vec()));
    }

    fn streamable_layout(&mut self, segment_index: usize, info: &StreamableInfo) {
        self.streamable.push((segment_index, info.clone()));
    }
}

fn diagnostics_shape(items: &[Diagnostic]) -> Vec<(String, String, Option<usize>)> {
    items
        .iter()
        .map(|d| (d.code.clone(), d.detail.clone(), d.frame_index))
        .collect()
}

fn streamable_shape(items: &[StreamableInfo]) -> Vec<(bool, usize, usize, Option<Vec<u8>>)> {
    items
        .iter()
        .map(|info| (info.claimed, info.covered, info.tail, info.head.clone()))
        .collect()
}

fn assert_final_state_matches(name: &str, data: &[u8], allow_segments: bool) -> RecordingSink {
    let full = read(data, allow_segments, None);
    let mut sink = RecordingSink::default();
    let streamed = read_to_sink(data, allow_segments, None, &mut sink);

    assert_eq!(
        diagnostics_shape(&streamed.diagnostics),
        diagnostics_shape(&full.diagnostics),
        "{name}: streaming diagnostics differ from full reader"
    );
    assert_eq!(
        diagnostics_shape(&sink.diagnostics),
        diagnostics_shape(&streamed.diagnostics),
        "{name}: diagnostic events differ from final streaming state"
    );
    assert_eq!(
        streamed.segment_heads, full.segment_heads,
        "{name}: segment heads differ from full reader"
    );
    assert_eq!(
        sink.segment_heads.clone(),
        streamed
            .segment_heads
            .iter()
            .cloned()
            .enumerate()
            .collect::<Vec<_>>(),
        "{name}: segment-head events differ from final streaming state"
    );
    assert_eq!(
        streamable_shape(&streamed.segment_streamable),
        streamable_shape(&full.segment_streamable),
        "{name}: streamable layout differs from full reader"
    );
    assert_eq!(
        sink.streamable
            .iter()
            .map(|(segment_index, info)| (
                *segment_index,
                info.claimed,
                info.covered,
                info.tail,
                info.head.clone(),
            ))
            .collect::<Vec<_>>(),
        streamed
            .segment_streamable
            .iter()
            .enumerate()
            .map(|(segment_index, info)| {
                (
                    segment_index,
                    info.claimed,
                    info.covered,
                    info.tail,
                    info.head.clone(),
                )
            })
            .collect::<Vec<_>>(),
        "{name}: streamable events differ from final streaming state"
    );

    sink
}

#[test]
fn streaming_sink_final_state_matches_full_reader_for_corpus() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    for entry in fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("gts") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = fs::read(&path).expect("vector bytes");
        assert_final_state_matches(&name, &data, true);
    }
}

#[test]
fn streaming_sink_events_match_segment_folds_for_corpus() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    for entry in fs::read_dir(&dir).expect("corpus dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("gts") {
            continue;
        }
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let data = fs::read(&path).expect("vector bytes");
        let sink = assert_final_state_matches(&name, &data, true);
        let segments = read_file_segments(&data);

        let expected_terms: usize = segments.segments.iter().map(|g| g.terms.len()).sum();
        let expected_quads: usize = segments.segments.iter().map(|g| g.quads.len()).sum();
        let expected_reifiers: usize = segments.segments.iter().map(|g| g.reifiers.len()).sum();
        let expected_annotations: usize =
            segments.segments.iter().map(|g| g.annotations.len()).sum();
        let expected_suppressions: usize =
            segments.segments.iter().map(|g| g.suppressions.len()).sum();
        let expected_opaque: usize = segments.segments.iter().map(|g| g.opaque.len()).sum();
        let expected_signatures: usize = segments.segments.iter().map(|g| g.signatures.len()).sum();
        let expected_blobs: usize = segments.segments.iter().map(|g| g.blobs.len()).sum();
        let expected_blob_digests: BTreeSet<String> = segments
            .segments
            .iter()
            .flat_map(|g| g.blobs.iter().map(|(digest, _)| digest.clone()))
            .collect();
        let event_blob_digests: BTreeSet<String> = sink
            .blobs
            .iter()
            .map(|(_, digest, _)| digest.clone())
            .collect();

        assert_eq!(sink.terms.len(), expected_terms, "{name}: term events");
        assert_eq!(sink.quads.len(), expected_quads, "{name}: quad events");
        assert_eq!(
            sink.reifiers.len(),
            expected_reifiers,
            "{name}: reifier events"
        );
        assert_eq!(
            sink.annotations.len(),
            expected_annotations,
            "{name}: annotation events"
        );
        assert_eq!(
            sink.suppressions.len(),
            expected_suppressions,
            "{name}: suppression events"
        );
        assert_eq!(sink.opaque.len(), expected_opaque, "{name}: opaque events");
        assert_eq!(
            sink.signatures.len(),
            expected_signatures,
            "{name}: signature events"
        );
        assert_eq!(sink.blobs.len(), expected_blobs, "{name}: blob events");
        assert_eq!(
            event_blob_digests, expected_blob_digests,
            "{name}: blob digest events"
        );
    }
}

#[test]
fn streaming_sink_pre_segment_mode_matches_full_reader() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    let data = fs::read(dir.join("17-pre-segment-hard-fail.gts")).expect("vector bytes");
    assert_final_state_matches("17-pre-segment-hard-fail.gts", &data, false);
}

#[test]
fn streaming_sink_expected_head_mismatch_matches_full_reader() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../vectors");
    let data = fs::read(dir.join("01-minimal.gts")).expect("vector bytes");
    let expected = [0u8; 32];
    let full = read(&data, true, Some(&expected));
    let mut sink = RecordingSink::default();
    let streamed = read_to_sink(&data, true, Some(&expected), &mut sink);

    assert_eq!(
        diagnostics_shape(&streamed.diagnostics),
        diagnostics_shape(&full.diagnostics)
    );
    assert_eq!(
        diagnostics_shape(&sink.diagnostics),
        diagnostics_shape(&full.diagnostics)
    );
}
