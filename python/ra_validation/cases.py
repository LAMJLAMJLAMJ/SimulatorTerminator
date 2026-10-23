"""Tier 1 analytic cases.

Each case is a plain dict: a scene the engine can be given, the closed-form
reference computed from `analytic`, and the tolerance from `tolerances`.
`write_case_dirs` turns them into validation/cases/<name>/ folders:

  scene.json      geometry, materials, sources, receivers, solver settings
                  (converted to project.raproj once ra-project fixes the schema)
  reference.json  expected values, with the formula they came from
  tolerance.json  allowed error per quantity
"""
from __future__ import annotations

import json
import math
from pathlib import Path

import numpy as np

from . import analytic as an
from .tolerances import TIER1

C = 343.0


def shoebox_polygons(dims):
    """Six inward-facing quads, named x0, x1, y0, y1, z0, z1 to match `analytic`."""
    X, Y, Z = dims
    v = lambda x, y, z: [x, y, z]
    return {
        "x0": [v(0, 0, 0), v(0, 0, Z), v(0, Y, Z), v(0, Y, 0)],
        "x1": [v(X, 0, 0), v(X, Y, 0), v(X, Y, Z), v(X, 0, Z)],
        "y0": [v(0, 0, 0), v(X, 0, 0), v(X, 0, Z), v(0, 0, Z)],
        "y1": [v(0, Y, 0), v(0, Y, Z), v(X, Y, Z), v(X, Y, 0)],
        "z0": [v(0, 0, 0), v(0, Y, 0), v(X, Y, 0), v(X, 0, 0)],
        "z1": [v(0, 0, Z), v(X, 0, Z), v(X, Y, Z), v(0, Y, Z)],
    }


def l_shaped_room():
    """Non-convex L-shaped room, 10 x 8 x 4 m with a 5 x 4 m bite removed. V = 240, S = 248."""
    Z = 4.0
    floor = [[0, 0], [10, 0], [10, 4], [5, 4], [5, 8], [0, 8]]
    polys = {
        "floor": [[x, y, 0] for x, y in floor],
        "ceiling": [[x, y, Z] for x, y in reversed(floor)],
    }
    for i in range(len(floor)):
        (x0, y0), (x1, y1) = floor[i], floor[(i + 1) % len(floor)]
        polys[f"wall{i}"] = [[x0, y0, 0], [x1, y1, 0], [x1, y1, Z], [x0, y0, Z]]
    return polys, 240.0, 248.0


def _scene(polys, alpha, scattering, sources, receivers, **settings):
    a = alpha if isinstance(alpha, dict) else {k: alpha for k in polys}
    return {
        "bands_hz": [1000],
        "speed_of_sound": C,
        "air": settings.pop("air", {"m_per_m": [0.0]}),
        "surfaces": [
            {"name": k, "polygon": p, "absorption": [a[k]], "scattering": [scattering]}
            for k, p in polys.items()
        ],
        "sources": [{"name": f"S{i+1}", "position": s, "directivity": "omni"} for i, s in enumerate(sources)],
        "receivers": [{"name": f"R{i+1}", "position": r, "directivity": "omni"} for i, r in enumerate(receivers)],
        "solver": settings,
    }


def _ism_reference(dims, src, rec, order, alpha):
    a = alpha if isinstance(alpha, dict) else {k: alpha for k in ("x0", "x1", "y0", "y1", "z0", "z1")}
    beta = {k: math.sqrt(1 - v) for k, v in a.items()}
    ims = an.shoebox_image_sources(dims, src, rec, order, beta, c=C)
    return {
        "image_sources": [
            {"order": s.order, "delay_s": s.delay, "amplitude": s.amplitude} for s in ims if s.amplitude > 0
        ],
        "count": an.image_source_count(order) if all(v < 1 for v in a.values()) else None,
        "formula": "Allen & Berkley 1979 image lattice; amplitude = prod(sqrt(1-alpha)) / (4 pi r)",
    }


