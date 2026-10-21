# Validation

- `formats/` holds one sample project per format version, written by the browser build.
  CI reads each natively and writes it back; the bytes must match.
- `cases/<name>/` holds reference scenes: `project.raproj`, `reference.json` (measured or
  ODEON/EASE values with their source) and `tolerance.json`. Owned by the validation work.

Tolerances are `[kind, value]` per quantity with optional per-band overrides. Kinds: `rel`,
`abs`, `jnd` (multiples of the ISO 3382-1 JND), `samples` (arrival time) and `db` (level).

```json
{ "T20": { "default": ["jnd", 1.0], "bands": { "125": ["jnd", 2.0] } } }
```
