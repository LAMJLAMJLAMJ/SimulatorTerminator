"""Tier 1: the engine against closed-form answers.

Skipped unless RA_ENGINE is set. RA_ENGINE=mock runs the harness against the
references themselves (it must pass, which proves the checks are wired up);
RA_ENGINE=ra-cli runs the real engine.
"""
import math

import numpy as np
import pytest

from ra_validation import toy_raytracer as rt
from ra_validation.cases import all_cases
from ra_validation.tolerances import error_in_jnd

FS = 48000
CASES = {c["name"]: c for c in all_cases() if c.get("tier", 1) == 1}


def _case(name, engine):
    case = CASES[name]
    missing = set(case["requires"]) - engine.capabilities
    if missing:
        pytest.skip(f"{engine.name} lacks {missing}")
    try:
        return case, engine.run(case["scene"])
    except NotImplementedError as e:
        pytest.skip(str(e))


def _match_images(ref, got, tol_samples, tol_db):
    """Pair every reference image with an unused engine image of the same order and delay."""
    pool = sorted(got, key=lambda g: g["delay_s"])
    used = [False] * len(pool)
    delays = np.array([g["delay_s"] for g in pool])
    errors = []
    for r in ref:
        lo, hi = np.searchsorted(delays, [r["delay_s"] - tol_samples / FS, r["delay_s"] + tol_samples / FS + 1e-12])
        cands = [i for i in range(lo, hi) if not used[i] and pool[i]["order"] == r["order"]]
        if not cands:
            errors.append(f"missing order {r['order']} image at {r['delay_s']*1e3:.3f} ms")
            continue
        i = min(cands, key=lambda i: abs(20 * math.log10(abs(pool[i]["amplitude"]) / r["amplitude"])))
        used[i] = True
        db = 20 * math.log10(abs(pool[i]["amplitude"]) / r["amplitude"])
        if abs(db) > tol_db:
            errors.append(f"order {r['order']} at {r['delay_s']*1e3:.3f} ms off by {db:.2f} dB")
    extra = sum(1 for u in used if not u)
    if extra:
        errors.append(f"{extra} images the reference does not have")
    return errors


@pytest.mark.parametrize("name", ["t1-anechoic-direct", "t1-shoebox-ism-uniform", "t1-shoebox-ism-per-wall"])
def test_image_sources(engine, name):
    case, res = _case(name, engine)
    got = res.image_sources[("S1", "R1")]
    tol = case["tolerance"]
    errors = _match_images(case["reference"]["image_sources"], got, tol["delay_samples"], tol["energy_db"])
    assert not errors, "\n".join(errors[:20])
    if case["reference"].get("count") is not None:
        assert len(got) == case["reference"]["count"]


def test_reciprocity(engine):
    case, res = _case("t1-reciprocity", engine)
    a, b = res.image_sources[("S1", "R1")], res.image_sources[("S2", "R2")]
    errors = _match_images(a, b, 1, case["tolerance"]["energy_db"])
    assert not errors, "\n".join(errors[:20])


@pytest.mark.parametrize("name", [n for n in CASES if n.startswith("t1-diffuse-decay")])
def test_diffuse_decay_against_kuttruff(engine, name):
    case, res = _case(name, engine)
    ts = [rt.decay_t60(t, e) for t, e in res.room_decay]
    T = float(np.mean(ts))
    err = error_in_jnd("T20", T, case["reference"]["T"])
    assert abs(err) <= case["tolerance"]["T"][1], f"T = {T:.3f} s, reference {case['reference']['T']:.3f} s, {err:+.2f} JND"


def test_air_only(engine):
    case, res = _case("t1-air-only", engine)
    t, e = res.room_decay[0]
    assert rt.decay_t60(t, e) == pytest.approx(case["reference"]["T"], rel=case["tolerance"]["T"][1])


def test_lossless_conservation(engine):
    case, res = _case("t1-lossless-conservation", engine)
    assert res.energy_lost_fraction <= case["tolerance"]["energy_lost_fraction"][1]


@pytest.mark.parametrize("name", ["t1-mean-free-path-shoebox", "t1-mean-free-path-l-room"])
def test_mean_free_path(engine, name):
    case, res = _case(name, engine)
    if res.mean_free_path is None:
        pytest.skip(f"{engine.name} does not report a mean free path")
    assert res.mean_free_path == pytest.approx(case["reference"]["mean_free_path"], rel=case["tolerance"]["mean_free_path"][1])
