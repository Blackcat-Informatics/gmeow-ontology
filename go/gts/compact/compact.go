// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package compact implements streamable compaction (GTS-SPEC §10.1):
// re-author the ordering, only the ordering.
//
// Streamable rewrites an accretive GTS file (or multi-segment composition)
// into ONE delivery-ordered segment in the streamable layout state (§3.3): a
// leading streaming index in the stream vocabulary (§13.3), the content
// graph, blobs most-significant-first, and a trailing offset index footer.
// Content signatures ride through untouched; frame signatures are carried
// detached in compaction provenance; the ordering commitment is re-issued —
// the compactor is the sole attester of the new ordering.
//
// The rewrite is byte-deterministic for the same input and parameters
// (§14.1): blob order is ascending decoded size with digest tie-break, the
// agent string is a constant, and the timestamp is a parameter — never
// ambient time.
package compact

import (
	"encoding/base64"
	"fmt"
	"sort"
	"strconv"
	"strings"

	"go.blackcatinformatics.ca/gts/model"
	"go.blackcatinformatics.ca/gts/reader"
	"go.blackcatinformatics.ca/gts/stream"
	"go.blackcatinformatics.ca/gts/wire"
	"go.blackcatinformatics.ca/gts/writer"
)

const (
	rdfType     = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
	xsdInteger  = "http://www.w3.org/2001/XMLSchema#integer"
	xsdDateTime = "http://www.w3.org/2001/XMLSchema#dateTime"
)

// RefusedError reports an input that is not safely compactable
// (§10.1/§14.1 refuse-don't-trust).
type RefusedError struct {
	msg string
}

func (e *RefusedError) Error() string {
	return e.msg
}

// refuse builds a RefusedError with a formatted message.
func refuse(format string, args ...interface{}) *RefusedError {
	return &RefusedError{msg: fmt.Sprintf(format, args...)}
}

// targetKind returns the "kind" field of a suppression target map.
func targetKind(target interface{}) string {
	m, ok := target.(map[interface{}]interface{})
	if !ok {
		return ""
	}
	if v, ok := wire.MapGet(m, "kind"); ok {
		return wire.TextOr(v, "")
	}
	return ""
}

// refusalGate verifies the input cleanly and returns its union fold + single
// profile.
func refusalGate(data []byte, sealOriginal bool) (*model.Graph, string, error) {
	fs := reader.ReadFileSegments(data)
	if fs.Fatal != nil {
		return nil, "", refuse("input is not a clean GTS file: %s: %s", fs.Fatal.Code, fs.Fatal.Detail)
	}
	if fs.Torn >= 0 {
		return nil, "", refuse("input has a torn append at byte %d", fs.Torn)
	}
	for idx, seg := range fs.Segments {
		if len(seg.Diagnostics) > 0 {
			first := seg.Diagnostics[0]
			return nil, "", refuse("segment %d does not verify cleanly: %s: %s", idx, first.Code, first.Detail)
		}
	}
	seen := make(map[string]struct{})
	var profiles []string
	for _, seg := range fs.Segments {
		for _, p := range seg.SegmentProfiles {
			if _, ok := seen[p]; !ok {
				seen[p] = struct{}{}
				profiles = append(profiles, p)
			}
		}
	}
	if len(profiles) > 1 {
		sort.Strings(profiles)
		quoted := make([]string, len(profiles))
		for i, p := range profiles {
			quoted[i] = "'" + p + "'"
		}
		return nil, "", refuse("mixed segment profiles [%s] are not compactable (v1)", strings.Join(quoted, ", "))
	}
	profile := "generic"
	if len(profiles) == 1 {
		profile = profiles[0]
	}
	if profile == "evidence" && !sealOriginal {
		return nil, "", refuse("an 'evidence' artifact's signed chain IS the artifact; refusing " +
			"to re-order it without --seal-original (§10.1)")
	}
	g := reader.Read(data, true, nil)
	for _, sup := range g.Suppressions {
		for _, target := range sup.Targets {
			if targetKind(target) == "frame" {
				return nil, "", refuse("input carries a frame-addressed suppression; the rewrite " +
					"assigns new frame ids, so the target would silently " +
					"dangle (§10.1)")
			}
		}
	}
	return g, profile, nil
}

