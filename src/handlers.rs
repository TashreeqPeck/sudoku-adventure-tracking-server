use std::time::Duration;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::Response;
use axum::Json;
use serde::Deserialize;
use serde_json::json;

use crate::db;
use crate::error::ApiError;
use crate::logic::{
    compute_stats, merge_puzzles, normalize_video_used, parse_time_to_seconds, passes_filters,
    resolve_import_csv_url, unique_constraints,
};
use crate::models::{CatalogEntry, FiltersBody, ImportBody, ProgressBody};
use crate::sync::{download_import_csv_text, sync_from_sheet};
use crate::AppState;

#[derive(Debug, Deserialize, Default)]
pub struct StateQuery {
    pub include: Option<String>,
    pub exclude: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ProgressPath {
    number: i32,
}

pub async fn api_health(State(st): State<AppState>) -> Json<serde_json::Value> {
    let cache = st.cache.read().await;
    let meta = st.meta.read().await;
    Json(json!({
        "ok": true,
        "puzzleCount": cache.len(),
        "lastSyncAt": meta.last_sync_at,
        "lastSyncError": meta.last_sync_error,
        "sheetSyncIntervalMs": st.sheet_sync_interval_ms,
    }))
}

pub async fn api_state(
    State(st): State<AppState>,
    Query(q): Query<StateQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let (mut include, mut exclude) = db::load_filter_settings(&st.pool).await?;
    let mut should_persist = false;
    if let Some(v) = q.include {
        include = v;
        should_persist = true;
    }
    if let Some(v) = q.exclude {
        exclude = v;
        should_persist = true;
    }
    if should_persist {
        db::save_filter_settings(&st.pool, &include, &exclude).await?;
    }
    let progress = db::load_progress_map(&st.pool).await?;
    let cache = st.cache.read().await.clone();
    let merged = merge_puzzles(&cache, &progress);
    let filtered: Vec<_> = merged
        .iter()
        .cloned()
        .filter(|p| passes_filters(&p.base.constraints, &include, &exclude))
        .collect();
    let next = filtered
        .iter()
        .find(|p| !p.solved && !p.skipped)
        .cloned();
    let stats_all = compute_stats(&merged);
    let stats_filtered = compute_stats(&filtered);
    let catalog: Vec<CatalogEntry> = merged
        .iter()
        .map(|p| CatalogEntry {
            merged: p.clone(),
            matches_filter: passes_filters(&p.base.constraints, &include, &exclude),
        })
        .collect();
    let meta = st.meta.read().await;
    Ok(Json(json!({
        "lastSyncAt": meta.last_sync_at,
        "lastSyncError": meta.last_sync_error,
        "sheetSyncIntervalMs": st.sheet_sync_interval_ms,
        "exportUrl": st.export_url,
        "uniqueConstraints": unique_constraints(&merged),
        "statsAll": stats_all,
        "statsFiltered": stats_filtered,
        "next": next,
        "include": include,
        "exclude": exclude,
        "puzzles": filtered,
        "catalog": catalog,
    })))
}

pub async fn api_progress_put(
    State(st): State<AppState>,
    Path(path): Path<ProgressPath>,
    Json(body): Json<ProgressBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let puzzle_number = path.number;
    if puzzle_number <= 0 {
        return Err(ApiError::BadRequest("Invalid puzzle number".into()));
    }
    let existing = db::get_progress_row(&st.pool, puzzle_number).await?;
    let mut solved = existing.as_ref().map(|e| e.solved).unwrap_or(false);
    let mut skipped = existing.as_ref().map(|e| e.skipped).unwrap_or(false);
    let mut video_used = existing.as_ref().and_then(|e| e.video_used.clone());
    let mut time_seconds = existing.as_ref().and_then(|e| e.time_seconds);

    if body.active == Some(true) || body.clear_status == Some(true) {
        solved = false;
        skipped = false;
    } else {
        if let Some(sk) = body.skipped {
            skipped = sk;
            if skipped {
                solved = false;
            }
        }
        if let Some(so) = body.solved {
            solved = so;
            if solved {
                skipped = false;
            }
        }
    }

    if let Some(raw) = &body.video_used {
        if raw.is_null() {
            video_used = None;
        } else if let Some(v) = raw.as_str() {
            if v.is_empty() {
                video_used = None;
            } else {
                let n = normalize_video_used(v).ok_or_else(|| {
                    ApiError::BadRequest("videoUsed must be none, partial, or full".into())
                })?;
                video_used = Some(n);
            }
        } else {
            return Err(ApiError::BadRequest(
                "videoUsed must be none, partial, or full".into(),
            ));
        }
    }

    if body.clear_time == Some(true) {
        time_seconds = None;
    } else if let Some(ts) = &body.time_seconds {
        if !ts.is_null() {
            if let Some(n) = ts.as_i64() {
                if n < 0 {
                    return Err(ApiError::BadRequest("Invalid timeSeconds".into()));
                }
                time_seconds = Some(n);
            } else if let Some(s) = ts.as_str() {
                if s.is_empty() {
                    // leave unchanged
                } else {
                    let n: i64 = s.parse().map_err(|_| {
                        ApiError::BadRequest("Invalid timeSeconds".into())
                    })?;
                    if n < 0 {
                        return Err(ApiError::BadRequest("Invalid timeSeconds".into()));
                    }
                    time_seconds = Some(n);
                }
            } else {
                return Err(ApiError::BadRequest("Invalid timeSeconds".into()));
            }
        }
    } else if let Some(ref t) = body.time {
        if !t.trim().is_empty() {
            time_seconds = Some(parse_time_to_seconds(t).ok_or_else(|| {
                ApiError::BadRequest(
                    "Use a time like 4:50, 04:50, 0:04:50, or 1:09:05 (minutes:seconds or hours:minutes:seconds)".into(),
                )
            })?);
        }
    }

    db::upsert_progress(
        &st.pool,
        puzzle_number,
        solved,
        skipped,
        video_used.as_deref(),
        time_seconds,
    )
    .await?;

    Ok(Json(json!({ "ok": true })))
}

pub async fn api_filters_put(
    State(st): State<AppState>,
    Json(body): Json<FiltersBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let include = body.include.unwrap_or_default();
    let exclude = body.exclude.unwrap_or_default();
    db::save_filter_settings(&st.pool, &include, &exclude).await?;
    Ok(Json(json!({
        "ok": true,
        "include": include,
        "exclude": exclude,
    })))
}

pub async fn api_refresh(State(st): State<AppState>) -> Result<Json<serde_json::Value>, ApiError> {
    match sync_from_sheet(&st.client, &st.export_url).await {
        Ok(puzzles) => {
            let mut cache = st.cache.write().await;
            *cache = puzzles;
            let mut meta = st.meta.write().await;
            meta.last_sync_error = None;
            meta.last_sync_at = Some(chrono::Utc::now().to_rfc3339());
            let count = cache.len();
            let at = meta.last_sync_at.clone();
            Ok(Json(json!({
                "ok": true,
                "count": count,
                "lastSyncAt": at,
            })))
        }
        Err(e) => {
            let mut meta = st.meta.write().await;
            meta.last_sync_error = Some(e.to_string());
            Err(ApiError::BadGateway(e.to_string()))
        }
    }
}

pub async fn api_export(State(st): State<AppState>) -> Result<Response, ApiError> {
    let progress = db::load_progress_map(&st.pool).await?;
    let cache = st.cache.read().await.clone();
    let merged = merge_puzzles(&cache, &progress);
    let csv = crate::csv_util::progress_export_csv(&merged)?;
    let filename = format!(
        "sudoku-adventure-export-{}.csv",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    let disposition = format!("attachment; filename=\"{filename}\"");
    let hv = HeaderValue::try_from(disposition.as_str()).map_err(|_| {
        ApiError::BadRequest("Invalid Content-Disposition".into())
    })?;
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "text/csv; charset=utf-8")
        .header(header::CONTENT_DISPOSITION, hv)
        .body(Body::from(csv))
        .map_err(|e| ApiError::Any(anyhow::Error::from(e)))
}

pub async fn api_import(
    State(st): State<AppState>,
    Json(body): Json<ImportBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let url = body.url.trim();
    if !url.starts_with("https://") {
        return Err(ApiError::BadRequest("URL must start with https://".into()));
    }
    let parsed = url::Url::parse(url).map_err(|_| ApiError::BadRequest("Invalid URL".into()))?;
    if parsed.scheme() != "https" {
        return Err(ApiError::BadRequest("Only https URLs are allowed".into()));
    }
    let replace_all = body.replace_all.unwrap_or(false);
    let fetch_url = resolve_import_csv_url(url);

    let text = tokio::time::timeout(
        Duration::from_secs(120),
        download_import_csv_text(&st.client, &fetch_url),
    )
    .await
    .map_err(|_| ApiError::GatewayTimeout("Download timed out".into()))?
    .map_err(ApiError::Any)?;

    const MAX: usize = 25 * 1024 * 1024;
    if text.is_empty() || text.len() > MAX {
        return Err(ApiError::BadRequest("CSV is empty or too large".into()));
    }

    let records = crate::csv_util::parse_csv_records(&text)?;
    if records.is_empty() {
        return Err(ApiError::BadRequest("No data rows found in CSV".into()));
    }

    let imported = db::import_progress_from_csv_records(&st.pool, &records, replace_all).await?;
    Ok(Json(json!({
        "ok": true,
        "importedCount": imported,
        "replaceAll": replace_all,
    })))
}
