"""Python reference metrics against closed forms and against pyrato."""
import numpy as np
import pytest

from ra_validation import analytic as an
from ra_validation import metrics as mt
from ra_validation.tolerances import error_in_jnd

FS = 48000


@pytest.mark.parametrize("t60", [0.4, 1.0, 2.5])
def test_metrics_on_ideal_exponential(t60):
    e = an.exponential_energy_envelope(t60, FS, length_s=2.5 * t60)
    got = mt.iso3382(e, FS)
    ref = an.exponential_decay_parameters(t60)
    for k, v in ref.items():
        if k == "Ts":  # truncation of the tail shortens Ts very slightly
            assert abs(error_in_jnd(k, got[k], v)) < 0.05, k
        else:
            assert abs(error_in_jnd(k, got[k], v)) < 0.02, (k, got[k], v)


@pytest.mark.parametrize("t60", [0.5, 1.0, 2.0])
def test_sti_indirect_on_ideal_exponential(t60):
    e = an.exponential_energy_envelope(t60, FS, length_s=max(1.6, 3 * t60))
    got = mt.sti_indirect([e] * 7, FS)
    assert got == pytest.approx(an.sti_exponential(t60), abs=0.003)


def test_onset_detection_skips_leading_silence():
    e = np.concatenate([np.zeros(480), an.exponential_energy_envelope(1.0, FS, 2.0)])
    assert mt.onset_index(np.sqrt(e)) == 480
    assert mt.iso3382(e, FS)["C80"] == pytest.approx(an.exponential_decay_parameters(1.0)["C80"], abs=0.02)
