# 1. One engine, two wrappers, one connector, one file

Date: 2026-10-21. Status: accepted.

Full design: <https://claude.ai/artifact/C9e9ZAuxdvcWfuy5zjV9JM>

## Decision

- The engine is a plain Rust workspace under `crates/` with no browser, Tauri or file
  system code. `ra-api` is the only surface the wrappers see: JSON `Command`s in,
  `Event`s out, large arrays as raw little-endian buffers beside them.
- `bindings/ra-wasm` wraps it for the browser; `apps/desktop/src-tauri` will wrap it
  natively for the Mac app. Neither contains physics.
- The UI talks to one TypeScript class, `EngineConnector`, over a `Transport`: Web
  Workers in the browser, Tauri `invoke` on the Mac. Message types are generated from
  the Rust types with ts-rs, and CI fails if they are stale.
- Runs are split into chunks with their own seeds and merged in chunk order. The browser
  uses a pool of separate wasm32 instances on stable Rust, with no shared memory or
  cross-origin isolation headers. Maths goes through `libm`, so browser and native
  results match bit for bit (confirmed by the ray-engine spike).
- Projects are `.raproj` files (see `docs/format.md`), read and written only by
  `ra-project`, so a project opens identically in both apps.

## Consequences

Each browser worker holds its own scene and BVH, about 250 bytes per triangle, so the
coordinator builds the BVH once and sends its flat arrays to the others. Dense
time-resolved grid maps, the wave solver and native libraries (Embree, Metal, the
SketchUp SDK) are Mac-app features.
