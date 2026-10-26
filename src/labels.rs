//! Human labels are independent of persistent identity and processing settings.
use crate::{Job, Library};
use chrono::{Datelike, Local};

impl Library {
    pub fn job_label(&self, job: &Job) -> String {
        let time = self.job_time_label(job);
        let memo = job.memo.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut preview: String = memo.chars().take(60).collect();
        if memo.chars().count() > 60 {
            preview.push('…');
        }
        if preview.is_empty() {
            time
        } else {
            format!("{time} · {preview}")
        }
    }

    pub fn job_time_label(&self, job: &Job) -> String {
        let Some(created) = job.created_at else {
            return "Date unknown".into();
        };
        let local = created.with_timezone(&Local);
        let date = if local.year() == Local::now().year() {
            "%m/%d %H:%M:%S"
        } else {
            "%Y/%m/%d %H:%M:%S"
        };
        local.format(date).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike, Utc};
    use uuid::Uuid;

    fn library() -> Library {
        let mut library = Library::default();
        let mut job = crate::model::test_job(
            "input.wav".into(),
            "output.wav".into(),
            library.draft.parse().unwrap(),
            1,
        );
        job.id = Uuid::new_v4();
        job.created_at = Some(
            Utc::now()
                .with_second(10)
                .unwrap()
                .with_nanosecond(0)
                .unwrap(),
        );
        library.jobs.push(job);
        library
    }

    #[test]
    fn labels_use_local_time_then_memo_and_preserve_full_memo() {
        let mut library = library();
        let created = library.jobs[0].created_at.unwrap();
        let expected = created
            .with_timezone(&Local)
            .format("%m/%d %H:%M:%S")
            .to_string();
        assert_eq!(library.job_label(&library.jobs[0]), expected);
        library.jobs[0].memo = "  低域控えめ  比較用  ".into();
        assert_eq!(
            library.job_label(&library.jobs[0]),
            format!("{expected} · 低域控えめ 比較用")
        );
        library.jobs[0].memo = "あ".repeat(100);
        assert_eq!(
            library.job_label(&library.jobs[0]),
            format!("{expected} · {}…", "あ".repeat(60))
        );
        assert_eq!(library.jobs[0].memo.chars().count(), 100);
    }

    #[test]
    fn same_second_labels_can_match_without_changing_identity() {
        let mut library = library();
        let first_id = library.jobs[0].id;
        let mut second = library.jobs[0].clone();
        second.id = Uuid::new_v4();
        second.created_at = second.created_at.map(|t| t + chrono::Duration::seconds(1));
        library.jobs.push(second);
        let label = library.job_time_label(&library.jobs[0]);
        assert!(label.ends_with(":10"));
        assert_ne!(label, library.job_time_label(&library.jobs[1]));
        library.jobs[1].created_at = library.jobs[0]
            .created_at
            .map(|t| t + chrono::Duration::nanoseconds(1));
        assert!(library.job_time_label(&library.jobs[0]).ends_with(":10"));
        assert_eq!(
            library.job_time_label(&library.jobs[0]),
            library.job_time_label(&library.jobs[1])
        );
        library.jobs[1].created_at = library.jobs[0].created_at;
        assert_eq!(
            library.job_time_label(&library.jobs[0]),
            library.job_time_label(&library.jobs[1])
        );
        assert_eq!(library.jobs[0].id, first_id);
    }

    #[test]
    fn old_dates_include_year_and_missing_dates_are_not_invented() {
        let mut library = library();
        let created = Utc.with_ymd_and_hms(2000, 6, 15, 12, 0, 0).unwrap();
        library.jobs[0].created_at = Some(created);
        assert_eq!(
            library.job_time_label(&library.jobs[0]),
            created
                .with_timezone(&Local)
                .format("%Y/%m/%d %H:%M:%S")
                .to_string()
        );
        library.jobs[0].created_at = None;
        library.jobs[0].memo = "比較用".into();
        assert_eq!(library.job_label(&library.jobs[0]), "Date unknown · 比較用");
    }
}
