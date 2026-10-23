//! Reverberation time from the room's volume and absorption, assuming a diffuse
//! field: Sabine, and Eyring, which stays right for well-damped rooms. Both ignore
//! where the absorption is, so they read long for rooms with absorption on one
//! surface (an audience floor) and are a check on the ray tracer, not a substitute.

/// Sabine and Eyring reverberation times for one band, s.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reverberation {
    pub sabine_s: f64,
    pub eyring_s: f64,
}

/// `area_m2` is the room's surface S, `absorption_area_m2` the sum of S·α over it,
/// `air_per_m` the energy attenuation m of the air and `c` the speed of sound.
pub fn reverberation(
    volume_m3: f64,
    area_m2: f64,
    absorption_area_m2: f64,
    air_per_m: f64,
    c: f64,
) -> Reverberation {
    // 60 dB of decay: T = 24 ln 10 · V / (c · A), which is 0.161 V / A at 343 m/s.
    let k = 24.0 * std::f64::consts::LN_10 / c;
    let air = 4.0 * air_per_m * volume_m3;
    let mean = if area_m2 > 0.0 { absorption_area_m2 / area_m2 } else { 0.0 };
    let eyring_area = if mean < 1.0 { -area_m2 * (1.0 - mean).ln() } else { f64::INFINITY };
    Reverberation {
        sabine_s: k * volume_m3 / (absorption_area_m2 + air),
        eyring_s: k * volume_m3 / (eyring_area + air),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shoebox_by_hand() {
        // 10 x 8 x 4 m: V = 320 m³, S = 304 m², α = 0.1 everywhere, no air.
        let r = reverberation(320.0, 304.0, 30.4, 0.0, 343.0);
        assert!((r.sabine_s - 0.161 * 320.0 / 30.4).abs() < 0.01, "{r:?}");
        assert!((r.eyring_s - 0.161 * 320.0 / (-304.0 * 0.9f64.ln())).abs() < 0.01, "{r:?}");
        assert!(r.eyring_s < r.sabine_s);
    }

    #[test]
    fn air_shortens_a_large_hall() {
        let dry = reverberation(20_000.0, 5_000.0, 1_000.0, 0.0, 343.0);
        let wet = reverberation(20_000.0, 5_000.0, 1_000.0, 0.0103, 343.0); // 8 kHz, 50 %
        assert!(wet.sabine_s < 0.6 * dry.sabine_s, "{dry:?} {wet:?}");
    }

    #[test]
    fn eyring_goes_to_zero_for_full_absorption() {
        let r = reverberation(100.0, 130.0, 130.0, 0.0, 343.0);
        assert_eq!(r.eyring_s, 0.0);
        assert!(r.sabine_s > 0.0);
    }
}
