// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

// Command gts inspects, folds, verifies, and composes GTS files.
//
// Exit codes: 0 clean; 1 diagnostics found or input refused; 2 usage/IO error.
package main

import (
	"fmt"
	"os"
	"strings"
	"time"

	"go.blackcatinformatics.ca/gts/compact"
	"go.blackcatinformatics.ca/gts/files"
	"go.blackcatinformatics.ca/gts/model"
	"go.blackcatinformatics.ca/gts/nquads"
	"go.blackcatinformatics.ca/gts/reader"
	"go.blackcatinformatics.ca/gts/stream"
	"go.blackcatinformatics.ca/gts/wire"
)

const usage = `usage: gts <command> [args]

commands:
  info <file>...            per-segment composition ledger
  fold <file>               fold to N-Quads on stdout
  verify <file>...          verify chains; ledger + diagnostics; exit 1 on any
  ls <file>                 list inline blobs: digest, size, declared media type
  extract <file> <digest> [-o out] [--mt TYPE] [--include-suppressed]
                            extract one blob by content digest
  cat -o <out> <file>...    validating composer: refuse degenerate inputs,
                            then byte-concatenate
  compact <file> -o <out> --streamable [--seal-original] [--timestamp ISO]
                            rewrite into the streamable layout state: leading
                            streaming index, blobs most-significant-first,
                            trailing index footer
  pack <dir|file>... -o out.gts
                            pack files/directories into a files-profile archive
  unpack <archive> [-C dir] [--include-suppressed]
                            unpack a files-profile archive
  diff <archive> <dir>      compare archive to directory by digest`

func main() {
	args := os.Args[1:]
	if len(args) == 0 {
		fmt.Fprintln(os.Stderr, usage)
		os.Exit(2)
	}
	cmd := args[0]
	switch cmd {
	case "info":
		os.Exit(cmdInfo(args[1:]))
	case "fold":
		os.Exit(cmdFold(args[1:]))
	case "verify":
		os.Exit(cmdVerify(args[1:]))
	case "ls":
		os.Exit(cmdLs(args[1:]))
	case "extract":
		os.Exit(cmdExtract(args[1:]))
	case "cat":
		os.Exit(cmdCat(args[1:]))
	case "compact":
		os.Exit(cmdCompact(args[1:]))
	case "pack":
		os.Exit(cmdPack(args[1:]))
	case "unpack":
		os.Exit(cmdUnpack(args[1:]))
	case "diff":
		os.Exit(cmdDiff(args[1:]))
	case "-h", "--help", "help":
		fmt.Println(usage)
		os.Exit(0)
	default:
		fmt.Fprintf(os.Stderr, "gts: unknown command '%s'\n%s\n", cmd, usage)
		os.Exit(2)
	}
}

func load(path string) ([]byte, int) {
	data, err := os.ReadFile(path)
	if err != nil {
		fmt.Fprintf(os.Stderr, "gts: cannot read %s: %v\n", path, err)
		return nil, 2
	}
	return data, 0
}

