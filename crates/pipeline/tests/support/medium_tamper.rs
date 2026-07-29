// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: AGPL-3.0-only

//! The NEGATIVE CONTROLS for the medium axis: five ways to break a shipped GTS artifact,
//! each aimed at exactly one of the six named `gmeow:Medium*` failure classes.
//!
//! A gate that has only ever been run on a green artifact is a gate nobody has seen work.
//! Each function here takes a real bundle emitted by the real DAG and returns a bundle
//! that is broken in ONE specific, named way, so `gmeow medium verify` and
//! `gmeow-dev medium-gate` can be shown to refuse it under the right class rather than
//! merely to accept the healthy one.
//!
//! # Every fixture stays otherwise well-formed
//!
//! GTS frames carry a `prev`/`id` hash chain and a segment header carries its own
//! self-hash (§3.1, §5). A naive byte poke breaks all three at once, and the reader then
//! reports "frame self-hash mismatch" for an artifact whose real defect is something
//! else entirely — so the fixture would prove only that tampering is detectable, not that
//! the CLAUSE under test fires. [`restamp`] therefore re-derives the whole chain after
//! every edit: the fixture is a *valid* GTS file that is wrong about exactly one medium
//! claim.
//!
//! # Shared across crates on purpose
//!
//! `gmeow medium verify` (a consumer verb) and `gmeow-dev medium-gate` (a repo gate) must
//! refuse the SAME artifacts. Two copies of these fixtures would drift, and the drift
//! would show up as two green suites disagreeing about what "broken" means, so both test
//! crates `#[path]`-include this one file.

#![allow(dead_code)]

use ciborium::value::Value;
use purrdf::gts::wire::{content_id, header_id, iter_items, map_get, unwrap_header};

/// The mandated transform every GMEOW payload frame rides (Rule 6).
const TRANSFORM: &str = "zstd-rsyncable";

/// The mandated zstd compression level (Rule 6).
const LEVEL: i32 = 12;

/// The CBOR self-describe tag purrdf's writer wraps a segment header in.
const SELF_DESCRIBE_TAG: u64 = 55799;

/// Parse an artifact into its CBOR items, refusing a torn sequence.
#[must_use]
pub fn items(bytes: &[u8]) -> Vec<Value> {
    let (items, torn) = iter_items(bytes);
    assert!(torn.is_none(), "the artifact is a torn CBOR sequence");
    items.into_iter().map(|(_, item)| item).collect()
}

/// Re-serialize a CBOR sequence.
#[must_use]
pub fn serialize(items: &[Value]) -> Vec<u8> {
    let mut out = Vec::new();
    for item in items {
        ciborium::into_writer(item, &mut out).expect("a parsed CBOR item re-serializes");
    }
    out
}

/// Whether `item` is a segment header rather than a frame.
#[must_use]
pub fn is_header(item: &Value) -> bool {
    match item {
        Value::Tag(tag, _) => *tag == SELF_DESCRIBE_TAG,
        Value::Map(entries) => {
            matches!(map_get(entries, "gts"), Some(Value::Text(magic)) if magic == "GTS1")
        }
        _ => false,
    }
}

/// The mutable entry list of a header or frame item, unwrapping the self-describe tag.
pub fn entries_mut(item: &mut Value) -> &mut Vec<(Value, Value)> {
    match item {
        Value::Tag(_, inner) => match inner.as_mut() {
            Value::Map(entries) => entries,
            other => panic!("a tagged GTS item must wrap a map, got {other:?}"),
        },
        Value::Map(entries) => entries,
        other => panic!("a GTS item must be a map, got {other:?}"),
    }
}

/// The entry list of a header or frame item.
#[must_use]
pub fn entries(item: &Value) -> &Vec<(Value, Value)> {
    match item {
        Value::Tag(_, inner) => match inner.as_ref() {
            Value::Map(entries) => entries,
            other => panic!("a tagged GTS item must wrap a map, got {other:?}"),
        },
        Value::Map(entries) => entries,
        other => panic!("a GTS item must be a map, got {other:?}"),
    }
}

