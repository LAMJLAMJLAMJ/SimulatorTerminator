"""The checks must fail on a broken engine, or a green run means nothing.

Each test runs the mock engine with one deliberate bug and asserts that the
Tier 1 comparison reports it. Runs without RA_ENGINE.
"""
import copy

from ra_validation.cases import all_cases
from ra_validation.engine import MockEngine
from test_engine_tier1 import _match_images

CASES = {c["name"]: c for c in all_cases()}


def _run(scene_edit, name="t1-shoebox-ism-per-wall"):
    case = CASES[name]
    scene = copy.deepcopy(case["scene"])
    scene_edit(scene)
    got = MockEngine().run(scene).image_sources[("S1", "R1")]
    return _match_images(case["reference"]["image_sources"], got, 1, 0.1)


def test_correct_engine_passes():
    assert _run(lambda s: None) == []


def test_swapped_wall_materials_are_caught():
    def swap(scene):
        x0, x1 = scene["surfaces"][0], scene["surfaces"][1]
        x0["absorption"], x1["absorption"] = x1["absorption"], x0["absorption"]
    assert _run(swap)


def test_wrong_speed_of_sound_is_caught():
    assert _run(lambda s: s.__setitem__("speed_of_sound", 340.0))


def test_missing_reflection_order_is_caught():
    assert _run(lambda s: s["solver"].__setitem__("max_order", 2))


def test_small_absorption_error_is_caught():
    def nudge(scene):
        scene["surfaces"][5]["absorption"] = [0.82]  # z1 is 0.80 in the reference
    assert _run(nudge)
