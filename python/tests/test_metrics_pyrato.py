"""Python reference metrics and closed forms against pyrato (skipped without pyrato)."""
import numpy as np
import pytest

from ra_validation import analytic as an
from ra_validation import metrics as mt
from ra_validation.tolerances import JND, error_in_jnd

pyrato = pytest.importorskip("pyrato")
pf = pytest.importorskip("pyfar")

FS = 48000


def _noise_ir(t60, seed=0, length_s=3.0):
    rng = np.random.default_rng(seed)
    env = an.exponential_energy_envelope(t60, FS, length_s)
    return rng.standard_normal(len(env)) * np.sqrt(env)


@pytest.mark.parametrize("t60", [0.6, 1.5])
def test_python_metrics_agree_with_pyrato(t60):
    h = _noise_ir(t60)
    sig = pf.Signal(h, FS)
    edc = pyrato.edc.schroeder_integration(sig)
    edc = pf.TimeData(edc.time / edc.time[..., :1], edc.times)  # pyrato regresses on the raw dB scale, so normalise first
    P = pyrato.parameters
    theirs = {
        "EDT": float(P.reverberation_time_linear_regression(edc, "EDT")[0]),
        "T20": float(P.reverberation_time_linear_regression(edc, "T20")[0]),
        "T30": float(P.reverberation_time_linear_regression(edc, "T30")[0]),
        "C50": float(P.clarity(edc, 50)[0]),
        "C80": float(P.clarity(edc, 80)[0]),
        "D50": float(P.definition(edc, 50)[0]),
        "Ts": float(P.center_time(edc)[0]),
    }
    ours = mt.iso3382(h**2, FS, onset=False)
    for k in theirs:
        assert abs(error_in_jnd(k, ours[k], theirs[k])) < 0.05, (k, ours[k], theirs[k])


@pytest.mark.parametrize("t60", [0.5, 1.2])
def test_pyrato_sti_agrees_with_closed_form(t60):
    """pyrato filters a broadband IR into octaves itself; on a white exponential decay
    every band decays alike, so its STI must match Schroeder's closed form."""
    h = _noise_ir(t60, seed=3, length_s=max(1.6, 3 * t60))
    theirs = float(np.ravel(pyrato.parameters.speech_transmission_index_indirect(pf.Signal(h, FS)))[0])
    assert theirs == pytest.approx(an.sti_exponential(t60), abs=JND["STI"][1] / 2)
