// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

//! A minimal GTS writer: build frames, maintain the id/prev chain, emit a CBOR
//! Sequence.
//!
//! This is the encoder counterpart to [`crate::reader`]. It currently supports
//! the frame types needed for the `files` profile (§13.2) and the conformance
//! vectors added with it. Deterministic CBOR and BLAKE3 self-hashes are handled
//! by [`crate::wire`].

use std::collections::HashMap;

use ciborium::value::Value;

use crate::codec::Codec;
use crate::model::{Quad, Term, Triple3};
use crate::wire::{canonical, content_id, digest_str, header_id, SELF_DESCRIBE_TAG};

fn iv(n: i64) -> Value {
    Value::Integer(ciborium::value::Integer::from(n))
}

/// Serialise a [`Term`] to its wire map (dropping absent fields).
pub fn term_to_wire(t: &Term) -> Value {
    let mut entries: Vec<(Value, Value)> = vec![("k".into(), iv(t.kind as i64))];
    if let Some(v) = &t.value {
        entries.push(("v".into(), v.clone().into()));
    }
    if let Some(dt) = t.datatype {
        entries.push(("dt".into(), iv(dt as i64)));
    }
    if let Some(l) = &t.lang {
        entries.push(("l".into(), l.clone().into()));
    }
    if let Some(rf) = t.reifier {
        entries.push(("rf".into(), iv(rf as i64)));
    }
    Value::Map(entries)
}

/// Accumulate a GTS log as a CBOR Sequence.
pub struct Writer {
    name_to_id: HashMap<String, i64>,
    prev: Vec<u8>,
    buf: Vec<u8>,
    // Per-frame byte offsets and types, in append order — the raw material
    // of an `index` footer (§6.2): offsets enable random access/parallel
    // verify, types the "ti" locator map.
    offsets: Vec<usize>,
    types: Vec<String>,
}

impl Writer {
    /// Create a writer and emit the Header (the chain genesis).
    pub fn new(profile: &str) -> Self {
        Self::with_layout(profile, None)
    }

    /// Create a writer with a header layout-state claim (§3.3;
    /// `"streamable"` is the only value this revision defines).
    pub fn with_layout(profile: &str, layout: Option<&str>) -> Self {
        let catalog: HashMap<i64, Codec> = [
            (
                0i64,
                Codec {
                    name: "identity".to_string(),
                    cls: "encode".to_string(),
                },
            ),
            (
                1,
                Codec {
                    name: "gzip".to_string(),
                    cls: "compress".to_string(),
                },
            ),
            (
                2,
                Codec {
                    name: "zstd".to_string(),
                    cls: "compress".to_string(),
                },
            ),
            (
                7,
                Codec {
                    name: "cose-encrypt0".to_string(),
                    cls: "encrypt".to_string(),
                },
            ),
        ]
        .into_iter()
        .collect();
        let name_to_id: HashMap<String, i64> = catalog
            .iter()
            .map(|(id, c)| (c.name.clone(), *id))
            .collect();

        let cat_entries: Vec<(Value, Value)> = catalog
            .iter()
            .map(|(id, c)| {
                let mut ce: Vec<(Value, Value)> = vec![
                    ("name".into(), c.name.clone().into()),
                    ("cls".into(), c.cls.clone().into()),
                ];
                ce.sort_by_key(|a| canonical(&a.0));
                (iv(*id), Value::Map(ce))
            })
            .collect();

        let mut header: Vec<(Value, Value)> = vec![
            ("gts".into(), "GTS1".into()),
            ("v".into(), iv(1)),
            ("prof".into(), profile.into()),
            ("cat".into(), Value::Map(cat_entries)),
        ];
        if let Some(layout) = layout {
            // The layout-state claim is part of the header content, so it is
            // covered by the genesis self-hash (§3.3, §5).
            header.push(("layout".into(), layout.into()));
        }
        header.sort_by_key(|a| canonical(&a.0));
        let id = header_id(&header);
        header.push(("id".into(), Value::Bytes(id.clone())));
        header.sort_by_key(|a| canonical(&a.0));

        let tagged = Value::Tag(SELF_DESCRIBE_TAG, Box::new(Value::Map(header)));
        let buf = canonical(&tagged);

        Self {
            name_to_id,
            prev: id,
            buf,
            offsets: Vec::new(),
            types: Vec::new(),
        }
    }

