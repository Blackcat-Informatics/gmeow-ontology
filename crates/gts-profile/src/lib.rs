// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The mandatory GMEOW GTS authorship profile.
//!
//! Every payload-bearing frame emitted by GMEOW production code uses one
//! transform, `zstd-rsyncable`, at compression level 12 — small blob frames,
//! large blob frames, the snapshot frame, transformed consumer output, appended
//! `ai-package` segments, and signed release bundles alike. No size threshold
//! may fall back to plain `zstd`, `gzip`, or `identity`.
//!
//! This crate is the SINGLE production gateway to purrdf's GTS-authorship
//! surface. Three doors exist and no fourth:
//!
//! * [`emit_gmeow_gts`] — snapshot bundles composed from a
//!   [`SnapshotBuilder`](purrdf::gts_compose::SnapshotBuilder) (the shipped
//!   `gmeow.gts`, release bundles, on-demand consumer bundles);
//! * [`dataset_to_gmeow_gts`] — a frozen carrier
//!   [`RdfDataset`](purrdf::RdfDataset) serialized straight to GTS bytes (the
//!   `gmeow convert --to gts` exit);
//! * [`GmeowGtsWriter`] — incremental, append-only segment authorship (the MCP
//!   agent-memory and conjecture-library segments).
//!
//! [`validate_mandated_frames`] is the wire-level audit that proves any of them.
//!
//! It is deliberately a LEAF crate: `gmeow-pipeline` depends on `gmeow-math` and
//! `gmeow-music`, so hosting the profile inside `gmeow-pipeline` would make it
//! unreachable from exactly the producers that need it. The `Transform`
//! diagnostic kind lives here for the same reason — leaving it in
//! `gmeow-pipeline` would merely relocate the cycle.

use ciborium::value::Value;
use gmeow_errors::{FindingCategory, Grade, Severity, Standpoint, define_diag_kind};
use purrdf::gts::model::{Quad, Term};
use purrdf::gts::wire::{SELF_DESCRIBE_TAG, iter_items, map_get, unwrap_header};
use purrdf::gts::writer::{FrameOptions, Writer, term_to_wire};
use purrdf::gts_compose::{BlobRow, SnapshotBuilder};

define_diag_kind! {
    /// A hard defect raised inside the native MAXIMAL(G) transform (skolemization,
    /// saturation, projection, GTS emission): a malformed cell, an unparsable
    /// input graph, or a serialization failure. The RDF value is invalid or the
    /// codec refused — a HARD FAIL, never papered over.
    pub struct Transform { message: String }
    code = "pipeline.transform";
    grade = Grade::new(Severity::Error, FindingCategory::ModelingDisciplineViolation, Standpoint::Binding);
    message = "transform error: {}", message;
}

/// Required transform on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_FRAME_TRANSFORM: &str = "zstd-rsyncable";

/// Required zstd compression level on every payload-bearing GMEOW GTS frame.
pub const GMEOW_GTS_ZSTD_LEVEL: i32 = 12;

// `emit_gts` applies its public dist-profile level to every blob and snapshot
// frame.  Turn an upstream default drift into a compile failure rather than a
// silently different committed bundle.
const _: () = assert!(purrdf::gts_compose::DIST_ZSTD_LEVEL == GMEOW_GTS_ZSTD_LEVEL);

fn profile_error(message: impl Into<String>) -> gmeow_errors::Diag {
    gmeow_errors::Diag::of_kind(Transform {
        message: message.into(),
    })
}

fn map<'a>(value: &'a Value, context: &str) -> gmeow_errors::Result<&'a [(Value, Value)]> {
    match value {
        Value::Map(entries) => Ok(entries),
        _ => Err(profile_error(format!("{context} is not a CBOR map"))),
    }
}

fn integer(value: &Value, context: &str) -> gmeow_errors::Result<i128> {
    match value {
        Value::Integer(value) => Ok(i128::from(*value)),
        _ => Err(profile_error(format!("{context} is not a CBOR integer"))),
    }
}

/// A quad row's term-table index as the CBOR signed integer the GTS wire format
/// stores. Hard-fails rather than truncating: a silently wrapped index would
/// point at the wrong term, so an out-of-range index is a codec fault, not a
/// value to coerce.
fn term_index(index: usize) -> gmeow_errors::Result<i64> {
    i64::try_from(index).map_err(|_| {
        profile_error(format!(
            "GTS quad row term index {index} exceeds the signed 64-bit wire range"
        ))
    })
}

