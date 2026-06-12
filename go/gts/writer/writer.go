// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package writer implements a minimal deterministic GTS encoder.
package writer

import (
	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/model"
	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/wire"
	"github.com/fxamacker/cbor/v2"
)

// termToWire serialises a Term to its wire map, dropping absent fields.
func termToWire(t *model.Term) map[interface{}]interface{} {
	entries := map[interface{}]interface{}{"k": int64(t.Kind)}
	if t.Value != "" || t.Kind == model.Literal {
		entries["v"] = t.Value
	}
	if t.Datatype != nil {
		entries["dt"] = int64(*t.Datatype)
	}
	if t.Lang != "" {
		entries["l"] = t.Lang
	}
	if t.Reifier != nil {
		entries["rf"] = int64(*t.Reifier)
	}
	return entries
}

// Writer accumulates a GTS log as a CBOR Sequence.
type Writer struct {
	nameToID map[string]int64
	prev     []byte
	buf      []byte
	// Per-frame byte offsets and types, in append order — the raw material
	// of an `index` footer (§6.2): offsets enable random access/parallel
	// verify, types the "ti" locator map.
	offsets []int
	types   []string
}

// New creates a writer and emits the Header (the chain genesis).
func New(profile string) *Writer {
	return NewWithLayout(profile, "")
}

// NewWithLayout creates a writer whose header carries a layout-state claim
// (§3.3; "streamable" is the only value this revision defines). The layout
// key participates in the header self-hash. An empty layout writes no claim.
func NewWithLayout(profile, layout string) *Writer {
	catalog := map[int64]struct {
		name string
		cls  string
	}{
		0: {name: "identity", cls: "encode"},
		1: {name: "gzip", cls: "compress"},
		2: {name: "zstd", cls: "compress"},
		7: {name: "cose-encrypt0", cls: "encrypt"},
	}

	nameToID := make(map[string]int64, len(catalog))
	catEntries := make(map[interface{}]interface{}, len(catalog))
	for id, c := range catalog {
		nameToID[c.name] = id
		ce := map[interface{}]interface{}{
			"name": c.name,
			"cls":  c.cls,
		}
		catEntries[int64(id)] = ce
	}

	header := map[interface{}]interface{}{
		"gts":  wire.Magic,
		"v":    int64(wire.Version),
		"prof": profile,
		"cat":  catEntries,
	}
	if layout != "" {
		header["layout"] = layout
	}
	id := wire.HeaderID(header)
	header["id"] = id

	tagged := cbor.Tag{Number: wire.SelfDescribeTag, Content: header}
	buf := wire.MustEncode(tagged)

	return &Writer{
		nameToID: nameToID,
		prev:     id,
		buf:      buf,
	}
}

// Head returns the id the next appended frame must reference as "prev".
func (w *Writer) Head() []byte {
	out := make([]byte, len(w.prev))
	copy(out, w.prev)
	return out
}

func (w *Writer) chainIDs(chain []string) []interface{} {
	out := make([]interface{}, len(chain))
	for i, name := range chain {
		out[i] = w.nameToID[name]
	}
	return out
}

// AddFrame appends one frame and returns its "id".
//
// payload and raw are mutually exclusive. transform, when non-empty, names a
// chain of catalog codecs (only "identity" is supported by this writer).
func (w *Writer) AddFrame(
	frameType string,
	payload interface{},
	raw []byte,
	transform []string,
	pubMeta interface{},
) []byte {
	if payload != nil && raw != nil {
		panic("payload and raw are mutually exclusive")
	}

	frame := map[interface{}]interface{}{"t": frameType}

	var data interface{}
	switch {
	case len(transform) > 0:
		if raw == nil && payload == nil {
			panic("transform requires a raw or payload source")
		}
		for _, name := range transform {
			if name != "identity" {
				panic("non-identity transforms require the Python producer")
			}
		}
		var source []byte
		if raw != nil {
			source = raw
		} else {
			source = wire.MustEncode(payload)
		}
		frame["x"] = w.chainIDs(transform)
		data = source
	case raw != nil:
		data = raw
	case payload != nil:
		data = payload
	}

	if data != nil {
		frame["d"] = data
	}
	if pubMeta != nil {
		frame["pub"] = pubMeta
	}
	frame["prev"] = w.prev

	id := wire.ContentID(frame)
	frame["id"] = id

	w.offsets = append(w.offsets, len(w.buf))
	w.types = append(w.types, frameType)
	w.buf = append(w.buf, wire.MustEncode(frame)...)
	w.prev = id
	return id
}

