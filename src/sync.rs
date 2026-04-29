use std::collections::HashSet;

use anyhow::Context;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, USER_AGENT};

use crate::csv_util::puzzles_from_csv;
use crate::models::Puzzle;

const UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36";

fn fetch_headers() -> anyhow::Result<HeaderMap> {
    let mut h = HeaderMap::new();
    h.insert(ACCEPT, HeaderValue::from_static("text/csv,text/plain,*/*"));
    h.insert(USER_AGENT, HeaderValue::from_str(UA).context("ua header")?);
    Ok(h)
}

pub async fn fetch_sheet_csv(client: &reqwest::Client, export_url: &str) -> anyhow::Result<String> {
    let res = client
        .get(export_url)
        .headers(fetch_headers()?)
        .send()
        .await
        .context("sheet fetch")?;
    if !res.status().is_success() {
        anyhow::bail!("Sheet export HTTP {}", res.status());
    }
    Ok(res.text().await?)
}

pub async fn sync_from_sheet(
    client: &reqwest::Client,
    export_url: &str,
) -> anyhow::Result<Vec<Puzzle>> {
    let text = fetch_sheet_csv(client, export_url).await?;
    puzzles_from_csv(&text)
}

async fn try_once(
    client: &reqwest::Client,
    tried: &mut HashSet<String>,
    url: &str,
    last_status: &mut String,
) -> Option<String> {
    if !tried.insert(url.to_string()) {
        return None;
    }
    let headers = match fetch_headers() {
        Ok(h) => h,
        Err(_) => return None,
    };
    let res = match client.get(url).headers(headers).send().await {
        Ok(r) => r,
        Err(e) => {
            *last_status = e.to_string();
            return None;
        }
    };
    *last_status = res.status().to_string();
    if !res.status().is_success() {
        return None;
    }
    let text = match res.text().await {
        Ok(t) => t,
        Err(e) => {
            *last_status = e.to_string();
            return None;
        }
    };
    let start = text.trim_start().chars().take(64).collect::<String>().to_lowercase();
    if start.starts_with("<!doctype") || start.starts_with("<html") {
        return None;
    }
    Some(text)
}

pub async fn download_import_csv_text(
    client: &reqwest::Client,
    first_url: &str,
) -> anyhow::Result<String> {
    let mut tried = HashSet::new();
    let mut last_status = String::new();

    if let Some(t) = try_once(client, &mut tried, first_url, &mut last_status).await {
        return Ok(t);
    }

    if let Ok(mut u) = url::Url::parse(first_url) {
        let had_gid = u.query_pairs().any(|(k, _)| k == "gid");
        if had_gid {
            let kept: Vec<(String, String)> = u
                .query_pairs()
                .filter(|(k, _)| k != "gid")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            u.set_query(None);
            if !kept.is_empty() {
                let mut q = u.query_pairs_mut();
                for (k, v) in &kept {
                    q.append_pair(k, v);
                }
            }
            if let Some(t) = try_once(client, &mut tried, u.as_str(), &mut last_status).await {
                return Ok(t);
            }
        }
    }

    if let Some(id) = crate::logic::extract_sheet_id(first_url) {
        let u1 = format!("https://docs.google.com/spreadsheets/d/{id}/export?format=csv");
        if let Some(t) = try_once(client, &mut tried, &u1, &mut last_status).await {
            return Ok(t);
        }
        let u2 = format!("https://docs.google.com/spreadsheets/d/{id}/gviz/tq?tqx=out:csv&gid=0");
        if let Some(t) = try_once(client, &mut tried, &u2, &mut last_status).await {
            return Ok(t);
        }
    }

    let msg = if last_status.contains("404") {
        format!("Google returned Not Found ({last_status}). Open the sheet with **Share → General access → Anyone with the link: Viewer**, then try again. If the puzzle tab is not the first sheet, open that tab and copy the URL so it includes `gid=`.")
    } else {
        format!(
            "Could not download CSV{}",
            if last_status.is_empty() {
                String::new()
            } else {
                format!(" ({last_status})")
            }
        )
    };
    anyhow::bail!(msg)
}