    /// The id the next appended frame must reference as `"prev"`.
    pub fn head(&self) -> &[u8] {
        &self.prev
    }

    fn chain_ids(&self, chain: &[String]) -> Vec<i64> {
        chain.iter().map(|name| self.name_to_id[name]).collect()
    }

    /// Append one frame and return its `"id"`.
    pub fn add_frame(
        &mut self,
        frame_type: &str,
        payload: Option<Value>,
        raw: Option<Vec<u8>>,
        transform: Option<&[String]>,
        pub_meta: Option<Value>,
    ) -> Vec<u8> {
        assert!(
            payload.is_none() || raw.is_none(),
            "payload and raw are mutually exclusive"
        );
        let mut frame: Vec<(Value, Value)> = vec![("t".into(), frame_type.into())];

        let data: Option<Value> = match (transform, &payload, &raw) {
            (Some(chain), _, _) if !chain.is_empty() => {
                assert!(
                    raw.is_some() || payload.is_some(),
                    "transform requires a raw or payload source"
                );
                let source = match (raw.as_ref(), payload.as_ref()) {
                    (Some(r), _) => r.clone(),
                    (None, Some(p)) => canonical(p),
                    (None, None) => panic!("transform requires a raw or payload source"),
                };
                // For the files profile we only need identity; compression is
                // intentionally not implemented in this minimal writer.
                assert!(
                    chain.iter().all(|n| n == "identity"),
                    "non-identity transforms require the Python producer"
                );
                let x_ids: Vec<Value> = self.chain_ids(chain).into_iter().map(iv).collect();
                frame.push(("x".into(), Value::Array(x_ids)));
                Some(Value::Bytes(source))
            }
            (None, _, Some(r)) => Some(Value::Bytes(r.clone())),
            (None, Some(p), None) => Some(p.clone()),
            _ => None,
        };
        if let Some(data) = data {
            frame.push(("d".into(), data));
        }

        if let Some(meta) = pub_meta {
            frame.push(("pub".into(), meta));
        }
        frame.push(("prev".into(), Value::Bytes(self.prev.clone())));

        frame.sort_by_key(|a| canonical(&a.0));
        let id = content_id(&frame);
        frame.push(("id".into(), Value::Bytes(id.clone())));
        frame.sort_by_key(|a| canonical(&a.0));

        self.offsets.push(self.buf.len());
        self.types.push(frame_type.to_string());
        self.buf.extend_from_slice(&canonical(&Value::Map(frame)));
        self.prev = id.clone();
        id
    }

    /// Append a `terms` frame.
    pub fn add_terms(&mut self, terms: &[Term]) -> Vec<u8> {
        let payload = Value::Array(terms.iter().map(term_to_wire).collect());
        self.add_frame("terms", Some(payload), None, None, None)
    }

    /// Append a `quads` frame (graph slot dropped when `None`).
    pub fn add_quads(&mut self, quads: &[Quad]) -> Vec<u8> {
        let rows: Vec<Value> = quads
            .iter()
            .map(|&(s, p, o, g)| {
                let mut row = vec![iv(s as i64), iv(p as i64), iv(o as i64)];
                if let Some(gv) = g {
                    row.push(iv(gv as i64));
                }
                Value::Array(row)
            })
            .collect();
        self.add_frame("quads", Some(Value::Array(rows)), None, None, None)
    }

