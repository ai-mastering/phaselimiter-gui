//! Display units follow the desktop analyzer and app.bakuage.com's statistics.
use crate::analysis::Analysis;

pub struct Metric {
    pub label: &'static str,
    key: &'static str,
    unit: &'static str,
    decimals: usize,
    scale: f64,
}
impl Metric {
    pub fn format(&self, analysis: Option<&Analysis>) -> String {
        let value = analysis
            .and_then(|a| {
                if self.key == "youtube_compensation" {
                    // The web application's historical estimate, not a live platform measurement.
                    a.number("youtube_loudness")
                        .map(|value| (-10.3 - value).min(0.))
                } else {
                    a.number(self.key)
                }
            })
            .map(|value| value * self.scale)
            .filter(|value| value.is_finite());
        value.map_or_else(
            || "—".into(),
            |v| format!("{:.*}{}", self.decimals, v, self.unit),
        )
    }
}
macro_rules! metric {
    ($label:literal, $key:literal, $unit:literal, $decimals:literal, $scale:literal) => {
        Metric {
            label: $label,
            key: $key,
            unit: $unit,
            decimals: $decimals,
            scale: $scale,
        }
    };
}
pub const ALL: &[Metric] = &[
    metric!("Loudness", "loudness", " LUFS", 1, 1.),
    metric!("Loudness range", "loudness_range", " LU", 1, 1.),
    metric!("True peak", "true_peak", " dBTP", 1, 1.),
    metric!("Professionality", "sound_quality", " %", 0, 100.),
    metric!("Professionality2", "sound_quality2", " %", 0, 100.),
    metric!("Peak", "peak", " dBFS", 1, 1.),
    metric!("RMS", "rms", " dBFS", 1, 1.),
    metric!(
        "True peak / LP 15 kHz",
        "lowpass_true_peak_15khz",
        " dBTP",
        1,
        1.
    ),
    metric!("YouTube loudness", "youtube_loudness", " LUFS", 1, 1.),
    metric!(
        "YouTube compensation*",
        "youtube_compensation",
        " dB",
        1,
        1.
    ),
    metric!("Micro dynamics", "sharpness", " dB", 1, 1.),
    metric!("Space", "space", " dB", 1, 1.),
    metric!("Acoustic entropy", "acoustic_entropy", " bit", 0, 1.),
    metric!("Dissonance", "dissonance", "", 2, 1.),
];

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scores_are_percentages_and_absent_values_are_not_zero() {
        let a = Analysis::new(json!({"sound_quality":0.72,"sound_quality2":0.815}));
        let score = ALL.iter().find(|m| m.key == "sound_quality").unwrap();
        let score2 = ALL.iter().find(|m| m.key == "sound_quality2").unwrap();
        let loudness = ALL.iter().find(|m| m.key == "loudness").unwrap();
        assert_eq!(score.format(Some(&a)), "72 %");
        assert_eq!(score2.format(Some(&a)), "82 %");
        assert_eq!(loudness.format(Some(&a)), "—");
        assert_eq!(score.format(None), "—");
    }

    #[test]
    fn youtube_estimate_matches_web_formula_including_zero_and_quiet_audio() {
        let metric = ALL
            .iter()
            .find(|m| m.key == "youtube_compensation")
            .unwrap();
        for (input, expected) in [(-8., "-2.3 dB"), (-18., "0.0 dB"), (0., "-10.3 dB")] {
            assert_eq!(
                metric.format(Some(&Analysis::new(json!({"youtube_loudness": input})))),
                expected
            );
        }
    }
}
