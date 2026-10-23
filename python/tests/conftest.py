import sys
from pathlib import Path

import pytest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from ra_validation.engine import get_engine  # noqa: E402


@pytest.fixture(scope="session")
def engine():
    eng = get_engine()
    if eng is None:
        pytest.skip("no engine selected (set RA_ENGINE=mock or RA_ENGINE=ra-cli)")
    return eng
