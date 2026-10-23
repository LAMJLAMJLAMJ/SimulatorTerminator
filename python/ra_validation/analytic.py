"""Closed-form references for engine verification.

Everything here is independent of the engine and of the metrics crate, so a
disagreement points at the code under test, not at the reference.
Units: SI throughout (m, s, Pa). Energies are squared pressures.
"""
from __future__ import annotations

import math
from dataclasses import dataclass

import numpy as np

LN10 = math.log(10.0)
# 60 dB of energy decay corresponds to a factor 1e6, i.e. 6 ln(10) nepers.
DECAY_60DB = 6.0 * LN10  # 13.8155...


# --------------------------------------------------------------------------
# Air absorption, ISO 9613-1:1993
# --------------------------------------------------------------------------
def air_attenuation_db_per_m(f, temp_c=20.0, rel_humidity=50.0, pa_kpa=101.325):
    """Pure-tone atmospheric attenuation coefficient alpha in dB/m (ISO 9613-1 eq. 3-5)."""
    f = np.asarray(f, dtype=float)
    T = temp_c + 273.15
    T0 = 293.15
    T01 = 273.16
    pr = 101.325
    pa = pa_kpa
    psat_over_pr = 10 ** (-6.8346 * (T01 / T) ** 1.261 + 4.6151)
    h = rel_humidity * psat_over_pr * (pr / pa)
    fr_o = (pa / pr) * (24 + 4.04e4 * h * (0.02 + h) / (0.391 + h))
    fr_n = (pa / pr) * (T / T0) ** -0.5 * (
        9 + 280 * h * math.exp(-4.170 * ((T / T0) ** (-1 / 3) - 1))
    )
    alpha = 8.686 * f**2 * (
        1.84e-11 * (pr / pa) * (T / T0) ** 0.5
        + (T / T0) ** -2.5
        * (
            0.01275 * np.exp(-2239.1 / T) / (fr_o + f**2 / fr_o)
            + 0.1068 * np.exp(-3352.0 / T) / (fr_n + f**2 / fr_n)
        )
    )
    return alpha


def air_energy_attenuation_m(f, **kw):
    """Energy attenuation coefficient m in 1/m, as used in 4mV (alpha_dB / (10 log10 e))."""
    return air_attenuation_db_per_m(f, **kw) / (10 * math.log10(math.e))


def speed_of_sound(temp_c=20.0):
    return 331.3 * math.sqrt(1 + temp_c / 273.15)


# --------------------------------------------------------------------------
# Diffuse-field reverberation formulas
# --------------------------------------------------------------------------
def sabine_t60(volume, areas, alphas, m=0.0, c=343.0):
    """Sabine: T = 24 ln10 V / (c (sum S_i a_i + 4 m V))."""
    A = float(np.dot(areas, alphas))
    return 4 * DECAY_60DB * volume / (c * (A + 4 * m * volume))


def eyring_t60(volume, areas, alphas, m=0.0, c=343.0):
    """Eyring-Norris: T = 24 ln10 V / (c (-S ln(1 - a_mean) + 4 m V))."""
    S = float(np.sum(areas))
    a_mean = float(np.dot(areas, alphas)) / S
    A = -S * math.log(1 - a_mean)
    return 4 * DECAY_60DB * volume / (c * (A + 4 * m * volume))


def air_only_t60(m, c=343.0):
    """Perfectly reflecting room, air absorption only: T = 6 ln10 / (c m). Exact."""
    return DECAY_60DB / (c * m)


def mean_free_path(volume, surface):
    """Kosten / Cauchy mean free path 4V/S for a diffuse field."""
    return 4 * volume / surface


# --------------------------------------------------------------------------
# Shoebox image sources (Allen & Berkley 1979), frequency-independent walls
# --------------------------------------------------------------------------
@dataclass(frozen=True)
class ImageSource:
    position: tuple
    order: int
    distance: float
    delay: float
    amplitude: float  # pressure, unit source strength: prod(beta) / (4 pi r)

    @property
    def energy(self):
        return self.amplitude**2


