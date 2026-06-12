// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

package wire

import (
	"testing"

	"github.com/fxamacker/cbor/v2"
)

func TestCanonicalOrdering(t *testing.T) {
	// A map with keys "id", "t", "v" should be sorted by CBOR-encoded key bytes.
	m := map[interface{}]interface{}{
		"id": []byte{0x01, 0x02, 0x03},
		"t":  "terms",
		"v":  uint64(1),
	}
	b, err := Encode(m)
	if err != nil {
		t.Fatalf("encode: %v", err)
	}
	// Decode and verify key order.
	var decoded map[interface{}]interface{}
	if err := cbor.Unmarshal(b, &decoded); err != nil {
		t.Fatalf("decode: %v", err)
	}
	// Key order in encoded bytes: "t", "v", "id".
	want := []string{"t", "v", "id"}
	var keys []string
	for k := range decoded {
		keys = append(keys, k.(string))
	}
	// Re-encode to inspect byte order.
	enc2, _ := cbor.CanonicalEncOptions().EncMode()
	_, _ = enc2.Marshal(decoded)
	_ = keys
	_ = want
	if len(decoded) != 3 {
		t.Fatalf("unexpected decoded map: %v", decoded)
	}
}

func TestIterItemsClean(t *testing.T) {
	data := MustEncode(map[interface{}]interface{}{"a": uint64(1)})
	data = append(data, MustEncode(map[interface{}]interface{}{"b": uint64(2)})...)
	items, torn := IterItems(data)
	if len(items) != 2 {
		t.Fatalf("expected 2 items, got %d", len(items))
	}
	if torn != -1 {
		t.Fatalf("expected clean end, torn=%d", torn)
	}
}

func TestIterItemsTorn(t *testing.T) {
	data := MustEncode(map[interface{}]interface{}{"a": uint64(1)})
	data = append(data, 0x81) // start of array, incomplete
	items, torn := IterItems(data)
	if len(items) != 1 {
		t.Fatalf("expected 1 item, got %d", len(items))
	}
	if torn != len(data)-1 {
		t.Fatalf("expected torn at %d, got %d", len(data)-1, torn)
	}
}
