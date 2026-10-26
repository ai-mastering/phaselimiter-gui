//! Descriptions traced through executable code, not README text or flag help.
//! Analyzer/limiter: 387d66903c62c771c96039f9810484ac34fb0a01 (bundled Intel runtime).
//! Detector: 6dd5105d7d9a7072fd3a374087ade0326786139d. Reaudit on runtime updates.
//! GUI: analysis::run_with_options / detect_sound_quality, Metric::format, backend::execute.
//! FFmpeg preparation forces two-channel float WAV without specifying a sample rate.
//! Analyzer: single_mode.h dispatch/JSON, peak.h, dynamics.h, acoustic_entropy.h;
//! bakuage: loudness_ebu_r128.{h,cpp}, sound_quality2.h, statistics.h, dissonance.cpp.
//! Detector: classic_detector/{data,detection}.cpp. Limiter: main.cpp PhaseLimitInplace.
//! In particular youtube_loudness[0] receives max_loudness, not the gated mean;
//! lowpass_true_peak_15khz is not max(true_peak, filtered_peak).
use crate::analysis::Analysis;

pub struct Metric {
    pub help: Help,
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
    ($help:ident, $label:literal, $key:literal, $unit:literal, $decimals:literal, $scale:literal) => {
        Metric {
            help: Help::$help,
            label: $label,
            key: $key,
            unit: $unit,
            decimals: $decimals,
            scale: $scale,
        }
    };
}
pub const ALL: &[Metric] = &[
    metric!(Loudness, "Loudness", "loudness", " LUFS", 1, 1.),
    metric!(
        LoudnessRange,
        "Loudness range",
        "loudness_range",
        " LU",
        1,
        1.
    ),
    metric!(TruePeak, "True peak", "true_peak", " dBFS", 1, 1.),
    metric!(
        Professionality,
        "Professionality",
        "sound_quality",
        " %",
        0,
        100.
    ),
    metric!(
        Professionality2,
        "Professionality2",
        "sound_quality2",
        " %",
        0,
        100.
    ),
    metric!(Peak, "Peak", "peak", " dBFS", 1, 1.),
    metric!(Rms, "RMS", "rms", " dBFS", 1, 1.),
    metric!(
        LowpassTruePeak,
        "True peak / LP 15 kHz",
        "lowpass_true_peak_15khz",
        " dBFS",
        1,
        1.
    ),
    metric!(
        YoutubeLoudness,
        "YouTube loudness",
        "youtube_loudness",
        " LUFS",
        1,
        1.
    ),
    metric!(
        YoutubeCompensation,
        "YouTube compensation",
        "youtube_compensation",
        " dB",
        1,
        1.
    ),
    metric!(MicroDynamics, "Micro dynamics", "sharpness", " dB", 1, 1.),
    metric!(Space, "Space", "space", " dB", 1, 1.),
    metric!(
        AcousticEntropy,
        "Acoustic entropy",
        "acoustic_entropy",
        " bit",
        0,
        1.
    ),
    metric!(Dissonance, "Dissonance", "dissonance", "", 2, 1.),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Help {
    Loudness,
    LoudnessRange,
    TruePeak,
    Professionality,
    Professionality2,
    Peak,
    Rms,
    LowpassTruePeak,
    YoutubeLoudness,
    YoutubeCompensation,
    MicroDynamics,
    Space,
    AcousticEntropy,
    Dissonance,
    LimitingError,
}

impl Help {
    pub fn description(self) -> &'static str {
        match self {
            Self::Loudness => {
                "Overall level from K-weighted stereo energy in 400 ms blocks, stepped every 100 ms. The backend averages block levels in dB, first above -70 and then above that mean minus 10 dB. It includes a final partial block; this is the backend calculation, not a strict standard-conformance reading."
            }
            Self::LoudnessRange => {
                "Spread between the 10th and 95th percentile block levels after gating. This backend uses 400 ms blocks every 100 ms, with an initial -70 gate followed by a mean-minus-20 dB gate. Larger values indicate a wider spread in level."
            }
            Self::TruePeak => {
                "Largest absolute reconstructed sample across both channels, using 4x oversampling and an FIR interpolation filter. Shown relative to full scale; it can exceed the sample peak."
            }
            Self::Professionality => {
                "Population-based score from audio_detector detect2. It compares loudness-normalized Mid/Side band means and covariance from 40 ms windows stepped every 10 ms. The app uses a 1% neighbor ratio and a 10% outlier ratio; the local outlier factor is mapped to a percentage. Higher means closer to the model’s typical local density, not a probability of professional quality."
            }
            Self::Professionality2 => {
                "Population-based score from the analyzer’s sound_quality2 model cache. It uses Mid/Side band means and covariance from 400 ms windows stepped every 100 ms. Local-outlier scores are ranked against the cached population and combined into a percentage. Higher means more typical of that model; it is a different model and scale from Professionality."
            }
            Self::Peak => {
                "Largest absolute decoded sample across both channels, expressed as 20 × log10(amplitude) relative to full scale. This measures sample peaks without interpolation."
            }
            Self::Rms => {
                "Average energy over all decoded samples in both channels, expressed as 10 × log10(mean squared amplitude). Includes quiet sections and silence, without loudness weighting or gating."
            }
            Self::LowpassTruePeak => {
                "Largest absolute peak after a fixed 15 kHz lowpass FIR filter with 4x oversampling. This row reports the filtered peak alone. The mastering Lowpass peak Hz setting does not change this measurement."
            }
            Self::YoutubeLoudness => {
                "Maximum block level after the backend’s custom YouTube frequency weighting, using 3-second windows every 100 ms, including the final partial window. The reported maximum is taken before gating. This is a local estimate, not a measurement of YouTube playback."
            }
            Self::YoutubeCompensation => {
                "Estimated attenuation calculated as min(0, -10.3 − YouTube loudness), in dB. Quiet audio receives 0 dB rather than a boost. The -10.3 target is fixed in this app; this does not query or predict current YouTube normalization."
            }
            Self::MicroDynamics => {
                "Variation in fast level changes: the standard deviation of the difference between short (25 ms) and long (200 ms) smoothed K-weighted levels. The backend applies two-pass gating and level floors. Larger values indicate more variation in those short-versus-long differences."
            }
            Self::Space => {
                "Average Side level minus Mid level in dB, using K-weighted audio, 25 ms energy smoothing, two-pass gating and level floors. Mid is (L + R) / √2 and Side is (L − R) / √2. Higher values mean more Side energy relative to Mid; this is not a reverberation-time measurement."
            }
            Self::AcousticEntropy => {
                "Variation of log-energy spectra across time and channels, using 40 mel bands from 0 to 11 kHz and roughly 46 ms windows every 23 ms. After applying a signal-dependent spectral floor, the backend sums log2 of covariance eigenvalues clamped to at least 1. Larger values mean more spectral variation under this model; the displayed bits are not file size or codec bitrate."
            }
            Self::Dissonance => {
                "Spectral roughness estimate from pairwise frequency-bin interactions using the backend’s Sethares curve. It uses roughly 372 ms windows with 50% overlap, normalizes by total spectral energy, then reports 10 × log10 of the result. Higher values mean more modeled roughness; this is not a musical harmony judgment."
            }
            Self::LimitingError => {
                "Error introduced by the phase-limiting stage, comparing its input with the downsampled limiter result before final encoding. The backend normalizes its weighted error objective by a noise reference, then converts it with 10 × log10(1 + normalized error × (10^0.1 − 1)). Zero means no error in that objective; larger values mean more change. It does not measure the whole mastering chain or the encoded file."
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn peak_measurements_use_full_scale_units_without_changing_values() {
        let a = Analysis::new(json!({"peak":-1.2,"true_peak":-0.8,"lowpass_true_peak_15khz":-0.5}));
        for (key, expected) in [
            ("peak", "-1.2 dBFS"),
            ("true_peak", "-0.8 dBFS"),
            ("lowpass_true_peak_15khz", "-0.5 dBFS"),
        ] {
            let m = ALL.iter().find(|m| m.key == key).unwrap();
            assert_eq!(m.format(Some(&a)), expected);
        }
    }
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
