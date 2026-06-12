// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Package files implements the GTS files-profile pack/unpack/diff logic
// (§13.2, §14.2).
package files

import (
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"sort"
	"strconv"
	"strings"
	"time"

	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/model"
	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/wire"
	"github.com/Blackcat-Informatics/gmeow-ontology/go/gts/writer"
)

const (
	filesNS     = "https://w3id.org/gts/files#"
	rdfType     = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
	xsdInteger  = "http://www.w3.org/2001/XMLSchema#integer"
	xsdDateTime = "http://www.w3.org/2001/XMLSchema#dateTime"
)

func iriTerm(value string) model.Term {
	return model.Term{Kind: model.Iri, Value: value}
}

func literalTerm(value string, datatype *int) model.Term {
	return model.Term{Kind: model.Literal, Value: value, Datatype: datatype}
}

func bnodeTerm(label string) model.Term {
	return model.Term{Kind: model.Bnode, Value: label}
}

func safeArchivePath(name string) error {
	if name == "" {
		return fmt.Errorf("empty archive path")
	}
	if strings.HasPrefix(name, "/") {
		return fmt.Errorf("absolute path not allowed in archive: %s", name)
	}
	for _, part := range strings.Split(name, "/") {
		if part == ".." {
			return fmt.Errorf("path traversal not allowed in archive: %s", name)
		}
	}
	return nil
}

func walkDirSorted(dir string) ([]string, error) {
	var out []string
	err := filepath.WalkDir(dir, func(path string, d fs.DirEntry, err error) error {
		if err != nil {
			return err
		}
		if d.Type()&fs.ModeSymlink != 0 {
			return fmt.Errorf("symlink not supported: %s", path)
		}
		if !d.IsDir() {
			out = append(out, path)
		}
		return nil
	})
	if err != nil {
		return nil, fmt.Errorf("walk %s: %w", dir, err)
	}
	sort.Strings(out)
	return out, nil
}

func resolveSources(sources []string) ([][2]string, error) {
	var entries [][2]string
	seen := make(map[string]struct{})
	for _, src := range sources {
		info, err := os.Stat(src)
		if err != nil {
			return nil, fmt.Errorf("%s: %w", src, err)
		}
		if info.IsDir() {
			files, err := walkDirSorted(src)
			if err != nil {
				return nil, err
			}
			for _, fpath := range files {
				rel, err := filepath.Rel(src, fpath)
				if err != nil {
					return nil, fmt.Errorf("path outside source: %s", fpath)
				}
				relpath := filepath.ToSlash(rel)
				if err := safeArchivePath(relpath); err != nil {
					return nil, err
				}
				if _, ok := seen[relpath]; ok {
					return nil, fmt.Errorf("duplicate archive path: %s", relpath)
				}
				seen[relpath] = struct{}{}
				entries = append(entries, [2]string{fpath, relpath})
			}
		} else {
			name := filepath.Base(src)
			if err := safeArchivePath(name); err != nil {
				return nil, err
			}
			if _, ok := seen[name]; ok {
				return nil, fmt.Errorf("duplicate archive path: %s", name)
			}
			seen[name] = struct{}{}
			entries = append(entries, [2]string{src, name})
		}
	}
	sort.Slice(entries, func(i, j int) bool { return entries[i][1] < entries[j][1] })
	return entries, nil
}

func guessMediaType(path string) string {
	switch strings.ToLower(filepath.Ext(path)) {
	case ".txt":
		return "text/plain"
	case ".html", ".htm":
		return "text/html"
	case ".json":
		return "application/json"
	case ".xml":
		return "application/xml"
	case ".png":
		return "image/png"
	case ".jpg", ".jpeg":
		return "image/jpeg"
	case ".gif":
		return "image/gif"
	case ".webp":
		return "image/webp"
	case ".pdf":
		return "application/pdf"
	case ".zip":
		return "application/zip"
	case ".gz":
		return "application/gzip"
	case ".tar":
		return "application/x-tar"
	default:
		return "application/octet-stream"
	}
}

