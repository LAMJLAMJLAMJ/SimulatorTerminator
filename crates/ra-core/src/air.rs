//! Sound in air: speed and ISO 9613-1:1993 atmospheric absorption. The maths goes through
//! `libm`, so the browser and native builds compute the same coefficients bit for bit.

use serde::{Deserialize, Serialize};

/// Temperature, humidity and pressure of the air in the room.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct AirConditions {
    pub temperature_c: f64,
    /// Relative humidity, %.
    pub humidity_pct: f64,
    #[serde(default = "standard_pressure")]
    pub pressure_kpa: f64,
}

const REFERENCE_PRESSURE_KPA: f64 = 101.325;
const REFERENCE_TEMPERATURE_K: f64 = 293.15;
const TRIPLE_POINT_K: f64 = 273.16;

fn standard_pressure() -> f64 {
    REFERENCE_PRESSURE_KPA
}

impl Default for AirConditions {
    /// 20 °C and 50 %, as ISO 3382-2 suggests when nothing is known.
    fn default() -> Self {
        AirConditions { temperature_c: 20.0, humidity_pct: 50.0, pressure_kpa: REFERENCE_PRESSURE_KPA }
    }
}

impl AirConditions {
    /// Speed of sound, m/s.
    pub fn speed_of_sound(&self) -> f64 {
        331.3 * libm::sqrt(1.0 + self.temperature_c / 273.15)
    }

    /// Pure-tone attenuation at `f_hz`, dB/m (ISO 9613-1 equations 3 to 5).
    pub fn attenuation_db_per_m(&self, f_hz: f64) -> f64 {
        let t = self.temperature_c + 273.15;
        let tr = t / REFERENCE_TEMPERATURE_K;
        let pr = self.pressure_kpa / REFERENCE_PRESSURE_KPA;
        let psat = libm::pow(10.0, -6.8346 * libm::pow(TRIPLE_POINT_K / t, 1.261) + 4.6151);
        let h = self.humidity_pct * psat / pr;
        let fr_o = pr * (24.0 + 4.04e4 * h * (0.02 + h) / (0.391 + h));
        let fr_n = pr
            * libm::pow(tr, -0.5)
            * (9.0 + 280.0 * h * libm::exp(-4.170 * (libm::pow(tr, -1.0 / 3.0) - 1.0)));
        let f2 = f_hz * f_hz;
        8.686
            * f2
            * (1.84e-11 / pr * libm::sqrt(tr)
                + libm::pow(tr, -2.5)
                    * (0.01275 * libm::exp(-2239.1 / t) / (fr_o + f2 / fr_o)
                        + 0.1068 * libm::exp(-3352.0 / t) / (fr_n + f2 / fr_n)))
    }

    /// Energy attenuation coefficient m at `f_hz`, 1/m: intensity falls as e^(-m·d).
    /// The `4mV` term of the reverberation formulas.
    pub fn energy_attenuation_per_m(&self, f_hz: f64) -> f64 {
        self.attenuation_db_per_m(f_hz) / (10.0 * std::f64::consts::LOG10_E)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_iso_9613_table() {
        // ISO 9613-1 Table 1, 20 °C, 50 %, dB/km.
        let air = AirConditions::default();
        for (f, table) in
            [(125.0, 0.444), (500.0, 2.73), (1000.0, 4.66), (2000.0, 9.86), (4000.0, 29.4), (8000.0, 104.0)]
        {
            let db_km = air.attenuation_db_per_m(f) * 1000.0;
            assert!((db_km / table - 1.0).abs() < 0.03, "{f} Hz: {db_km} dB/km, table {table}");
        }
    }

    #[test]
    fn speed_of_sound_at_20_c() {
        assert!((AirConditions::default().speed_of_sound() - 343.2).abs() < 0.2);
    }
}