// graphBuilder accumulates the streaming-index terms and quads with stable ids.
type graphBuilder struct {
	terms []model.Term
	quads []model.Quad
}

// add appends a term and returns its stable id in the builder.
func (b *graphBuilder) add(t model.Term) int {
	b.terms = append(b.terms, t)
	return len(b.terms) - 1
}

// literal adds a literal term and returns its stable id.
func (b *graphBuilder) literal(value string, datatype *int) int {
	return b.add(model.Term{Kind: model.Literal, Value: value, Datatype: datatype})
}

// quad appends a triple to the builder.
func (b *graphBuilder) quad(s, p, o int) {
	b.quads = append(b.quads, model.Quad{S: s, P: p, O: o})
}

// blobBytes returns the inline blob data for digest, or nil if absent.
func blobBytes(g *model.Graph, digest string) []byte {
	for _, b := range g.Blobs {
		if b.Digest == digest {
			return b.Data
		}
	}
	return nil
}

// blobMetaString returns a string value from a blob's declared metadata.
func blobMetaString(g *model.Graph, digest, key string) (string, bool) {
	for _, bm := range g.BlobMeta {
		if bm.Digest != digest {
			continue
		}
		m, ok := bm.Meta.(map[interface{}]interface{})
		if !ok {
			continue
		}
		if v, ok := wire.MapGet(m, key); ok {
			if s, ok := v.(string); ok {
				return s, true
			}
		}
	}
	return "", false
}

// streamingIndex builds the leading streaming index + compaction provenance
// (§3.3, §13.3).
func streamingIndex(
	g *model.Graph,
	blobOrder []string,
	timestamp string,
	sealedDigest string,
	sealedSize int,
) *graphBuilder {
	b := &graphBuilder{}
	// Fixed vocabulary block — constant ids across engines for determinism.
	tType := b.add(model.Term{Kind: model.Iri, Value: rdfType})
	tInt := b.add(model.Term{Kind: model.Iri, Value: xsdInteger})
	tDt := b.add(model.Term{Kind: model.Iri, Value: xsdDateTime})
	tManifestation := b.add(model.Term{Kind: model.Iri, Value: stream.Manifestation})
	tDigest := b.add(model.Term{Kind: model.Iri, Value: stream.Digest})
	tMt := b.add(model.Term{Kind: model.Iri, Value: stream.MediaType})
	tSize := b.add(model.Term{Kind: model.Iri, Value: stream.Size})
	tRole := b.add(model.Term{Kind: model.Iri, Value: stream.Role})
	tOrder := b.add(model.Term{Kind: model.Iri, Value: stream.Order})
	tCompaction := b.add(model.Term{Kind: model.Iri, Value: stream.Compaction})
	tAgent := b.add(model.Term{Kind: model.Iri, Value: stream.Agent})
	tTimestamp := b.add(model.Term{Kind: model.Iri, Value: stream.Timestamp})
	tSourceHead := b.add(model.Term{Kind: model.Iri, Value: stream.SourceHead})
	tSealedSource := b.add(model.Term{Kind: model.Iri, Value: stream.SealedSource})
	tDetachedSig := b.add(model.Term{Kind: model.Iri, Value: stream.DetachedSignature})
	tSourceFrame := b.add(model.Term{Kind: model.Iri, Value: stream.SourceFrame})
	tCose := b.add(model.Term{Kind: model.Iri, Value: stream.Cose})

	// One Manifestation per promised blob, in delivery order.
	for order, digest := range blobOrder {
		m := b.add(model.Term{Kind: model.Bnode, Value: fmt.Sprintf("m%d", order)})
		sealed := digest == sealedDigest
		size := sealedSize
		mt, haveMt := "application/gts", true
		if !sealed {
			size = len(blobBytes(g, digest))
			mt, haveMt = blobMetaString(g, digest, "mt")
		}
		b.quad(m, tType, tManifestation)
		b.quad(m, tDigest, b.literal(digest, nil))
		if haveMt {
			b.quad(m, tMt, b.literal(mt, nil))
		}
		b.quad(m, tSize, b.literal(strconv.Itoa(size), &tInt))
		role := "primary"
		if sealed {
			role = "source"
		}
		b.quad(m, tRole, b.literal(role, nil))
		b.quad(m, tOrder, b.literal(strconv.Itoa(order), &tInt))
	}

	// The Compaction provenance node (§10.1).
	c := b.add(model.Term{Kind: model.Bnode, Value: "c"})
	b.quad(c, tType, tCompaction)
	b.quad(c, tAgent, b.literal(stream.CompactAgent, nil))
	b.quad(c, tTimestamp, b.literal(timestamp, &tDt))
	for _, head := range g.SegmentHeads {
		b.quad(c, tSourceHead, b.literal("blake3:"+wire.Hex(head), nil))
	}
	if sealedDigest != "" {
		b.quad(c, tSealedSource, b.literal(sealedDigest, nil))
	}

	// Detached frame signatures (§10.1): checkable claims about the original log.
	j := 0
	for _, sig := range g.Signatures {
		if sig.Cose == nil {
			continue
		}
		node := b.add(model.Term{Kind: model.Bnode, Value: fmt.Sprintf("s%d", j)})
		j++
		coseB64 := base64.RawURLEncoding.EncodeToString(sig.Cose)
		b.quad(node, tType, tDetachedSig)
		b.quad(node, tSourceFrame, b.literal("blake3:"+wire.Hex(sig.FrameID), nil))
		b.quad(node, tCose, b.literal(coseB64, nil))
	}
	return b
}

