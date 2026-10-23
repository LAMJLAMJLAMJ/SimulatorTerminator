"""How the test suite talks to an engine.

The suite never imports engine internals. It hands over a scene and reads
back an `EngineResult`. Pick the engine with RA_ENGINE:

  RA_ENGINE unset   engine tests are skipped (only references and metrics run)
  RA_ENGINE=mock    the toy references stand in, to test the harness itself
  RA_ENGINE=ra-cli  the real engine through tools/ra-cli (RA_CLI = binary path)
"""
from __future__ import annotations

import math
import os
from dataclasses import dataclass, field

import numpy as np

from . import analytic as an
from . import toy_raytracer as rt


@dataclass
class EngineResult:
    # Image sources per (source, receiver): list of dicts with order, delay_s, amplitude
    image_sources: dict = field(default_factory=dict)
    # Room energy decay per seed, from the ray tracer: (times_s, energy)
    room_decay: list = field(default_factory=list)
    mean_free_path: float | None = None
    energy_lost_fraction: float | None = None
    engine_version: str = "unknown"


class Engine:
    name = "abstract"
    capabilities: set = set()

    def run(self, scene: dict) -> EngineResult:
        raise NotImplementedError


def _shoebox_from_scene(scene):
    pts = np.array([p for s in scene["surfaces"] for p in s["polygon"]], float)
    return tuple(pts.max(axis=0) - pts.min(axis=0))


class MockEngine(Engine):
    """Answers with the analytic image lattice and the toy ray tracer. Shoebox only."""

    name = "mock"
    capabilities = {"ism", "rt"}

    def run(self, scene):
        names = [s["name"] for s in scene["surfaces"]]
        if sorted(names) != ["x0", "x1", "y0", "y1", "z0", "z1"]:
            raise NotImplementedError("mock engine handles shoeboxes only")
        dims = _shoebox_from_scene(scene)
        alpha = {s["name"]: s["absorption"][0] for s in scene["surfaces"]}
        c = scene["speed_of_sound"]
        st = scene["solver"]
        res = EngineResult(engine_version="mock")
        if "max_order" in st:
            beta = {k: math.sqrt(1 - a) for k, a in alpha.items()}
            for s in scene["sources"]:
                for r in scene["receivers"]:
                    if np.allclose(s["position"], r["position"]):
                        continue  # coincident pair, no defined response
                    ims = an.shoebox_image_sources(dims, s["position"], r["position"], st["max_order"], beta, c)
                    res.image_sources[(s["name"], r["name"])] = [
                        {"order": i.order, "delay_s": i.delay, "amplitude": i.amplitude}
                        for i in ims if i.amplitude > 0
                    ]
        if "n_rays" in st:
            a = float(np.mean(list(alpha.values())))
            sc = scene["surfaces"][0]["scattering"][0]
            m = scene["air"]["m_per_m"][0]
            V = float(np.prod(dims))
            S = 2 * (dims[0] * dims[1] + dims[0] * dims[2] + dims[1] * dims[2])
            t_est = an.eyring_t60(V, [S], [a], m=m, c=c) if (a > 0 or m > 0) else 1.0
            mfps = []
            for seed in st.get("seeds", [0]):
                t, e, mfp = rt.shoebox_decay(dims, a, sc, m, c, st["n_rays"],
                                             t_max=min(0.8 * t_est, 3.0), seed=seed,
                                             src=scene["sources"][0]["position"])
                res.room_decay.append((t, e))
                mfps.append(mfp)
            res.mean_free_path = float(np.mean(mfps))
            res.energy_lost_fraction = 0.0  # analytic walls cannot leak
        return res


class RaCliEngine(Engine):
    """The real engine. Filled in once `ra-cli run` writes runs/<id>/."""

    name = "ra-cli"
    capabilities = {"ism", "rt"}

    def __init__(self, binary):
        self.binary = binary

    def run(self, scene):
        # 1. convert scene -> project.raproj (format 1.0 scene and materials)
        # 2. subprocess: ra-cli run <proj> --solver ism-rt --out <tmp>/runs/
        # 3. read runs/<id>/ with python/raproj.py into EngineResult
        raise NotImplementedError("ra-cli run is not implemented yet")


def get_engine():
    kind = os.environ.get("RA_ENGINE")
    if not kind:
        return None
    if kind == "mock":
        return MockEngine()
    if kind == "ra-cli":
        return RaCliEngine(os.environ.get("RA_CLI", "ra-cli"))
    raise ValueError(f"unknown RA_ENGINE={kind}")