func printLedger(path string, fs *reader.FileSegments) {
	tornSuffix := ""
	if fs.Torn >= 0 {
		tornSuffix = fmt.Sprintf(", TORN at byte %d", fs.Torn)
	}
	fmt.Printf("%s: %d segment(s)%s\n", path, len(fs.Segments), tornSuffix)
	if fs.Fatal != nil {
		fmt.Printf("  FATAL %s: %s\n", fs.Fatal.Code, fs.Fatal.Detail)
		return
	}
	for idx, seg := range fs.Segments {
		head := "<none>"
		if len(seg.SegmentHeads) > 0 {
			head = wire.Hex(seg.SegmentHeads[0])
		}
		profile := "<none>"
		if len(seg.SegmentProfiles) > 0 {
			profile = seg.SegmentProfiles[0]
		}
		signers := 0
		for _, s := range seg.Signatures {
			if s.Status != "invalid" {
				signers++
			}
		}
		fmt.Printf(
			"  segment %d: head %s profile %s terms %d quads %d reifies %d annot %d blobs %d suppress %d opaque %d sigs %d\n",
			idx, head, profile,
			len(seg.Terms), len(seg.Quads), len(seg.Reifiers), len(seg.Annotations),
			len(seg.Blobs), len(seg.Suppressions), len(seg.Opaque), signers,
		)
		// Layout-state line (§3.3): declared streamable claims report their
		// covered boundary and any accretive tail.
		if len(seg.SegmentStreamable) > 0 && seg.SegmentStreamable[0].Claimed {
			layout := seg.SegmentStreamable[0]
			headHex := "<none>"
			if layout.Head != nil {
				headHex = wire.Hex(layout.Head)
			}
			tail := ""
			if layout.Tail > 0 {
				tail = fmt.Sprintf(", accretive tail %d frame(s)", layout.Tail)
			}
			fmt.Printf("    layout: streamable through frame %d (head %s)%s\n", layout.Covered, headHex, tail)
		}
		for _, o := range seg.Opaque {
			fmt.Printf("    opaque: %s (%s)\n", o.FrameType, o.Reason)
		}
		for _, d := range seg.Diagnostics {
			idxStr := ""
			if d.FrameIndex != nil {
				idxStr = fmt.Sprintf(" [item %d]", *d.FrameIndex)
			}
			fmt.Printf("    diagnostic %s: %s%s\n", d.Code, d.Detail, idxStr)
		}
	}
}

func hasProblems(fs *reader.FileSegments) bool {
	if fs.Fatal != nil || fs.Torn >= 0 {
		return true
	}
	for _, seg := range fs.Segments {
		if len(seg.Diagnostics) > 0 {
			return true
		}
	}
	return false
}

func cmdInfo(paths []string) int {
	if len(paths) == 0 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	problems := false
	for _, path := range paths {
		data, code := load(path)
		if code != 0 {
			return code
		}
		fs := reader.ReadFileSegments(data)
		printLedger(path, fs)
		if hasProblems(fs) {
			problems = true
		}
	}
	if problems {
		return 1
	}
	return 0
}

func cmdFold(paths []string) int {
	if len(paths) != 1 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	data, code := load(paths[0])
	if code != 0 {
		return code
	}
	g := reader.Read(data, true, nil)
	for _, d := range g.Diagnostics {
		fmt.Fprintf(os.Stderr, "gts: diagnostic %s: %s\n", d.Code, d.Detail)
	}
	fmt.Print(nquads.ToNQuads(g))
	if len(g.Diagnostics) > 0 || len(g.SegmentHeads) == 0 {
		return 1
	}
	return 0
}

// quadTermIDs returns every term position of a quad, including the graph
// slot when present (§14.1): a vocabulary IRI used only as a graph name still
// rots a declaration.
func quadTermIDs(q model.Quad) []int {
	if q.G != nil {
		return []int{q.S, q.P, q.O, *q.G}
	}
	return []int{q.S, q.P, q.O}
}

// profileVocabs maps profile names to the spec-owned vocabulary they imply.
var profileVocabs = map[string]string{
	"files": "https://w3id.org/gts/files#",
}

func namespaceOf(iri string) string {
	if i := strings.LastIndex(iri, "#"); i >= 0 {
		return iri[:i+1]
	}
	if i := strings.LastIndex(iri, "/"); i >= 0 {
		return iri[:i+1]
	}
	return iri
}

// profileCheck implements the §14.1 declared-vs-computed profile checks:
// vocabulary used without its profile declared is an error; a declared-but-
// unused profile is a warning. Returns (message, isError) pairs.
func profileCheck(seg *model.Graph) []struct {
	Msg   string
	IsErr bool
} {
	declared := make(map[string]struct{}, len(seg.SegmentProfiles))
	for _, p := range seg.SegmentProfiles {
		declared[p] = struct{}{}
	}
	used := make(map[string]struct{})
	for _, q := range seg.Quads {
		for _, tid := range quadTermIDs(q) {
			if tid < 0 || tid >= len(seg.Terms) {
				continue // never crash a report over a malformed reference
			}
			term := &seg.Terms[tid]
			if term.Kind != model.Iri || term.Value == "" {
				continue
			}
			ns := namespaceOf(term.Value)
			for _, vocab := range profileVocabs {
				if ns == vocab {
					used[ns] = struct{}{}
				}
			}
		}
	}
	var out []struct {
		Msg   string
		IsErr bool
	}
	for prof, vocab := range profileVocabs {
		_, declares := declared[prof]
		_, uses := used[vocab]
		if uses && !declares {
			out = append(out, struct {
				Msg   string
				IsErr bool
			}{fmt.Sprintf("profile error: segment uses %s vocabulary "+
				"but does not declare '%s'", vocab, prof), true})
		}
		if declares && !uses {
			out = append(out, struct {
				Msg   string
				IsErr bool
			}{fmt.Sprintf("profile warning: segment declares '%s' "+
				"but uses no %s vocabulary", prof, vocab), false})
		}
	}
	return out
}

