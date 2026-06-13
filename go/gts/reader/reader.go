// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package reader implements the GTS Baseline Reader contract (§2.1).
package reader

import (
	"bytes"
	"fmt"

	"go.blackcatinformatics.ca/gts/codec"
	"go.blackcatinformatics.ca/gts/model"
	"go.blackcatinformatics.ca/gts/stream"
	"go.blackcatinformatics.ca/gts/wire"
	"github.com/fxamacker/cbor/v2"
)

// asInt64 coerces a decoded CBOR value to int64.
func asInt64(v interface{}) (int64, bool) {
	return wire.AsInt64(v)
}

// asIdx coerces a decoded CBOR value to a non-negative int (term index).
func asIdx(v interface{}) (int, bool) {
	return wire.AsInt(v)
}

// asText coerces a decoded CBOR value to a string.
func asText(v interface{}) (string, bool) {
	return wire.AsText(v)
}

// textOr returns a text value or a default.
func textOr(v interface{}, def string) string {
	return wire.TextOr(v, def)
}

// fmtOpt formats an optional graph slot for diagnostics.
func fmtOpt(g *int) string {
	if g == nil {
		return "None"
	}
	return fmt.Sprintf("%d", *g)
}

// diagCodeFor maps a codec failure reason to a diagnostic code.
func diagCodeFor(reason string) string {
	if reason == "missing-key" {
		return "MissingKey"
	}
	return "UnknownCodec"
}

type payloadError struct {
	unavailable bool
	reason      string
	detail      string
	damaged     bool
}

// codecErrToPayload translates a codec.Error into a reader payloadError.
func codecErrToPayload(e error) *payloadError {
	if ce, ok := e.(*codec.Error); ok {
		return &payloadError{
			unavailable: !ce.Failed,
			reason:      ce.Reason,
			detail:      ce.Detail,
			damaged:     ce.Failed,
		}
	}
	return &payloadError{damaged: true, detail: e.Error()}
}

// indexRecord is one intact index frame (§6.2): its absolute item position
// plus the covered-region boundary (count, head).
type indexRecord struct {
	pos   int
	count int
	head  []byte
}

// blobEvent is one inline blob arrival (§3.3): its absolute item position,
// digest, and whether a stream:digest description preceded it.
type blobEvent struct {
	pos       int
	digest    string
	described bool
}

type folder struct {
	g       *model.Graph
	catalog map[int64]*codec.Codec
	// Layout-state bookkeeping (§3.3): intact index frames seen, digests the
	// graph has described via stream:digest so far, and each inline blob's
	// arrival.
	indexRecords []indexRecord
	described    map[string]struct{}
	blobEvents   []blobEvent
}

// diag appends a diagnostic to the folded graph.
func (f *folder) diag(code, detail string, index *int) {
	f.g.Diagnostics = append(f.g.Diagnostics, model.Diagnostic{
		Code:       code,
		Detail:     detail,
		FrameIndex: index,
	})
}

// resolveCodecs looks up codec ids in the header catalog.
func (f *folder) resolveCodecs(ids []interface{}) ([]*codec.Codec, *payloadError) {
	var chain []*codec.Codec
	for _, cid := range ids {
		n, ok := asInt64(cid)
		if !ok {
			return nil, &payloadError{
				unavailable: true,
				reason:      "unknown-codec",
				detail:      fmt.Sprintf("codec id %v not an integer", cid),
			}
		}
		c, ok := f.catalog[n]
		if !ok {
			return nil, &payloadError{
				unavailable: true,
				reason:      "unknown-codec",
				detail:      fmt.Sprintf("codec id %v not in catalog", cid),
			}
		}
		chain = append(chain, c)
	}
	return chain, nil
}

func (f *folder) payload(frame map[interface{}]interface{}, isBlob bool) (interface{}, *payloadError) {
	d := frame["d"]
	if x, ok := frame["x"]; ok {
		if ids, ok := x.([]interface{}); ok && len(ids) > 0 {
			b, ok := d.([]byte)
			if !ok {
				return nil, &payloadError{damaged: true, detail: "transformed frame 'd' must be a byte string"}
			}
			chain, err := f.resolveCodecs(ids)
			if err != nil {
				return nil, err
			}
			decoded, derr := codec.DecodeChain(chain, b)
			if derr != nil {
				return nil, codecErrToPayload(derr)
			}
			if isBlob {
				return decoded, nil
			}
			var out interface{}
			if uerr := cbor.Unmarshal(decoded, &out); uerr != nil {
				return nil, &payloadError{damaged: true, detail: fmt.Sprintf("payload decode failed: %v", uerr)}
			}
			return out, nil
		}
	}
	if d == nil {
		return nil, nil
	}
	return d, nil
}

