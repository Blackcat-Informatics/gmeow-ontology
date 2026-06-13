// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

package compact

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"

	"go.blackcatinformatics.ca/gts/model"
	"go.blackcatinformatics.ca/gts/reader"
	"go.blackcatinformatics.ca/gts/wire"
	"go.blackcatinformatics.ca/gts/writer"
)

func vector(t *testing.T, name string) []byte {
	t.Helper()
	dir, err := filepath.Abs("../../../generated/gts-vectors")
	if err != nil {
		t.Fatal(err)
	}
	//nolint:gosec // test reads a frozen conformance vector by caller-supplied name.
	data, err := os.ReadFile(filepath.Join(dir, name))
	if err != nil {
		t.Fatal(err)
	}
	return data
}

// TestCompactReproducesFrozenVector is the cross-engine determinism oracle
// (§14.1): compacting the frozen vector 25 bytes with the frozen timestamp
// MUST reproduce the frozen vector 25b bytes exactly.
func TestCompactReproducesFrozenVector(t *testing.T) {
	source := vector(t, "25-streamable-source.gts")
	expected := vector(t, "25b-streamable-compacted.gts")
	got, err := Streamable(source, "2026-01-01T00:00:00Z", false)
	if err != nil {
		t.Fatalf("compact refused: %v", err)
	}
	if !bytes.Equal(got, expected) {
		t.Fatalf("compacted bytes diverge from the frozen oracle: got %d bytes, want %d bytes", len(got), len(expected))
	}
}

func TestCompactOutputVerifiesStreamable(t *testing.T) {
	source := vector(t, "25-streamable-source.gts")
	got, err := Streamable(source, "2026-01-01T00:00:00Z", false)
	if err != nil {
		t.Fatalf("compact refused: %v", err)
	}
	g := reader.Read(got, true, nil)
	if len(g.Diagnostics) > 0 {
		t.Fatalf("compacted output has diagnostics: %v", g.Diagnostics)
	}
	if len(g.SegmentStreamable) != 1 {
		t.Fatalf("expected one segment, got %d", len(g.SegmentStreamable))
	}
	info := g.SegmentStreamable[0]
	if !info.Claimed || info.Tail != 0 || info.Covered == 0 {
		t.Fatalf("unexpected layout state: %+v", info)
	}
}

func TestCompactSealOriginalCarriesSourceBlob(t *testing.T) {
	source := vector(t, "25-streamable-source.gts")
	got, err := Streamable(source, "2026-01-01T00:00:00Z", true)
	if err != nil {
		t.Fatalf("compact refused: %v", err)
	}
	g := reader.Read(got, true, nil)
	if len(g.Diagnostics) > 0 {
		t.Fatalf("sealed output has diagnostics: %v", g.Diagnostics)
	}
	sealed := wire.DigestStr(source)
	data := blobBytes(g, sealed)
	if !bytes.Equal(data, source) {
		t.Fatalf("sealed blob does not carry the verbatim source bytes")
	}
	if mt, _ := blobMetaString(g, sealed, "mt"); mt != "application/gts" {
		t.Fatalf("sealed blob media type: %q", mt)
	}
	if rep, _ := blobMetaString(g, sealed, "rep"); rep != "source" {
		t.Fatalf("sealed blob rep: %q", rep)
	}
}

func TestCompactRefusesEvidenceWithoutSeal(t *testing.T) {
	w := writer.New("evidence")
	w.AddTerms([]model.Term{
		{Kind: model.Iri, Value: "https://example.org/Cat"},
		{Kind: model.Iri, Value: "http://www.w3.org/2000/01/rdf-schema#label"},
		{Kind: model.Literal, Value: "Cat", Lang: "en"},
	})
	w.AddQuads([]model.Quad{{S: 0, P: 1, O: 2}})
	data := w.ToBytes()

	if _, err := Streamable(data, "2026-01-01T00:00:00Z", false); err == nil {
		t.Fatal("expected a refusal for evidence input without seal-original")
	}
	if _, err := Streamable(data, "2026-01-01T00:00:00Z", true); err != nil {
		t.Fatalf("seal-original should permit evidence compaction: %v", err)
	}
}

func TestCompactRefusesFrameSuppression(t *testing.T) {
	w := writer.New("generic")
	w.AddTerms([]model.Term{
		{Kind: model.Iri, Value: "https://example.org/Cat"},
		{Kind: model.Iri, Value: "http://www.w3.org/2000/01/rdf-schema#label"},
		{Kind: model.Literal, Value: "Cat", Lang: "en"},
	})
	w.AddQuads([]model.Quad{{S: 0, P: 1, O: 2}})
	w.AddSuppress([]interface{}{
		map[interface{}]interface{}{"kind": "frame", "digest": "blake3:00"},
	}, "", nil)
	data := w.ToBytes()

	_, err := Streamable(data, "2026-01-01T00:00:00Z", false)
	if err == nil {
		t.Fatal("expected a refusal for a frame-addressed suppression")
	}
	if _, ok := err.(*RefusedError); !ok {
		t.Fatalf("expected *RefusedError, got %T", err)
	}
}
