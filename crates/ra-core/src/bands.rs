use serde::{Deserialize, Serialize};

/// The frequency bands a project computes in. Named in the manifest.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
#[serde(rename_all = "snake_case")]
pub enum BandSet {
    /// 8 octave bands, 63 Hz to 8 kHz.
    #[serde(rename = "octave_63_8k")]
    Octave63To8k,
    /// 24 third-octave bands, 50 Hz to 10 kHz.
    #[serde(rename = "third_50_10k")]
    Third50To10k,
}

/// One band of a [`BandSet`], by index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Band {
    pub index: usize,
    pub centre_hz: f32,
}

impl BandSet {
    /// Nominal centre frequencies in Hz.
    pub fn centres_hz(self) -> &'static [f32] {
        match self {
            BandSet::Octave63To8k => &[63.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0],
            BandSet::Third50To10k => &[
                50.0, 63.0, 80.0, 100.0, 125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0,
                1000.0, 1250.0, 1600.0, 2000.0, 2500.0, 3150.0, 4000.0, 5000.0, 6300.0, 8000.0, 10000.0,
            ],
        }
    }

    pub fn len(self) -> usize {
        self.centres_hz().len()
    }

    pub fn is_empty(self) -> bool {
        false
    }

    pub fn bands(self) -> impl Iterator<Item = Band> {
        self.centres_hz().iter().enumerate().map(|(index, &centre_hz)| Band { index, centre_hz })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_counts() {
        assert_eq!(BandSet::Octave63To8k.len(), 8);
        assert_eq!(BandSet::Third50To10k.len(), 24);
    }

    #[test]
    fn manifest_names() {
        let s = serde_json::to_string(&BandSet::Octave63To8k).unwrap();
        assert_eq!(s, "\"octave_63_8k\"");
    }
}