func (f *folder) foldFrame(frame map[interface{}]interface{}, index int) {
	ftype := textOr(frame["t"], "")
	payload, perr := f.payload(frame, ftype == "blob")
	if perr != nil {
		if perr.unavailable {
			f.opaque(frame, ftype, perr.reason)
			f.diag(diagCodeFor(perr.reason), perr.detail, &index)
		} else {
			f.opaque(frame, ftype, "damaged")
			f.diag("DamagedFrame", fmt.Sprintf("payload decode failed: %s", perr.detail), &index)
		}
		return
	}
	switch ftype {
	case "terms":
		f.hTerms(payload, index)
	case "quads":
		f.hQuads(payload, index)
	case "reifies":
		f.hReifies(payload, index)
	case "annot":
		f.hAnnot(payload, index)
	case "blob":
		f.hBlob(payload, frame, index)
	case "meta":
		f.hMeta(payload)
	case "suppress":
		f.hSuppress(payload)
	case "snapshot":
		f.hSnapshot(payload, index)
	case "index":
		f.hIndex(payload, index)
	case "opaque":
		f.hOpaque(payload)
	default:
		f.opaque(frame, ftype, "unknown-frame-type")
		f.diag("UnknownFrameType", fmt.Sprintf("unsupported frame type %q", ftype), &index)
	}
}

func (f *folder) hTerms(payload interface{}, index int) {
	rows, ok := payload.([]interface{})
	if !ok {
		return
	}
	for _, raw := range rows {
		entries, ok := raw.(map[interface{}]interface{})
		if !ok {
			continue
		}
		kind := model.FromWire(func() int64 {
			if v, ok := wire.MapGet(entries, "k"); ok {
				if n, ok := asInt64(v); ok {
					return n
				}
			}
			return -1
		}())
		value := ""
		if v, ok := wire.MapGet(entries, "v"); ok {
			if s, ok := asText(v); ok {
				value = s
			}
		}
		lang := ""
		if v, ok := wire.MapGet(entries, "l"); ok {
			if s, ok := asText(v); ok {
				lang = s
			}
		}
		dtRaw, hasDt := wire.MapGet(entries, "dt")
		rfRaw, hasRf := wire.MapGet(entries, "rf")
		tid := len(f.g.Terms)
		sanitize := func(r interface{}) *int {
			if r == nil {
				return nil
			}
			n, ok := asInt64(r)
			if !ok || n < 0 || n >= int64(tid) {
				return nil
			}
			i := int(n)
			return &i
		}
		dt := sanitize(dtRaw)
		rf := sanitize(rfRaw)
		outOfRange := func(r interface{}) bool {
			if r == nil {
				return false
			}
			n, ok := asInt64(r)
			return ok && n >= int64(tid)
		}
		if (hasDt && outOfRange(dtRaw)) || (hasRf && outOfRange(rfRaw)) {
			f.diag("ForwardReference", fmt.Sprintf("term %d has an out-of-range ref", tid), &index)
		}
		f.g.Terms = append(f.g.Terms, model.Term{
			Kind:     kind,
			Value:    value,
			Datatype: dt,
			Lang:     lang,
			Reifier:  rf,
		})
	}
}

func (f *folder) hQuads(payload interface{}, index int) {
	rows, ok := payload.([]interface{})
	if !ok {
		return
	}
	for _, row := range rows {
		items, ok := row.([]interface{})
		if !ok || len(items) < 3 {
			continue
		}
		s, sOk := asIdx(items[0])
		p, pOk := asIdx(items[1])
		o, oOk := asIdx(items[2])
		var gslot *int
		hasGraph := len(items) >= 4
		if hasGraph {
			if g, ok := asIdx(items[3]); ok {
				gslot = &g
			}
		}
		if !sOk || !pOk || !oOk || (hasGraph && gslot == nil) {
			f.diag("DamagedFrame", "quad has non-integer term ids", &index)
			continue
		}
		if !f.checkPositions(s, p, o, gslot, index) {
			continue
		}
		f.g.Quads = append(f.g.Quads, model.Quad{S: s, P: p, O: o, G: gslot})
		// Layout bookkeeping (§3.3): a stream:digest quad describes an
		// upcoming manifestation — record the IOU for the blob check.
		if f.g.Terms[p].Value == stream.Digest {
			if obj := &f.g.Terms[o]; obj.Value != "" {
				f.described[obj.Value] = struct{}{}
			}
		}
	}
}

