# Python harness

Validation runs on the developer's Mac: pyfar, pyrato, pyroomacoustics and I-Simpa
check the engine's results. `raproj.py` reads project files with `zipfile` and `numpy`.

```sh
pip install numpy pytest pyfar pyrato
pytest python                                   # everything that needs no engine
RA_ENGINE=mock pytest python/tests/test_engine_tier1.py   # harness against the references
python -m ra_validation.cases ../validation/cases       # regenerate case folders (run from python/)
```

## ra_validation

| Module | What it holds |
| --- | --- |
| `analytic.py` | Closed forms: Sabine, Eyring, Kuttruff, ISO 9613-1 air absorption, shoebox image sources, ISO 3382-1 values of an exponential decay, STI |
| `metrics.py` | Plain-numpy ISO 3382-1 and indirect STI, the second opinion on `ra-metrics` |
| `tolerances.py` | JNDs and the Tier 1 limits, in one place |
| `cases.py` | The Tier 1 and Tier 3 case definitions behind `validation/cases/` |
| `engine.py` | How tests reach an engine: `RA_ENGINE=mock` or `ra-cli` (unset skips engine tests) |
| `toy_raytracer.py` | A small Lambert shoebox tracer used to calibrate tolerances and as the mock engine |

## Tests

| File | What it proves |
| --- | --- |
| `tests/test_analytic.py` | The references are right: ISO 9613-1 table values, image counts and mirrors, reciprocity, Kuttruff against a correct tracer within 1 % |
| `tests/test_metrics_python.py` | Python metrics match the closed forms |
| `tests/test_metrics_pyrato.py` | Python metrics and closed-form STI agree with pyrato |
| `tests/test_metrics_rust.py` | `ra-metrics` matches the closed forms and the Python reference within 0.01 JND |
| `tests/test_harness_catches_bugs.py` | The image-source check fails on swapped walls, wrong c, a missing order, alpha off by 0.02 |
| `tests/test_engine_tier1.py` | The engine against the 11 Tier 1 cases |