/// True when `item` is a GTS segment header rather than a frame.
///
/// A header is either the self-describe-tagged map the writer mints by default
/// (`Tag(55799, Map)`) or, when the magic tag is suppressed, a bare map carrying
/// the `"gts": "GTS1"` magic. A frame is always an untagged map keyed by
/// `t`/`d`/`x`/`prev`/`id` and never carries `"gts"`, so the two cannot collide.
fn is_segment_header(item: &Value) -> bool {
    match item {
        Value::Tag(tag, _) => *tag == SELF_DESCRIBE_TAG,
        Value::Map(entries) => {
            matches!(map_get(entries, "gts"), Some(Value::Text(magic)) if magic == "GTS1")
        }
        _ => false,
    }
}

/// The single codec id a segment header binds to the mandated transform.
fn required_codec_id(header: &[(Value, Value)], offset: usize) -> gmeow_errors::Result<i128> {
    let catalog = map(
        map_get(header, "cat").ok_or_else(|| {
            profile_error(format!(
                "GTS header at byte offset {offset} has no codec catalog"
            ))
        })?,
        "GTS codec catalog",
    )?;

    let mut ids = Vec::new();
    for (id, descriptor) in catalog {
        let descriptor = map(descriptor, "GTS codec descriptor")?;
        if matches!(
            map_get(descriptor, "name"),
            Some(Value::Text(name)) if name == GMEOW_GTS_FRAME_TRANSFORM
        ) {
            ids.push(integer(id, "GTS codec id")?);
        }
    }
    if ids.len() != 1 {
        return Err(profile_error(format!(
            "GTS codec catalog at byte offset {offset} must contain exactly one {} entry; found {}",
            GMEOW_GTS_FRAME_TRANSFORM,
            ids.len()
        )));
    }
    Ok(ids[0])
}

/// Validate the mandatory transform profile on materialized GMEOW GTS bytes.
///
/// Works on a single bundle (the shipped `gmeow.gts`, a feedback/music/math
/// bundle, a `convert --to gts` output) and equally on an append-only MULTI-
/// SEGMENT file (the MCP agent-memory and conjecture-library `.gts` files): each
/// appended segment mints its own header, and every one of them must bind the
/// mandated transform for the frames that follow it.
///
/// This is intentionally a cheap wire-level audit: it does not fold or parse the
/// RDF payload. Deep semantic validation remains the responsibility of the
/// validation gate. The compile-time assertion above separately pins the only
/// production emitter's zstd level to 12.
///
/// # Errors
/// A torn CBOR sequence, bytes that do not start with a header, a
/// missing/duplicated `zstd-rsyncable` codec-catalog entry in any segment header,
/// a payload-bearing frame with no (or a non-conforming) transform chain, a
/// payload-free frame that nonetheless carries one, or bytes with no payload
/// frames at all.
pub fn validate_mandated_frames(bytes: &[u8]) -> gmeow_errors::Result<()> {
    let (items, torn) = iter_items(bytes);
    if let Some(offset) = torn {
        return Err(profile_error(format!(
            "GTS CBOR sequence is torn at byte offset {offset}"
        )));
    }
    if items.is_empty() {
        return Err(profile_error("GTS bytes carry no header"));
    }
    if !is_segment_header(&items[0].1) {
        return Err(profile_error("GTS bytes do not begin with a header"));
    }

    let mut codec_id: Option<i128> = None;
    let mut payload_frames = 0usize;
    for (offset, item) in &items {
        if is_segment_header(item) {
            let header = unwrap_header(item).map_err(|message| {
                profile_error(format!("invalid header at byte offset {offset}: {message}"))
            })?;
            codec_id = Some(required_codec_id(header, *offset)?);
            continue;
        }

        let required = codec_id.ok_or_else(|| {
            profile_error(format!(
                "GTS frame at byte offset {offset} precedes any segment header"
            ))
        })?;
        let frame = map(item, &format!("GTS frame at byte offset {offset}"))?;
        if map_get(frame, "d").is_none() {
            // A signed release may carry a metadata-only transport-key frame.
            // It has no payload bytes to transform and is not a codec exception.
            if map_get(frame, "x").is_some() {
                return Err(profile_error(format!(
                    "payload-free GTS frame at byte offset {offset} carries a transform chain"
                )));
            }
            continue;
        }

        payload_frames += 1;
        let transforms = match map_get(frame, "x") {
            Some(Value::Array(values)) => values,
            _ => {
                return Err(profile_error(format!(
                    "payload-bearing GTS frame at byte offset {offset} has no transform chain"
                )));
            }
        };
        if transforms.len() != 1 {
            return Err(profile_error(format!(
                "payload-bearing GTS frame at byte offset {offset} must carry exactly one transform; found {}",
                transforms.len()
            )));
        }
        let transform = integer(&transforms[0], "GTS frame transform id")?;
        if transform != required {
            return Err(profile_error(format!(
                "payload-bearing GTS frame at byte offset {offset} uses codec id {transform}, not {} ({required})",
                GMEOW_GTS_FRAME_TRANSFORM
            )));
        }
    }

    if payload_frames == 0 {
        return Err(profile_error("GTS bytes carry no payload-bearing frames"));
    }
    Ok(())
}

