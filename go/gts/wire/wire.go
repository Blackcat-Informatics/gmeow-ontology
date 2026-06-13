// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package wire provides deterministic CBOR encoding, BLAKE3 content-ids, and
// CBOR-Sequence iteration for GTS.
package wire

import (
	"encoding/hex"
	"fmt"
	"io"
	"math"

	"github.com/fxamacker/cbor/v2"
	"github.com/zeebo/blake3"
)

// SelfDescribeTag is the CBOR self-describe tag (RFC 8949 §3.4.6); MAY prefix the Header item (§3).
const SelfDescribeTag uint64 = 55799

// Magic and Version identify a GTS1 CBOR Sequence (§3).
const (
	Magic   = "GTS1"
	Version = 1
)

var encMode cbor.EncMode

func init() {
	mode, err := cbor.CanonicalEncOptions().EncMode()
	if err != nil {
		panic(err)
	}
	encMode = mode
}

// Encode returns deterministic CBOR bytes for v (RFC 8949 §4.2).
func Encode(v interface{}) ([]byte, error) {
	return encMode.Marshal(v)
}

// MustEncode panics on encoding error; safe for values built from known types.
func MustEncode(v interface{}) []byte {
	b, err := Encode(v)
	if err != nil {
		panic(err)
	}
	return b
}

// Blake3_256 returns the 32-byte BLAKE3-256 digest of data.
func Blake3_256(data []byte) []byte {
	h := blake3.Sum256(data)
	return h[:]
}

// Hex returns lowercase hex of a byte string.
func Hex(data []byte) string {
	return hex.EncodeToString(data)
}

// DigestStr returns a "blake3:<hex>" content digest for inline blob addressing (§12).
func DigestStr(data []byte) string {
	return "blake3:" + Hex(Blake3_256(data))
}

// MapGet looks up a text key in a decoded CBOR map (first match).
func MapGet(m map[interface{}]interface{}, key string) (interface{}, bool) {
	for k, v := range m {
		if s, ok := k.(string); ok && s == key {
			return v, true
		}
	}
	return nil, false
}

// AsText coerces a decoded CBOR value to a string.
func AsText(v interface{}) (string, bool) {
	s, ok := v.(string)
	return s, ok
}

// AsBytes coerces a decoded CBOR value to a byte string.
func AsBytes(v interface{}) ([]byte, bool) {
	b, ok := v.([]byte)
	return b, ok
}

// AsInt coerces a decoded CBOR value to a non-negative int.
func AsInt(v interface{}) (int, bool) {
	switch n := v.(type) {
	case uint64:
		max := uint64(int(^uint(0) >> 1))
		if n <= max {
			return int(n), true
		}
	case int64:
		max := int64(int(^uint(0) >> 1))
		if n >= 0 && n <= max {
			return int(n), true
		}
	case int:
		if n >= 0 {
			return n, true
		}
	}
	return 0, false
}

// AsInt64 coerces a decoded CBOR value to an int64.
func AsInt64(v interface{}) (int64, bool) {
	switch n := v.(type) {
	case uint64:
		if n > math.MaxInt64 {
			return 0, false
		}
		return int64(n), true
	case int64:
		return n, true
	case int:
		return int64(n), true
	}
	return 0, false
}

// TextOr returns a text value or a default.
func TextOr(v interface{}, def string) string {
	if s, ok := AsText(v); ok {
		return s
	}
	return def
}

// cloneMap returns a shallow copy of m.
func cloneMap(m map[interface{}]interface{}) map[interface{}]interface{} {
	out := make(map[interface{}]interface{}, len(m))
	for k, v := range m {
		out[k] = v
	}
	return out
}

// hashExcluding computes the BLAKE3 digest of m after removing excluded keys.
func hashExcluding(m map[interface{}]interface{}, excluded []string) []byte {
	content := cloneMap(m)
	for _, k := range excluded {
		delete(content, k)
	}
	return Blake3_256(MustEncode(content))
}

// ContentID computes a frame's "id" over its content, excluding "id" and "sig".
func ContentID(frame map[interface{}]interface{}) []byte {
	return hashExcluding(frame, []string{"id", "sig"})
}

// HeaderID computes the Header's genesis "id", excluding only "id".
func HeaderID(header map[interface{}]interface{}) []byte {
	return hashExcluding(header, []string{"id"})
}

