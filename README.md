# SimulatorTerminator

In-house room acoustic simulation: a browser app first, a Mac app (Tauri) later, both
running the same Rust engine.

```text
crates/
  ra-core       scene model, units, bands, materials
  ra-geom       mesh checks and repair, BVH
  ra-speaker    internal loudspeaker model, CLF and SOFA readers
  ra-engine     Solver trait, chunk planning, solvers
  ra-metrics    ISO 3382 and STI
  ra-project    .raproj read and write
  ra-api        the facade the wrappers call
bindings/ra-wasm  browser wrapper (wasm-bindgen)
apps/web          Vite + TypeScript front end and EngineConnector
apps/desktop      Tauri shell (later)
tools/ra-cli      headless runs for validation
python/           validation harness and .raproj reader
validation/       format samples and reference cases
docs/             format spec and decision records
```

## Develop

Requires Rust (stable, with the `wasm32-unknown-unknown` target), Node 22 and
`wasm-bindgen-cli` at the version in `Cargo.lock`.

```sh
cargo test --workspace                 # engine
cargo test -p ra-api --features ts     # regenerate apps/web/src/engine/protocol
cd apps/web && npm ci
npm run wasm                           # build the engine for the browser
npm run dev                            # http://localhost:5173
```

The built app (`npm run build`, then `apps/web/dist`) runs from any static host and
needs no special headers. Design: [docs/adr/0001-architecture.md](docs/adr/0001-architecture.md).
