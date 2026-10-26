//! Read legacy execution paths from config, but write the separated job format.
use super::{Config, ExecutionTools, Job, Status};
use crate::analysis::Analysis;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use uuid::Uuid;

#[derive(Deserialize)]
pub(super) struct SavedJob {
    #[serde(default)]
    command: Option<String>,
    #[serde(default)]
    track_id: Uuid,
    #[serde(default)]
    output_hash: Option<String>,
    #[serde(default)]
    input_hash: Option<String>,
    #[serde(default)]
    reference_hash: Option<String>,
    #[serde(default)]
    analysis: Option<Analysis>,
    #[serde(default)]
    analysis_error: Option<String>,
    #[serde(default)]
    limiting_error: Option<f64>,
    #[serde(default)]
    tools: Option<ExecutionTools>,
    #[serde(default)]
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    #[serde(default)]
    memo: String,
    id: Uuid,
    input: PathBuf,
    output: PathBuf,
    config: Value,
    status: Status,
    progress: f32,
    log: Vec<String>,
}

impl TryFrom<SavedJob> for Job {
    type Error = String;
    fn try_from(saved: SavedJob) -> Result<Self, String> {
        let tools = match saved.tools {
            Some(tools) => tools,
            None => serde_json::from_value(saved.config.clone())
                .map_err(|e| format!("Invalid legacy execution paths: {e}"))?,
        };
        let config: Config = serde_json::from_value(saved.config).map_err(|e| e.to_string())?;
        Ok(Self {
            id: saved.id,
            created_at: saved.created_at,
            memo: saved.memo,
            track_id: saved.track_id,
            input: saved.input,
            output: saved.output,
            config,
            tools,
            status: saved.status,
            progress: saved.progress,
            log: saved.log,
            command: saved.command,
            output_hash: saved.output_hash,
            input_hash: saved.input_hash,
            reference_hash: saved.reference_hash,
            analysis: saved.analysis,
            analysis_error: saved.analysis_error,
            limiting_error: saved.limiting_error,
        })
    }
}
