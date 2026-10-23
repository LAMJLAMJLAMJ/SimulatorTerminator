"""The references must be right before anything is judged against them."""
import math

import numpy as np
import pytest

from ra_validation import analytic as an
from ra_validation import toy_raytracer as rt

DIMS = (10.0, 8.0, 6.0)
V = float(np.prod(DIMS))
S = 2 * (10 * 8 + 10 * 6 + 8 * 6)


def test_sabine_eyring_agree_for_small_absorption():
    a = 1e-4
    assert an.eyring_t60(V, [S], [a]) == pytest.approx(an.sabine_t60(V, [S], [a]), rel=1e-4)


def test_eyring_shorter_than_sabine():
    for a in (0.1, 0.3, 0.6):
        assert an.eyring_t60(V, [S], [a]) < an.sabine_t60(V, [S], [a])


def test_sabine_constant_is_0161():
    # 24 ln10 / 343 = 0.1611 s/m
    assert an.sabine_t60(1.0, [1.0], [1.0]) == pytest.approx(0.1611, abs=1e-4)


def test_air_absorption_matches_iso_9613_table():
    # ISO 9613-1 Table 1, 20 C: 50 % RH 1 kHz 4.66 dB/km; 70 % RH 4 kHz 22.9 dB/km
    assert an.air_attenuation_db_per_m(1000, 20, 50) * 1000 == pytest.approx(4.66, abs=0.02)
    assert an.air_attenuation_db_per_m(4000, 20, 70) * 1000 == pytest.approx(22.9, abs=0.2)


@pytest.mark.parametrize("order", [0, 1, 2, 3, 5, 8])
def test_image_source_count(order):
    ims = an.shoebox_image_sources(DIMS, (3.1, 2.7, 1.6), (7.3, 5.2, 1.2), order)
    assert len(ims) == an.image_source_count(order)
    assert max(i.order for i in ims) == order


def test_first_order_images_are_wall_mirrors():
    src, rec = (3.1, 2.7, 1.6), (7.3, 5.2, 1.2)
    ims = an.shoebox_image_sources(DIMS, src, rec, 1)
    first = sorted(i.position for i in ims if i.order == 1)
    expected = sorted([(-3.1, 2.7, 1.6), (16.9, 2.7, 1.6), (3.1, -2.7, 1.6),
                       (3.1, 13.3, 1.6), (3.1, 2.7, -1.6), (3.1, 2.7, 10.4)])
    assert np.allclose(first, expected)


def test_per_wall_reflection_bookkeeping():
    # Only the floor reflects: every image must be the direct sound or the floor mirror.
    beta = {"x0": 0, "x1": 0, "y0": 0, "y1": 0, "z0": 1.0, "z1": 0}
    ims = [i for i in an.shoebox_image_sources(DIMS, (3, 3, 2), (6, 4, 1), 4, beta) if i.amplitude > 0]
    assert sorted(i.position for i in ims) == [(3.0, 3.0, -2.0), (3.0, 3.0, 2.0)]


def test_image_source_reciprocity():
    beta = {"x0": 0.9, "x1": 0.5, "y0": 0.7, "y1": 0.95, "z0": 0.99, "z1": 0.3}
    a = an.shoebox_image_sources(DIMS, (3.1, 2.7, 1.6), (7.3, 5.2, 1.2), 5, beta)
    b = an.shoebox_image_sources(DIMS, (7.3, 5.2, 1.2), (3.1, 2.7, 1.6), 5, beta)
    ka = sorted((round(i.delay, 12), round(i.amplitude, 15)) for i in a)
    kb = sorted((round(i.delay, 12), round(i.amplitude, 15)) for i in b)
    assert np.allclose(ka, kb)


def test_high_order_ism_decay_is_close_to_eyring():
    # A specular shoebox is not diffuse, so only a loose check that the lattice is sane.
    a = 0.2
    beta = {k: math.sqrt(1 - a) for k in ("x0", "x1", "y0", "y1", "z0", "z1")}
    ims = an.shoebox_image_sources(DIMS, (3.1, 2.7, 1.6), (7.3, 5.2, 1.2), 40, beta)
    fs = 1000
    h2 = np.zeros(fs)
    for i in ims:
        k = int(i.delay * fs)
        if k < fs:
            h2[k] += i.energy
    t = np.arange(fs) / fs
    sel = (t > 0.05) & (t < 0.45)
    slope = np.polyfit(t[sel], 10 * np.log10(np.convolve(h2, np.ones(20) / 20, "same")[sel]), 1)[0]
    assert -60 / slope == pytest.approx(an.eyring_t60(V, [S], [a]), rel=0.10)


def test_exponential_decay_closed_forms_are_consistent():
    p = an.exponential_decay_parameters(1.0)
    # D50 and C50 describe the same split: C50 = 10 log10(D50 / (1 - D50))
    assert p["C50"] == pytest.approx(10 * math.log10(p["D50"] / (1 - p["D50"])))
    assert p["Ts"] == pytest.approx(1 / 13.8155, rel=1e-4)


def test_sti_known_values():
    assert an.sti_exponential(1e-6) == pytest.approx(1.0, abs=1e-6)
    # Shorter reverberation gives higher STI, monotonically.
    vals = [an.sti_exponential(t) for t in (0.3, 0.6, 1.0, 2.0, 4.0)]
    assert all(x > y for x, y in zip(vals, vals[1:]))


@pytest.mark.parametrize("a", [0.05, 0.2])
def test_kuttruff_reference_matches_a_correct_diffuse_tracer(a):
    """Calibrates the Tier 1 decay tolerance: a correct Lambert tracer lands within 1 % of Kuttruff."""
    T_ref = an.kuttruff_t60(V, [S], [a])
    t, e, _ = rt.shoebox_decay(DIMS, a, n_rays=20000, t_max=0.8 * T_ref, seed=1)
    assert rt.decay_t60(t, e) == pytest.approx(T_ref, rel=0.01)


def test_toy_tracer_mean_free_path():
    _, _, mfp = rt.shoebox_decay(DIMS, 0.5, n_rays=20000, t_max=0.3, seed=2)
    assert mfp == pytest.approx(an.mean_free_path(V, S), rel=0.01)
