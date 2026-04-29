use std::collections::HashMap;
use std::path::Path;

use anyhow::Context;
use chrono::Utc;
use sqlx::sqlite::SqliteConnectOptions;
use sqlx::{Row, SqlitePool};

pub async fn init_pool(data_dir: &Path) -> anyhow::Result<SqlitePool> {
    std::fs::create_dir_all(data_dir).context("create data dir")?;
    let db_path = data_dir.join("progress.sqlite");
    let opts = SqliteConnectOptions::new()
        .filename(&db_path)
        .create_if_missing(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(opts)
        .await
        .context("connect sqlite")?;

    sqlx::query(
        r#"CREATE TABLE IF NOT EXISTS progress (
        puzzle_number INTEGER PRIMARY KEY,
        solved INTEGER NOT NULL DEFAULT 0,
        skipped INTEGER NOT NULL DEFAULT 0,
        video_used TEXT,
        time_seconds INTEGER,
        updated_at TEXT NOT NULL
    )"#,
    )
    .execute(&pool)
    .await?;

    migrate_columns(&pool).await?;
    Ok(pool)
}

async fn migrate_columns(pool: &SqlitePool) -> anyhow::Result<()> {
    let cols: Vec<String> = sqlx::query("PRAGMA table_info(progress)")
        .fetch_all(pool)
        .await?
        .into_iter()
        .filter_map(|r| r.try_get::<String, _>("name").ok())
        .collect();
    if !cols.iter().any(|c| c == "skipped") {
        sqlx::query("ALTER TABLE progress ADD COLUMN skipped INTEGER NOT NULL DEFAULT 0")
            .execute(pool)
            .await?;
    }
    if !cols.iter().any(|c| c == "video_used") {
        sqlx::query("ALTER TABLE progress ADD COLUMN video_used TEXT")
            .execute(pool)
            .await?;
    }
    Ok(())
}

pub async fn load_progress_map(pool: &SqlitePool) -> anyhow::Result<HashMap<i32, crate::logic::ProgressRow>> {
    let rows = sqlx::query(
        "SELECT puzzle_number, solved, skipped, video_used, time_seconds FROM progress",
    )
    .fetch_all(pool)
    .await?;
    let mut m = HashMap::new();
    for r in rows {
        let num: i32 = r.try_get("puzzle_number")?;
        m.insert(
            num,
            crate::logic::ProgressRow {
                solved: r.get::<i64, _>("solved") != 0,
                skipped: r.get::<i64, _>("skipped") != 0,
                video_used: r.try_get("video_used").ok(),
                time_seconds: r.try_get("time_seconds").ok(),
            },
        );
    }
    Ok(m)
}

pub async fn upsert_progress(
    pool: &SqlitePool,
    puzzle_number: i32,
    solved: bool,
    skipped: bool,
    video_used: Option<&str>,
    time_seconds: Option<i64>,
) -> anyhow::Result<()> {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        r#"INSERT INTO progress (puzzle_number, solved, skipped, video_used, time_seconds, updated_at)
        VALUES (?, ?, ?, ?, ?, ?)
        ON CONFLICT(puzzle_number) DO UPDATE SET
          solved = excluded.solved,
          skipped = excluded.skipped,
          video_used = excluded.video_used,
          time_seconds = excluded.time_seconds,
          updated_at = excluded.updated_at"#,
    )
    .bind(puzzle_number)
    .bind(if solved { 1i64 } else { 0 })
    .bind(if skipped { 1i64 } else { 0 })
    .bind(video_used)
    .bind(time_seconds)
    .bind(now)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn get_progress_row(
    pool: &SqlitePool,
    puzzle_number: i32,
) -> anyhow::Result<Option<crate::logic::ProgressRow>> {
    let r = sqlx::query(
        "SELECT solved, skipped, video_used, time_seconds FROM progress WHERE puzzle_number = ?",
    )
    .bind(puzzle_number)
    .fetch_optional(pool)
    .await?;
    Ok(r.map(|row| crate::logic::ProgressRow {
        solved: row.get::<i64, _>("solved") != 0,
        skipped: row.get::<i64, _>("skipped") != 0,
        video_used: row.try_get("video_used").ok(),
        time_seconds: row.try_get("time_seconds").ok(),
    }))
}

pub async fn import_progress_from_csv_records(
    pool: &SqlitePool,
    records: &[HashMap<String, String>],
    replace_all: bool,
) -> anyhow::Result<usize> {
    use crate::logic::{get_time_string_from_import_row, parse_solved_from_import, parse_time_to_seconds};

    let mut tx = pool.begin().await?;
    if replace_all {
        sqlx::query("DELETE FROM progress").execute(&mut *tx).await?;
    }
    let mut n = 0usize;
    let now = Utc::now().to_rfc3339();
    for row in records {
        let num_str = row.get("Puzzle Number").map(|s| s.as_str()).unwrap_or("").trim();
        let Ok(num) = num_str.parse::<i32>() else {
            continue;
        };
        let solved = parse_solved_from_import(row);
        let tstr = get_time_string_from_import_row(row);
        let time_seconds = if tstr.is_empty() {
            None
        } else {
            parse_time_to_seconds(&tstr)
        };
        sqlx::query(
            r#"INSERT INTO progress (puzzle_number, solved, skipped, video_used, time_seconds, updated_at)
            VALUES (?, ?, 0, NULL, ?, ?)
            ON CONFLICT(puzzle_number) DO UPDATE SET
              solved = excluded.solved,
              skipped = 0,
              video_used = NULL,
              time_seconds = excluded.time_seconds,
              updated_at = excluded.updated_at"#,
        )
        .bind(num)
        .bind(if solved { 1i64 } else { 0 })
        .bind(time_seconds)
        .bind(&now)
        .execute(&mut *tx)
        .await?;
        n += 1;
    }
    tx.commit().await?;
    Ok(n)
}
