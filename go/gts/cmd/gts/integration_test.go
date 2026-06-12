// SPDX-FileCopyrightText: 2026 Blackcat Informatics® Inc. <paudley@blackcatinformatics.ca>
// SPDX-License-Identifier: Apache-2.0

package main

import (
	"bytes"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"
)

var binPath string

func TestMain(m *testing.M) {
	dir, err := os.MkdirTemp("", "gts-cli-test")
	if err != nil {
		fmt.Fprintf(os.Stderr, "cannot create temp dir: %v\n", err)
		os.Exit(1)
	}
	defer os.RemoveAll(dir)

	binPath = filepath.Join(dir, "gts")
	cmd := exec.Command("go", "build", "-o", binPath, "github.com/Blackcat-Informatics/gmeow-ontology/go/gts/cmd/gts")
	if out, err := cmd.CombinedOutput(); err != nil {
		fmt.Fprintf(os.Stderr, "cannot build gts binary: %v\n%s\n", err, out)
		os.Exit(1)
	}

	os.Exit(m.Run())
}

func run(t *testing.T, args ...string) (*exec.Cmd, *bytes.Buffer, *bytes.Buffer) {
	t.Helper()
	cmd := exec.Command(binPath, args...)
	var stdout, stderr bytes.Buffer
	cmd.Stdout = &stdout
	cmd.Stderr = &stderr
	err := cmd.Run()
	if err != nil {
		if _, ok := err.(*exec.ExitError); !ok {
			t.Fatalf("failed to start command: %v", err)
		}
	}
	return cmd, &stdout, &stderr
}

func vectorsDir(t *testing.T) string {
	t.Helper()
	dir, err := filepath.Abs("../../../../generated/gts-vectors")
	if err != nil {
		t.Fatal(err)
	}
	return dir
}

func vector(t *testing.T, name string) string {
	t.Helper()
	return filepath.Join(vectorsDir(t), name)
}

func TestFoldEmitsNQuads(t *testing.T) {
	v := vector(t, "01-minimal.gts")
	_, stdout, stderr := run(t, "fold", v)
	if stderr.Len() > 0 {
		t.Fatalf("fold produced stderr: %s", stderr.String())
	}
	want := "<https://example.org/Cat> <http://www.w3.org/2000/01/rdf-schema#label> \"Cat\"@en .\n"
	if got := stdout.String(); got != want {
		t.Fatalf("fold output mismatch\ngot:  %q\nwant: %q", got, want)
	}
}

func TestVerifyFlagsDamageWithExit1(t *testing.T) {
	v := vector(t, "04-damaged-frame.gts")
	cmd, stdout, _ := run(t, "verify", v)
	if cmd.ProcessState.ExitCode() != 1 {
		t.Fatalf("expected exit 1, got %d", cmd.ProcessState.ExitCode())
	}
	if !bytes.Contains(stdout.Bytes(), []byte("DamagedFrame")) {
		t.Fatalf("ledger did not list DamagedFrame")
	}
}

func TestCatComposesCleanInputs(t *testing.T) {
	a := vector(t, "01-minimal.gts")
	b := vector(t, "14-bnode-label.gts")
	cmd, stdout, _ := run(t, "cat", a, b)
	if cmd.ProcessState.ExitCode() != 0 {
		t.Fatalf("expected exit 0, got %d", cmd.ProcessState.ExitCode())
	}
	adata, err := os.ReadFile(a)
	if err != nil {
		t.Fatal(err)
	}
	bdata, err := os.ReadFile(b)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(stdout.Bytes(), append(adata, bdata...)) {
		t.Fatalf("cat output is not raw concatenation")
	}
}

func TestCatRefusesDamagedInput(t *testing.T) {
	a := vector(t, "01-minimal.gts")
	b := vector(t, "04-damaged-frame.gts")
	cmd, _, stderr := run(t, "cat", a, b)
	if cmd.ProcessState.ExitCode() != 1 {
		t.Fatalf("expected exit 1, got %d", cmd.ProcessState.ExitCode())
	}
	if !bytes.Contains(stderr.Bytes(), []byte("refusing")) {
		t.Fatalf("stderr did not name refusal: %s", stderr.String())
	}
}

func TestLsListsDigestSizeAndMediaType(t *testing.T) {
	v := vector(t, "22-inline-blob.gts")
	_, stdout, _ := run(t, "ls", v)
	out := stdout.String()

	var found bool
	for _, line := range strings.Split(strings.TrimSpace(out), "\n") {
		fields := strings.Fields(line)
		if len(fields) != 3 {
			continue
		}
		if !strings.HasPrefix(fields[0], "blake3:") {
			continue
		}
		found = true
		if fields[1] != "21" {
			t.Fatalf("size not 21: %s", line)
		}
		if fields[2] != "image/webp" {
			t.Fatalf("media type not image/webp: %s", line)
		}
	}
	if !found {
		t.Fatalf("no blob line found in: %s", out)
	}
}

func TestPackUnpackRoundTrip(t *testing.T) {
	tmp := t.TempDir()
	src := filepath.Join(tmp, "src")
	if err := os.MkdirAll(filepath.Join(src, "subdir"), 0o755); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "a.txt"), []byte("hello"), 0o644); err != nil {
		t.Fatal(err)
	}
	if err := os.WriteFile(filepath.Join(src, "subdir", "b.txt"), []byte("world"), 0o644); err != nil {
		t.Fatal(err)
	}

	archive := filepath.Join(tmp, "out.gts")
	cmd, _, stderr := run(t, "pack", src, "-o", archive)
	if cmd.ProcessState.ExitCode() != 0 {
		t.Fatalf("pack exit %d: %s", cmd.ProcessState.ExitCode(), stderr.String())
	}

	dst := filepath.Join(tmp, "dst")
	cmd, _, stderr = run(t, "unpack", archive, "-C", dst)
	if cmd.ProcessState.ExitCode() != 0 {
		t.Fatalf("unpack exit %d: %s", cmd.ProcessState.ExitCode(), stderr.String())
	}
	if got := readFile(t, filepath.Join(dst, "a.txt")); got != "hello" {
		t.Fatalf("a.txt: got %q", got)
	}
	if got := readFile(t, filepath.Join(dst, "subdir", "b.txt")); got != "world" {
		t.Fatalf("subdir/b.txt: got %q", got)
	}

	archive2 := filepath.Join(tmp, "out2.gts")
	cmd, _, stderr = run(t, "pack", dst, "-o", archive2)
	if cmd.ProcessState.ExitCode() != 0 {
		t.Fatalf("re-pack exit %d: %s", cmd.ProcessState.ExitCode(), stderr.String())
	}
	orig, err := os.ReadFile(archive)
	if err != nil {
		t.Fatal(err)
	}
	repack, err := os.ReadFile(archive2)
	if err != nil {
		t.Fatal(err)
	}
	if !bytes.Equal(orig, repack) {
		t.Fatalf("re-packed archive differs")
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