    /// Append a `reifies` frame binding reifier-ids to triples.
    pub fn add_reifies(&mut self, bindings: &[(usize, Triple3)]) -> Vec<u8> {
        let mut map: Vec<(Value, Value)> = Vec::new();
        for (rid, (s, p, o)) in bindings {
            map.push((
                iv(*rid as i64),
                Value::Array(vec![iv(*s as i64), iv(*p as i64), iv(*o as i64)]),
            ));
        }
        self.add_frame("reifies", Some(Value::Map(map)), None, None, None)
    }

    /// Append an `annot` frame.
    pub fn add_annot(&mut self, rows: &[Triple3]) -> Vec<u8> {
        let rows: Vec<Value> = rows
            .iter()
            .map(|&(s, p, o)| Value::Array(vec![iv(s as i64), iv(p as i64), iv(o as i64)]))
            .collect();
        self.add_frame("annot", Some(Value::Array(rows)), None, None, None)
    }

    /// Append an inline `blob` frame; metadata goes in `pub` (§12).
    pub fn add_blob(&mut self, data: &[u8], mt: Option<&str>, rep: Option<&str>) -> Vec<u8> {
        let mut pub_entries: Vec<(Value, Value)> = Vec::new();
        if let Some(m) = mt {
            pub_entries.push(("mt".into(), m.into()));
        }
        if let Some(r) = rep {
            pub_entries.push(("rep".into(), r.into()));
        }
        let pub_meta = (!pub_entries.is_empty()).then_some(Value::Map(pub_entries));
        self.add_frame("blob", None, Some(data.to_vec()), None, pub_meta)
    }

    /// Append a `meta` frame.
    pub fn add_meta(&mut self, meta: Value) -> Vec<u8> {
        self.add_frame("meta", Some(meta), None, None, None)
    }

    /// Append a `suppress` frame.
    pub fn add_suppress(
        &mut self,
        targets: Vec<Value>,
        reason: Option<&str>,
        by: Option<usize>,
    ) -> Vec<u8> {
        let mut payload: Vec<(Value, Value)> = vec![("targets".into(), Value::Array(targets))];
        if let Some(r) = reason {
            payload.push(("reason".into(), r.into()));
        }
        if let Some(b) = by {
            payload.push(("by".into(), Value::from(b as u64)));
        }
        payload.sort_by_key(|a| canonical(&a.0));
        self.add_frame("suppress", Some(Value::Map(payload)), None, None, None)
    }

    /// Append an `index` footer covering every frame appended so far (§6.2).
    ///
    /// `count`/`head` delimit the covered region (the streamable boundary,
    /// §3.3); `off` carries each covered frame's byte offset from the start
    /// of this writer's output; `ti` locates frames by type (0-based frame
    /// positions). A later `add_index` covers the earlier one too — the last
    /// index wins (§6.2).
    pub fn add_index(&mut self) -> Vec<u8> {
        let mut payload: Vec<(Value, Value)> = vec![
            ("count".into(), iv(self.types.len() as i64)),
            ("head".into(), Value::Bytes(self.prev.clone())),
        ];
        if !self.offsets.is_empty() {
            // "off"/"ti" are [+ uint]-shaped — omit when empty
            let off: Vec<Value> = self.offsets.iter().map(|&o| iv(o as i64)).collect();
            let mut ti: Vec<(Value, Value)> = Vec::new();
            for (pos, ftype) in self.types.iter().enumerate() {
                match ti
                    .iter_mut()
                    .find(|(k, _)| matches!(k, Value::Text(t) if t == ftype))
                {
                    Some((_, Value::Array(positions))) => positions.push(iv(pos as i64)),
                    _ => ti.push((ftype.clone().into(), Value::Array(vec![iv(pos as i64)]))),
                }
            }
            payload.push(("off".into(), Value::Array(off)));
            payload.push(("ti".into(), Value::Map(ti)));
        }
        self.add_frame("index", Some(Value::Map(payload)), None, None, None)
    }

    /// Return the complete GTS file bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.buf.clone()
    }
}

/// Pack bytes into a `blake3:<hex>` digest string.
pub fn digest_string(data: &[u8]) -> String {
    digest_str(data)
}