/// Set (or insert) one key of a CBOR map, preserving the order of the keys already
/// present — the hash the chain is re-derived over is order-sensitive.
pub fn set(map: &mut Vec<(Value, Value)>, key: &str, value: Value) {
    for (existing, slot) in map.iter_mut() {
        if matches!(existing, Value::Text(text) if text == key) {
            *slot = value;
            return;
        }
    }
    map.push((Value::Text(key.to_string()), value));
}

/// Re-derive the whole `prev`/`id` chain and every segment header's self-hash.
///
/// Run after ANY edit. Without it the reader reports a damaged frame — which is true but
/// useless: it would be the same diagnosis for every fixture here, and the point of each
/// fixture is that a DIFFERENT clause fires.
pub fn restamp(items: &mut [Value]) {
    let mut expected_prev: Vec<u8> = Vec::new();
    for item in items.iter_mut() {
        if is_header(item) {
            let map = entries_mut(item);
            // The genesis id excludes only `"id"` itself (§5), so it must be recomputed
            // from a map that no longer carries a stale one.
            map.retain(|(key, _)| !matches!(key, Value::Text(text) if text == "id"));
            let id = header_id(map);
            set(map, "id", Value::Bytes(id.clone()));
            expected_prev = id;
            continue;
        }
        let map = entries_mut(item);
        set(map, "prev", Value::Bytes(expected_prev.clone()));
        map.retain(|(key, _)| !matches!(key, Value::Text(text) if text == "id"));
        let id = content_id(map);
        set(map, "id", Value::Bytes(id.clone()));
        expected_prev = id;
    }
}

/// The index of the segment header governing item `index`.
fn header_index(items: &[Value], index: usize) -> usize {
    (0..index)
        .rev()
        .find(|candidate| is_header(&items[*candidate]))
        .expect("every payload frame follows a segment header")
}

/// Whether an item is a payload-bearing frame.
fn is_payload(item: &Value) -> bool {
    !is_header(item) && matches!(item, Value::Map(map) if map_get(map, "d").is_some())
}

/// The `pub.rep` / `pub.digest` of a frame, when it carries public metadata.
fn public_meta(item: &Value) -> (Option<String>, Option<String>) {
    let Value::Map(map) = item else {
        return (None, None);
    };
    match map_get(map, "pub") {
        Some(Value::Map(meta)) => (
            match map_get(meta, "rep") {
                Some(Value::Text(rep)) => Some(rep.clone()),
                _ => None,
            },
            match map_get(meta, "digest") {
                Some(Value::Text(digest)) => Some(digest.clone()),
                _ => None,
            },
        ),
        _ => (None, None),
    }
}

/// The single transform id a payload frame references.
fn codec_of(item: &Value) -> i128 {
    let Value::Map(map) = item else {
        panic!("a payload frame is a CBOR map")
    };
    match map_get(map, "x") {
        Some(Value::Array(chain)) if chain.len() == 1 => match &chain[0] {
            Value::Integer(id) => i128::from(*id),
            other => panic!("a transform id must be a CBOR integer, got {other:?}"),
        },
        other => panic!("a payload frame carries exactly one transform, got {other:?}"),
    }
}

/// The frame's `"d"` bytes.
fn payload_of(item: &Value) -> &Vec<u8> {
    let Value::Map(map) = item else {
        panic!("a payload frame is a CBOR map")
    };
    match map_get(map, "d") {
        Some(Value::Bytes(bytes)) => bytes,
        other => panic!("a payload frame carries byte-string \"d\", got {other:?}"),
    }
}

/// The index of the SMALLEST blob payload frame that states an in-band `pub.digest`.
///
/// Smallest on purpose: the fixtures that decode and re-encode a payload pay a level-12
/// zstd round trip, and the bundle's largest archives are tens of megabytes.
fn smallest_blob_frame(items: &[Value]) -> usize {
    items
        .iter()
        .enumerate()
        .filter(|(_, item)| is_payload(item) && public_meta(item).1.is_some())
        .min_by_key(|(_, item)| payload_of(item).len())
        .map(|(index, _)| index)
        .expect("the artifact carries at least one blob frame with an in-band digest")
}