func (f *folder) hReifies(payload interface{}, index int) {
	entries, ok := payload.(map[interface{}]interface{})
	if !ok {
		return
	}
	for k, spo := range entries {
		rid, ok := asInt64(k)
		if !ok {
			continue
		}
		items, ok := spo.([]interface{})
		if !ok || len(items) != 3 {
			continue
		}
		s, sOk := asIdx(items[0])
		p, pOk := asIdx(items[1])
		o, oOk := asIdx(items[2])
		n := len(f.g.Terms)
		ridOk := rid >= 0 && int(rid) < n
		spoOk := sOk && pOk && oOk && s < n && p < n && o < n
		if !ridOk || !spoOk {
			f.diag("DamagedFrame", fmt.Sprintf("reifier %d has bad/out-of-range ids", rid), &index)
			continue
		}
		irid := int(rid)
		spo := model.Triple3{S: s, P: p, O: o}
		if existing, ok := f.g.Reifier(irid); ok {
			if existing != spo {
				f.diag("ConflictingReifier", fmt.Sprintf("reifier %d rebound", irid), &index)
				continue
			}
		}
		f.g.SetReifier(irid, spo)
	}
}

func (f *folder) hAnnot(payload interface{}, index int) {
	rows, ok := payload.([]interface{})
	if !ok {
		return
	}
	for _, row := range rows {
		items, ok := row.([]interface{})
		if !ok || len(items) != 3 {
			continue
		}
		r, rOk := asIdx(items[0])
		p, pOk := asIdx(items[1])
		v, vOk := asIdx(items[2])
		n := len(f.g.Terms)
		if !rOk || !pOk || !vOk || r >= n || p >= n || v >= n {
			f.diag("DamagedFrame", "annot row has bad/out-of-range ids", &index)
			continue
		}
		if f.g.Terms[p].Kind != model.Iri {
			f.diag("PositionConstraint", fmt.Sprintf("annot predicate %d not an IRI", p), &index)
			continue
		}
		f.g.Annotations = append(f.g.Annotations, model.Triple3{S: r, P: p, O: v})
	}
}

func (f *folder) hBlob(payload interface{}, frame map[interface{}]interface{}, index int) {
	if b, ok := payload.([]byte); ok {
		digest := wire.DigestStr(b)
		if pub, ok := wire.MapGet(frame, "pub"); ok {
			if _, ok := pub.(map[interface{}]interface{}); ok {
				f.g.SetBlobMeta(digest, pub)
			}
		}
		f.g.SetBlob(digest, b)
		// Layout bookkeeping (§3.3): was this delivery presaged by a
		// stream:digest description in an earlier frame?
		_, described := f.described[digest]
		f.blobEvents = append(f.blobEvents, blobEvent{pos: index, digest: digest, described: described})
	}
	// else: external blob — bytes live elsewhere, referenced by "pub".digest (§12).
}

func (f *folder) hMeta(payload interface{}) {
	entries, ok := payload.(map[interface{}]interface{})
	if !ok {
		return
	}
	for k, v := range entries {
		key := fmt.Sprintf("%v", k)
		if s, ok := asText(k); ok {
			key = s
		}
		f.g.SetMeta(key, v)
	}
}

func (f *folder) hSuppress(payload interface{}) {
	entries, ok := payload.(map[interface{}]interface{})
	if !ok {
		return
	}
	targetsRaw, ok := wire.MapGet(entries, "targets")
	if !ok {
		return
	}
	targets, ok := targetsRaw.([]interface{})
	if !ok {
		return
	}
	var filtered []interface{}
	for _, t := range targets {
		if _, ok := t.(map[interface{}]interface{}); ok {
			filtered = append(filtered, t)
		}
	}
	s := model.Suppression{Targets: filtered}
	if reason, ok := wire.MapGet(entries, "reason"); ok {
		s.Reason = textOr(reason, "")
	}
	if by, ok := wire.MapGet(entries, "by"); ok {
		if b, ok := asIdx(by); ok {
			s.By = &b
		}
	}
	f.g.Suppressions = append(f.g.Suppressions, s)
}

