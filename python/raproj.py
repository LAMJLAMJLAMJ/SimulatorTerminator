"""Read .raproj project files from Python with only the standard library and numpy.

    from raproj import Project
    p = Project("hall-a.raproj")
    p.manifest["bands"]
    v = p.array("scene/vertices.npy")          # float32 [V, 3], metres, Z up
    e = p.array("runs/<id>/receivers/R1/energy.npy")
"""

from __future__ import annotations

import io
import json
import zipfile

import numpy as np

SUPPORTED_MAJOR = 1


class Project:
    def __init__(self, path: str):
        self.zip = zipfile.ZipFile(path)
        self.manifest = self.json("manifest.json")
        major = int(self.manifest["format"].split(".")[0])
        if major > SUPPORTED_MAJOR:
            raise ValueError(f"format {self.manifest['format']} is newer than this reader")

    def names(self) -> list[str]:
        return self.zip.namelist()

    def json(self, name: str):
        return json.loads(self.zip.read(name))

    def array(self, name: str) -> np.ndarray:
        return np.load(io.BytesIO(self.zip.read(name)), allow_pickle=False)

    def runs(self) -> list[str]:
        return sorted({n.split("/")[1] for n in self.names() if n.startswith("runs/")})
