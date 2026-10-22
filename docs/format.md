# `.raproj` project format, version 1.0

A project is one ZIP file of JSON documents and NumPy `.npy` arrays (version 1.0,
little-endian, C order). Only `crates/ra-project` reads and writes it; the browser and
the Mac app run the same compiled code. Arrays are stored uncompressed so they can be
sliced in place; JSON entries are deflated. Writing is reproducible: entries are sorted
after `manifest.json` and timestamps are fixed.

```text
manifest.json                 format, app and engine version, units, bands, content hashes
scene/
  scene.json                  origin (float64, m), groups, importer, import report
  vertices.npy                float32 [V, 3], metres, Z up, relative to origin
  triangles.npy               uint32  [T, 3]
  tri_group.npy               uint16  [T]
  tri_flags.npy               uint8   [T], optional, bit 0 = curved source face
materials/library.json        absorption and scattering per band, source of each value
speakers/<sha256>/            converted loudspeaker models (planned)
setup/                        sources, receivers, grids (planned)
runs/<run-id>/                request, summary, histograms, grids, ambisonics (planned)
originals/                    optional source files
```

Entries this build doesn't interpret are kept and written back unchanged.

## Conventions

- SI units; Z up, right-handed, as in Rhino and SketchUp.
- Loudspeaker frame: +X on-axis, +Y left, +Z up. Balloon grid: theta from +X (0..180),
  phi roll around +X (0 = up, 90 = right).
- Bands: `octave_63_8k` (8 bands) or `third_50_10k` (24 bands), named in the manifest.
- Ambisonics: AmbiX (ACN, SN3D), float32 `[channels, samples]`, sidecar `ambi.json`
  with sample rate, order and reference orientation.

## Versioning

1. `format` is `major.minor`.
2. A minor bump only adds optional fields; readers ignore unknown fields and writers keep them.
3. A major bump ships a migration in `ra-project` from every earlier major.
4. Each version has a browser-written sample in `validation/formats/`; CI checks that
   native code writes it back byte for byte.