// Pack files/directories into a deterministic GTS files-profile archive.
func Pack(sources []string) ([]byte, error) {
	w := writer.New("files")

	shared := []model.Term{
		iriTerm(filesNS + "FileEntry"),
		iriTerm(filesNS + "path"),
		iriTerm(filesNS + "digest"),
		iriTerm(filesNS + "size"),
		iriTerm(filesNS + "mode"),
		iriTerm(filesNS + "modified"),
		iriTerm(filesNS + "mediaType"),
		iriTerm(rdfType),
		iriTerm(xsdInteger),
		iriTerm(xsdDateTime),
	}
	w.AddTerms(shared)
	const (
		fileEntryID = 0
		pathID      = 1
		digestID    = 2
		sizeID      = 3
		modeID      = 4
		modifiedID  = 5
		mediaTypeID = 6
		typeID      = 7
	)
	xsdIntegerID := 8
	xsdDateTimeID := 9

	entries, err := resolveSources(sources)
	if err != nil {
		return nil, err
	}

	var fileTerms []model.Term
	var quads []model.Quad
	blobs := make(map[string]struct {
		data []byte
		mt   string
	})
	var blobOrder []string

	for idx, entry := range entries {
		fpath, relpath := entry[0], entry[1]
		data, err := os.ReadFile(fpath)
		if err != nil {
			return nil, fmt.Errorf("read %s: %w", fpath, err)
		}
		digest := writer.DigestString(data)
		info, err := os.Stat(fpath)
		if err != nil {
			return nil, fmt.Errorf("stat %s: %w", fpath, err)
		}
		size := info.Size()
		mode := uint32(info.Mode()) & 0o7777
		mtime, err := fileModTime(info)
		if err != nil {
			return nil, fmt.Errorf("mtime %s: %w", fpath, err)
		}
		mt := guessMediaType(fpath)

		entryLabel := fmt.Sprintf("f%d", idx)
		entryTerm := bnodeTerm(entryLabel)
		pathTerm := literalTerm(relpath, nil)
		digestTerm := literalTerm(digest, nil)
		sizeTerm := literalTerm(strconv.FormatInt(size, 10), &xsdIntegerID)
		modeTerm := literalTerm(strconv.FormatUint(uint64(mode), 8), &xsdIntegerID)
		modifiedTerm := literalTerm(formatDateTime(mtime), &xsdDateTimeID)
		mediaTerm := literalTerm(mt, nil)

		base := len(shared) + len(fileTerms)
		fileTerms = append(fileTerms,
			entryTerm,
			pathTerm,
			digestTerm,
			sizeTerm,
			modeTerm,
			modifiedTerm,
			mediaTerm,
		)
		entryID := base
		quads = append(quads,
			model.Quad{S: entryID, P: typeID, O: fileEntryID},
			model.Quad{S: entryID, P: pathID, O: base + 1},
			model.Quad{S: entryID, P: digestID, O: base + 2},
			model.Quad{S: entryID, P: sizeID, O: base + 3},
			model.Quad{S: entryID, P: modeID, O: base + 4},
			model.Quad{S: entryID, P: modifiedID, O: base + 5},
			model.Quad{S: entryID, P: mediaTypeID, O: base + 6},
		)
		if _, ok := blobs[digest]; !ok {
			blobs[digest] = struct {
				data []byte
				mt   string
			}{data: data, mt: mt}
			blobOrder = append(blobOrder, digest)
		}
	}

	if len(fileTerms) > 0 {
		w.AddTerms(fileTerms)
	}
	if len(quads) > 0 {
		w.AddQuads(quads)
	}

	for _, digest := range blobOrder {
		b := blobs[digest]
		w.AddBlob(b.data, b.mt)
	}

	return w.ToBytes(), nil
}

func fileModTime(info fs.FileInfo) (time.Time, error) {
	t := info.ModTime()
	if t.IsZero() {
		return time.Time{}, fmt.Errorf("no modification time available")
	}
	return t, nil
}

func formatDateTime(t time.Time) string {
	s := t.UTC().Format(time.RFC3339)
	return strings.Replace(s, "+00:00", "Z", 1)
}