func (f *folder) hSnapshot(payload interface{}, index int) {
	entries, ok := payload.(map[interface{}]interface{})
	if !ok {
		return
	}
	base := len(f.g.Terms)
	shift := func(v interface{}) interface{} {
		if n, ok := asIdx(v); ok {
			return uint64(n + base)
		}
		return v
	}
	shiftRow := func(row interface{}) interface{} {
		if items, ok := row.([]interface{}); ok {
			out := make([]interface{}, len(items))
			for i, it := range items {
				out[i] = shift(it)
			}
			return out
		}
		return row
	}

	if snapTerms, ok := wire.MapGet(entries, "terms"); ok {
		if terms, ok := snapTerms.([]interface{}); ok {
			shifted := make([]interface{}, len(terms))
			for i, raw := range terms {
				if termMap, ok := raw.(map[interface{}]interface{}); ok {
					newEntries := make(map[interface{}]interface{})
					for k, v := range termMap {
						newEntries[k] = v
						if s, ok := asText(k); ok && (s == "dt" || s == "rf") {
							newEntries[k] = shift(v)
						}
					}
					shifted[i] = newEntries
				} else {
					shifted[i] = raw
				}
			}
			f.hTerms(shifted, index)
		}
	}
	if quads, ok := wire.MapGet(entries, "quads"); ok {
		if q, ok := quads.([]interface{}); ok {
			shifted := make([]interface{}, len(q))
			for i, row := range q {
				shifted[i] = shiftRow(row)
			}
			f.hQuads(shifted, index)
		}
	}
	if reifies, ok := wire.MapGet(entries, "reifies"); ok {
		if r, ok := reifies.(map[interface{}]interface{}); ok {
			shifted := make(map[interface{}]interface{})
			for k, v := range r {
				shifted[shift(k)] = shiftRow(v)
			}
			f.hReifies(shifted, index)
		}
	}
	if annot, ok := wire.MapGet(entries, "annot"); ok {
		if a, ok := annot.([]interface{}); ok {
			shifted := make([]interface{}, len(a))
			for i, row := range a {
				shifted[i] = shiftRow(row)
			}
			f.hAnnot(shifted, index)
		}
	}
	if blobs, ok := wire.MapGet(entries, "blobs"); ok {
		if b, ok := blobs.(map[interface{}]interface{}); ok {
			for _, v := range b {
				if data, ok := v.([]byte); ok {
					f.g.SetBlob(wire.DigestStr(data), data)
				}
			}
		}
	}
	if meta, ok := wire.MapGet(entries, "meta"); ok {
		if m, ok := meta.(map[interface{}]interface{}); ok {
			for k, v := range m {
				key := fmt.Sprintf("%v", k)
				if s, ok := asText(k); ok {
					key = s
				}
				f.g.SetMeta(key, v)
			}
		}
	}
}

// hIndex records an intact index frame (§6.2) for the layout check (§3.3).
//
// The index stays an accelerator for the fold itself; only "count" and
// "head" are consumed here, as the covered-region boundary. A payload
// without a valid count/head pair is simply not an intact index.
func (f *folder) hIndex(payload interface{}, index int) {
	entries, ok := payload.(map[interface{}]interface{})
	if !ok {
		return
	}
	countRaw, ok := wire.MapGet(entries, "count")
	if !ok {
		return
	}
	count, ok := asIdx(countRaw)
	if !ok {
		return
	}
	headRaw, ok := wire.MapGet(entries, "head")
	if !ok {
		return
	}
	head, ok := headRaw.([]byte)
	if !ok {
		return
	}
	f.indexRecords = append(f.indexRecords, indexRecord{pos: index, count: count, head: head})
}

func (f *folder) hOpaque(payload interface{}) {
	entries, ok := payload.(map[interface{}]interface{})
	if !ok {
		return
	}
	var id []byte
	if v, ok := wire.MapGet(entries, "id"); ok {
		if b, ok := v.([]byte); ok {
			id = b
		}
	}
	f.g.Opaque = append(f.g.Opaque, model.OpaqueNode{
		ID:        id,
		FrameType: textOr(entries["type"], "opaque"),
		Reason:    textOr(entries["reason"], "unknown-codec"),
		SigStat:   textOr(entries["sigstat"], "none"),
		PubMeta:   entries["pub"],
	})
}

func (f *folder) checkPositions(s, p, o int, g *int, index int) bool {
	n := len(f.g.Terms)
	inBounds := s < n && p < n && o < n && (g == nil || *g < n)
	if !inBounds {
		f.diag("PositionConstraint", fmt.Sprintf("quad (%d,%d,%d,%s) has out-of-range term ids", s, p, o, fmtOpt(g)), &index)
		return false
	}
	ok := f.g.Terms[p].Kind == model.Iri
	if f.g.Terms[s].Kind == model.Literal {
		ok = false
	}
	if g != nil {
		kind := f.g.Terms[*g].Kind
		if kind == model.Literal || kind == model.Triple {
			ok = false
		}
	}
	if !ok {
		f.diag("PositionConstraint", fmt.Sprintf("quad (%d,%d,%d,%s) violates positions", s, p, o, fmtOpt(g)), &index)
	}
	return ok
}

func (f *folder) opaque(frame map[interface{}]interface{}, ftype, reason string) {
	var id []byte
	if v, ok := frame["id"]; ok {
		if b, ok := v.([]byte); ok {
			id = b
		}
	}
	sigstat := "none"
	if _, ok := frame["sig"]; ok {
		sigstat = "unverified"
	}
	var recipients []interface{}
	if to, ok := frame["to"]; ok {
		if arr, ok := to.([]interface{}); ok {
			for _, it := range arr {
				if _, ok := it.(map[interface{}]interface{}); ok {
					recipients = append(recipients, it)
				}
			}
		}
	}
	f.g.Opaque = append(f.g.Opaque, model.OpaqueNode{
		ID:         id,
		FrameType:  ftype,
		Reason:     reason,
		SigStat:    sigstat,
		PubMeta:    frame["pub"],
		Recipients: recipients,
	})
}

