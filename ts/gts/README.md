# TypeScript GTS engine

A TypeScript/npm implementation of the Graph Transport Substrate (GTS) engine,
matching the Go (`go/gts`) and Rust (`crates/gts`) engines.

## Local development

```bash
cd ts/gts
npm ci
npm run build
npm run lint
npm test
```

## CLI

After building:

```bash
npm run cli -- info path/to/file.gts
npm run cli -- fold path/to/file.gts
npm run cli -- verify path/to/file.gts
npm run cli -- ls path/to/file.gts
npm run cli -- pack dir/ -o out.gts
npm run cli -- unpack out.gts -C dest/
```

Exit codes: `0` clean, `1` diagnostics/input refused, `2` usage/IO error.
