use std::collections::HashMap;

use crate::models::{MergedPuzzle, Puzzle, Stats};

pub fn parse_time_to_seconds(s: &str) -> Option<i64> {
    let s = s.trim().replace(char::is_whitespace, "");
    if s.is_empty() {
        return None;
    }
    let parts: Vec<&str> = s.split(':').map(|p| p.trim()).collect();
    if parts.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit())) {
        return None;
    }
    match parts.len() {
        3 => {
            let h: i64 = parts[0].parse().ok()?;
            let m: i64 = parts[1].parse().ok()?;
            let sec: i64 = parts[2].parse().ok()?;
            if m > 59 || sec > 59 {
                return None;
            }
            Some(h * 3600 + m * 60 + sec)
        }
        2 => {
            let minutes: i64 = parts[0].parse().ok()?;
            let sec: i64 = parts[1].parse().ok()?;
            if sec > 59 {
                return None;
            }
            Some(minutes * 60 + sec)
        }
        _ => None,
    }
}

pub fn format_seconds(sec: i64) -> String {
    if sec < 0 {
        return String::new();
    }
    let h = sec / 3600;
    let m = (sec % 3600) / 60;
    let s = sec % 60;
    format!("{h}:{m:02}:{s:02}")
}

pub fn normalize_video_used(v: &str) -> Option<String> {
    match v.trim().to_lowercase().as_str() {
        "none" | "partial" | "full" => Some(v.trim().to_lowercase()),
        _ => None,
    }
}

pub fn video_used_label(video_used: Option<&str>) -> String {
    match video_used {
        Some("partial") => "Partial".into(),
        Some("full") => "Full".into(),
        _ => "Not used".into(),
    }
}

pub fn merge_puzzles(cache: &[Puzzle], progress: &HashMap<i32, ProgressRow>) -> Vec<MergedPuzzle> {
    cache
        .iter()
        .map(|p| {
            let pr = progress.get(&p.number);
            let solved = pr.map(|r| r.solved).unwrap_or(false);
            let skipped = pr.map(|r| r.skipped).unwrap_or(false);
            let video_used = pr.and_then(|r| r.video_used.clone());
            let time_seconds = pr.and_then(|r| r.time_seconds);
            let vu_str = video_used.as_deref();
            MergedPuzzle {
                base: p.clone(),
                solved,
                skipped,
                video_used_label: video_used_label(vu_str),
                time_seconds,
                time_formatted: time_seconds
                    .map(format_seconds)
                    .unwrap_or_default(),
                video_used,
            }
        })
        .collect()
}

#[derive(Clone, Default)]
pub struct ProgressRow {
    pub solved: bool,
    pub skipped: bool,
    pub video_used: Option<String>,
    pub time_seconds: Option<i64>,
}

pub fn tokenize_filters(raw: &str) -> Vec<String> {
    raw.split(['\n', ','])
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect()
}

pub fn passes_filters(constraints_text: &str, include_raw: &str, exclude_raw: &str) -> bool {
    let c = constraints_text.to_lowercase();
    let include = tokenize_filters(include_raw);
    let exclude = tokenize_filters(exclude_raw);
    if exclude
        .iter()
        .any(|term| c.contains(&term.to_lowercase()))
    {
        return false;
    }
    if include.is_empty() {
        return true;
    }
    include
        .iter()
        .any(|term| c.contains(&term.to_lowercase()))
}

pub fn compute_stats(merged: &[MergedPuzzle]) -> Stats {
    let solved: Vec<_> = merged.iter().filter(|p| p.solved).collect();
    let skipped = merged.iter().filter(|p| p.skipped && !p.solved).count();
    let active = merged.iter().filter(|p| !p.solved && !p.skipped).count();
    let with_time: Vec<_> = solved
        .iter()
        .filter(|p| p.time_seconds.is_some())
        .collect();
    let sum: i64 = with_time.iter().filter_map(|p| p.time_seconds).sum();
    let n = with_time.len();
    let (avg_sec, avg_fmt) = if n > 0 {
        let a = (sum as f64 / n as f64).round() as i64;
        (Some(a), format_seconds(a))
    } else {
        (None, String::new())
    };
    Stats {
        total_puzzles: merged.len(),
        solved_count: solved.len(),
        skipped_count: skipped,
        active_remaining: active,
        average_seconds: avg_sec,
        average_formatted: avg_fmt,
        timed_count: n,
    }
}