func isHeaderItem(item interface{}) bool {
	inner := item
	if tag, ok := item.(cbor.Tag); ok {
		inner = tag.Content
	}
	m, ok := inner.(map[interface{}]interface{})
	if !ok {
		return false
	}
	_, hasGts := wire.MapGet(m, "gts")
	_, hasT := wire.MapGet(m, "t")
	return hasGts && !hasT
}

func catalogFrom(header map[interface{}]interface{}) map[int64]*codec.Codec {
	out := make(map[int64]*codec.Codec)
	cat, ok := wire.MapGet(header, "cat")
	if !ok {
		return out
	}
	raw, ok := cat.(map[interface{}]interface{})
	if !ok {
		return out
	}
	for cid, entry := range raw {
		n, ok := asInt64(cid)
		if !ok {
			continue
		}
		fields, ok := entry.(map[interface{}]interface{})
		if !ok {
			continue
		}
		out[n] = &codec.Codec{
			Name: textOr(fields["name"], ""),
			Cls:  textOr(fields["cls"], "encode"),
		}
	}
	return out
}

// Read folds a GTS file into a Graph.
//
// With allowSegments=false the reader emulates a pre-§3.1 reader: a segment
// boundary is a FATAL SegmentBoundary diagnostic and nothing past it is folded.
// expectedHead, when given, is compared against the LAST segment's head.
func Read(data []byte, allowSegments bool, expectedHead []byte) *model.Graph {
	items, torn := wire.IterItems(data)
	if len(items) == 0 {
		g := emptyGraph()
		idx := 0
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{Code: "EmptyFile", Detail: "no CBOR items", FrameIndex: &idx})
		return g
	}

	var bounds []int
	for i, it := range items {
		if isHeaderItem(it.Item) {
			bounds = append(bounds, i)
		}
	}

	g := emptyGraph()
	if len(bounds) == 0 || bounds[0] != 0 {
		idx := 0
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{Code: "DamagedFrame", Detail: "first item is not a header", FrameIndex: &idx})
		return g
	}

	if len(bounds) > 1 && !allowSegments {
		g = readSegment(items[:bounds[1]], 0)
		idx := bounds[1]
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{
			Code:       "SegmentBoundary",
			Detail:     fmt.Sprintf("segment boundary at item %d but reader is in pre-segment mode; remainder of file NOT folded", idx),
			FrameIndex: &idx,
		})
		return g
	}

	var folded []*model.Graph
	for i, a := range bounds {
		b := len(items)
		if i+1 < len(bounds) {
			b = bounds[i+1]
		}
		folded = append(folded, readSegment(items[a:b], a))
	}

	if len(folded) == 1 {
		g = folded[0]
	} else {
		g = unionSegments(folded)
	}

	if expectedHead != nil {
		var lastHead []byte
		if len(g.SegmentHeads) > 0 {
			lastHead = g.SegmentHeads[len(g.SegmentHeads)-1]
		}
		if !bytesEqual(lastHead, expectedHead) {
			g.Diagnostics = append(g.Diagnostics, model.Diagnostic{Code: "TruncatedLog", Detail: "observed head does not match expected head"})
		}
	}
	if torn >= 0 {
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{Code: "TornAppendError", Detail: fmt.Sprintf("torn at offset %d", torn)})
	}
	return g
}

func bytesEqual(a, b []byte) bool {
	return bytes.Equal(a, b)
}

// emptyGraph returns a Graph whose slice fields are non-nil so JSON
// serialization and downstream consumers see empty arrays instead of null.
func emptyGraph() *model.Graph {
	return &model.Graph{
		Terms:             []model.Term{},
		Quads:             []model.Quad{},
		Reifiers:          []model.ReifierEntry{},
		Annotations:       []model.Triple3{},
		Blobs:             []model.BlobEntry{},
		BlobMeta:          []model.BlobMetaEntry{},
		Meta:              []model.MetaEntry{},
		Suppressions:      []model.Suppression{},
		Opaque:            []model.OpaqueNode{},
		Signatures:        []model.Signature{},
		Diagnostics:       []model.Diagnostic{},
		SegmentHeads:      [][]byte{},
		SegmentProfiles:   []string{},
		SegmentMeta:       [][]model.MetaEntry{},
		SegmentStreamable: []model.StreamableInfo{},
	}
}