// shiftTerm shifts a term's id references into the output id space.
func shiftTerm(t model.Term, base int) model.Term {
	if t.Datatype != nil {
		dt := *t.Datatype + base
		t.Datatype = &dt
	}
	if t.Reifier != nil {
		rf := *t.Reifier + base
		t.Reifier = &rf
	}
	return t
}

// shiftedSuppressions carries suppressions forward, one output suppression
// per input (§10.1).
//
// Re-authoring of the ordering only: each original suppression keeps its own
// frame with its reason/by metadata intact — blob targets verbatim
// (content-addressing is layout-independent), id-addressed targets and "by"
// shifted into the output id space.
func shiftedSuppressions(g *model.Graph, base int) []model.Suppression {
	var out []model.Suppression
	for _, sup := range g.Suppressions {
		var targets []interface{}
		for _, target := range sup.Targets {
			m, ok := target.(map[interface{}]interface{})
			if !ok {
				targets = append(targets, target)
				continue
			}
			kind := targetKind(target)
			t := make(map[interface{}]interface{}, len(m))
			for k, v := range m {
				t[k] = v
				key := wire.TextOr(k, "")
				if (kind == "term" || kind == "reifier") && key == "id" {
					if tid, ok := wire.AsInt(v); ok {
						t[k] = int64(tid + base)
					}
				} else if kind == "quad" && key == "q" {
					if ids, ok := v.([]interface{}); ok {
						shifted := make([]interface{}, len(ids))
						for i, x := range ids {
							if n, ok := wire.AsInt(x); ok {
								shifted[i] = int64(n + base)
							} else {
								shifted[i] = x
							}
						}
						t[k] = shifted
					}
				}
			}
			targets = append(targets, t)
		}
		shiftedBy := sup.By
		if sup.By != nil {
			b := *sup.By + base
			shiftedBy = &b
		}
		out = append(out, model.Suppression{
			Targets: targets,
			Reason:  sup.Reason,
			By:      shiftedBy,
		})
	}
	return out
}