pub fn normalize_constraint_key(s: &str) -> String {
    s.trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn unique_constraints(merged: &[MergedPuzzle]) -> Vec<String> {
    let mut by_norm: HashMap<String, String> = HashMap::new();
    for p in merged {
        for part in p.base.constraints.split(',') {
            let t = part.trim().split_whitespace().collect::<Vec<_>>().join(" ");
            if t.is_empty() {
                continue;
            }
            let key = normalize_constraint_key(&t);
            by_norm.entry(key).or_insert_with(|| t.to_string());
        }
    }
    let mut v: Vec<String> = by_norm.into_values().collect();
    v.sort_by(|a, b| a.to_lowercase().cmp(&b.to_lowercase()));
    v
}

pub fn parse_solved_from_import(row: &HashMap<String, String>) -> bool {
    let v = row
        .get("Solved")
        .map(|s| s.trim().to_uppercase())
        .unwrap_or_default();
    v == "TRUE" || v == "1" || v == "YES"
}

pub fn get_time_string_from_import_row(row: &HashMap<String, String>) -> String {
    let named_keys = [
        "Time (Include Hour, like 0:04:50)",
        "Time",
        "Time ",
    ];
    for k in named_keys {
        if let Some(v) = row.get(k) {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    for (k, v) in row.iter() {
        let kt = k.trim();
        if kt.to_lowercase().starts_with("time") {
            let t = v.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
    }
    String::new()
}

pub fn extract_sheet_id(trimmed: &str) -> Option<String> {
    let needle = "/spreadsheets/d/";
    let start = trimmed.find(needle)? + needle.len();
    let rest = &trimmed[start..];
    let end = rest
        .find(|c| c == '/' || c == '?' || c == '#' || c == '&')
        .unwrap_or(rest.len());
    let id = rest.get(..end)?;
    if id.is_empty() {
        return None;
    }
    if !id
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    {
        return None;
    }
    Some(id.to_string())
}

pub fn resolve_import_csv_url(url_string: &str) -> String {
    let trimmed = url_string.trim();
    let lower = trimmed.to_lowercase();
    if lower.contains("/export")
        && (lower.contains("format=csv") || lower.contains("format%3dcsv"))
    {
        if let Ok(mut u) = url::Url::parse(trimmed) {
            u.set_fragment(None);
            return u.to_string();
        }
        return trimmed.to_string();
    }
    let sheet_id = extract_sheet_id(trimmed);
    let Some(sheet_id) = sheet_id else {
        return trimmed.to_string();
    };
    let host_ok = url::Url::parse(trimmed)
        .map(|u| u.host_str() == Some("docs.google.com"))
        .unwrap_or(false);
    if !host_ok {
        return trimmed.to_string();
    }
    let mut gid = "0".to_string();
    if let Ok(u) = url::Url::parse(trimmed) {
        if let Some(q) = u.query_pairs().find(|(k, _)| k == "gid").map(|(_, v)| v.into_owned()) {
            if q.chars().all(|c| c.is_ascii_digit()) {
                gid = q;
            }
        }
        if let Some(fragment) = u.fragment() {
            if let Some(m) = fragment.split('&').find_map(|p| {
                p.strip_prefix("gid=")
                    .filter(|g| g.chars().all(|c| c.is_ascii_digit()))
            }) {
                if gid == "0" {
                    gid = m.to_string();
                }
            }
        }
    }
    format!("https://docs.google.com/spreadsheets/d/{sheet_id}/export?format=csv&gid={gid}")
}