// streamVocabCheck warns on stream# vocabulary in an unclaimed segment (§13.3).
//
// A warning, never an error: compaction-provenance quads legitimately survive
// nq → gts round trips and re-accretion — the error class is reserved for a
// claimed layout the bytes contradict (the reader's StreamableLayoutError).
func streamVocabCheck(seg *model.Graph) []string {
	claimed := len(seg.SegmentStreamable) > 0 && seg.SegmentStreamable[0].Claimed
	if claimed {
		return nil
	}
	for _, q := range seg.Quads {
		for _, tid := range quadTermIDs(q) {
			if tid < 0 || tid >= len(seg.Terms) {
				continue // never crash a report over a malformed reference
			}
			term := &seg.Terms[tid]
			if term.Kind == model.Iri && strings.HasPrefix(term.Value, stream.NS) {
				return []string{
					fmt.Sprintf("layout warning: segment uses %s vocabulary but does "+
						"not claim layout 'streamable' (§13.3)", stream.NS),
				}
			}
		}
	}
	return nil
}

func cmdVerify(paths []string) int {
	if len(paths) == 0 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	problems := false
	for _, path := range paths {
		data, code := load(path)
		if code != 0 {
			return code
		}
		fs := reader.ReadFileSegments(data)
		printLedger(path, fs)
		if hasProblems(fs) {
			problems = true
		}
		// §14.1: declared-vs-computed profile requirements + layout warnings.
		for idx, seg := range fs.Segments {
			for _, check := range profileCheck(seg) {
				prefix := "warning"
				if check.IsErr {
					prefix = "error"
					problems = true
				}
				fmt.Fprintf(os.Stderr, "  segment %d: %s: %s\n", idx, prefix, check.Msg)
			}
			for _, msg := range streamVocabCheck(seg) {
				fmt.Fprintf(os.Stderr, "  segment %d: warning: %s\n", idx, msg)
			}
		}
	}
	if problems {
		return 1
	}
	return 0
}

func blobMT(g *model.Graph, digest string) string {
	for _, bm := range g.BlobMeta {
		if bm.Digest != digest {
			continue
		}
		m, ok := bm.Meta.(map[interface{}]interface{})
		if !ok {
			continue
		}
		if v, ok := m["mt"]; ok {
			if s, ok := v.(string); ok {
				return s
			}
		}
	}
	return ""
}

func cmdLs(paths []string) int {
	if len(paths) != 1 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	data, code := load(paths[0])
	if code != 0 {
		return code
	}
	g := reader.Read(data, true, nil)
	for _, d := range g.Diagnostics {
		fmt.Fprintf(os.Stderr, "gts: diagnostic %s: %s\n", d.Code, d.Detail)
	}
	for _, b := range g.Blobs {
		mt := blobMT(g, b.Digest)
		if mt == "" {
			mt = "-"
		}
		fmt.Printf("%s  %10d  %s\n", b.Digest, len(b.Data), mt)
	}
	if len(g.Diagnostics) > 0 || len(g.SegmentHeads) == 0 {
		return 1
	}
	return 0
}

