"""The Rust ra-metrics crate against the Python reference and closed forms."""
import json
import shutil
import subprocess
from pathlib import Path

import numpy as np
import pytest

from ra_validation import analytic as an
from ra_validation import metrics as mt
from ra_validation.tolerances import JND, TIER1, error_in_jnd

FS = 48000
ROOT = Path(__file__).resolve().parents[2]


@pytest.fixture(scope="module")
def ra_metrics():
    if shutil.which("cargo") is None:
        pytest.skip("cargo not installed")
    subprocess.run(["cargo", "build", "--release", "--quiet", "-p", "ra-metrics", "--example", "metrics_json"],
                   cwd=ROOT, check=True)
    exe = ROOT / "target" / "release" / "examples" / "metrics_json"
    def run(x, *flags, fs=FS):
        text = "\n".join(" ".join(repr(float(v)) for v in np.atleast_1d(row)) for row in x)
        out = subprocess.run([str(exe), "--fs", str(fs), *flags], input=text,
                             capture_output=True, text=True, check=True).stdout
        return json.loads(out)
    return run


def _noise_ir(t60, seed, length_s=3.0):
    rng = np.random.default_rng(seed)
    env = an.exponential_energy_envelope(t60, FS, length_s)
    return rng.standard_normal(len(env)) * np.sqrt(env)


@pytest.mark.parametrize("t60", [0.4, 1.0, 2.5])
def test_rust_on_ideal_exponential(ra_metrics, t60):
    got = ra_metrics(an.exponential_energy_envelope(t60, FS, 2.5 * t60), "--energy")
    for k, v in an.exponential_decay_parameters(t60).items():
        assert abs(error_in_jnd(k, got[k], v)) < TIER1["metrics_vs_closed_form_jnd"] * (2.5 if k == "Ts" else 1), k


@pytest.mark.parametrize("t60,seed", [(0.6, 0), (1.5, 1), (3.0, 2)])
def test_rust_matches_python_reference(ra_metrics, t60, seed):
    h = _noise_ir(t60, seed, length_s=max(3.0, 1.5 * t60))
    got = ra_metrics(h, "--no-onset")
    ref = mt.iso3382(h**2, FS, onset=False)
    for k in ref:
        assert abs(error_in_jnd(k, got[k], ref[k])) < TIER1["metrics_rust_vs_python_jnd"], (k, got[k], ref[k])


def test_rust_refuses_t30_on_truncated_decay(ra_metrics):
    got = ra_metrics(an.exponential_energy_envelope(1.0, FS, 0.7), "--energy")  # 42 dB of decay
    assert got["T30"] is None and got["T20"] is not None


@pytest.mark.parametrize("t60", [0.5, 1.2])
def test_rust_sti_matches_closed_form(ra_metrics, t60):
    fs = 8000
    e = an.exponential_energy_envelope(t60, fs, max(1.6, 3 * t60))
    got = ra_metrics(np.tile(e[:, None], (1, 7)), "--sti", fs=fs)["STI"]
    assert got == pytest.approx(an.sti_exponential(t60), abs=JND["STI"][1] / 10)


def test_rust_sti_with_different_band_decays(ra_metrics):
    fs = 8000
    t60s = [2.0, 1.6, 1.3, 1.1, 1.0, 0.9, 0.7]
    cols = np.stack([an.exponential_energy_envelope(t, fs, 6.0) for t in t60s], axis=1)
    got = ra_metrics(cols, "--sti", fs=fs)["STI"]
    assert got == pytest.approx(an.sti_exponential(t60s), abs=0.003)
    assert got == pytest.approx(mt.sti_indirect(cols.T, fs), abs=1e-9)
