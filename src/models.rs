use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
pub struct Puzzle {
    pub number: i32,
    pub title: String,
    pub setter: String,
    pub constraints: String,
    #[serde(rename = "puzzleLink")]
    pub puzzle_link: String,
    #[serde(rename = "videoLink")]
    pub video_link: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MergedPuzzle {
    #[serde(flatten)]
    pub base: Puzzle,
    pub solved: bool,
    pub skipped: bool,
    #[serde(rename = "videoUsed")]
    pub video_used: Option<String>,
    #[serde(rename = "videoUsedLabel")]
    pub video_used_label: String,
    #[serde(rename = "timeSeconds")]
    pub time_seconds: Option<i64>,
    #[serde(rename = "timeFormatted")]
    pub time_formatted: String,
}

#[derive(Debug, Serialize)]
pub struct CatalogEntry {
    #[serde(flatten)]
    pub merged: MergedPuzzle,
    #[serde(rename = "matchesFilter")]
    pub matches_filter: bool,
}

#[derive(Debug, Serialize)]
pub struct Stats {
    #[serde(rename = "totalPuzzles")]
    pub total_puzzles: usize,
    #[serde(rename = "solvedCount")]
    pub solved_count: usize,
    #[serde(rename = "skippedCount")]
    pub skipped_count: usize,
    #[serde(rename = "activeRemaining")]
    pub active_remaining: usize,
    #[serde(rename = "averageSeconds")]
    pub average_seconds: Option<i64>,
    #[serde(rename = "averageFormatted")]
    pub average_formatted: String,
    #[serde(rename = "timedCount")]
    pub timed_count: usize,
}

#[derive(Debug, Deserialize)]
pub struct ProgressBody {
    pub solved: Option<bool>,
    pub skipped: Option<bool>,
    pub active: Option<bool>,
    #[serde(rename = "clearStatus")]
    pub clear_status: Option<bool>,
    #[serde(rename = "videoUsed")]
    pub video_used: Option<serde_json::Value>,
    #[serde(rename = "clearTime")]
    pub clear_time: Option<bool>,
    pub time: Option<String>,
    #[serde(rename = "timeSeconds")]
    pub time_seconds: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct ImportBody {
    pub url: String,
    #[serde(rename = "replaceAll")]
    pub replace_all: Option<bool>,
}
