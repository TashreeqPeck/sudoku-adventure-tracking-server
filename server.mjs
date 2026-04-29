import express from "express";
import Database from "better-sqlite3";
import { parse } from "csv-parse/sync";
import fs from "fs";
import path from "path";
import { fileURLToPath } from "url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const DATA_DIR = process.env.DATA_DIR || path.join(__dirname, "data");
const SHEET_ID = process.env.SHEET_ID || "1y4BYBEuXbzReb_tx3bTUdKwynL2JveL3ob55g6c-D-Y";
const SHEET_GID = process.env.SHEET_GID || "0";
const PORT = parseInt(process.env.PORT || "3840", 10);
const SYNC_MS = Math.max(
  60_000,
  parseInt(
    process.env.SHEET_SYNC_INTERVAL_MS ||
      process.env.SYNC_INTERVAL_MS ||
      String(86_400_000),
    10
  )
);

const EXPORT_URL = `https://docs.google.com/spreadsheets/d/${SHEET_ID}/export?format=csv&gid=${SHEET_GID}`;

const fetchImportHeaders = {
  Accept: "text/csv,text/plain,*/*",
  "User-Agent":
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/122.0.0.0 Safari/537.36",
};

fs.mkdirSync(DATA_DIR, { recursive: true });
const dbPath = path.join(DATA_DIR, "progress.sqlite");
const db = new Database(dbPath);

db.exec(`
  CREATE TABLE IF NOT EXISTS progress (
    puzzle_number INTEGER PRIMARY KEY,
    solved INTEGER NOT NULL DEFAULT 0,
    skipped INTEGER NOT NULL DEFAULT 0,
    video_used TEXT,
    time_seconds INTEGER,
    updated_at TEXT NOT NULL
  );
`);

(function migrateProgressTable() {
  const cols = new Set(
    db.prepare("PRAGMA table_info(progress)").all().map((r) => r.name)
  );
  if (!cols.has("skipped")) {
    db.exec(
      "ALTER TABLE progress ADD COLUMN skipped INTEGER NOT NULL DEFAULT 0"
    );
  }
  if (!cols.has("video_used")) {
    db.exec("ALTER TABLE progress ADD COLUMN video_used TEXT");
  }
})();

const selectProgress = db.prepare(
  "SELECT puzzle_number, solved, skipped, video_used, time_seconds FROM progress"
);
const upsertProgress = db.prepare(`
  INSERT INTO progress (puzzle_number, solved, skipped, video_used, time_seconds, updated_at)
  VALUES (@puzzle_number, @solved, @skipped, @video_used, @time_seconds, @updated_at)
  ON CONFLICT(puzzle_number) DO UPDATE SET
    solved = excluded.solved,
    skipped = excluded.skipped,
    video_used = excluded.video_used,
    time_seconds = excluded.time_seconds,
    updated_at = excluded.updated_at
`);

let puzzleCache = [];
let lastSyncError = null;
let lastSyncAt = null;

/** Accepts H:MM:SS, HH:MM:SS, M:SS, MM:SS (minutes:seconds). */
function parseTimeToSeconds(str) {
  if (str == null || String(str).trim() === "") return null;
  const s = String(str).trim().replace(/\s+/g, "");
  const parts = s.split(":").map((p) => p.trim());
  if (parts.some((p) => p === "" || !/^\d+$/.test(p))) return null;

  if (parts.length === 3) {
    const h = parseInt(parts[0], 10);
    const m = parseInt(parts[1], 10);
    const sec = parseInt(parts[2], 10);
    if (m > 59 || sec > 59) return null;
    return h * 3600 + m * 60 + sec;
  }
  if (parts.length === 2) {
    const minutes = parseInt(parts[0], 10);
    const sec = parseInt(parts[1], 10);
    if (sec > 59) return null;
    return minutes * 60 + sec;
  }
  return null;
}

