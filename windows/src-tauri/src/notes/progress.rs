use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Stage { Microphone, Playback, Speakers, Saving }

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Progress {
    pub stage: Stage,
    pub completed_seconds: f64,
    pub total_seconds: Option<f64>,
    pub started_at: u64,
    pub updated_at: u64,
}

#[derive(Clone, Serialize)]
pub(super) struct Changed<'a> {
    pub id: &'a str,
    pub progress: Progress,
}