func normalizeDigest(digest string) string {
	if strings.HasPrefix(digest, "blake3:") {
		return digest
	}
	return "blake3:" + digest
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
			var digest string
			haveDigest := false
			for k, v := range m {
				key := wire.TextOr(k, "")
				if key == "kind" {
					kind = wire.TextOr(v, "")
				} else if key == "digest" {
					digest = digestFromValue(v)
					haveDigest = true
				}
			}
			if kind == "blob" && haveDigest {
				out[digest] = struct{}{}
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

func cmdExtract(args []string) int {
	var outPath, mt string
	includeSuppressed := false
	var positional []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch a {
		case "-o", "--out":
			if i+1 >= len(args) {
				fmt.Fprintf(os.Stderr, "gts: -o requires a path\n%s\n", usage)
				return 2
			}
			outPath = args[i+1]
			i++
		case "--mt":
			if i+1 >= len(args) {
				fmt.Fprintf(os.Stderr, "gts: --mt requires a media type\n%s\n", usage)
				return 2
			}
			mt = args[i+1]
			i++
		case "--include-suppressed":
			includeSuppressed = true
		default:
			positional = append(positional, a)
		}
	}
	if len(positional) != 2 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	path, digest := positional[0], positional[1]
	data, code := load(path)
	if code != 0 {
		return code
	}
	g := reader.Read(data, true, nil)
	for _, d := range g.Diagnostics {
		fmt.Fprintf(os.Stderr, "gts: diagnostic %s: %s\n", d.Code, d.Detail)
	}
	if len(g.Diagnostics) > 0 || len(g.SegmentHeads) == 0 {
		fmt.Fprintln(os.Stderr, "gts: refusing extract: archive did not read cleanly")
		return 1
	}
	digest = normalizeDigest(digest)

	var blobData []byte
	for _, b := range g.Blobs {
		if b.Digest == digest {
			blobData = b.Data
			break
		}
	}
	if blobData == nil {
		fmt.Fprintf(os.Stderr, "gts: no inline blob %s in %s\n", digest, path)
		return 1
	}
	if !includeSuppressed {
		if _, suppressed := suppressedBlobDigests(g)[digest]; suppressed {
			fmt.Fprintf(os.Stderr, "gts: refusing %s: suppressed (§11); pass --include-suppressed to extract anyway\n", digest)
			return 1
		}
	}
	if mt != "" {
		declared := blobMT(g, digest)
		if declared != mt {
			fmt.Fprintf(os.Stderr, "gts: refusing %s: declared media type %q does not match asserted %q\n", digest, declared, mt)
			return 1
		}
	}
	if wire.DigestStr(blobData) != digest {
		fmt.Fprintf(os.Stderr, "gts: integrity failure: %s bytes re-hash differently\n", digest)
		return 1
	}
	if outPath != "" {
		if err := os.WriteFile(outPath, blobData, 0o644); err != nil {
			fmt.Fprintf(os.Stderr, "gts: cannot write %s: %v\n", outPath, err)
			return 2
		}
	} else {
		if _, err := os.Stdout.Write(blobData); err != nil {
			fmt.Fprintf(os.Stderr, "gts: cannot write stdout: %v\n", err)
			return 2
		}
	}
	return 0
}

func cmdCat(args []string) int {
	var outPath string
	var inputs []string
	for i := 0; i < len(args); i++ {
		if args[i] == "-o" {
			if i+1 >= len(args) {
				fmt.Fprintf(os.Stderr, "gts: -o requires a path\n%s\n", usage)
				return 2
			}
			outPath = args[i+1]
			i++
		} else {
			inputs = append(inputs, args[i])
		}
	}
	if len(inputs) < 2 {
		fmt.Fprintf(os.Stderr, "gts: cat needs at least two inputs\n%s\n", usage)
		return 2
	}

	var combined []byte
	for _, path := range inputs {
		data, code := load(path)
		if code != 0 {
			return code
		}
		fs := reader.ReadFileSegments(data)
		if hasProblems(fs) {
			fmt.Fprintf(os.Stderr, "gts: refusing %s: not a clean GTS input\n", path)
			printLedger(path, fs)
			return 1
		}
		for idx, seg := range fs.Segments {
			contributes := len(seg.Quads) > 0 || len(seg.Blobs) > 0 ||
				len(seg.Reifiers) > 0 || len(seg.Annotations) > 0 ||
				len(seg.Suppressions) > 0
			if !contributes {
				fmt.Fprintf(os.Stderr, "gts: refusing %s: segment %d folds to nothing (no quads/blobs/reifies/annot/suppress) — wiring bug?\n", path, idx)
				return 1
			}
		}
		combined = append(combined, data...)
	}

	folded := reader.Read(combined, true, nil)
	if allQuadsSuppressed(folded) {
		fmt.Fprintln(os.Stderr, "gts: refusing composition: suppressions hide every quad in the folded output")
		return 1
	}

	if outPath != "" {
		if err := os.WriteFile(outPath, combined, 0o644); err != nil {
			fmt.Fprintf(os.Stderr, "gts: cannot write %s: %v\n", outPath, err)
			return 2
		}
	} else {
		if _, err := os.Stdout.Write(combined); err != nil {
			fmt.Fprintf(os.Stderr, "gts: cannot write stdout: %v\n", err)
			return 2
		}
	}
	return 0
}

// cmdCompact rewrites a GTS file into the streamable layout state (§10.1, §14.1).
func cmdCompact(args []string) int {
	var outPath, timestamp string
	streamable := false
	sealOriginal := false
	var positional []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch a {
		case "-o", "--out":
			if i+1 >= len(args) {
				fmt.Fprintf(os.Stderr, "gts: -o requires a path\n%s\n", usage)
				return 2
			}
			outPath = args[i+1]
			i++
		case "--streamable":
			streamable = true
		case "--seal-original":
			sealOriginal = true
		case "--timestamp":
			if i+1 >= len(args) {
				fmt.Fprintf(os.Stderr, "gts: --timestamp requires a value\n%s\n", usage)
				return 2
			}
			timestamp = args[i+1]
			i++
		default:
			positional = append(positional, a)
		}
	}
	if len(positional) != 1 || outPath == "" {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	if !streamable {
		// The verb is reserved for layout rewrites; a future --snapshot mode
		// (§10) would land here. Without a mode the request is ambiguous.
		fmt.Fprintln(os.Stderr, "gts: compact requires --streamable")
		return 2
	}
	data, code := load(positional[0])
	if code != 0 {
		return code
	}
	if timestamp == "" {
		timestamp = time.Now().UTC().Format("2006-01-02T15:04:05Z")
	}
	out, err := compact.Streamable(data, timestamp, sealOriginal)
	if err != nil {
		fmt.Fprintf(os.Stderr, "gts: refusing compact: %v\n", err)
		return 1
	}
	if err := os.WriteFile(outPath, out, 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "gts: cannot write %s: %v\n", outPath, err)
		return 2
	}
	return 0
}