/// A segment header's `"dct"` map: dictionary name → verbatim bytes.
fn pinned(header: &Value) -> Vec<(String, Vec<u8>)> {
    match map_get(entries(header), "dct") {
        Some(Value::Map(dicts)) => dicts
            .iter()
            .filter_map(|(name, bytes)| match (name, bytes) {
                (Value::Text(name), Value::Bytes(bytes)) => Some((name.clone(), bytes.clone())),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// The `(codec name, declared level, dictionary name)` of one catalog entry.
fn catalog_entry(header: &Value, codec: i128) -> (String, Option<i32>, Option<String>) {
    let Some(Value::Map(catalog)) = map_get(entries(header), "cat") else {
        panic!("a segment header carries a codec catalog")
    };
    for (id, descriptor) in catalog {
        let (Value::Integer(id), Value::Map(fields)) = (id, descriptor) else {
            continue;
        };
        if i128::from(*id) != codec {
            continue;
        }
        return (
            match map_get(fields, "name") {
                Some(Value::Text(name)) => name.clone(),
                _ => String::new(),
            },
            match map_get(fields, "level") {
                Some(Value::Integer(level)) => i32::try_from(i128::from(*level)).ok(),
                _ => None,
            },
            match map_get(fields, "dct") {
                Some(Value::Text(dict)) => Some(dict.clone()),
                _ => None,
            },
        );
    }
    panic!("codec id {codec} is not declared in the segment catalog");
}

/// Decode one payload frame through the chain its own segment header declares.
fn decode(items: &[Value], index: usize) -> Vec<u8> {
    let header = &items[header_index(items, index)];
    let codec = codec_of(&items[index]);
    let (name, level, dict_name) = catalog_entry(header, codec);
    let dict = dict_name.map(|wanted| {
        pinned(header)
            .into_iter()
            .find(|(name, _)| *name == wanted)
            .map(|(_, bytes)| bytes)
            .unwrap_or_else(|| panic!("the header pins no dictionary named {wanted:?}"))
    });
    purrdf::gts::codec::decode_chain(
        &[purrdf::gts::codec::Codec {
            name,
            cls: "compress".to_string(),
            dct: dict,
            level,
        }],
        payload_of(&items[index]),
    )
    .expect("a healthy payload frame decodes through its declared chain")
}

/// Re-encode a payload through the chain a frame's segment header declares.
fn encode(items: &[Value], index: usize, plaintext: &[u8]) -> Vec<u8> {
    let header = &items[header_index(items, index)];
    let codec = codec_of(&items[index]);
    let (name, level, dict_name) = catalog_entry(header, codec);
    let dict = dict_name.map(|wanted| {
        pinned(header)
            .into_iter()
            .find(|(name, _)| *name == wanted)
            .map(|(_, bytes)| bytes)
            .unwrap_or_else(|| panic!("the header pins no dictionary named {wanted:?}"))
    });
    purrdf::gts::codec::encode_chain_with_options(
        &[name],
        plaintext,
        purrdf::gts::codec::EncodeOptions {
            zstd_level: level.or(Some(LEVEL)),
            dict: dict.as_deref(),
        },
    )
    .expect("the mandated chain re-encodes a payload")
}

// ── The five negative controls ───────────────────────────────────────────────

/// FLIP ONE BYTE OF A PAYLOAD → `pipeline.medium.digest-mismatch`.
///
/// The flip is applied to the DECODED plaintext and the frame is then re-encoded through
/// its own declared chain, so the artifact stays perfectly decodable and every
/// declaration in it stays intact. What is now false is the frame's own in-band
/// `pub.digest`: it commits to bytes the artifact no longer carries.
///
/// This is the fixture that proves the verb DECODES rather than trusting the fold.
/// purrdf's reader stores a blob frame lazily and takes `pub.digest` on the frame's word,
/// so this artifact folds with zero diagnostics and zero opaque nodes — a verifier built
/// on the fold alone would pass it.
#[must_use]
pub fn flipped_payload_byte(bundle: &[u8]) -> Vec<u8> {
    let mut items = items(bundle);
    let index = smallest_blob_frame(&items);
    let mut plaintext = decode(&items, index);
    assert!(
        !plaintext.is_empty(),
        "the chosen frame decodes to no bytes, so there is nothing to perturb"
    );
    let last = plaintext.len() - 1;
    plaintext[last] ^= 0xFF;
    let reencoded = encode(&items, index, &plaintext);
    set(entries_mut(&mut items[index]), "d", Value::Bytes(reencoded));
    restamp(&mut items);
    serialize(&items)
}

/// CORRUPT A PAYLOAD SO IT CANNOT DECODE AT ALL → `pipeline.medium.opaque-frame`.
///
/// The zstd frame magic is overwritten, so the transform chain the catalog declares
/// cannot be reversed. A reader that reaches for this blob gets an opaque node with no
/// content and the fold silently omits it — which is exactly why the zero-opaque-node
/// clause exists, and why "the bundle parsed" is not evidence the bundle is whole.
#[must_use]
pub fn undecodable_payload(bundle: &[u8]) -> Vec<u8> {
    let mut items = items(bundle);
    let index = smallest_blob_frame(&items);
    let mut payload = payload_of(&items[index]).clone();
    assert!(
        payload.len() > 8,
        "the chosen frame's payload is too short to carry a zstd frame header"
    );
    for byte in payload.iter_mut().take(4) {
        *byte ^= 0xFF;
    }
    set(entries_mut(&mut items[index]), "d", Value::Bytes(payload));
    restamp(&mut items);
    serialize(&items)
}

/// A CATALOG ENTRY CITING A DICTIONARY THE HEADER DOES NOT PIN →
/// `pipeline.medium.unknown-dictionary`.
///
/// Every byte of the payload is intact and every other declaration is untouched; the
/// artifact is simply no longer self-contained. purrdf fails CLOSED here (§8.3) by
/// dropping the whole catalog entry, so every frame riding it degrades to an opaque node
/// — the payload is permanently undecodable even with its bytes intact, and there is no
/// dictionary-less retry that would recover it.
#[must_use]
pub fn unknown_dictionary_id(bundle: &[u8]) -> Vec<u8> {
    let mut items = items(bundle);
    let index = smallest_blob_frame(&items);
    let header = header_index(&items, index);
    let codec = codec_of(&items[index]);
    let map = entries_mut(&mut items[header]);
    let Some((_, Value::Map(catalog))) = map
        .iter_mut()
        .find(|(key, _)| matches!(key, Value::Text(text) if text == "cat"))
    else {
        panic!("a segment header carries a codec catalog");
    };
    let mut rewritten = false;
    for (id, descriptor) in catalog.iter_mut() {
        let Value::Integer(id) = id else { continue };
        if i128::from(*id) != codec {
            continue;
        }
        let Value::Map(fields) = descriptor else {
            continue;
        };
        set(
            fields,
            "dct",
            Value::Text("gmeow-not-a-shipped-dictionary-v1".to_string()),
        );
        rewritten = true;
    }
    assert!(rewritten, "the chosen frame's catalog entry was not found");
    restamp(&mut items);
    serialize(&items)
}

/// A FRAME TAGGED WITH AN UNREGISTERED REP → `pipeline.medium.unknown-schema`.
///
/// The rep→medium assignment is TOTAL over registered `gmeow:PayloadSchema` individuals,
/// so a rep nothing registers has no medium at all: it would decode as an unclassified
/// blob whose medium assignment is UNDEFINED, which is precisely the silent capability
/// degradation no-optionality forbids.
#[must_use]
pub fn unregistered_rep(bundle: &[u8]) -> Vec<u8> {
    let mut items = items(bundle);
    let index = smallest_blob_frame(&items);
    let map = entries_mut(&mut items[index]);
    let Some((_, Value::Map(meta))) = map
        .iter_mut()
        .find(|(key, _)| matches!(key, Value::Text(text) if text == "pub"))
    else {
        panic!("a blob frame carries public metadata");
    };
    set(
        meta,
        "rep",
        Value::Text("not-a-registered-payload-schema".to_string()),
    );
    restamp(&mut items);
    serialize(&items)
}

/// A FRAME QUIETLY RIDING AN UNPRIMED CATALOG ENTRY →
/// `pipeline.medium.undeclared-dictionary`.
///
/// The frame's rep still DECLARES a dictionary; the frame just no longer uses it. Nothing
/// about the wire is malformed — a catalog legitimately carries an unprimed
/// `zstd-rsyncable` entry beside its primed ones (§5) — which is the whole danger: the
/// artifact discards the density its own declaration promises while still shipping under
/// a reader contract that demands dictionary priming. If the emitted catalog carries no
/// unprimed entry, one is added, so the fixture is about the FRAME's selection rather
/// than about which entries happen to exist.
#[must_use]
pub fn undeclared_dictionary(bundle: &[u8]) -> Vec<u8> {
    let mut items = items(bundle);
    let index = smallest_blob_frame(&items);
    let header = header_index(&items, index);

    let unprimed = {
        let Some(Value::Map(catalog)) = map_get(entries(&items[header]), "cat") else {
            panic!("a segment header carries a codec catalog");
        };
        let existing = catalog.iter().find_map(|(id, descriptor)| {
            let (Value::Integer(id), Value::Map(fields)) = (id, descriptor) else {
                return None;
            };
            let mandated =
                matches!(map_get(fields, "name"), Some(Value::Text(name)) if name == TRANSFORM);
            (mandated && map_get(fields, "dct").is_none()).then(|| i128::from(*id))
        });
        match existing {
            Some(id) => id,
            None => {
                let next = catalog
                    .iter()
                    .filter_map(|(id, _)| match id {
                        Value::Integer(id) => Some(i128::from(*id)),
                        _ => None,
                    })
                    .max()
                    .unwrap_or(0)
                    + 1;
                let map = entries_mut(&mut items[header]);
                let Some((_, Value::Map(catalog))) = map
                    .iter_mut()
                    .find(|(key, _)| matches!(key, Value::Text(text) if text == "cat"))
                else {
                    panic!("a segment header carries a codec catalog");
                };
                catalog.push((
                    Value::Integer(
                        i64::try_from(next)
                            .expect("a catalog id fits in i64")
                            .into(),
                    ),
                    Value::Map(vec![
                        (
                            Value::Text("name".to_string()),
                            Value::Text(TRANSFORM.to_string()),
                        ),
                        (
                            Value::Text("cls".to_string()),
                            Value::Text("compress".to_string()),
                        ),
                        (
                            Value::Text("level".to_string()),
                            Value::Integer(LEVEL.into()),
                        ),
                    ]),
                ));
                next
            }
        }
    };

    let map = entries_mut(&mut items[index]);
    set(
        map,
        "x",
        Value::Array(vec![Value::Integer(
            i64::try_from(unprimed)
                .expect("a catalog id fits in i64")
                .into(),
        )]),
    );
    restamp(&mut items);
    serialize(&items)
}

/// Assert that a healthy artifact really is healthy before any fixture is derived from
/// it: a negative control derived from an already-broken subject proves nothing.
pub fn assert_wire_is_intact(bundle: &[u8]) {
    let graph = purrdf::gts::reader::read(bundle, true, None);
    assert!(
        graph.opaque.is_empty() && graph.diagnostics.is_empty(),
        "the fixture subject must fold cleanly: {} opaque node(s), {} diagnostic(s)",
        graph.opaque.len(),
        graph.diagnostics.len()
    );
    let _ = unwrap_header(&items(bundle)[0]).expect("the subject begins with a segment header");
}

/// A round trip through [`items`]/[`restamp`]/[`serialize`] with NO edit must leave a
/// still-conformant artifact.
///
/// The control on the controls: if re-stamping alone perturbed the artifact, every
/// fixture below would be testing the harness rather than the clause it names.
#[must_use]
pub fn identity_rewrite(bundle: &[u8]) -> Vec<u8> {
    let mut items = items(bundle);
    restamp(&mut items);
    serialize(&items)
}