// FileSegments is the per-segment view of a file.
type FileSegments struct {
	Segments []*model.Graph
	Torn     int // -1 for clean end
	Fatal    *model.Diagnostic
}

// ReadFileSegments folds a file segment-by-segment WITHOUT unioning.
func ReadFileSegments(data []byte) *FileSegments {
	items, torn := wire.IterItems(data)
	if len(items) == 0 {
		idx := 0
		return &FileSegments{
			Torn:  torn,
			Fatal: &model.Diagnostic{Code: "EmptyFile", Detail: "no CBOR items", FrameIndex: &idx},
		}
	}
	var bounds []int
	for i, it := range items {
		if isHeaderItem(it.Item) {
			bounds = append(bounds, i)
		}
	}
	if len(bounds) == 0 || bounds[0] != 0 {
		idx := 0
		return &FileSegments{
			Torn:  torn,
			Fatal: &model.Diagnostic{Code: "DamagedFrame", Detail: "first item is not a header", FrameIndex: &idx},
		}
	}
	var segments []*model.Graph
	for i, a := range bounds {
		b := len(items)
		if i+1 < len(bounds) {
			b = bounds[i+1]
		}
		segments = append(segments, readSegment(items[a:b], a))
	}
	return &FileSegments{Segments: segments, Torn: torn}
}

func readSegment(items []struct {
	Offset int
	Item   interface{}
}, indexOffset int) *model.Graph {
	g := emptyGraph()
	rawHeader := items[0].Item
	header, err := wire.UnwrapHeader(rawHeader)
	if err != nil {
		idx := indexOffset
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{Code: "DamagedFrame", Detail: fmt.Sprintf("invalid header: %v", err), FrameIndex: &idx})
		return g
	}
	var storedHID []byte
	if v, ok := wire.MapGet(header, "id"); ok {
		storedHID, _ = v.([]byte)
	}
	if !bytesEqual(storedHID, wire.HeaderID(header)) {
		idx := indexOffset
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{Code: "DamagedFrame", Detail: "header self-hash mismatch", FrameIndex: &idx})
	}
	expectedPrev := storedHID

	catalog := catalogFrom(header)
	fld := &folder{g: g, catalog: catalog, described: make(map[string]struct{})}
	var frameIDs [][]byte // per-frame chain ids, by 0-based frame position
	for idx, it := range items[1:] {
		absIndex := idx + 1 + indexOffset
		frame, ok := it.Item.(map[interface{}]interface{})
		if !ok {
			fld.diag("DamagedFrame", "frame is not a map", &absIndex)
			frameIDs = append(frameIDs, []byte{})
			continue
		}
		var storedID []byte
		if v, ok := frame["id"]; ok {
			storedID, _ = v.([]byte)
		}
		computed := wire.ContentID(frame)
		if !bytesEqual(storedID, computed) {
			fld.diag("DamagedFrame", "frame self-hash mismatch", &absIndex)
			ftype := textOr(frame["t"], "")
			fld.opaque(frame, ftype, "damaged")
			if storedID != nil {
				expectedPrev = storedID
			} else {
				expectedPrev = computed
			}
			frameIDs = append(frameIDs, expectedPrev)
			continue
		}
		prevOk := false
		if v, ok := frame["prev"]; ok {
			if b, ok := v.([]byte); ok {
				prevOk = bytesEqual(b, expectedPrev)
			}
		}
		if !prevOk {
			fld.diag("BrokenChain", "prev does not match", &absIndex)
		}
		expectedPrev = computed
		frameIDs = append(frameIDs, expectedPrev)
		if sig, ok := frame["sig"]; ok {
			// The raw COSE bytes are retained so streamable compaction (§10.1)
			// can carry the signature detached.
			if cose, ok := sig.([]byte); ok {
				g.Signatures = append(g.Signatures, model.Signature{FrameID: computed, Status: "unverified", Cose: cose})
			} else {
				// present but malformed — record as invalid, never silently drop
				g.Signatures = append(g.Signatures, model.Signature{FrameID: computed, Status: "invalid"})
			}
		}
		fld.foldFrame(frame, absIndex)
	}

	g.SegmentHeads = append(g.SegmentHeads, expectedPrev)
	segMeta := make([]model.MetaEntry, len(g.Meta))
	copy(segMeta, g.Meta)
	g.SegmentMeta = append(g.SegmentMeta, segMeta)
	g.SegmentProfiles = append(g.SegmentProfiles, textOr(header["prof"], "generic"))
	g.SegmentStreamable = append(g.SegmentStreamable, layoutCheck(g, header, fld, frameIDs, indexOffset))
	return g
}

