// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

package files

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"go.blackcatinformatics.ca/gts/model"
	"go.blackcatinformatics.ca/gts/reader"
	"go.blackcatinformatics.ca/gts/writer"
)

func makeTree(t *testing.T, root string) {
	t.Helper()
	if err := os.MkdirAll(filepath.Join(root, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "a.txt"), []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(root, "subdir", "b.txt"), []byte("world"), 0o644); err != nil {
		t.Fatal(err)
	}
}

func TestPackUnpackRoundTripBitForBit(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src")
	dst := filepath.Join(tmp, "dst")
	makeTree(t, src)

	archive, err := Pack([]string{src})
	if err != nil {
		t.Fatalf("pack failed: %v", err)
	}
	g := reader.Read(archive, true, nil)
	if len(g.Diagnostics) > 0 {
		t.Fatalf("archive diagnostics: %v", g.Diagnostics)
	}
	if err := Unpack(g, dst, false); err != nil {
		t.Fatalf("unpack failed: %v", err)
	}
	if got, want := readFile(t, filepath.Join(dst, "a.txt")), "hello"; got != want {
		t.Fatalf("a.txt: got %q want %q", got, want)
	}
	if got, want := readFile(t, filepath.Join(dst, "subdir", "b.txt")), "world"; got != want {
		t.Fatalf("subdir/b.txt: got %q want %q", got, want)
	}

	archive2, err := Pack([]string{dst})
	if err != nil {
		t.Fatalf("re-pack failed: %v", err)
	}
	if string(archive) != string(archive2) {
		t.Fatalf("re-packed archive differs")
	}
}

func TestPackDeduplicatesIdenticalContent(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src")
	if err := os.MkdirAll(src, 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "a.txt"), []byte("shared"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "b.txt"), []byte("shared"), 0o644); err != nil {
		t.Fatal(err)
	}

	archive, err := Pack([]string{src})
	if err != nil {
		t.Fatalf("pack failed: %v", err)
	}
	g := reader.Read(archive, true, nil)
	if len(g.Blobs) != 1 {
		t.Fatalf("expected one blob for identical content, got %d", len(g.Blobs))
	}
}

func TestUnpackRefusesTraversal(t *testing.T) {
	tmp := t.TempDir()
	archive := filepath.Join(tmp, "traversal.gts")

	payload := []byte("traversal-test")
	digest := writer.DigestString(payload)

	w := writer.New("files")
	w.AddTerms([]model.Term{
		{Kind: model.Iri, Value: "https://w3id.org/gts/files#FileEntry"},
		{Kind: model.Iri, Value: "https://w3id.org/gts/files#path"},
		{Kind: model.Iri, Value: "https://w3id.org/gts/files#digest"},
		{Kind: model.Iri, Value: "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"},
		{Kind: model.Bnode, Value: "e0"},
		{Kind: model.Literal, Value: "../escape.txt"},
		{Kind: model.Literal, Value: digest},
	})
	w.AddQuads([]model.Quad{{S: 4, P: 3, O: 0}, {S: 4, P: 1, O: 5}, {S: 4, P: 2, O: 6}})
	w.AddBlob(payload, "", "")
	if err := os.WriteFile(archive, w.ToBytes(), 0o644); err != nil {
		t.Fatal(err)
	}

	data, err := os.ReadFile(archive)
	if err != nil {
		t.Fatal(err)
	}
	g := reader.Read(data, true, nil)
	if err := Unpack(g, filepath.Join(tmp, "dst"), false); err == nil {
		t.Fatal("expected traversal refusal")
	} else if !contains(err.Error(), "traversal") && !contains(err.Error(), "escapes") {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestDiffReportsChanges(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src")
	makeTree(t, src)

	archive, err := Pack([]string{src})
	if err != nil {
		t.Fatalf("pack failed: %v", err)
	}
	g := reader.Read(archive, true, nil)

	lines, err := Diff(g, src)
	if err != nil {
		t.Fatalf("diff failed: %v", err)
	}
	if len(lines) != 0 {
		t.Fatalf("expected no changes, got %v", lines)
	}

	if err := os.WriteFile(filepath.Join(src, "a.txt"), []byte("changed"), 0o644); err != nil {
		t.Fatal(err)
	}
	lines, err = Diff(g, src)
	if err != nil {
		t.Fatalf("diff failed: %v", err)
	}
	if len(lines) != 1 || lines[0] != "modified: a.txt" {
		t.Fatalf("expected modified: a.txt, got %v", lines)
	}
}

func readFile(t *testing.T, path string) string {
	t.Helper()
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	return string(b)
}

func contains(s, substr string) bool {
	return strings.Contains(s, substr)
}