func readFileEntries(g *model.Graph) (map[string]map[string]string, error) {
	var typeID, fileEntryID *int
	fieldIDs := make(map[string]int)
	for idx, term := range g.Terms {
		if term.Kind != model.Iri {
			continue
		}
		switch term.Value {
		case rdfType:
			i := idx
			typeID = &i
		case filesNS + "FileEntry":
			i := idx
			fileEntryID = &i
		default:
			if rest, ok := strings.CutPrefix(term.Value, filesNS); ok {
				fieldIDs[rest] = idx
			}
		}
	}
	if typeID == nil {
		return nil, fmt.Errorf("not a files-profile archive: missing rdf:type")
	}
	if fileEntryID == nil {
		return nil, fmt.Errorf("not a files-profile archive: missing FileEntry")
	}

	entries := make(map[int]map[string]string)
	fileEntrySubjects := make(map[int]struct{})
	for _, q := range g.Quads {
		if q.P == *typeID && q.O == *fileEntryID {
			fileEntrySubjects[q.S] = struct{}{}
			if _, ok := entries[q.S]; !ok {
				entries[q.S] = make(map[string]string)
			}
		} else {
			for name, id := range fieldIDs {
				if id == q.P {
					if q.O < 0 || q.O >= len(g.Terms) {
						return nil, fmt.Errorf("invalid term reference %d for files:%s", q.O, name)
					}
					if _, ok := entries[q.S]; !ok {
						entries[q.S] = make(map[string]string)
					}
					entries[q.S][name] = g.Terms[q.O].Value
				}
			}
		}
	}

	byPath := make(map[string]map[string]string)
	for s, entry := range entries {
		if _, ok := fileEntrySubjects[s]; !ok {
			continue
		}
		if path, ok := entry["path"]; ok {
			if _, exists := byPath[path]; exists {
				return nil, fmt.Errorf("duplicate files:path in archive: %s", path)
			}
			byPath[path] = entry
		}
	}
	return byPath, nil
}

func destPath(dest, archivePath string) (string, error) {
	if strings.HasPrefix(archivePath, "/") {
		return "", fmt.Errorf("absolute path in archive: %s", archivePath)
	}
	for _, part := range strings.Split(archivePath, "/") {
		if part == ".." {
			return "", fmt.Errorf("path traversal in archive: %s", archivePath)
		}
	}
	destAbs, err := filepath.Abs(dest)
	if err != nil {
		return "", fmt.Errorf("resolve destination: %w", err)
	}
	destCanon, err := filepath.EvalSymlinks(destAbs)
	if err != nil {
		destCanon = destAbs
	}
	target := filepath.Join(destCanon, filepath.FromSlash(archivePath))
	targetCanon, err := filepath.EvalSymlinks(target)
	if err != nil {
		targetCanon = target
	}
	prefix := filepath.Clean(destCanon) + string(os.PathSeparator)
	if !strings.HasPrefix(filepath.Clean(targetCanon)+string(os.PathSeparator), prefix) {
		return "", fmt.Errorf("path escapes destination: %s", archivePath)
	}
	return target, nil
}

func suppressedBlobDigests(g *model.Graph) map[string]struct{} {
	out := make(map[string]struct{})
	for _, sup := range g.Suppressions {
		for _, target := range sup.Targets {
			m, ok := target.(map[interface{}]interface{})
			if !ok {
				continue
			}
			kind := ""
			var digest *string
			for k, v := range m {
				key := wire.TextOr(k, "")
				if key == "kind" {
					kind = wire.TextOr(v, "")
				} else if key == "digest" {
					s := digestFromValue(v)
					digest = &s
				}
			}
			if kind == "blob" && digest != nil {
				out[*digest] = struct{}{}
			}
		}
	}
	return out
}

func digestFromValue(v interface{}) string {
	if s, ok := v.(string); ok {
		return normalizeDigest(s)
	}
	if b, ok := v.([]byte); ok {
		return "blake3:" + wire.Hex(b)
	}
	return ""
}

func normalizeDigest(digest string) string {
	if strings.HasPrefix(digest, "blake3:") {
		return digest
	}
	return "blake3:" + digest
}