// layoutCheck computes one segment's layout state and checks its claim (§3.3).
//
// For a segment claiming "layout": "streamable": (a) it must carry an intact
// index footer, (b) the last index's head must be the id of frame count, and
// (c) every covered inline blob must arrive after the stream:digest quad
// describing it. Frames after the last index are the legal accretive tail —
// boundary info, never a diagnostic. Unknown layout values impose no check (§5).
func layoutCheck(
	g *model.Graph,
	header map[interface{}]interface{},
	fld *folder,
	frameIDs [][]byte,
	indexOffset int,
) model.StreamableInfo {
	layout, _ := wire.MapGet(header, "layout")
	claimed := textOr(layout, "") == "streamable"
	total := len(frameIDs)
	if !claimed {
		return model.StreamableInfo{}
	}
	if len(fld.indexRecords) == 0 {
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{
			Code: "StreamableLayoutError",
			Detail: "segment claims layout 'streamable' but carries no intact " +
				"index footer (§3.3)",
		})
		return model.StreamableInfo{Claimed: true, Covered: 0, Tail: total}
	}
	last := fld.indexRecords[len(fld.indexRecords)-1]
	absPos, count, head := last.pos, last.count, last.head
	relPos := absPos - indexOffset // 1-based frame position of the index
	tail := total - relPos
	// The footer must IMMEDIATELY follow the frames it covers (§3.3): a
	// permissive count <= relPos-1 would let frames sit between the covered
	// prefix and the footer, counted neither as covered nor as tail.
	if count != relPos-1 || count < 1 || !bytesEqual(frameIDs[count-1], head) {
		pos := absPos
		g.Diagnostics = append(g.Diagnostics, model.Diagnostic{
			Code: "StreamableLayoutError",
			Detail: fmt.Sprintf("index footer contradicts the frames it covers: count %d "+
				"must name the frame immediately before the footer and head "+
				"must be that frame's id (§3.3)", count),
			FrameIndex: &pos,
		})
	}
	for _, ev := range fld.blobEvents {
		blobRel := ev.pos - indexOffset
		if blobRel <= count && !ev.described {
			pos := ev.pos
			g.Diagnostics = append(g.Diagnostics, model.Diagnostic{
				Code: "StreamableLayoutError",
				Detail: fmt.Sprintf("covered blob %s delivered before its stream:digest "+
					"description (catalog-before-payload, §3.3)", ev.digest),
				FrameIndex: &pos,
			})
		}
	}
	return model.StreamableInfo{Claimed: true, Covered: count, Tail: tail, Head: head}
}

type internKey struct {
	typ          byte // 0=iri, 1=lit, 2=bnode, 3=qt
	a            string
	b            string
	c            string
	seg          int
	rf           *int
	bnodeTID     int // anonymous bnode source term id (typ==2, value empty)
	bnodeLabeled bool
}

type unioner struct {
	out    *model.Graph
	intern map[internKey]int
}

func newUnioner() *unioner {
	return &unioner{
		out:    emptyGraph(),
		intern: make(map[internKey]int),
	}
}

func (u *unioner) keyFor(seg *model.Graph, segIdx, tid int) internKey {
	t := &seg.Terms[tid]
	switch t.Kind {
	case model.Iri:
		return internKey{typ: 0, a: t.Value}
	case model.Literal:
		return internKey{typ: 1, a: t.Value, b: seg.DatatypeIRI(t), c: t.Lang}
	case model.Bnode:
		if t.Value != "" {
			return internKey{typ: 2, seg: segIdx, a: t.Value, bnodeLabeled: true}
		}
		return internKey{typ: 2, seg: segIdx, bnodeTID: tid}
	case model.Triple:
		var rf *int
		if t.Reifier != nil {
			r := u.mapTerm(seg, segIdx, *t.Reifier)
			rf = &r
		}
		return internKey{typ: 3, rf: rf}
	}
	return internKey{}
}

func (u *unioner) mapTerm(seg *model.Graph, segIdx, tid int) int {
	key := u.keyFor(seg, segIdx, tid)
	if got, ok := u.intern[key]; ok {
		return got
	}
	t := seg.Terms[tid]
	var datatype *int
	if t.Datatype != nil {
		d := u.mapTerm(seg, segIdx, *t.Datatype)
		datatype = &d
	}
	var reifier *int
	if t.Reifier != nil {
		r := u.mapTerm(seg, segIdx, *t.Reifier)
		reifier = &r
	}
	value := t.Value
	if t.Kind == model.Bnode {
		if value != "" {
			value = fmt.Sprintf("s%d.%s", segIdx, value)
		} else {
			value = fmt.Sprintf("s%d._anon%d", segIdx, len(u.out.Terms))
		}
	}
	u.out.Terms = append(u.out.Terms, model.Term{
		Kind:     t.Kind,
		Value:    value,
		Datatype: datatype,
		Lang:     t.Lang,
		Reifier:  reifier,
	})
	newID := len(u.out.Terms) - 1
	u.intern[key] = newID
	return newID
}

