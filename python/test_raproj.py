import pathlib

from raproj import Project

SAMPLE = pathlib.Path(__file__).parents[1] / "validation" / "formats" / "1.0-test-room.raproj"


def test_sample_opens():
    p = Project(str(SAMPLE))
    assert p.manifest["format"] == "1.0"
    v = p.array("scene/vertices.npy")
    assert v.shape == (8, 3) and v.dtype == "float32"
    assert v.max(axis=0).tolist() == [20.0, 30.0, 12.0]
    assert p.json("scene/scene.json")["report"]["enclosed_volume_m3"] == 7200.0
