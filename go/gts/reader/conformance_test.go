// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

package reader

import (
	"encoding/json"
	"os"
	"path/filepath"
	"sort"
	"strings"
	"testing"

	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/model"
	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/nquads"
	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/wire"
)

func vectorsDir(t *testing.T) string {
	dir, err := filepath.Abs("../../../generated/gts-vectors")
	if err != nil {
		t.Fatal(err)
	}
	return dir
}

func summarize(g *model.Graph, mode string) map[string]interface{} {
	lines := strings.Split(strings.TrimSuffix(nquads.ToNQuads(g), "\n"), "\n")
	if len(lines) == 1 && lines[0] == "" {
		lines = []string{}
	}
	sort.Strings(lines)
	opaqueReasons := []string{}
	for _, o := range g.Opaque {
		opaqueReasons = append(opaqueReasons, o.Reason)
	}
	sort.Strings(opaqueReasons)

	blobs := map[string]interface{}{}
	for _, b := range g.Blobs {
		mt := ""
		for _, bm := range g.BlobMeta {
			if bm.Digest == b.Digest {
				if m, ok := bm.Meta.(map[interface{}]interface{}); ok {
					if v, ok := m["mt"]; ok {
						if s, ok := v.(string); ok {
							mt = s
						}
					}
				}
			}
		}
		blobs[b.Digest] = map[string]interface{}{"size": len(b.Data), "mt": mt}
	}

	heads := []string{}
	for _, h := range g.SegmentHeads {
		heads = append(heads, wire.Hex(h))
	}

	// Per-segment layout state (§3.3) — pins the streamable claim, its
	// covered boundary, and the accretive tail across implementations.
	streamable := []map[string]interface{}{}
	for _, s := range g.SegmentStreamable {
		streamable = append(streamable, map[string]interface{}{
			"claimed": s.Claimed,
			"covered": s.Covered,
			"tail":    s.Tail,
		})
	}

	diags := []string{}
	for _, d := range g.Diagnostics {
		diags = append(diags, d.Code)
	}

	return map[string]interface{}{
		"mode":           mode,
		"diagnostics":    diags,
		"terms":          len(g.Terms),
		"quads":          len(g.Quads),
		"segments":       len(g.SegmentHeads),
		"segment_heads":  heads,
		"profiles":       g.SegmentProfiles,
		"streamable":     streamable,
		"opaque_reasons": opaqueReasons,
		"suppressions":   len(g.Suppressions),
		"blobs":          blobs,
		"nquads":         lines,
	}
}

func TestCorpus(t *testing.T) {
	dir := vectorsDir(t)
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for _, e := range entries {
		if ext := filepath.Ext(e.Name()); ext == ".gts" {
			names = append(names, e.Name()[:len(e.Name())-4])
		}
	}
	if len(names) < 16 {
		t.Fatalf("corpus too small: %d vectors", len(names))
	}
	for _, name := range names {
		t.Run(name, func(t *testing.T) {
			data, err := os.ReadFile(filepath.Join(dir, name+".gts"))
			if err != nil {
				t.Fatal(err)
			}
			expectedRaw, err := os.ReadFile(filepath.Join(dir, name+".expected.json"))
			if err != nil {
				t.Fatal(err)
			}
			var expected map[string]interface{}
			if err := json.Unmarshal(expectedRaw, &expected); err != nil {
				t.Fatal(err)
			}
			mode, _ := expected["mode"].(string)
			g := Read(data, mode != "pre-segment", nil)
			actual := summarize(g, mode)
			actualJSON, _ := json.Marshal(actual)
			expectedJSON, _ := json.Marshal(expected)
			if string(actualJSON) != string(expectedJSON) {
				t.Fatalf("divergence\nactual:   %s\nexpected: %s", actualJSON, expectedJSON)
			}
		})
	}
}
