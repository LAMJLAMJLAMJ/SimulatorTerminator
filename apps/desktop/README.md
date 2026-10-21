# Mac app (later)

A Tauri 2 shell around the same `apps/web` build, with `src-tauri` calling `ra-api`
natively. It implements `TauriTransport` (`invoke('engine', …)`, raw bytes back, a
Channel per job for progress) and opens `.raproj` files by double-click. Native-only
features (Embree, Metal, the wave solver, the SketchUp SDK) sit behind Cargo features
and show up in `capabilities()`.
