// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package codec provides the GTS transform catalog decoder (§8).
package codec

import (
	"bytes"
	"fmt"
	"io"

	"github.com/klauspost/compress/gzip"
	"github.com/klauspost/compress/zstd"
)

// Codec is a catalog entry (§5, §8.5).
type Codec struct {
	Name string
	// "encode" | "compress" | "encrypt"
	Cls string
}

// CodecError describes why a transform chain could not be reversed.
type CodecError struct {
	Reason string // "unknown-codec" | "missing-key"
	Detail string
	Failed bool // true if codec is known but data is corrupt
}

func (e *CodecError) Error() string {
	return e.Detail
}

func decodeOne(codec *Codec, data []byte) ([]byte, error) {
	if codec.Cls == "encrypt" {
		return nil, &CodecError{
			Reason: "missing-key",
			Detail: fmt.Sprintf("no key for encrypt codec '%s'", codec.Name),
		}
	}
	switch codec.Name {
	case "identity":
		return data, nil
	case "gzip":
		r, err := gzip.NewReader(bytes.NewReader(data))
		if err != nil {
			return nil, &CodecError{Failed: true, Detail: fmt.Sprintf("gzip decode failed: %v", err)}
		}
		out, err := io.ReadAll(r)
		_ = r.Close()
		if err != nil {
			return nil, &CodecError{Failed: true, Detail: fmt.Sprintf("gzip decode failed: %v", err)}
		}
		return out, nil
	case "zstd":
		r, err := zstd.NewReader(nil)
		if err != nil {
			return nil, &CodecError{Failed: true, Detail: fmt.Sprintf("zstd decoder init failed: %v", err)}
		}
		defer r.Close()
		out, err := r.DecodeAll(data, nil)
		if err != nil {
			return nil, &CodecError{Failed: true, Detail: fmt.Sprintf("zstd decode failed: %v", err)}
		}
		return out, nil
	default:
		return nil, &CodecError{
			Reason: "unknown-codec",
			Detail: fmt.Sprintf("unknown codec '%s'", codec.Name),
		}
	}
}

// DecodeChain reverses a resolved codec chain, last to first (§6.1, §8.2).
//
// The baseline carries no keys, so every encrypt-class codec degrades to
// missing-key (matching the Python reader with keys=None).
func DecodeChain(chain []*Codec, data []byte) ([]byte, error) {
	current := data
	for i := len(chain) - 1; i >= 0; i-- {
		var err error
		current, err = decodeOne(chain[i], current)
		if err != nil {
			return nil, err
		}
	}
	return current, nil
}