func (u *unioner) remapSuppression(sup model.Suppression, seg *model.Graph, segIdx int) model.Suppression {
	n := len(seg.Terms)
	newTargets := make([]interface{}, len(sup.Targets))
	for i, target := range sup.Targets {
		m, ok := target.(map[interface{}]interface{})
		if !ok {
			newTargets[i] = target
			continue
		}
		kind := ""
		if v, ok := wire.MapGet(m, "kind"); ok {
			kind = textOr(v, "")
		}
		if kind == "frame" || kind == "blob" {
			newTargets[i] = target
			continue
		}
		newMap := make(map[interface{}]interface{})
		for k, v := range m {
			newMap[k] = v
			key := ""
			if s, ok := asText(k); ok {
				key = s
			}
			if (kind == "term" || kind == "reifier") && key == "id" {
				if tid, ok := asIdx(v); ok && tid < n {
					newMap[k] = uint64(u.mapTerm(seg, segIdx, tid))
				}
			} else if kind == "quad" && key == "q" {
				if ids, ok := v.([]interface{}); ok {
					remapped := make([]interface{}, len(ids))
					for j, x := range ids {
						if tid, ok := asIdx(x); ok && tid < n {
							remapped[j] = uint64(u.mapTerm(seg, segIdx, tid))
						} else {
							remapped[j] = x
						}
					}
					newMap[k] = remapped
				}
			}
		}
		newTargets[i] = newMap
	}
	newSup := model.Suppression{Targets: newTargets, Reason: sup.Reason}
	if sup.By != nil && *sup.By < n {
		by := u.mapTerm(seg, segIdx, *sup.By)
		newSup.By = &by
	}
	return newSup
}

func unionQuadKey(q model.Quad) string {
	if q.G != nil {
		return fmt.Sprintf("%d,%d,%d,%d", q.S, q.P, q.O, *q.G)
	}
	return fmt.Sprintf("%d,%d,%d", q.S, q.P, q.O)
}

func unionSegments(segments []*model.Graph) *model.Graph {
	u := newUnioner()
	seen := make(map[string]struct{})
	for segIdx, seg := range segments {
		for _, q := range seg.Quads {
			uq := model.Quad{
				S: u.mapTerm(seg, segIdx, q.S),
				P: u.mapTerm(seg, segIdx, q.P),
				O: u.mapTerm(seg, segIdx, q.O),
			}
			if q.G != nil {
				g := u.mapTerm(seg, segIdx, *q.G)
				uq.G = &g
			}
			key := unionQuadKey(uq)
			if _, ok := seen[key]; !ok {
				seen[key] = struct{}{}
				u.out.Quads = append(u.out.Quads, uq)
			}
		}
		for _, r := range seg.Reifiers {
			newRf := u.mapTerm(seg, segIdx, r.RID)
			spo := model.Triple3{
				S: u.mapTerm(seg, segIdx, r.SPO.S),
				P: u.mapTerm(seg, segIdx, r.SPO.P),
				O: u.mapTerm(seg, segIdx, r.SPO.O),
			}
			u.out.SetReifier(newRf, spo)
		}
		for _, a := range seg.Annotations {
			u.out.Annotations = append(u.out.Annotations, model.Triple3{
				S: u.mapTerm(seg, segIdx, a.S),
				P: u.mapTerm(seg, segIdx, a.P),
				O: u.mapTerm(seg, segIdx, a.O),
			})
		}
		for _, b := range seg.Blobs {
			u.out.SetBlob(b.Digest, b.Data)
		}
		for _, bm := range seg.BlobMeta {
			u.out.SetBlobMeta(bm.Digest, bm.Meta)
		}
		for _, m := range seg.Meta {
			u.out.SetMeta(m.Key, m.Value)
		}
		u.out.SegmentMeta = append(u.out.SegmentMeta, seg.SegmentMeta...)
		for _, sup := range seg.Suppressions {
			u.out.Suppressions = append(u.out.Suppressions, u.remapSuppression(sup, seg, segIdx))
		}
		u.out.Opaque = append(u.out.Opaque, seg.Opaque...)
		u.out.Signatures = append(u.out.Signatures, seg.Signatures...)
		u.out.Diagnostics = append(u.out.Diagnostics, seg.Diagnostics...)
		u.out.SegmentHeads = append(u.out.SegmentHeads, seg.SegmentHeads...)
		u.out.SegmentProfiles = append(u.out.SegmentProfiles, seg.SegmentProfiles...)
		u.out.SegmentStreamable = append(u.out.SegmentStreamable, seg.SegmentStreamable...)
	}
	return u.out
}
