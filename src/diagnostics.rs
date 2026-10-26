//! Collect current application and analysis errors for status and reports.
use crate::App;

impl App {
    pub(super) fn diagnostic_messages(&self) -> Vec<String> {
        let mut errors = Vec::new();
        if let Some(error) = &self.error {
            errors.push(error.clone());
        }
        if let Some(track) = self.track() {
            if let Some(error) = &track.analysis_error {
                errors.push(format!("Original analysis: {error}"));
            }
            if let Some(error) = self.job().and_then(|job| job.analysis_error.as_ref()) {
                errors.push(format!("Selected analysis: {error}"));
            }
            for (label, report) in [
                ("Original", track.analysis.as_ref()),
                (
                    "Compare A",
                    self.comparison_job().and_then(|j| j.analysis.as_ref()),
                ),
                ("Selected", self.job().and_then(|j| j.analysis.as_ref())),
                ("Reference", self.reference_analysis()),
            ] {
                if let Some(error) = report.and_then(|a| a.sound_quality_error()) {
                    errors.push(format!("{label} Professionality: {error}"));
                }
            }
            if self.job().is_some_and(|j| j.config.reference.is_some())
                && let Some((_, Err(error))) = &self.reference_report
            {
                errors.push(format!("Reference analysis unavailable: {error}"));
            }
        }
        errors
    }
}