func cmdPack(args []string) int {
	var outPath string
	var sources []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch a {
		case "-o", "--out":
			if i+1 >= len(args) {
				fmt.Fprintf(os.Stderr, "gts: -o requires a path\n%s\n", usage)
				return 2
			}
			outPath = args[i+1]
			i++
		default:
			sources = append(sources, a)
		}
	}
	if len(sources) == 0 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	if outPath == "" {
		fmt.Fprintf(os.Stderr, "gts: pack requires -o\n%s\n", usage)
		return 2
	}

	data, err := files.Pack(sources)
	if err != nil {
		fmt.Fprintf(os.Stderr, "gts: refusing pack: %v\n", err)
		return 1
	}
	if err := os.WriteFile(outPath, data, 0o644); err != nil {
		fmt.Fprintf(os.Stderr, "gts: cannot write %s: %v\n", outPath, err)
		return 2
	}
	return 0
}

func cmdUnpack(args []string) int {
	var dest string
	includeSuppressed := false
	var positional []string
	for i := 0; i < len(args); i++ {
		a := args[i]
		switch a {
		case "-C":
			if i+1 >= len(args) {
				fmt.Fprintf(os.Stderr, "gts: -C requires a directory\n%s\n", usage)
				return 2
			}
			dest = args[i+1]
			i++
		case "--include-suppressed":
			includeSuppressed = true
		default:
			positional = append(positional, a)
		}
	}
	if len(positional) != 1 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	path := positional[0]
	data, code := load(path)
	if code != 0 {
		return code
	}
	g := reader.Read(data, true, nil)
	for _, d := range g.Diagnostics {
		fmt.Fprintf(os.Stderr, "gts: diagnostic %s: %s\n", d.Code, d.Detail)
	}
	if len(g.Diagnostics) > 0 || len(g.SegmentHeads) == 0 {
		fmt.Fprintln(os.Stderr, "gts: refusing unpack: archive did not read cleanly")
		return 1
	}
	destPath := dest
	if destPath == "" {
		destPath = "."
	}
	if err := files.Unpack(g, destPath, includeSuppressed); err != nil {
		fmt.Fprintf(os.Stderr, "gts: refusing unpack: %v\n", err)
		return 1
	}
	return 0
}

