"""A deliberately small shoebox ray tracer, used for two things only:

1. calibrating Tier 1 tolerances (how close does a correct stochastic tracer
   with Lambert scattering get to Eyring and to 4V/S?), and
2. the "mock" engine, so the test harness itself can be exercised before
   the real engine exists.

It is not the engine and should never be used for project work.
"""
from __future__ import annotations

import numpy as np


def _lambert(normals, rng):
    """Cosine-weighted directions on the hemisphere around each inward normal."""
    n = len(normals)
    u1, u2 = rng.random(n), rng.random(n)
    r, phi = np.sqrt(u1), 2 * np.pi * u2
    local = np.stack([r * np.cos(phi), r * np.sin(phi), np.sqrt(1 - u1)], axis=1)
    # Build an orthonormal frame per normal (normals are axis-aligned here).
    a = np.where(np.abs(normals[:, :1]) < 0.9, [[1.0, 0, 0]], [[0, 1.0, 0]])
    t1 = np.cross(normals, a)
    t1 /= np.linalg.norm(t1, axis=1, keepdims=True)
    t2 = np.cross(normals, t1)
    return local[:, :1] * t1 + local[:, 1:2] * t2 + local[:, 2:] * normals


def shoebox_decay(dims, alpha, scattering=1.0, m=0.0, c=343.0, n_rays=20000,
                  t_max=None, dt=1e-3, seed=0, src=None):
    """Total ray energy in the room against time, plus the mean free path.

    Returns (times, energy_histogram, mean_free_path). The histogram is the
    energy remaining in the room at each dt step (a room decay, not a
    receiver response), so its slope gives the room's reverberation time.
    """
    rng = np.random.default_rng(seed)
    L = np.asarray(dims, dtype=float)
    if t_max is None:
        t_max = 3.0
    pos = np.tile(L / 2 if src is None else np.asarray(src, float), (n_rays, 1))
    d = rng.normal(size=(n_rays, 3))
    d /= np.linalg.norm(d, axis=1, keepdims=True)
    energy = np.full(n_rays, 1.0 / n_rays)
    t = np.zeros(n_rays)
    n_bins = int(np.ceil(t_max / dt))
    hist = np.zeros(n_bins)
    path_total, hits = 0.0, 0
    alive = np.ones(n_rays, bool)
    while alive.any():
        p, v = pos[alive], d[alive]
        with np.errstate(divide="ignore", invalid="ignore"):
            tx = np.where(v > 0, (L - p) / v, np.where(v < 0, -p / v, np.inf))
        axis = np.argmin(tx, axis=1)
        s = tx[np.arange(len(p)), axis]
        t0 = t[alive]
        e0 = energy[alive]
        # Deposit the energy present during this flight into the time bins.
        b0 = np.minimum((t0 / dt).astype(int), n_bins)
        t1 = t0 + s / c
        b1 = np.minimum((t1 / dt).astype(int), n_bins)
        for k in range(b0.min(), min(b1.max(), n_bins - 1) + 1):
            mid = (k + 0.5) * dt
            sel = (b0 <= k) & (b1 >= k)
            hist[k] += np.sum(e0[sel] * np.exp(-m * c * (mid - t0[sel])))
        path_total += s.sum()
        hits += len(s)
        p = p + v * s[:, None]
        e0 = e0 * (1 - alpha) * np.exp(-m * s)
        normal = np.zeros_like(p)
        idx = np.arange(len(p))
        normal[idx, axis] = -np.sign(v[idx, axis])
        spec = v.copy()
        spec[idx, axis] *= -1
        diffuse = rng.random(len(p)) < scattering
        if diffuse.any():
            spec[diffuse] = _lambert(normal[diffuse], rng)
        aidx = np.flatnonzero(alive)
        pos[aidx], d[aidx], energy[aidx], t[aidx] = p, spec, e0, t1
        alive[aidx[t1 >= t_max]] = False
    times = (np.arange(n_bins) + 0.5) * dt
    return times, hist, path_total / hits


def decay_t60(times, energy, upper_db=-5.0, lower_db=-25.0):
    """Reverberation time from a room-energy decay by least squares on the dB curve."""
    db = 10 * np.log10(energy / energy[0] + 1e-300)
    sel = (db <= upper_db) & (db >= lower_db)
    slope, _ = np.polyfit(times[sel], db[sel], 1)
    return -60.0 / slope