/// Emit a GMEOW snapshot using the one permitted production frame profile.
///
/// The single production call to [`purrdf::gts_compose::emit_gts`] in the whole
/// workspace; `gmeow_validate::repo_static`'s Seal A censuses that and hard-fails
/// a second one.
///
/// # Errors
/// A composition or codec failure inside `emit_gts` (including its
/// all-three-or-none signing precondition).
#[allow(clippy::too_many_arguments)]
pub fn emit_gmeow_gts(
    builder: &SnapshotBuilder,
    archive_blobs: Vec<BlobRow>,
    report_blobs: Vec<BlobRow>,
    signer_secret: Option<[u8; 32]>,
    signer_kid: Option<String>,
    public_key_armor: Option<String>,
) -> gmeow_errors::Result<Vec<u8>> {
    purrdf::gts_compose::emit_gts(
        builder,
        "dist",
        Some(vec![GMEOW_GTS_FRAME_TRANSFORM.to_string()]),
        archive_blobs,
        report_blobs,
        signer_secret,
        signer_kid,
        public_key_armor,
        purrdf::gts_compose::DEFAULT_RSYNCABLE_THRESHOLD,
    )
    .map_err(|message| gmeow_errors::Diag::of_kind(Transform { message }))
}

/// Serialize a frozen carrier [`RdfDataset`](purrdf::RdfDataset) to GMEOW GTS
/// bytes under the mandated frame profile.
///
/// This is the transformed-consumer-output door (`gmeow convert --to gts`). It
/// deliberately does NOT route through [`purrdf::gts_write::to_gts`]: that path
/// authors its `terms`/`quads`/`reifies`/`annot`/`blob` frames through
/// `Writer::deterministic`, which passes no transform chain at all, so its bytes
/// ship identity-framed. Composing the same dataset through a
/// [`SnapshotBuilder`] and [`emit_gmeow_gts`] yields the identical snapshot
/// shape the shipped `gmeow.gts` carries — one canonical GMEOW authorship form,
/// not two.
///
/// # Errors
/// A carrier term that is not directly representable in the snapshot frame (a
/// quoted-triple term outside the reifier/annotation tables), or a codec failure.
pub fn dataset_to_gmeow_gts(dataset: &purrdf::RdfDataset) -> gmeow_errors::Result<Vec<u8>> {
    let mut builder = SnapshotBuilder::new();
    builder
        .add_dataset(dataset)
        .map_err(|message| gmeow_errors::Diag::of_kind(Transform { message }))?;
    emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
}

/// An append-only GTS segment writer that stamps the mandated transform profile
/// on every frame it authors.
///
/// purrdf's `Writer::add_terms` / `add_quads` convenience methods hard-code
/// `transform: None`, so a bare `Writer` emits payload frames with no transform
/// chain at all — invisible to a seal that only watches `emit_gts`, and rejected
/// by [`validate_mandated_frames`]. This wrapper authors the same two frame types
/// through `add_frame_with_options`, carrying
/// [`GMEOW_GTS_FRAME_TRANSFORM`] at [`GMEOW_GTS_ZSTD_LEVEL`].
///
/// It carries no pack dictionary: the pinned purrdf revision primes a dictionary
/// only for plain `zstd` frames (`zstd-rsyncable`'s independent blocks are out of
/// scope for a single-frame dictionary), so a dictionary here would be a silent
/// no-op or a codec rejection.
pub struct GmeowGtsWriter {
    inner: Writer,
}

