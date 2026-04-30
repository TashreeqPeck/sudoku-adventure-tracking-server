use std::collections::HashMap;

use anyhow::Context;

use crate::models::Puzzle;

pub fn parse_csv_records(text: &str) -> anyhow::Result<Vec<HashMap<String, String>>> {
    let mut r = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(text.as_bytes());
    let headers = r.headers()?.clone();
    let mut out = Vec::new();
    for rec in r.into_records() {
        let rec = rec?;
        let mut map = HashMap::new();
        for (i, h) in headers.iter().enumerate() {
            if let Some(f) = rec.get(i) {
                map.insert(h.to_string(), f.to_string());
            }
        }
        out.push(map);
    }
    Ok(out)
}

pub fn normalize_row(row: &HashMap<String, String>) -> Option<Puzzle> {
    let num_str = row
        .get("Puzzle Number")
        .map(|s| s.trim())
        .unwrap_or("")
        .to_string();
    let number: i32 = num_str.parse().ok()?;
    Some(Puzzle {
        number,
        title: row.get("Title").cloned().unwrap_or_default(),
        setter: row.get("Setter").cloned().unwrap_or_default(),
        constraints: row.get("Constraints").cloned().unwrap_or_default(),
        puzzle_link: row.get("Puzzle Link").cloned().unwrap_or_default(),
        video_link: row.get("Video Link").cloned().unwrap_or_default(),
    })
}

pub fn puzzles_from_csv(text: &str) -> anyhow::Result<Vec<Puzzle>> {
    let records = parse_csv_records(text).context("csv parse")?;
    let mut out: Vec<Puzzle> = records.iter().filter_map(normalize_row).collect();
    out.sort_by_key(|p| p.number);
    Ok(out)
}

/// CSV suitable for **File → Import** in Google Sheets or merge into a copy of the puzzle list.
/// Columns align with sheet sync and with `import_progress_from_csv_records` (`Solved`, `Time`, …).
pub fn progress_export_csv(merged: &[crate::models::MergedPuzzle]) -> anyhow::Result<String> {
    use crate::logic::format_seconds;

    let mut w = csv::WriterBuilder::new()
        .from_writer(Vec::new());

    w.write_record([
        "Puzzle Number",
        "Title",
        "Setter",
        "Constraints",
        "Puzzle Link",
        "Video Link",
        "Solved",
        "Time (Include Hour, like 0:04:50)",
        "Skipped",
        "Video Used",
    ])
    .context("csv header")?;

    for p in merged {
        let time = p
            .time_seconds
            .map(format_seconds)
            .unwrap_or_default();
        let video = p.video_used.as_deref().unwrap_or("");
        w.write_record([
            p.base.number.to_string(),
            p.base.title.clone(),
            p.base.setter.clone(),
            p.base.constraints.clone(),
            p.base.puzzle_link.clone(),
            p.base.video_link.clone(),
            if p.solved {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            },
            time,
            if p.skipped {
                "TRUE".to_string()
            } else {
                "FALSE".to_string()
            },
            video.to_string(),
        ])
        .context("csv row")?;
    }

    let buf = w.into_inner().context("csv finish")?;
    String::from_utf8(buf).context("csv utf8")
}