// Unpack extracts FileEntry quads from a folded graph into dest.
func Unpack(g *model.Graph, dest string, includeSuppressed bool) error {
	entries, err := readFileEntries(g)
	if err != nil {
		return err
	}
	suppressed := make(map[string]struct{})
	if !includeSuppressed {
		suppressed = suppressedBlobDigests(g)
	}
	if err := os.MkdirAll(dest, 0o755); err != nil {
		return fmt.Errorf("create %s: %w", dest, err)
	}
	destAbs, err := filepath.Abs(dest)
	if err != nil {
		return fmt.Errorf("resolve destination: %w", err)
	}
	destCanon, err := filepath.EvalSymlinks(destAbs)
	if err != nil {
		destCanon = destAbs
	}
	prefix := filepath.Clean(destCanon) + string(os.PathSeparator)

	for path, entry := range entries {
		target, err := destPath(dest, path)
		if err != nil {
			return err
		}
		digest, ok := entry["digest"]
		if !ok {
			return fmt.Errorf("missing digest for %s", path)
		}
		if _, skip := suppressed[digest]; skip {
			continue
		}
		var data []byte
		for _, b := range g.Blobs {
			if b.Digest == digest {
				data = b.Data
				break
			}
		}
		if data == nil {
			return fmt.Errorf("missing inline blob for %s: %s", path, digest)
		}
		if writer.DigestString(data) != digest {
			return fmt.Errorf("integrity failure for %s: %s", path, digest)
		}

		if parent := filepath.Dir(target); parent != "" {
			if err := os.MkdirAll(parent, 0o755); err != nil {
				return fmt.Errorf("create dir %s: %w", parent, err)
			}
			parentCanon, err := filepath.EvalSymlinks(parent)
			if err != nil {
				parentCanon = parent
			}
			if !strings.HasPrefix(filepath.Clean(parentCanon)+string(os.PathSeparator), prefix) {
				return fmt.Errorf("path escapes destination: %s", path)
			}
		}
		if err := os.WriteFile(target, data, 0o644); err != nil {
			return fmt.Errorf("write %s: %w", target, err)
		}

		if modeStr, ok := entry["mode"]; ok {
			if m, err := strconv.ParseUint(modeStr, 8, 32); err == nil {
				_ = os.Chmod(target, os.FileMode(m))
			}
		}

		if modifiedStr, ok := entry["modified"]; ok {
			if ts, err := parseDateTime(modifiedStr); err == nil {
				mt := time.Unix(ts, 0)
				_ = os.Chtimes(target, mt, mt)
			}
		}
	}
	return nil
}

func parseDateTime(text string) (int64, error) {
	t, err := time.Parse(time.RFC3339, text)
	if err == nil {
		return t.Unix(), nil
	}
	t, err = time.Parse(time.RFC3339, text+"Z")
	if err == nil {
		return t.Unix(), nil
	}
	return 0, fmt.Errorf("parse datetime %s: %w", text, err)
}

// Diff compares an archive to a directory by content digest.
func Diff(g *model.Graph, directory string) ([]string, error) {
	entries, err := readFileEntries(g)
	if err != nil {
		return nil, err
	}
	archiveDigests := make(map[string]string)
	for path, entry := range entries {
		archiveDigests[path] = entry["digest"]
	}

	if _, err := os.Stat(directory); err != nil {
		return nil, fmt.Errorf("diff destination does not exist: %s", directory)
	}

	diskDigests := make(map[string]string)
	files, err := walkDirSorted(directory)
	if err != nil {
		return nil, err
	}
	for _, fpath := range files {
		rel, err := filepath.Rel(directory, fpath)
		if err != nil {
			return nil, fmt.Errorf("path outside directory: %s", fpath)
		}
		relpath := filepath.ToSlash(rel)
		data, err := os.ReadFile(fpath)
		if err != nil {
			return nil, fmt.Errorf("read %s: %w", fpath, err)
		}
		diskDigests[relpath] = writer.DigestString(data)
	}

	var lines []string
	for path := range archiveDigests {
		if _, ok := diskDigests[path]; !ok {
			lines = append(lines, fmt.Sprintf("removed: %s", path))
		}
	}
	for path := range diskDigests {
		if _, ok := archiveDigests[path]; !ok {
			lines = append(lines, fmt.Sprintf("added: %s", path))
		}
	}
	for path, ad := range archiveDigests {
		if dd, ok := diskDigests[path]; ok && ad != dd {
			lines = append(lines, fmt.Sprintf("modified: %s", path))
		}
	}
	sort.Strings(lines)
	return lines, nil
}
