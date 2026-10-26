use crate::library::Library;
use uuid::Uuid;

#[test]
fn uuid_library_round_trip_preserves_ids_memos_timestamps_and_manifest_on_load() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("mix.wav");
    std::fs::write(&source, b"source").unwrap();
    let track_id = Uuid::parse_str("c2d1dd5e-7afe-4027-8b53-15c5e21a0334").unwrap();
    let job_id = Uuid::new_v4();
    let mut library = Library::default();
    let track = crate::library::import(dir.path(), source, track_id).unwrap();
    let input = track.input.clone();
    library.tracks.push(track);
    let mut job = crate::model::test_job(
        input,
        dir.path().join("result.wav"),
        library.draft.parse().unwrap(),
        0,
    );
    job.id = job_id;
    job.track_id = track_id;
    job.created_at = Some("2026-10-26T03:00:00Z".parse().unwrap());
    job.memo = "検証済みのマスター".into();
    job.status = crate::model::Status::Succeeded;
    library.jobs.push(job);
    library.selected = Some((track_id, Some(job_id)));
    library.comparison_job = Some(job_id);
    library.save(dir.path()).unwrap();
    let path = dir.path().join("library.json");
    let original = std::fs::read(&path).unwrap();
    let loaded = Library::load(dir.path()).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    assert_eq!(loaded.tracks[0].id, track_id);
    assert_eq!(loaded.jobs[0].id, job_id);
    assert_eq!(
        serde_json::to_value(&loaded).unwrap(),
        serde_json::to_value(&library).unwrap()
    );
    loaded.save(dir.path()).unwrap();
    let saved = Library::load(dir.path()).unwrap();
    assert_eq!(saved.jobs[0].memo, "検証済みのマスター");
    assert_eq!(saved.jobs[0].created_at, library.jobs[0].created_at);
    assert_eq!(saved.selected, library.selected);
    assert_eq!(saved.comparison_job, library.comparison_job);
}