// Streamable rewrites a GTS file into one streamable segment (§10.1).
//
// data must verify cleanly (refuse-don't-trust). timestamp is the rewrite
// time recorded as stream:timestamp — an explicit parameter so the output is
// byte-reproducible. sealOriginal carries the verbatim source bytes as a
// nested GTS blob (§12.1), role "source" — REQUIRED for evidence input.
//
// Returns the compacted single-segment streamable GTS bytes, or a
// *RefusedError on any §10.1/§14.1 refusal condition.
func Streamable(data []byte, timestamp string, sealOriginal bool) ([]byte, error) {
	g, profile, err := refusalGate(data, sealOriginal)
	if err != nil {
		return nil, err
	}

	// Delivery plan: most-significant-first — ascending decoded size, digest
	// tie-break; the sealed original (least significant) always travels last.
	// Sizes are paired up front so the sort never re-scans the blob table.
	type blobKey struct {
		size   int
		digest string
	}
	keyed := make([]blobKey, 0, len(g.Blobs))
	for _, b := range g.Blobs {
		keyed = append(keyed, blobKey{size: len(b.Data), digest: b.Digest})
	}
	sort.Slice(keyed, func(i, j int) bool {
		if keyed[i].size != keyed[j].size {
			return keyed[i].size < keyed[j].size
		}
		return keyed[i].digest < keyed[j].digest
	})
	blobOrder := make([]string, 0, len(keyed))
	for _, k := range keyed {
		blobOrder = append(blobOrder, k.digest)
	}
	sealedDigest := ""
	if sealOriginal {
		sealedDigest = wire.DigestStr(data)
		kept := make([]string, 0, len(blobOrder))
		for _, d := range blobOrder {
			if d != sealedDigest {
				kept = append(kept, d)
			}
		}
		blobOrder = kept
		blobOrder = append(blobOrder, sealedDigest)
	}

	index := streamingIndex(g, blobOrder, timestamp, sealedDigest, len(data))
	base := len(index.terms)

	w := writer.NewWithLayout(profile, "streamable")
	// Leading streaming index: the catalog presages everything below it.
	w.AddTerms(index.terms)
	w.AddQuads(index.quads)
	// Content graph, re-emitted from the folded union (ids shifted by base).
	if len(g.Terms) > 0 {
		shifted := make([]model.Term, len(g.Terms))
		for i, t := range g.Terms {
			shifted[i] = shiftTerm(t, base)
		}
		w.AddTerms(shifted)
	}
	if len(g.Quads) > 0 {
		shifted := make([]model.Quad, len(g.Quads))
		for i, q := range g.Quads {
			nq := model.Quad{S: q.S + base, P: q.P + base, O: q.O + base}
			if q.G != nil {
				gr := *q.G + base
				nq.G = &gr
			}
			shifted[i] = nq
		}
		w.AddQuads(shifted)
	}
	if len(g.Reifiers) > 0 {
		shifted := make([]model.ReifierEntry, len(g.Reifiers))
		for i, r := range g.Reifiers {
			shifted[i] = model.ReifierEntry{
				RID: r.RID + base,
				SPO: model.Triple3{S: r.SPO.S + base, P: r.SPO.P + base, O: r.SPO.O + base},
			}
		}
		w.AddReifies(shifted)
	}
	if len(g.Annotations) > 0 {
		shifted := make([]model.Triple3, len(g.Annotations))
		for i, a := range g.Annotations {
			shifted[i] = model.Triple3{S: a.S + base, P: a.P + base, O: a.O + base}
		}
		w.AddAnnot(shifted)
	}
	for _, sup := range shiftedSuppressions(g, base) {
		w.AddSuppress(sup.Targets, sup.Reason, sup.By)
	}
	// Blobs in delivery order; declared metadata rides along.
	for _, digest := range blobOrder {
		if digest == sealedDigest {
			w.AddBlob(data, "application/gts", "source")
			continue
		}
		mt, _ := blobMetaString(g, digest, "mt")
		rep, _ := blobMetaString(g, digest, "rep")
		w.AddBlob(blobBytes(g, digest), mt, rep)
	}
	// The re-issued ordering commitment: the compactor is its sole attester.
	w.AddIndex()
	return w.ToBytes(), nil
}