function formatSeconds(sec) {
  if (sec == null || sec < 0) return "";
  const h = Math.floor(sec / 3600);
  const m = Math.floor((sec % 3600) / 60);
  const s = sec % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function normalizeVideoUsed(v) {
  if (v == null || v === "") return null;
  const x = String(v).toLowerCase();
  if (x === "none" || x === "partial" || x === "full") return x;
  return null;
}

function normalizeRow(row) {
  const num = parseInt(String(row["Puzzle Number"] ?? "").trim(), 10);
  if (!Number.isFinite(num)) return null;
  return {
    number: num,
    title: row.Title ?? "",
    setter: row.Setter ?? "",
    constraints: row.Constraints ?? "",
    puzzleLink: row["Puzzle Link"] ?? "",
    videoLink: row["Video Link"] ?? "",
  };
}

async function syncFromSheet() {
  const res = await fetch(EXPORT_URL, {
    redirect: "follow",
    headers: {
      Accept: "text/csv,text/plain,*/*",
      "User-Agent": fetchImportHeaders["User-Agent"],
    },
  });
  if (!res.ok) throw new Error(`Sheet export HTTP ${res.status}`);
  const text = await res.text();
  const records = parse(text, {
    columns: true,
    skip_empty_lines: true,
    relax_column_count: true,
  });
  const out = [];
  for (const row of records) {
    const p = normalizeRow(row);
    if (p) out.push(p);
  }
  out.sort((a, b) => a.number - b.number);
  puzzleCache = out;
  lastSyncError = null;
  lastSyncAt = new Date().toISOString();
}

function loadProgressMap() {
  const map = new Map();
  for (const r of selectProgress.all()) {
    map.set(r.puzzle_number, {
      solved: !!r.solved,
      skipped: !!r.skipped,
      videoUsed: r.video_used,
      timeSeconds: r.time_seconds,
    });
  }
  return map;
}

function mergePuzzles(progressMap) {
  return puzzleCache.map((p) => {
    const pr = progressMap.get(p.number) || {};
    const skipped = pr.skipped ?? false;
    const solved = pr.solved ?? false;
    const videoUsed = pr.videoUsed ?? null;
    return {
      ...p,
      solved,
      skipped,
      videoUsed,
      videoUsedLabel:
        videoUsed === "partial"
          ? "Partial"
          : videoUsed === "full"
            ? "Full"
            : "Not used",
      timeSeconds: pr.timeSeconds ?? null,
      timeFormatted:
        pr.timeSeconds != null ? formatSeconds(pr.timeSeconds) : "",
    };
  });
}

function tokenizeFilters(raw) {
  if (!raw || typeof raw !== "string") return [];
  return raw
    .split(/[\n,]+/)
    .map((t) => t.trim())
    .filter(Boolean);
}

function passesFilters(constraintsText, includeRaw, excludeRaw) {
  const c = (constraintsText || "").toLowerCase();
  const include = tokenizeFilters(includeRaw);
  const exclude = tokenizeFilters(excludeRaw);
  if (exclude.some((term) => c.includes(term.toLowerCase()))) return false;
  if (include.length === 0) return true;
  return include.some((term) => c.includes(term.toLowerCase()));
}

function computeStats(merged) {
  const solved = merged.filter((p) => p.solved);
  const skipped = merged.filter((p) => p.skipped && !p.solved);
  const active = merged.filter((p) => !p.solved && !p.skipped);
  const withTime = solved.filter((p) => p.timeSeconds != null);
  const sum = withTime.reduce((a, p) => a + p.timeSeconds, 0);
  return {
    totalPuzzles: merged.length,
    solvedCount: solved.length,
    skippedCount: skipped.length,
    activeRemaining: active.length,
    averageSeconds:
      withTime.length > 0 ? Math.round(sum / withTime.length) : null,
    averageFormatted:
      withTime.length > 0 ? formatSeconds(Math.round(sum / withTime.length)) : "",
    timedCount: withTime.length,
  };
}

function normalizeConstraintKey(s) {
  return String(s || "")
    .trim()
    .replace(/\s+/g, " ")
    .toLowerCase();
}

function uniqueConstraints(merged) {
  const byNorm = new Map();
  for (const p of merged) {
    for (const part of (p.constraints || "").split(",")) {
      const t = String(part).trim().replace(/\s+/g, " ");
      if (!t) continue;
      const key = normalizeConstraintKey(t);
      if (!byNorm.has(key)) byNorm.set(key, t);
    }
  }
  return [...byNorm.values()].sort((a, b) =>
    a.localeCompare(b, undefined, { sensitivity: "base" })
  );
}

function parseSolvedFromImport(row) {
  const v = String(row.Solved ?? "").trim().toUpperCase();
  return v === "TRUE" || v === "1" || v === "YES";
}

function getTimeStringFromImportRow(row) {
  if (!row || typeof row !== "object") return "";
  const named =
    row["Time (Include Hour, like 0:04:50)"] ?? row.Time ?? row["Time "];
  if (named != null && String(named).trim() !== "") return String(named).trim();
  for (const [k, v] of Object.entries(row)) {
    const kt = k.trim();
    if (/^time/i.test(kt) && v != null && String(v).trim() !== "")
      return String(v).trim();
  }
  return "";
}

function importProgressFromCsvRecords(records, replaceAll) {
  const run = db.transaction((rows) => {
    if (replaceAll) db.exec("DELETE FROM progress");
    let n = 0;
    for (const row of rows) {
      const num = parseInt(String(row["Puzzle Number"] ?? "").trim(), 10);
      if (!Number.isFinite(num)) continue;
      const solved = parseSolvedFromImport(row) ? 1 : 0;
      const tstr = getTimeStringFromImportRow(row);
      let timeSeconds = null;
      if (tstr !== "") {
        timeSeconds = parseTimeToSeconds(tstr);
      }
      upsertProgress.run({
        puzzle_number: num,
        solved,
        skipped: 0,
        video_used: null,
        time_seconds: timeSeconds,
        updated_at: new Date().toISOString(),
      });
      n++;
    }
    return n;
  });
  return run(records);
}

/** Turn a normal Google Sheets link into a CSV export URL when needed. */
function resolveImportCsvUrl(urlString) {
  const trimmed = urlString.trim();
  const lower = trimmed.toLowerCase();
  if (
    lower.includes("/export") &&
    (lower.includes("format=csv") || lower.includes("format%3dcsv"))
  ) {
    try {
      const u = new URL(trimmed);
      u.hash = "";
      return u.toString();
    } catch {
      return trimmed;
    }
  }

  const idMatch = trimmed.match(/\/spreadsheets\/d\/([a-zA-Z0-9-_]+)(?:\/|$|\?|#)/);
  if (!idMatch) return trimmed;

  try {
    const host = new URL(trimmed).hostname;
    if (host !== "docs.google.com") return trimmed;
  } catch {
    return trimmed;
  }

  const sheetId = idMatch[1];
  let gid = "0";
  try {
    const u = new URL(trimmed);
    const q = u.searchParams.get("gid");
    if (q && /^\d+$/.test(q)) gid = q;
    const hm = (u.hash || "").match(/gid=(\d+)/);
    if (hm && gid === "0") gid = hm[1];
  } catch {
    /* keep gid 0 */
  }

  return `https://docs.google.com/spreadsheets/d/${sheetId}/export?format=csv&gid=${gid}`;
}

const app = express();
app.use(express.json({ limit: "256kb" }));

app.get("/api/health", (_req, res) => {
  res.json({
    ok: true,
    puzzleCount: puzzleCache.length,
    lastSyncAt,
    lastSyncError,
    sheetSyncIntervalMs: SYNC_MS,
  });
});

app.get("/api/state", (req, res) => {
  const include = req.query.include ?? "";
  const exclude = req.query.exclude ?? "";
  const progressMap = loadProgressMap();
  const merged = mergePuzzles(progressMap);
  const filtered = merged.filter((p) =>
    passesFilters(p.constraints, include, exclude)
  );
  const next =
    filtered.find((p) => !p.solved && !p.skipped) || null;
  const statsAll = computeStats(merged);
  const statsFiltered = computeStats(filtered);
  const catalog = merged.map((p) => ({
    ...p,
    matchesFilter: passesFilters(p.constraints, include, exclude),
  }));
  res.json({
    lastSyncAt,
    lastSyncError,
    sheetSyncIntervalMs: SYNC_MS,
    exportUrl: EXPORT_URL,
    uniqueConstraints: uniqueConstraints(merged),
    statsAll,
    statsFiltered,
    next,
    include,
    exclude,
    puzzles: filtered,
    catalog,
  });
});

app.put("/api/progress/:number", (req, res) => {
  const puzzleNumber = parseInt(req.params.number, 10);
  if (!Number.isFinite(puzzleNumber)) {
    res.status(400).json({ error: "Invalid puzzle number" });
    return;
  }
  const body = req.body || {};
  const existing = db
    .prepare(
      "SELECT solved, skipped, video_used, time_seconds FROM progress WHERE puzzle_number = ?"
    )
    .get(puzzleNumber);

  let solved = existing ? !!existing.solved : false;
  let skipped = existing ? !!existing.skipped : false;
  let videoUsed = existing?.video_used ?? null;
  let timeSeconds =
    existing && existing.time_seconds != null ? existing.time_seconds : null;

  if (body.active === true || body.clearStatus === true) {
    solved = false;
    skipped = false;
  } else {
    if (typeof body.skipped === "boolean") {
      skipped = body.skipped;
      if (skipped) solved = false;
    }
    if (typeof body.solved === "boolean") {
      solved = body.solved;
      if (solved) skipped = false;
    }
  }

  if ("videoUsed" in body) {
    const raw = body.videoUsed;
    if (raw == null || raw === "") videoUsed = null;
    else {
      const n = normalizeVideoUsed(raw);
      if (n == null) {
        res.status(400).json({ error: "videoUsed must be none, partial, or full" });
        return;
      }
      videoUsed = n;
    }
  }

  if (body.clearTime) {
    timeSeconds = null;
  } else if (body.timeSeconds != null && body.timeSeconds !== "") {
    const n = parseInt(String(body.timeSeconds), 10);
    if (!Number.isFinite(n) || n < 0) {
      res.status(400).json({ error: "Invalid timeSeconds" });
      return;
    }
    timeSeconds = Math.round(n);
  } else if (body.time != null && String(body.time).trim() !== "") {
    timeSeconds = parseTimeToSeconds(body.time);
    if (timeSeconds == null) {
      res.status(400).json({
        error:
          "Use a time like 4:50, 04:50, 0:04:50, or 1:09:05 (minutes:seconds or hours:minutes:seconds)",
      });
      return;
    }
  }

  upsertProgress.run({
    puzzle_number: puzzleNumber,
    solved: solved ? 1 : 0,
    skipped: skipped ? 1 : 0,
    video_used: videoUsed,
    time_seconds: timeSeconds,
    updated_at: new Date().toISOString(),
  });
  res.json({ ok: true });
});

app.post("/api/refresh", async (_req, res) => {
  try {
    await syncFromSheet();
    res.json({ ok: true, count: puzzleCache.length, lastSyncAt });
  } catch (e) {
    lastSyncError = e.message || String(e);
    res.status(502).json({ ok: false, error: lastSyncError });
  }
});

async function downloadImportCsvText(firstUrl, signal) {
  const tried = [];
  let lastStatus = "";

  async function tryOnce(url) {
    if (tried.includes(url)) return null;
    tried.push(url);
    let r;
    try {
      r = await fetch(url, {
        redirect: "follow",
        signal,
        headers: fetchImportHeaders,
      });
    } catch (e) {
      if (e.name === "AbortError") throw e;
      lastStatus = e.message || "network error";
      return null;
    }
    lastStatus = `${r.status} ${r.statusText}`;
    if (!r.ok) return null;
    const text = await r.text();
    const start = text.trimStart().slice(0, 64).toLowerCase();
    if (start.startsWith("<!doctype") || start.startsWith("<html"))
      return null;
    return text;
  }

  let text = await tryOnce(firstUrl);
  if (text) return text;

  try {
    const u = new URL(firstUrl);
    if (u.searchParams.has("gid")) {
      const u2 = new URL(firstUrl);
      u2.searchParams.delete("gid");
      text = await tryOnce(u2.toString());
      if (text) return text;
    }
  } catch {
    /* ignore */
  }

  const idMatch = firstUrl.match(/\/spreadsheets\/d\/([a-zA-Z0-9-_]+)/);
  if (idMatch) {
    const id = idMatch[1];
    text = await tryOnce(
      `https://docs.google.com/spreadsheets/d/${id}/export?format=csv`
    );
    if (text) return text;
    text = await tryOnce(
      `https://docs.google.com/spreadsheets/d/${id}/gviz/tq?tqx=out:csv&gid=0`
    );
    if (text) return text;
  }

  throw new Error(
    lastStatus.includes("404")
      ? `Google returned Not Found (${lastStatus}). Open the sheet with **Share → General access → Anyone with the link: Viewer**, then try again. If the puzzle tab is not the first sheet, open that tab and copy the URL so it includes \`gid=\`.`
      : `Could not download CSV${lastStatus ? ` (${lastStatus})` : ""}.`
  );
}

app.post("/api/import-from-url", async (req, res) => {
  try {
    const url = String(req.body?.url || "").trim();
    const replaceAll = Boolean(req.body?.replaceAll);
    if (!url.startsWith("https://")) {
      res.status(400).json({ error: "URL must start with https://" });
      return;
    }
    let parsedUrl;
    try {
      parsedUrl = new URL(url);
    } catch {
      res.status(400).json({ error: "Invalid URL" });
      return;
    }
    if (parsedUrl.protocol !== "https:") {
      res.status(400).json({ error: "Only https URLs are allowed" });
      return;
    }

    const fetchUrl = resolveImportCsvUrl(url);

    const controller = new AbortController();
    const timer = setTimeout(() => controller.abort(), 120_000);
    let text;
    try {
      text = await downloadImportCsvText(fetchUrl, controller.signal);
    } finally {
      clearTimeout(timer);
    }

    const maxBytes = 25 * 1024 * 1024;
    if (!text || text.length > maxBytes) {
      res.status(400).json({ error: "CSV is empty or too large" });
      return;
    }

    const records = parse(text, {
      columns: true,
      skip_empty_lines: true,
      relax_column_count: true,
    });
    if (!Array.isArray(records) || records.length === 0) {
      res.status(400).json({ error: "No data rows found in CSV" });
      return;
    }

    const importedCount = importProgressFromCsvRecords(records, replaceAll);
    res.json({ ok: true, importedCount, replaceAll });
  } catch (e) {
    if (e.name === "AbortError") {
      res.status(504).json({ error: "Download timed out" });
      return;
    }
    res.status(500).json({ error: e.message || String(e) });
  }
});

app.use(express.static(path.join(__dirname, "public")));

async function boot() {
  try {
    await syncFromSheet();
  } catch (e) {
    lastSyncError = e.message || String(e);
    console.error("Initial sheet sync failed:", lastSyncError);
  }
  setInterval(() => {
    syncFromSheet().catch((e) => {
      lastSyncError = e.message || String(e);
      console.error("Sheet sync failed:", lastSyncError);
    });
  }, SYNC_MS);

  app.listen(PORT, () => {
    const h = (SYNC_MS / 3_600_000).toFixed(2);
    console.log(
      `Sudoku Adventure tracker http://0.0.0.0:${PORT} (sheet sync every ${SYNC_MS} ms ≈ ${h} h)`
    );
  });
}

boot();