// AddTerms appends a terms frame.
func (w *Writer) AddTerms(terms []model.Term) []byte {
	rows := make([]interface{}, len(terms))
	for i := range terms {
		rows[i] = termToWire(&terms[i])
	}
	return w.AddFrame("terms", rows, nil, nil, nil)
}

// AddQuads appends a quads frame (graph slot omitted when nil).
func (w *Writer) AddQuads(quads []model.Quad) []byte {
	rows := make([]interface{}, len(quads))
	for i, q := range quads {
		row := []interface{}{int64(q.S), int64(q.P), int64(q.O)}
		if q.G != nil {
			row = append(row, int64(*q.G))
		}
		rows[i] = row
	}
	return w.AddFrame("quads", rows, nil, nil, nil)
}

// AddReifies appends a reifies frame binding reifier-ids to triples.
func (w *Writer) AddReifies(bindings []model.ReifierEntry) []byte {
	m := make(map[interface{}]interface{}, len(bindings))
	for _, b := range bindings {
		m[int64(b.RID)] = []interface{}{
			int64(b.SPO.S), int64(b.SPO.P), int64(b.SPO.O),
		}
	}
	return w.AddFrame("reifies", m, nil, nil, nil)
}

// AddAnnot appends an annot frame.
func (w *Writer) AddAnnot(rows []model.Triple3) []byte {
	arr := make([]interface{}, len(rows))
	for i, r := range rows {
		arr[i] = []interface{}{int64(r.S), int64(r.P), int64(r.O)}
	}
	return w.AddFrame("annot", arr, nil, nil, nil)
}

// AddBlob appends an inline blob frame; metadata goes in "pub" (§12).
func (w *Writer) AddBlob(data []byte, mt, rep string) []byte {
	pub := map[interface{}]interface{}{}
	if mt != "" {
		pub["mt"] = mt
	}
	if rep != "" {
		pub["rep"] = rep
	}
	if len(pub) == 0 {
		return w.AddFrame("blob", nil, data, nil, nil)
	}
	return w.AddFrame("blob", nil, data, nil, pub)
}

// AddMeta appends a meta frame.
func (w *Writer) AddMeta(meta map[interface{}]interface{}) []byte {
	return w.AddFrame("meta", meta, nil, nil, nil)
}

// AddSuppress appends a suppress frame.
func (w *Writer) AddSuppress(targets []interface{}, reason string, by *int) []byte {
	payload := map[interface{}]interface{}{"targets": targets}
	if reason != "" {
		payload["reason"] = reason
	}
	if by != nil {
		if *by < 0 {
			// Mirrors Rust's usize contract: a negative suppress.by is a
			// caller bug, never valid wire content.
			panic("suppress.by must be >= 0")
		}
		payload["by"] = int64(*by)
	}
	return w.AddFrame("suppress", payload, nil, nil, nil)
}

// AddIndex appends an index footer covering every frame appended so far (§6.2).
//
// "count"/"head" delimit the covered region (the streamable boundary, §3.3);
// "off" carries each covered frame's byte offset from the start of this
// writer's output; "ti" locates frames by type (0-based frame positions). A
// later AddIndex covers the earlier one too — the last index wins (§6.2).
func (w *Writer) AddIndex() []byte {
	ti := map[interface{}]interface{}{}
	for pos, ftype := range w.types {
		existing, _ := ti[ftype].([]interface{})
		ti[ftype] = append(existing, int64(pos))
	}
	payload := map[interface{}]interface{}{
		"count": int64(len(w.types)),
		"head":  w.prev,
	}
	if len(w.offsets) > 0 { // "off"/"ti" are [+ uint]-shaped — omit when empty
		off := make([]interface{}, len(w.offsets))
		for i, o := range w.offsets {
			off[i] = int64(o)
		}
		payload["off"] = off
		payload["ti"] = ti
	}
	return w.AddFrame("index", payload, nil, nil, nil)
}

// ToBytes returns the complete GTS file bytes.
func (w *Writer) ToBytes() []byte {
	out := make([]byte, len(w.buf))
	copy(out, w.buf)
	return out
}

// DigestString packs bytes into a blake3:<hex> digest string.
func DigestString(data []byte) string {
	return wire.DigestStr(data)
}