// cborItemLength returns the byte length of the next well-formed CBOR item at
// data[offset], or an error if the item is incomplete or uses unsupported encoding.
// It handles definite-length items; indefinite-length items are accepted only if
// complete and well-formed.
func cborItemLength(data []byte, offset int) (int, error) {
	if offset >= len(data) {
		return 0, io.EOF
	}
	start := offset

	// stack holds open containers. remaining is the number of child items still
	// expected. We only decrement after fully reading a child item.
	var stack []struct {
		major     byte
		remaining int64
	}

	// complete pops completed containers after a child item is finished.
	complete := func() {
		for len(stack) > 0 {
			top := &stack[len(stack)-1]
			if top.remaining > 0 {
				top.remaining--
			}
			if top.remaining == 0 {
				stack = stack[:len(stack)-1]
				continue
			}
			break
		}
	}

	for {
		if offset >= len(data) {
			return 0, io.ErrUnexpectedEOF
		}
		b := data[offset]
		major := b >> 5
		info := b & 0x1f
		offset++

		var extra int
		var length int64 = -1
		switch {
		case info <= 23:
			length = int64(info)
		case info == 24:
			extra = 1
		case info == 25:
			extra = 2
		case info == 26:
			extra = 4
		case info == 27:
			extra = 8
		case info >= 28 && info <= 30:
			return 0, fmt.Errorf("reserved additional info %d", info)
		case info == 31:
			// Indefinite length.
			switch major {
			case 2, 3:
				// Indefinite byte/text string: scan definite-length chunks until break.
				for {
					if offset >= len(data) {
						return 0, io.ErrUnexpectedEOF
					}
					nb := data[offset]
					if nb == 0xff {
						offset++
						break
					}
					nmajor := nb >> 5
					ninfo := nb & 0x1f
					if nmajor != major || ninfo == 31 {
						return 0, fmt.Errorf("invalid indefinite string chunk")
					}
					var nlen int64
					nextra := 0
					if ninfo <= 23 {
						nlen = int64(ninfo)
					} else {
						var err error
						nlen, nextra, err = readLength(data, offset, ninfo)
						if err != nil {
							return 0, err
						}
					}
					offset += nextra
					if int64(len(data)-offset) < nlen {
						return 0, io.ErrUnexpectedEOF
					}
					offset += int(nlen)
				}
			case 4, 5:
				// Indefinite array/map: not expected in canonical GTS; treat as error.
				return 0, fmt.Errorf("indefinite-length %s not supported", map[bool]string{true: "map", false: "array"}[major == 5])
			default:
				return 0, fmt.Errorf("indefinite length for major type %d", major)
			}
		}

		if length < 0 && extra > 0 {
			var err error
			length, extra, err = readLength(data, offset, info)
			if err != nil {
				return 0, err
			}
			offset += extra
		}

		switch major {
		case 0, 1, 7:
			// Primitive: nothing more to read.
			complete()
		case 2, 3:
			if int64(len(data)-offset) < length {
				return 0, io.ErrUnexpectedEOF
			}
			offset += int(length)
			complete()
		case 4:
			if length == 0 {
				complete()
			} else {
				stack = append(stack, struct {
					major     byte
					remaining int64
				}{major: major, remaining: length})
			}
		case 5:
			if length == 0 {
				complete()
			} else {
				stack = append(stack, struct {
					major     byte
					remaining int64
				}{major: major, remaining: length * 2})
			}
		case 6:
			// Tag: the tagged value follows.
			stack = append(stack, struct {
				major     byte
				remaining int64
			}{major: major, remaining: 1})
		}

		if len(stack) == 0 {
			return offset - start, nil
		}
	}
}

// readLength decodes the additional-info length for a CBOR head byte.
func readLength(data []byte, offset int, info byte) (int64, int, error) {
	switch info {
	case 24:
		if offset >= len(data) {
			return 0, 0, io.ErrUnexpectedEOF
		}
		return int64(data[offset]), 1, nil
	case 25:
		if offset+2 > len(data) {
			return 0, 0, io.ErrUnexpectedEOF
		}
		return int64(uint16(data[offset])<<8 | uint16(data[offset+1])), 2, nil
	case 26:
		if offset+4 > len(data) {
			return 0, 0, io.ErrUnexpectedEOF
		}
		return int64(uint32(data[offset])<<24 | uint32(data[offset+1])<<16 | uint32(data[offset+2])<<8 | uint32(data[offset+3])), 4, nil
	case 27:
		if offset+8 > len(data) {
			return 0, 0, io.ErrUnexpectedEOF
		}
		var n uint64
		for i := 0; i < 8; i++ {
			n = n<<8 | uint64(data[offset+i])
		}
		if n > math.MaxInt64 {
			return 0, 0, fmt.Errorf("length exceeds int64 max")
		}
		return int64(n), 8, nil
	}
	return 0, 0, fmt.Errorf("unsupported additional info for length: %d", info)
}

// IterItems decodes a CBOR Sequence into (byte_offset, item) pairs plus a torn marker.
//
// Detects a torn append by position: a decode failure at an item boundary is a torn
// trailing item. Returns the intact prefix and the torn offset (or -1 for clean end).
func IterItems(data []byte) ([]struct {
	Offset int
	Item   interface{}
}, int) {
	var out []struct {
		Offset int
		Item   interface{}
	}
	torn := -1
	offset := 0
	for offset < len(data) {
		start := offset
		length, err := cborItemLength(data, offset)
		if err != nil {
			torn = start
			break
		}
		end := offset + length
		var item interface{}
		if err := cbor.Unmarshal(data[offset:end], &item); err != nil {
			torn = start
			break
		}
		out = append(out, struct {
			Offset int
			Item   interface{}
		}{Offset: start, Item: item})
		offset = end
	}
	return out, torn
}

// UnwrapHeader returns the Header map, unwrapping the optional self-describe tag (§3).
func UnwrapHeader(item interface{}) (map[interface{}]interface{}, error) {
	inner := item
	if tag, ok := item.(cbor.Tag); ok {
		if tag.Number != SelfDescribeTag {
			return nil, fmt.Errorf("unexpected CBOR tag %d on the header item", tag.Number)
		}
		inner = tag.Content
	}
	m, ok := inner.(map[interface{}]interface{})
	if !ok {
		return nil, fmt.Errorf("header item is not a CBOR map")
	}
	return m, nil
}
