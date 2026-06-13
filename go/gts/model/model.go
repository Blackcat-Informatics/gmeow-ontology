// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package model provides the in-memory data model for a folded GTS graph.
package model

// Well-known datatype IRIs used by the literal-defaulting rule (§7.1).
const (
	XSDString     = "http://www.w3.org/2001/XMLSchema#string"
	RDFLangString = "http://www.w3.org/1999/02/22-rdf-syntax-ns#langString"
)

// TermKind is the kind of an RDF term, matching the wire "k" field (§7.1).
type TermKind int

const (
	// Iri is a fully qualified Internationalized Resource Identifier.
	Iri TermKind = iota
	// Literal is a lexical value, optionally with a datatype or language tag.
	Literal
	// Bnode is a file-local blank node label.
	Bnode
	// Triple is a quoted triple (RDF 1.2 triple term) carried by reifier id.
	Triple
)

// FromWire parses the wire "k" value; an unknown kind defaults to IRI (§7.1).
func FromWire(k int64) TermKind {
	switch k {
	case 1:
		return Literal
	case 2:
		return Bnode
	case 3:
		return Triple
	default:
		return Iri
	}
}

// Term is a single RDF term carried by append-order id.
type Term struct {
	Kind TermKind
	// IRI string, literal lexical form, or blank-node label (file-local).
	Value string
	// Term-id of the literal's datatype IRI, when explicit.
	Datatype *int
	// Literal language tag (BCP 47).
	Lang string
	// Term-id of the reifier of a quoted triple (kind == Triple).
	Reifier *int
}

// Quad is a tuple of term-ids; the graph slot is nil for the default graph.
type Quad struct {
	S, P, O int
	G       *int
}

// Triple3 is a triple of term-ids.
type Triple3 struct {
	S, P, O int
}

// OpaqueNode represents a frame the reader could not decode (§7.6).
type OpaqueNode struct {
	ID        []byte
	FrameType string
	// "unknown-codec" | "missing-key" | "damaged"
	Reason string
	// "none" | "valid" | "invalid" | "unverified"
	SigStat    string
	PubMeta    interface{}
	Recipients []interface{}
}

// Suppression is a recorded suppress directive (§11).
type Suppression struct {
	// Target maps ({"kind": "term"|"quad"|"reifier"|"frame"|"blob", ...}).
	Targets []interface{}
	Reason  string
	By      *int
}

// Diagnostic is a machine-observable reader diagnostic (§2.3).
type Diagnostic struct {
	Code       string
	Detail     string
	FrameIndex *int
}

// Signature is the verification outcome for a signed frame (§9.2).
//
// Cose retains the raw COSE_Sign1 bytes so streamable compaction (§10.1) can
// carry the signature detached — forever verifiable against FrameID even
// after the frame itself is re-authored into a new chain.
type Signature struct {
	FrameID []byte
	Kid     string
	// "valid" | "invalid" | "unverified"
	Status string
	Cose   []byte
}

// StreamableInfo is one segment's layout state (§3.3).
//
// Covered/Head come from the segment's last intact index frame; Tail counts
// the legal unpresaged frames after it ("streamable through frame Covered,
// accretive tail of Tail frame(s)"). For an unclaimed (accretive) segment all
// fields are their zero values.
type StreamableInfo struct {
	Claimed bool
	Covered int
	Tail    int
	Head    []byte
}

// MetaEntry is a single key/value metadata pair.
type MetaEntry struct {
	Key   string
	Value interface{}
}

// BlobEntry is a single inline blob.
type BlobEntry struct {
	Digest string
	Data   []byte
}

// BlobMetaEntry is declared blob metadata by digest.
type BlobMetaEntry struct {
	Digest string
	Meta   interface{}
}

// ReifierEntry binds a reifier-id to a triple.
type ReifierEntry struct {
	RID int
	SPO Triple3
}

// Graph is the folded result of a GTS log.
type Graph struct {
	Terms           []Term
	Quads           []Quad
	Reifiers        []ReifierEntry
	Annotations     []Triple3
	Blobs           []BlobEntry
	BlobMeta        []BlobMetaEntry
	Meta            []MetaEntry
	Suppressions    []Suppression
	Opaque          []OpaqueNode
	Signatures      []Signature
	Diagnostics     []Diagnostic
	SegmentHeads    [][]byte
	SegmentProfiles []string
	SegmentMeta     [][]MetaEntry
	// SegmentStreamable is the per-segment layout state (§3.3), in file order —
	// the declared-vs-computed streamable claim, its covered boundary, and the
	// accretive tail.
	SegmentStreamable []StreamableInfo
}

// Reifier looks up a reifier binding.
func (g *Graph) Reifier(rid int) (Triple3, bool) {
	for _, r := range g.Reifiers {
		if r.RID == rid {
			return r.SPO, true
		}
	}
	return Triple3{}, false
}

// SetReifier binds a reifier, replacing in place (Python dict assignment).
func (g *Graph) SetReifier(rid int, spo Triple3) {
	for i := range g.Reifiers {
		if g.Reifiers[i].RID == rid {
			g.Reifiers[i].SPO = spo
			return
		}
	}
	g.Reifiers = append(g.Reifiers, ReifierEntry{RID: rid, SPO: spo})
}

// SetMeta sets a meta key, replacing in place.
func (g *Graph) SetMeta(key string, value interface{}) {
	for i := range g.Meta {
		if g.Meta[i].Key == key {
			g.Meta[i].Value = value
			return
		}
	}
	g.Meta = append(g.Meta, MetaEntry{Key: key, Value: value})
}

// SetBlobMeta records a blob's declared metadata, replacing in place.
func (g *Graph) SetBlobMeta(digest string, meta interface{}) {
	for i := range g.BlobMeta {
		if g.BlobMeta[i].Digest == digest {
			g.BlobMeta[i].Meta = meta
			return
		}
	}
	g.BlobMeta = append(g.BlobMeta, BlobMetaEntry{Digest: digest, Meta: meta})
}

// SetBlob stores an inline blob under its digest, replacing in place.
func (g *Graph) SetBlob(digest string, data []byte) {
	for i := range g.Blobs {
		if g.Blobs[i].Digest == digest {
			g.Blobs[i].Data = data
			return
		}
	}
	g.Blobs = append(g.Blobs, BlobEntry{Digest: digest, Data: data})
}

// DatatypeIRI returns the effective datatype IRI of a literal, applying §7.1 defaulting.
func (g *Graph) DatatypeIRI(t *Term) string {
	if t.Datatype != nil {
		dt := *t.Datatype
		if dt >= 0 && dt < len(g.Terms) {
			return g.Terms[dt].Value
		}
		return XSDString
	}
	if t.Lang != "" {
		return RDFLangString
	}
	return XSDString
}
