"""Pass/fail tolerances, in one place so every test and report uses the same numbers.

JND values are the subjective limens of ISO 3382-1:2009 Table A.1 (EDT, C80,
D50, Ts, G). T20/T30 have no JND in that table; 5 % is the common
convention. The STI JND of 0.03 is the value usually quoted alongside
IEC 60268-16 (Bradley 1986; Bork 2005 round robin). Re-check against the
standards on file before relying on them in a report.
"""

# name: (kind, value). kind "rel" = fraction of reference, "abs" = parameter units
JND = {
    "EDT": ("rel", 0.05),
    "T20": ("rel", 0.05),
    "T30": ("rel", 0.05),
    "C50": ("abs", 1.0),   # dB
    "C80": ("abs", 1.0),   # dB
    "D50": ("abs", 0.05),
    "Ts": ("abs", 0.010),  # s
    "G": ("abs", 1.0),     # dB
    "STI": ("abs", 0.03),
}

# Tier 1, analytic: the engine must reproduce its own model almost exactly.
# Expressed as a fraction of one JND unless noted.
TIER1 = {
    "ism_delay_samples": 1,          # arrival time of every image, in samples
    "ism_energy_db": 0.1,            # level of every image
    "ism_count": 0,                  # exact
    "decay_t_jnd": 0.5,              # diffuse-room T against Eyring, half a JND
    "air_only_t_rel": 0.01,          # air-only room against 6 ln10/(c m)
    "mean_free_path_rel": 0.01,      # measured mean free path against 4V/S
    "energy_loss_rel": 1e-3,         # rays lost in a closed lossless room
    "reciprocity_db": 0.1,           # source/receiver swap, ISM part
    "metrics_vs_closed_form_jnd": 0.02,
    "metrics_rust_vs_python_jnd": 0.01,
}

# Tiers 2 to 4: simulated against measured or against another solver.
# Pass = within 1 JND; "acceptable" = within 2 JND, matching the spread seen
# in published round robins. Band-averaged 500 Hz-1 kHz and per octave band.
TIER_MEASURED = {"pass_jnd": 1.0, "acceptable_jnd": 2.0}


def within(name, value, reference, n_jnd=1.0):
    kind, jnd = JND[name]
    limit = n_jnd * jnd * (abs(reference) if kind == "rel" else 1.0)
    return abs(value - reference) <= limit


def error_in_jnd(name, value, reference):
    kind, jnd = JND[name]
    scale = jnd * (abs(reference) if kind == "rel" else 1.0)
    return (value - reference) / scale
