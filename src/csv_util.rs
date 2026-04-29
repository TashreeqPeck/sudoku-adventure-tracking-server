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
    let mut out: Vec<Puzzle> = records
        .iter()
        .filter_map(|r| normalize_row(r))
        .collect();
    out.sort_by_key(|p| p.number);
    Ok(out)
}