impl GmeowGtsWriter {
    /// Mint a segment writer for `profile` and emit its header (the chain genesis).
    #[must_use]
    pub fn new(profile: &str) -> Self {
        Self {
            inner: Writer::new(profile),
        }
    }

    /// Append a `terms` frame under the mandated transform profile, returning the
    /// frame id.
    ///
    /// # Errors
    /// A codec failure encoding the frame payload.
    pub fn add_terms(&mut self, terms: &[Term]) -> gmeow_errors::Result<Vec<u8>> {
        let payload = Value::Array(terms.iter().map(term_to_wire).collect());
        self.add_mandated_frame("terms", payload)
    }

    /// Append a `quads` frame under the mandated transform profile, returning the
    /// frame id. The graph slot is dropped when `None`, exactly as purrdf's own
    /// `add_quads` row encoding does.
    ///
    /// # Errors
    /// A term index outside the CBOR signed-64-bit wire range, or a codec failure
    /// encoding the frame payload.
    pub fn add_quads(&mut self, quads: &[Quad]) -> gmeow_errors::Result<Vec<u8>> {
        let mut rows: Vec<Value> = Vec::with_capacity(quads.len());
        for &(s, p, o, g) in quads {
            let mut row = Vec::with_capacity(3 + usize::from(g.is_some()));
            for slot in [Some(s), Some(p), Some(o), g].into_iter().flatten() {
                row.push(Value::Integer(term_index(slot)?.into()));
            }
            rows.push(Value::Array(row));
        }
        self.add_mandated_frame("quads", Value::Array(rows))
    }

    fn add_mandated_frame(
        &mut self,
        frame_type: &str,
        payload: Value,
    ) -> gmeow_errors::Result<Vec<u8>> {
        let options = FrameOptions {
            payload: Some(payload),
            transform: vec![GMEOW_GTS_FRAME_TRANSFORM.to_string()],
            zstd_level: Some(GMEOW_GTS_ZSTD_LEVEL),
            ..FrameOptions::default()
        };
        self.inner
            .add_frame_with_options(frame_type, options)
            .map_err(|err| {
                gmeow_errors::Diag::of_kind(Transform {
                    message: format!("{frame_type} frame: {err}"),
                })
            })
    }

