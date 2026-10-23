"""Plain-numpy reference implementation of ISO 3382-1 parameters and the
IEC 60268-16 indirect STI. It exists to cross-check the Rust metrics crate;
pyrato is the third, independent opinion.

All functions take an already band-filtered impulse response (or squared
energy sequence) and a sampling rate.
"""
from __future__ import annotations

import math

import numpy as np

from .analytic import STI_MOD_FREQS_HZ, sti_from_mtf


def onset_index(h, threshold_db=-20.0):
    """ISO 3382-1 A.3.4: direct sound starts where the level first exceeds max - 20 dB."""
    e = np.asarray(h, dtype=float) ** 2
    return int(np.argmax(e >= e.max() * 10 ** (threshold_db / 10)))


def schroeder_edc(energy):
    """Backward-integrated energy decay curve, not normalised."""
    return np.cumsum(energy[::-1])[::-1]


def _regression_t(edc_db, fs, upper_db, lower_db):
    i0 = int(np.argmax(edc_db <= upper_db))
    i1 = int(np.argmax(edc_db <= lower_db))
    if i1 <= i0 + 1 or edc_db[i1] > lower_db:
        return math.nan
    t = np.arange(i0, i1 + 1) / fs
    slope, _ = np.polyfit(t, edc_db[i0 : i1 + 1], 1)
    return -60.0 / slope


def iso3382(energy, fs, onset=True):
    """EDT, T20, T30, C50, C80, D50, Ts from a squared-pressure sequence."""
    e = np.asarray(energy, dtype=float)
    if onset:
        e = e[onset_index(np.sqrt(e)) :]
    edc = schroeder_edc(e)
    edc_db = 10 * np.log10(edc / edc[0] + 1e-300)
    n50 = int(round(0.05 * fs))
    n80 = int(round(0.08 * fs))
    total = e.sum()
    t = np.arange(len(e)) / fs
    return {
        "EDT": _regression_t(edc_db, fs, 0.0, -10.0),
        "T20": _regression_t(edc_db, fs, -5.0, -25.0),
        "T30": _regression_t(edc_db, fs, -5.0, -35.0),
        "C50": 10 * math.log10(e[:n50].sum() / e[n50:].sum()),
        "C80": 10 * math.log10(e[:n80].sum() / e[n80:].sum()),
        "D50": e[:n50].sum() / total,
        "Ts": float((t * e).sum() / total),
    }


def mtf_band(energy, fs, mod_freqs=STI_MOD_FREQS_HZ):
    """Indirect MTF of one band: |sum e(t) exp(-j 2 pi F t)| / sum e(t)."""
    e = np.asarray(energy, dtype=float)
    t = np.arange(len(e)) / fs
    F = np.asarray(mod_freqs)[:, None]
    return np.abs((e * np.exp(-2j * np.pi * F * t)).sum(axis=1)) / e.sum()


def sti_indirect(band_energies, fs):
    """STI from seven band-filtered energy sequences (125 Hz to 8 kHz)."""
    mtf = np.array([mtf_band(e, fs) for e in band_energies])
    return sti_from_mtf(mtf)