func cmdDiff(args []string) int {
	if len(args) != 2 {
		fmt.Fprintln(os.Stderr, usage)
		return 2
	}
	archive, directory := args[0], args[1]
	data, code := load(archive)
	if code != 0 {
		return code
	}
	g := reader.Read(data, true, nil)
	for _, d := range g.Diagnostics {
		fmt.Fprintf(os.Stderr, "gts: diagnostic %s: %s\n", d.Code, d.Detail)
	}
	if len(g.Diagnostics) > 0 || len(g.SegmentHeads) == 0 {
		fmt.Fprintln(os.Stderr, "gts: refusing diff: archive did not read cleanly")
		return 1
	}
	lines, err := files.Diff(g, directory)
	if err != nil {
		fmt.Fprintf(os.Stderr, "gts: refusing diff: %v\n", err)
		return 1
	}
	hasChanges := false
	for _, line := range lines {
		fmt.Println(line)
		hasChanges = true
	}
	if hasChanges {
		return 1
	}
	return 0
}

func targetKind(target interface{}) string {
	m, ok := target.(map[interface{}]interface{})
	if !ok {
		return ""
	}
	if v, ok := wire.MapGet(m, "kind"); ok {
		return wire.TextOr(v, "")
	}
	return ""
}

func targetIdx(target interface{}) (int, bool) {
	m, ok := target.(map[interface{}]interface{})
	if !ok {
		return 0, false
	}
	if v, ok := wire.MapGet(m, "id"); ok {
		return wire.AsInt(v)
	}
	return 0, false
}

func allQuadsSuppressed(g *model.Graph) bool {
	if len(g.Quads) == 0 || len(g.Suppressions) == 0 {
		return false
	}
	termSup := make(map[int]struct{})
	quadSup := make(map[string]struct{})
	for _, sup := range g.Suppressions {
		collectSuppressed(sup, termSup, quadSup)
	}
	for _, q := range g.Quads {
		key := quadKey(q)
		if _, ok := quadSup[key]; ok {
			continue
		}
		if _, ok := termSup[q.S]; ok {
			continue
		}
		if _, ok := termSup[q.P]; ok {
			continue
		}
		if _, ok := termSup[q.O]; ok {
			continue
		}
		if q.G != nil {
			if _, ok := termSup[*q.G]; ok {
				continue
			}
		}
		return false
	}
	return true
}

func quadKey(q model.Quad) string {
	if q.G != nil {
		return fmt.Sprintf("%d,%d,%d,%d", q.S, q.P, q.O, *q.G)
	}
	return fmt.Sprintf("%d,%d,%d", q.S, q.P, q.O)
}

func collectSuppressed(sup model.Suppression, termSup map[int]struct{}, quadSup map[string]struct{}) {
	for _, target := range sup.Targets {
		switch targetKind(target) {
		case "term", "reifier":
			if id, ok := targetIdx(target); ok {
				termSup[id] = struct{}{}
			}
		case "quad":
			m, ok := target.(map[interface{}]interface{})
			if !ok {
				continue
			}
			if v, ok := wire.MapGet(m, "q"); ok {
				if ids, ok := v.([]interface{}); ok {
					parts := make([]string, len(ids))
					valid := true
					for i, x := range ids {
						n, ok := wire.AsInt64(x)
						if !ok {
							valid = false
							break
						}
						parts[i] = fmt.Sprintf("%d", n)
					}
					if valid {
						quadSup[strings.Join(parts, ",")] = struct{}{}
					}
				}
			}
		}
	}
}
