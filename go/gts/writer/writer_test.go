// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

package writer

import (
	"testing"

	"go.blackcatinformatics.ca/gts/model"
	"go.blackcatinformatics.ca/gts/reader"
)

func TestRoundTripTermsAndQuads(t *testing.T) {
	w := New("generic")
	terms := []model.Term{
		{Kind: model.Iri, Value: "https://example.org/Cat"},
		{Kind: model.Iri, Value: "http://www.w3.org/2000/01/rdf-schema#label"},
		{Kind: model.Literal, Value: "Cat", Lang: "en"},
	}
	w.AddTerms(terms)
	w.AddQuads([]model.Quad{{S: 0, P: 1, O: 2}})

	data := w.ToBytes()
	g := reader.Read(data, true, nil)
	if len(g.Diagnostics) > 0 {
		t.Fatalf("unexpected diagnostics: %v", g.Diagnostics)
	}
	if len(g.Terms) != 3 {
		t.Fatalf("expected 3 terms, got %d", len(g.Terms))
	}
	if len(g.Quads) != 1 {
		t.Fatalf("expected 1 quad, got %d", len(g.Quads))
	}
	if g.SegmentProfiles[0] != "generic" {
		t.Fatalf("expected profile generic, got %q", g.SegmentProfiles[0])
	}
}

func TestBlobDedupInWriter(t *testing.T) {
	w := New("files")
	terms := []model.Term{
		{Kind: model.Iri, Value: "https://w3id.org/gts/files#FileEntry"},
		{Kind: model.Iri, Value: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"},
		{Kind: model.Bnode, Value: "e0"},
		{Kind: model.Literal, Value: "a.txt"},
	}
	w.AddTerms(terms)
	w.AddQuads([]model.Quad{{S: 2, P: 1, O: 0}})
	payload := []byte("shared")
	w.AddBlob(payload, "text/plain", "")
	w.AddBlob(payload, "text/plain", "")

	data := w.ToBytes()
	g := reader.Read(data, true, nil)
	if len(g.Blobs) != 1 {
		t.Fatalf("expected one blob after dedup in writer, got %d", len(g.Blobs))
	}
}