def shoebox_image_sources(dims, src, rec, max_order, beta=None, c=343.0):
    """All image sources of a rectangular room up to reflection order `max_order`.

    dims  : (Lx, Ly, Lz), room spans [0, L] on each axis
    beta  : pressure reflection coefficients, dict with keys
            'x0','x1','y0','y1','z0','z1' (wall at 0 and at L), default 1.
    Returns a list sorted by delay. Order 0 is the direct sound.
    """
    if beta is None:
        beta = {k: 1.0 for k in ("x0", "x1", "y0", "y1", "z0", "z1")}
    walls = (("x0", "x1"), ("y0", "y1"), ("z0", "z1"))
    out = []
    N = max_order
    rng = range(-(N // 2) - 1, (N // 2) + 2)
    axis_terms = []
    for ax in range(3):
        L = dims[ax]
        terms = []
        for n in rng:
            for u in (0, 1):
                order = abs(2 * n - u)
                if order > N:
                    continue
                pos = (1 - 2 * u) * src[ax] + 2 * n * L
                b = beta[walls[ax][0]] ** abs(n - u) * beta[walls[ax][1]] ** abs(n)
                terms.append((pos, order, b))
        axis_terms.append(terms)
    rec = np.asarray(rec, dtype=float)
    for px, ox, bx in axis_terms[0]:
        for py, oy, by in axis_terms[1]:
            if ox + oy > N:
                continue
            for pz, oz, bz in axis_terms[2]:
                order = ox + oy + oz
                if order > N:
                    continue
                p = np.array([px, py, pz])
                r = float(np.linalg.norm(p - rec))
                out.append(
                    ImageSource(tuple(p), order, r, r / c, bx * by * bz / (4 * math.pi * r))
                )
    out.sort(key=lambda s: s.delay)
    return out


def image_source_count(max_order):
    """Number of shoebox image sources with order <= N (direct included).

    Images map one-to-one onto lattice points (i, j, k) in Z^3 with
    |i| + |j| + |k| <= N, the octahedral numbers (2N+1)(2N^2+2N+3)/3.
    """
    N = max_order
    return (2 * N + 1) * (2 * N * N + 2 * N + 3) // 3


def image_sources_to_ir(images, fs, length_s):
    """Sample-rounded pressure impulse response from an image-source list."""
    n = int(round(length_s * fs))
    h = np.zeros(n)
    for im in images:
        k = int(round(im.delay * fs))
        if k < n:
            h[k] += im.amplitude
    return h


# --------------------------------------------------------------------------
# Ideal exponential decay: closed-form ISO 3382-1 parameter values
# --------------------------------------------------------------------------
def exponential_energy_envelope(t60, fs, length_s):
    """Squared-pressure envelope e^{-delta t}, delta = 6 ln10 / T, starting at t=0."""
    t = np.arange(int(round(length_s * fs))) / fs
    return np.exp(-DECAY_60DB / t60 * t)


def exponential_decay_parameters(t60):
    """ISO 3382-1 parameters of an ideal exponential decay starting at t=0.

    For e(t) = exp(-d t): EDT = T20 = T30 = T; C_te = 10 log10(exp(d te) - 1);
    D50 = 1 - exp(-0.05 d); Ts = 1/d.
    """
    d = DECAY_60DB / t60
    return {
        "EDT": t60,
        "T20": t60,
        "T30": t60,
        "C50": 10 * math.log10(math.exp(d * 0.05) - 1),
        "C80": 10 * math.log10(math.exp(d * 0.08) - 1),
        "D50": 1 - math.exp(-d * 0.05),
        "Ts": 1 / d,
    }


# IEC 60268-16:2020 constants (male speech weights, revision 5)
STI_BANDS_HZ = (125, 250, 500, 1000, 2000, 4000, 8000)
STI_MOD_FREQS_HZ = (0.63, 0.8, 1.0, 1.25, 1.6, 2.0, 2.5, 3.15, 4.0, 5.0, 6.3, 8.0, 10.0, 12.5)
STI_ALPHA = (0.085, 0.127, 0.230, 0.233, 0.309, 0.224, 0.173)
STI_BETA = (0.085, 0.078, 0.065, 0.011, 0.047, 0.095)


def mtf_exponential(t60, F):
    """Schroeder 1981: m(F) = 1 / sqrt(1 + (2 pi F T / 13.8)^2), no noise."""
    return 1.0 / np.sqrt(1.0 + (2 * math.pi * np.asarray(F) * t60 / DECAY_60DB) ** 2)


def sti_from_mtf(mtf):
    """STI from a 7 x 14 MTF matrix, IEC 60268-16:2020 without level-dependent masking."""
    m = np.clip(np.asarray(mtf, dtype=float), 1e-12, 1 - 1e-12)
    snr = np.clip(10 * np.log10(m / (1 - m)), -15, 15)
    ti = (snr + 15) / 30
    mti = ti.mean(axis=1)
    sti = float(np.dot(STI_ALPHA, mti))
    sti -= float(np.dot(STI_BETA, np.sqrt(mti[:-1] * mti[1:])))
    return min(max(sti, 0.0), 1.0)


def sti_exponential(t60s):
    """STI for a noise-free exponential decay; t60s is one value or one per band."""
    t60s = np.broadcast_to(np.asarray(t60s, dtype=float), (7,))
    mtf = np.array([mtf_exponential(t, STI_MOD_FREQS_HZ) for t in t60s])
    return sti_from_mtf(mtf)


def kuttruff_t60(volume, areas, alphas, gamma2=0.4, m=0.0, c=343.0):
    """Eyring with Kuttruff's path-length variance correction (Room Acoustics, 6th ed., 5.4).

    A diffusely reflecting room decays slower than Eyring because the number
    of wall hits in a time t varies between sound paths. With gamma2 the
    relative variance of the free path lengths (about 0.4 for rooms that are
    not too irregular): A = -S ln(1-a) (1 + gamma2/2 ln(1-a)).
    """
    S = float(np.sum(areas))
    a_mean = float(np.dot(areas, alphas)) / S
    ln = math.log(1 - a_mean)
    A = -S * ln * (1 + gamma2 / 2 * ln)
    return 4 * DECAY_60DB * volume / (c * (A + 4 * m * volume))