    /// Consume the writer and return the authored segment bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.inner.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_builder() -> SnapshotBuilder {
        let dataset = purrdf::parse_dataset(
            b"<https://e/s> <https://e/p> <https://e/o> .\n",
            purrdf::NativeRdfFormat::NTriples.media_type(),
            None,
        )
        .expect("parse fixture");
        let mut builder = SnapshotBuilder::new();
        builder.add_dataset(&dataset).expect("add fixture");
        builder
    }

    #[test]
    fn production_profile_pins_transform_and_level() {
        assert_eq!(GMEOW_GTS_FRAME_TRANSFORM, "zstd-rsyncable");
        assert_eq!(GMEOW_GTS_ZSTD_LEVEL, 12);
        assert_eq!(purrdf::gts_compose::DIST_ZSTD_LEVEL, 12);

        let builder = fixture_builder();
        let bytes = emit_gmeow_gts(
            &builder,
            vec![BlobRow {
                data: b"small payload must not fall back to plain zstd".to_vec(),
                media_type: "text/plain".to_string(),
                rep: "profile-test".to_string(),
            }],
            Vec::new(),
            None,
            None,
            None,
        )
        .expect("emit fixture");
        validate_mandated_frames(&bytes).expect("fixture uses mandated frame profile");
    }

    #[test]
    fn transform_kind_keeps_its_registered_code() {
        assert_eq!(Transform::CODE, "pipeline.transform");
    }

    #[test]
    fn profile_validator_rejects_a_payload_without_a_transform_chain() {
        let builder = fixture_builder();
        let bytes = emit_gmeow_gts(&builder, Vec::new(), Vec::new(), None, None, None)
            .expect("emit fixture");
        let (mut items, torn) = iter_items(&bytes);
        assert!(torn.is_none());
        let payload = items
            .iter_mut()
            .skip(1)
            .find_map(|(_, item)| match item {
                Value::Map(entries) if map_get(entries, "d").is_some() => Some(entries),
                _ => None,
            })
            .expect("fixture has a payload frame");
        payload.retain(|(key, _)| !matches!(key, Value::Text(value) if value == "x"));

        let mut malformed = Vec::new();
        for (_, item) in items {
            ciborium::ser::into_writer(&item, &mut malformed).expect("serialize fixture item");
        }
        let error = validate_mandated_frames(&malformed).expect_err("missing transform must fail");
        assert!(
            error.to_string().contains("has no transform chain"),
            "{error}"
        );
    }

    #[test]
    fn dataset_exit_carries_the_mandated_profile_and_reads_back() {
        let dataset = purrdf::parse_dataset(
            concat!(
                "<https://e/s> <https://e/p> <https://e/o> .\n",
                "<https://e/r> <http://www.w3.org/1999/02/22-rdf-syntax-ns#reifies> ",
                "<<( <https://e/s> <https://e/p> <https://e/o> )>> .\n",
            )
            .as_bytes(),
            purrdf::NativeRdfFormat::NTriples.media_type(),
            None,
        )
        .expect("parse fixture");
        let bytes = dataset_to_gmeow_gts(&dataset).expect("serialize the carrier exit");
        validate_mandated_frames(&bytes).expect("carrier exit uses the mandated frame profile");
        let graph = purrdf::gts::reader::read(&bytes, false, None);
        assert!(!graph.quads.is_empty(), "the exit bytes read back as quads");
    }

    /// A bare `purrdf` `Writer` — the shape every non-`emit_gts` authorship path
    /// used to take — emits payload frames with NO transform chain. Pin the
    /// counter-example so the wrapper below is demonstrably load-bearing.
    #[test]
    fn a_bare_purrdf_writer_fails_the_mandated_profile() {
        let mut writer = Writer::new("ai-package");
        writer.add_terms(&[iri_term("https://e/s")]);
        let bytes = writer.into_bytes();
        let error =
            validate_mandated_frames(&bytes).expect_err("a bare writer must fail the profile");
        assert!(
            error.to_string().contains("has no transform chain"),
            "{error}"
        );
    }

    /// An append-only file concatenates whole segments, each with its own header.
    /// The audit must walk every segment (not stop at the first) and must not
    /// mistake a later header for a malformed frame.
    #[test]
    fn multi_segment_append_is_audited_segment_by_segment() {
        let mut appended = mandated_segment("https://e/a");
        appended.extend_from_slice(&mandated_segment("https://e/b"));
        validate_mandated_frames(&appended).expect("every appended segment is audited");

        // A second segment authored WITHOUT the profile is caught, proving the walk
        // does not stop after the first header.
        let mut mixed = mandated_segment("https://e/a");
        let mut bare = Writer::new("ai-package");
        bare.add_terms(&[iri_term("https://e/b")]);
        mixed.extend_from_slice(&bare.into_bytes());
        let error =
            validate_mandated_frames(&mixed).expect_err("an unprofiled appended segment must fail");
        assert!(
            error.to_string().contains("has no transform chain"),
            "{error}"
        );
    }

    fn iri_term(iri: &str) -> Term {
        Term {
            kind: purrdf::gts::model::TermKind::Iri,
            value: Some(iri.to_string()),
            datatype: None,
            lang: None,
            direction: None,
            reifier: None,
        }
    }

    fn mandated_segment(iri: &str) -> Vec<u8> {
        let mut writer = GmeowGtsWriter::new("ai-package");
        writer.add_terms(&[iri_term(iri)]).expect("terms frame");
        writer.into_bytes()
    }

    #[test]
    fn segment_writer_stamps_the_mandated_profile_on_every_frame() {
        let terms = vec![
            iri_term("https://e/s"),
            iri_term("https://e/p"),
            iri_term("https://e/o"),
        ];
        let mut writer = GmeowGtsWriter::new("ai-package");
        writer.add_terms(&terms).expect("terms frame");
        writer.add_quads(&[(0, 1, 2, None)]).expect("quads frame");
        let bytes = writer.into_bytes();
        validate_mandated_frames(&bytes).expect("segment writer uses the mandated frame profile");

        // The segment still reads back as the quad it encodes — the transform is
        // decoded transparently, not a write-only stamp.
        let graph = purrdf::gts::reader::read(&bytes, false, None);
        assert_eq!(graph.quads.len(), 1, "one quad round-trips");
    }
}
