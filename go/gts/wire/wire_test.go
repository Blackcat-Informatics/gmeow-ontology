// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

package wire

import (
	"encoding/hex"
	"testing"
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
	// Canonical key order in encoded bytes: "t", "v", "id".
	const wantHex = "a36174657465726d7361760162696443010203"
	if got := hex.EncodeToString(b); got != wantHex {
		t.Fatalf("unexpected canonical bytes: got %s want %s", got, wantHex)
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
