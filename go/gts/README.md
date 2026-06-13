# GTS Go engine

The Go implementation of the Graph Transport Substrate (GTS) baseline reader and
files-profile writer.

## Install

```bash
go install go.blackcatinformatics.ca/gts/cmd/gts@latest
```

The module path is `go.blackcatinformatics.ca/gts`. Releases are tagged in the
monorepo with the Go submodule prefix, e.g. `go/gts/v0.1.0`.

## Binary releases

Pre-built binaries for Linux, macOS, and Windows are published to GitHub Releases
when a `go/gts/v*` tag is pushed. See the
[releases page](https://github.com/Blackcat-Informatics/gmeow-ontology/releases).

## Build and test

```bash
cd go/gts
go build ./...
go vet ./...
go test ./...
```

## Layout

- `cmd/gts` — `gts` CLI
- `reader` / `writer` — baseline GTS reader and files-profile writer
- `files` / `wire` / `compact` / `nquads` — format plumbing
- `model` / `stream` / `codec` — core data types and codecs