def all_cases():
    dims = (10.0, 8.0, 6.0)
    V, S = float(np.prod(dims)), 2 * (10 * 8 + 10 * 6 + 8 * 6)
    src, rec = [3.1, 2.7, 1.6], [7.3, 5.2, 1.2]
    box = shoebox_polygons(dims)
    cases = []

    cases.append({
        "name": "t1-anechoic-direct",
        "purpose": "All walls fully absorbing: the response is the direct sound alone, 1/(4 pi r) at r/c.",
        "requires": ["ism"],
        "scene": _scene(box, 1.0, 0.0, [src], [rec], max_order=3),
        "reference": _ism_reference(dims, src, rec, 3, 1.0),
        "tolerance": {"delay_samples": TIER1["ism_delay_samples"], "energy_db": TIER1["ism_energy_db"]},
    })
    cases.append({
        "name": "t1-shoebox-ism-uniform",
        "purpose": "Every image source to order 4, uniform alpha 0.2: count, delays and levels exact.",
        "requires": ["ism"],
        "scene": _scene(box, 0.2, 0.0, [src], [rec], max_order=4),
        "reference": _ism_reference(dims, src, rec, 4, 0.2),
        "tolerance": {"delay_samples": TIER1["ism_delay_samples"], "energy_db": TIER1["ism_energy_db"], "count": 0},
    })
    per_wall = {"x0": 0.05, "x1": 0.3, "y0": 0.6, "y1": 0.1, "z0": 0.02, "z1": 0.8}
    cases.append({
        "name": "t1-shoebox-ism-per-wall",
        "purpose": "Different alpha on each wall, catches swapped wall or normal indexing.",
        "requires": ["ism"],
        "scene": _scene(box, per_wall, 0.0, [src], [rec], max_order=3),
        "reference": _ism_reference(dims, src, rec, 3, per_wall),
        "tolerance": {"delay_samples": TIER1["ism_delay_samples"], "energy_db": TIER1["ism_energy_db"], "count": 0},
    })
    cases.append({
        "name": "t1-reciprocity",
        "purpose": "Swapping omni source and receiver leaves the image-source response unchanged.",
        "requires": ["ism"],
        "scene": _scene(box, per_wall, 0.0, [src, rec], [rec, src], max_order=4),
        "reference": {"pairs": [["S1", "R1"], ["S2", "R2"]], "formula": "reciprocity"},
        "tolerance": {"energy_db": TIER1["reciprocity_db"]},
    })
    for a in (0.05, 0.1, 0.2):
        cases.append({
            "name": f"t1-diffuse-decay-a{int(a*100):02d}",
            "purpose": f"Fully scattering shoebox, uniform alpha {a}: room decay against Eyring with Kuttruff's variance term.",
            "requires": ["rt"],
            "scene": _scene(box, a, 1.0, [[5.0, 4.0, 3.0]], [rec], n_rays=20000, seeds=[0, 1, 2]),
            "reference": {
                "T": an.kuttruff_t60(V, [S], [a], c=C),
                "T_eyring": an.eyring_t60(V, [S], [a], c=C),
                "T_sabine": an.sabine_t60(V, [S], [a], c=C),
                "formula": "Kuttruff: A = -S ln(1-a)(1 + 0.2 ln(1-a)), gamma^2 = 0.4",
            },
            "tolerance": {"T": ["jnd", TIER1["decay_t_jnd"]]},
        })
    m = float(an.air_energy_attenuation_m(4000.0, temp_c=20, rel_humidity=50))
    cases.append({
        "name": "t1-air-only",
        "purpose": "Rigid walls, air absorption only (4 kHz, 20 C, 50 %): T = 6 ln10 / (c m) exactly.",
        "requires": ["rt"],
        "scene": _scene(box, 0.0, 1.0, [[5.0, 4.0, 3.0]], [rec], n_rays=5000, seeds=[0],
                        air={"m_per_m": [m]}),
        "reference": {"T": an.air_only_t60(m, c=C), "m_per_m": m, "formula": "6 ln10 / (c m), m from ISO 9613-1"},
        "tolerance": {"T": ["rel", TIER1["air_only_t_rel"]]},
    })
    cases.append({
        "name": "t1-lossless-conservation",
        "purpose": "Rigid walls, no air: no energy may leave the room (leaks through edges and corners).",
        "requires": ["rt"],
        "scene": _scene(box, 0.0, 0.5, [[5.0, 4.0, 3.0]], [rec], n_rays=20000, seeds=[0]),
        "reference": {"energy_lost_fraction": 0.0},
        "tolerance": {"energy_lost_fraction": ["abs", TIER1["energy_loss_rel"]]},
    })
    cases.append({
        "name": "t1-mean-free-path-shoebox",
        "purpose": "Measured mean free path of diffusely reflected rays equals 4V/S.",
        "requires": ["rt"],
        "scene": _scene(box, 0.0, 1.0, [[5.0, 4.0, 3.0]], [rec], n_rays=20000, seeds=[0]),
        "reference": {"mean_free_path": an.mean_free_path(V, S), "formula": "4V/S"},
        "tolerance": {"mean_free_path": ["rel", TIER1["mean_free_path_rel"]]},
    })
    lpolys, lV, lS = l_shaped_room()
    cases.append({
        "name": "t1-mean-free-path-l-room",
        "purpose": "Same in a non-convex L-shaped room: exercises the general mesh intersection path.",
        "requires": ["rt"],
        "scene": _scene(lpolys, 0.0, 1.0, [[2.5, 2.0, 2.0]], [[2.5, 6.0, 1.2]], n_rays=20000, seeds=[0]),
        "reference": {"mean_free_path": an.mean_free_path(lV, lS), "formula": "4V/S"},
        "tolerance": {"mean_free_path": ["rel", TIER1["mean_free_path_rel"]]},
    })
    # Tier 3: the same shoebox modelled by hand in EASE 5 and Treble.
    rec3 = [[7.3, 5.2, 1.2], [2.0, 6.5, 1.2], [8.5, 1.5, 1.2]]
    jnd_band = {"default": ["jnd", 1.0], "bands": {b: ["jnd", 2.0] for b in ("125", "250", "500", "1000", "2000", "4000")}}
    for tag, alpha, scat, refs in (
        ("a-diffuse", 0.10, 1.0, {"T_sabine": an.sabine_t60(V, [S], [0.10], c=C),
                                  "T_eyring": an.eyring_t60(V, [S], [0.10], c=C),
                                  "T_kuttruff": an.kuttruff_t60(V, [S], [0.10], c=C)}),
        ("b-uneven", per_wall, 0.10, {}),
    ):
        cases.append({
            "name": f"t3-shoebox-{tag}",
            "purpose": "Cross-solver shoebox; reference values come from EASE 5 and Treble exports.",
            "tier": 3,
            "requires": ["ism", "rt"],
            "scene": _scene(box, alpha, scat, [src], rec3, max_order=3, n_rays=50000, seeds=[0, 1, 2]),
            "reference": {"formulas": refs, "solvers": {"ease5": None, "treble_ga": None, "treble_wave": None},
                          "fill_in": "per receiver and octave band: T20 T30 EDT C80 D50 Ts STI, plus version and settings"},
            "tolerance": {k: jnd_band for k in ("T20", "T30", "EDT", "C80", "D50", "Ts", "STI")},
        })
    return cases


def tolerance_json(tol):
    """Architecture format: {"T20": {"default": ["jnd", 1.0], "bands": {"125": ["jnd", 2.0]}}}.

    Tier 1 bare numbers become explicit kinds: delays in samples, levels in dB.
    """
    kinds = {"delay_samples": "samples", "energy_db": "db", "count": "abs"}
    out = {}
    for k, v in tol.items():
        if isinstance(v, dict):
            out[k] = v
        else:
            out[k] = {"default": list(v) if isinstance(v, (list, tuple)) else [kinds[k], v], "bands": {}}
    return out


def write_case_dirs(root):
    root = Path(root)
    for case in all_cases():
        d = root / case["name"]
        d.mkdir(parents=True, exist_ok=True)
        (d / "scene.json").write_text(json.dumps(
            {"name": case["name"], "purpose": case["purpose"], "tier": case.get("tier", 1),
             "requires": case["requires"], **case["scene"]}, indent=1))
        (d / "reference.json").write_text(json.dumps(case["reference"], indent=1))
        (d / "tolerance.json").write_text(json.dumps(tolerance_json(case["tolerance"]), indent=1))


if __name__ == "__main__":
    import sys
    write_case_dirs(sys.argv[1] if len(sys.argv) > 1 else "validation/cases")
