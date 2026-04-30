const $ = (id) => document.getElementById(id);

/** Resolve `/api/...` against the current page path (works behind a path prefix). */
function apiUrl(pathWithQuery) {
  const path = pathWithQuery.startsWith("/")
    ? pathWithQuery.slice(1)
    : pathWithQuery;
  const { origin, pathname } = window.location;
  const baseDir =
    pathname === "/" || pathname.endsWith("/")
      ? origin + pathname
      : origin + pathname.slice(0, pathname.lastIndexOf("/") + 1);
  return new URL(path, baseDir).href;
}

const STORAGE_SCOPE = "sa-scope";
const STORAGE_BROWSE_MODE = "sa-browse-mode";

const BROWSE_PAGE_SIZE = 50;

let state = null;
let currentNext = null;
/** @type {string[]} */
let includeTags = [];
/** @type {string[]} */
let excludeTags = [];
/** @type {"filtered" | "all"} — stats + browse list */
let scopeMode = "filtered";
let browsePage = 0;
let lastBrowseFilterSig = null;

function loadScopeFromStorage() {
  try {
    let m = sessionStorage.getItem(STORAGE_SCOPE);
    if (!m) {
      m = sessionStorage.getItem(STORAGE_BROWSE_MODE);
      if (m) sessionStorage.setItem(STORAGE_SCOPE, m);
    }
    if (m === "all" || m === "filtered") scopeMode = m;
  } catch {
    scopeMode = "filtered";
  }
}

function persistScopeMode() {
  sessionStorage.setItem(STORAGE_SCOPE, scopeMode);
}

function tagsToQueryValue(tags) {
  return tags.map((t) => t.trim()).filter(Boolean).join("\n");
}

function parseFilterValue(raw) {
  const lines = String(raw || "")
    .split(/\r?\n/)
    .map((s) => s.trim())
    .filter(Boolean);
  const out = [];
  const seen = new Set();
  for (const line of lines) {
    const key = line.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    out.push(line);
  }
  return out;
}

async function saveFiltersToDb() {
  const res = await fetch(apiUrl("api/filters"), {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      include: tagsToQueryValue(includeTags),
      exclude: tagsToQueryValue(excludeTags),
    }),
  });
  if (!res.ok) throw new Error(await res.text());
}

async function loadState() {
  const res = await fetch(apiUrl("api/state"));
  if (!res.ok) throw new Error(await res.text());
  state = await res.json();
  includeTags = parseFilterValue(state.include);
  excludeTags = parseFilterValue(state.exclude);
  return state;
}

function fillConstraintSelect(constraints) {
  const sel = $("constraint-select");
  const keep = sel.value;
  sel.innerHTML = "";
  const opt0 = document.createElement("option");
  opt0.value = "";
  opt0.textContent = "— Choose from sheet —";
  sel.appendChild(opt0);
  const byNorm = new Map();
  for (const c of constraints) {
    const display = String(c).trim().replace(/\s+/g, " ");
    const key = display.toLowerCase();
    if (!key) continue;
    if (!byNorm.has(key)) byNorm.set(key, display);
  }
  const sorted = [...byNorm.values()].sort((a, b) =>
    a.localeCompare(b, undefined, { sensitivity: "base" })
  );
  for (const c of sorted) {
    const o = document.createElement("option");
    o.value = c;
    o.textContent = c;
    sel.appendChild(o);
  }
  if ([...sel.options].some((o) => o.value === keep)) sel.value = keep;
}

function renderChipLists() {
  renderChipContainer($("include-chips"), includeTags, "include");
  renderChipContainer($("exclude-chips"), excludeTags, "exclude");
}

function renderChipContainer(el, tags, which) {
  el.innerHTML = "";
  tags.forEach((text, idx) => {
    const wrap = document.createElement("span");
    wrap.className = which === "exclude" ? "chip exclude" : "chip";
    const label = document.createElement("span");
    label.textContent = text;
    const rm = document.createElement("button");
    rm.type = "button";
    rm.setAttribute("aria-label", `Remove ${text}`);
    rm.textContent = "×";
    rm.addEventListener("click", async () => {
      if (which === "include") includeTags.splice(idx, 1);
      else excludeTags.splice(idx, 1);
      try {
        await saveFiltersToDb();
        await refreshAll();
      } catch (err) {
        setMsg(String(err.message || err), true);
      }
    });
    wrap.appendChild(label);
    wrap.appendChild(rm);
    el.appendChild(wrap);
  });
}

function getPickerValue() {
  const sel = $("constraint-select").value.trim();
  const cust = $("constraint-custom").value.trim();
  if (sel) return sel;
  if (cust) return cust;
  return "";
}

function hasTagInsensitive(list, t) {
  const low = t.toLowerCase();
  return list.some((x) => x.toLowerCase() === low);
}

function addTag(which, raw) {
  const t = String(raw || "").trim();
  if (!t) return false;
  const list = which === "include" ? includeTags : excludeTags;
  const other = which === "include" ? excludeTags : includeTags;
  if (hasTagInsensitive(list, t)) return false;
  if (hasTagInsensitive(other, t)) {
    setMsg(
      `"${t}" is already in the ${which === "include" ? "exclude" : "include"} list. Remove it there first.`,
      true
    );
    return false;
  }
  list.push(t);
  $("constraint-custom").value = "";
  $("constraint-select").value = "";
  renderChipLists();
  return true;
}

function formatIntervalHuman(ms) {
  if (!Number.isFinite(ms) || ms < 1000) return "—";
  if (ms % 86_400_000 === 0) return `${ms / 86_400_000} day(s)`;
  if (ms % 3_600_000 === 0) return `${ms / 3_600_000} hour(s)`;
  if (ms % 60_000 === 0) return `${ms / 60_000} minute(s)`;
  if (ms % 1000 === 0) return `${ms / 1000} second(s)`;
  return `${Math.round(ms / 1000)}s`;
}

function parseFlexibleTime(str) {
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

function timeFromCombinedInput() {
  return parseFlexibleTime($("t-combined").value);
}

function setTimeInputs(seconds) {
  if (seconds == null || seconds === "") {
    $("t-combined").value = "";
    return;
  }
  const n = parseInt(String(seconds), 10);
  $("t-combined").value = formatHMS(n);
}

function syncVideoRadios(p) {
  const v = p?.videoUsed;
  const val =
    v === "partial" ? "partial" : v === "full" ? "full" : "none";
  for (const input of document.querySelectorAll('input[name="video-used"]')) {
    input.checked = input.value === val;
  }
}

function getSelectedVideoUsed() {
  const el = document.querySelector('input[name="video-used"]:checked');
  return el && el.value ? el.value : "none";
}

function puzzleStatusLabel(p) {
  if (p.solved) return "Solved";
  if (p.skipped) return "Skip";
  return "";
}

function renderNext() {
  const next = state.next;
  currentNext = next;
  const msg = $("next-msg");
  msg.textContent = "";
  msg.classList.remove("err");

  if (!next) {
    $("next-label").textContent = "No puzzle";
    $("next-number").textContent = "";
    $("next-title").textContent =
      "No matching puzzles in queue (solved or skipped, or adjust filters).";
    $("next-setter").textContent = "";
    $("next-constraints").textContent = "";
    $("next-links").innerHTML = "";
    setTimeInputs(null);
    syncVideoRadios(null);
    return;
  }

  if (next.skipped) {
    $("next-label").textContent = "Skipped puzzle";
  } else if (next.solved) {
    $("next-label").textContent = "Solved puzzle";
  } else {
    $("next-label").textContent = "Next puzzle";
  }
  $("next-number").textContent = `#${next.number}`;
  $("next-title").textContent = next.title || "(untitled)";
  $("next-setter").textContent = next.setter ? `Setter: ${next.setter}` : "";
  $("next-constraints").textContent = next.constraints || "—";

  const links = $("next-links");
  links.innerHTML = "";
  if (next.puzzleLink) {
    const a = document.createElement("a");
    a.href = next.puzzleLink;
    a.target = "_blank";
    a.rel = "noopener";
    a.textContent = "Puzzle";
    links.appendChild(a);
  }
  if (next.videoLink) {
    const a = document.createElement("a");
    a.href = next.videoLink;
    a.target = "_blank";
    a.rel = "noopener";
    a.textContent = "Solution";
    links.appendChild(a);
  }

  setTimeInputs(next.timeSeconds);
  syncVideoRadios(next);
}

/** Stats for the stats card — always from the puzzle rows in scope (catalog vs filtered). */
function statsFromPuzzleList(list) {
  const solved = list.filter((p) => p.solved);
  const skipped = list.filter((p) => p.skipped && !p.solved);
  const active = list.filter((p) => !p.solved && !p.skipped);
  const withTime = solved.filter((p) => p.timeSeconds != null);
  const sum = withTime.reduce((a, p) => a + p.timeSeconds, 0);
  const avg =
    withTime.length > 0 ? Math.round(sum / withTime.length) : null;
  return {
    totalPuzzles: list.length,
    solvedCount: solved.length,
    skippedCount: skipped.length,
    activeRemaining: active.length,
    averageFormatted: avg != null ? formatHMS(avg) : "",
  };
}

function currentStats() {
  if (!state) return null;
  const list =
    scopeMode === "all"
      ? Array.isArray(state.catalog)
        ? state.catalog
        : state.puzzles || []
      : state.puzzles || [];
  return statsFromPuzzleList(list);
}

function renderStats() {
  const s = currentStats();
  if (!s) return;
  $("st-solved").textContent = `${s.solvedCount} / ${s.totalPuzzles}`;
  $("st-skipped").textContent = `${s.skippedCount} / ${s.totalPuzzles}`;
  $("st-remain").textContent = String(s.activeRemaining);
  $("st-avg").textContent = s.averageFormatted || "—";
  $("scope-mode").value = scopeMode;
}

function getCatalog() {
  if (!state) return [];
  if (Array.isArray(state.catalog)) return state.catalog;
  return state.puzzles || [];
}

function getBrowseList() {
  if (!state) return [];
  if (scopeMode === "all") return getCatalog();
  return state.puzzles || [];
}

function renderBrowsePager(total, pageCount) {
  const el = $("browse-pager");
  if (!total) {
    el.innerHTML = "";
    return;
  }
  const cur = browsePage + 1;
  const atStart = browsePage <= 0;
  const atEnd = browsePage >= pageCount - 1;
  el.innerHTML = `
    <span class="pager-info">Page ${cur} of ${pageCount} (${BROWSE_PAGE_SIZE} / page)</span>
    <button type="button" class="btn ghost" data-pager="first" ${atStart ? "disabled" : ""}>« First</button>
    <button type="button" class="btn ghost" data-pager="prev" ${atStart ? "disabled" : ""}>‹ Prev</button>
    <button type="button" class="btn ghost" data-pager="next" ${atEnd ? "disabled" : ""}>Next ›</button>
    <button type="button" class="btn ghost" data-pager="last" ${atEnd ? "disabled" : ""}>Last »</button>
  `;
  el.querySelectorAll("[data-pager]").forEach((btn) => {
    btn.addEventListener("click", () => {
      const a = btn.dataset.pager;
      if (a === "first") browsePage = 0;
      else if (a === "prev") browsePage = Math.max(0, browsePage - 1);
      else if (a === "next") browsePage = Math.min(pageCount - 1, browsePage + 1);
      else if (a === "last") browsePage = pageCount - 1;
      renderBrowse();
    });
  });
}

function renderBrowse() {
  const tbody = $("browse-body");
  tbody.innerHTML = "";

  const list = getBrowseList();
  const total = list.length;
  const hasActiveFilters =
    (includeTags && includeTags.length > 0) ||
    (excludeTags && excludeTags.length > 0);

  $("browse-scope-note").textContent =
    scopeMode === "all"
      ? "List: all puzzles (same scope as Stats)"
      : "List: filtered (same scope as Stats)";

  const pageCount = Math.max(1, Math.ceil(total / BROWSE_PAGE_SIZE));
  browsePage = Math.max(0, Math.min(browsePage, pageCount - 1));
  const start = browsePage * BROWSE_PAGE_SIZE;
  const pageRows = list.slice(start, start + BROWSE_PAGE_SIZE);

  for (const p of pageRows) {
    const tr = document.createElement("tr");
    if (
      scopeMode === "all" &&
      hasActiveFilters &&
      p.matchesFilter === false
    ) {
      tr.className = "row-outside-filter";
      tr.title = "Does not match current include / exclude filters";
    }
    tr.innerHTML = `
      <td class="num">${p.number}</td>
      <td>${escapeHtml(p.title)}</td>
      <td>${escapeHtml(p.constraints)}</td>
      <td>${escapeHtml(puzzleStatusLabel(p))}</td>
      <td class="num">${p.timeFormatted || ""}</td>
      <td>${escapeHtml(p.videoUsedLabel || "")}</td>
      <td><button type="button" class="btn ghost btn-focus" data-n="${p.number}">Focus</button></td>
    `;
    tbody.appendChild(tr);
  }
  tbody.querySelectorAll(".btn-focus").forEach((btn) => {
    btn.addEventListener("click", () => {
      focusPuzzle(parseInt(btn.dataset.n, 10));
    });
  });

  renderBrowsePager(total, pageCount);

  const note = $("browse-note");
  if (total === 0) {
    note.textContent =
      scopeMode === "filtered"
        ? "No puzzles match the current filters."
        : "No puzzles loaded.";
    return;
  }

  const from = start + 1;
  const to = start + pageRows.length;
  if (scopeMode === "filtered") {
    note.textContent = `${total} puzzle(s) in filtered list. Rows ${from}–${to} of ${total}.`;
  } else if (hasActiveFilters) {
    const matching = getCatalog().filter((p) => p.matchesFilter !== false)
      .length;
    note.textContent = `${total} puzzles total (${matching} match filters). Rows ${from}–${to}. Dimmed rows are outside the filter.`;
  } else {
    note.textContent = `${total} puzzles total. Rows ${from}–${to}.`;
  }
}

function escapeHtml(s) {
  return String(s)
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

async function focusPuzzle(num) {
  const p = getCatalog().find((x) => x.number === num);
  if (!p) return;
  state.next = { ...p };
  renderNext();
  $("browse-wrap").open = false;
  window.scrollTo({ top: 0, behavior: "smooth" });
}

async function putProgress(num, body) {
  const res = await fetch(apiUrl(`api/progress/${num}`), {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) {
    const err = await res.json().catch(() => ({}));
    throw new Error(err.error || res.statusText);
  }
}

function setMsg(text, isErr) {
  const el = $("next-msg");
  el.textContent = text;
  el.classList.toggle("err", !!isErr);
}

function updateSyncStatus() {
  const st = $("sync-status");
  const intervalMs = state.sheetSyncIntervalMs;
  const intervalLabel = formatIntervalHuman(intervalMs);
  if (state.lastSyncError) {
    st.textContent = `Last sync error: ${state.lastSyncError} · Auto-refresh: every ${intervalLabel}`;
  } else if (state.lastSyncAt) {
    st.textContent = `Sheet data from ${new Date(state.lastSyncAt).toLocaleString()} · Auto-refresh: every ${intervalLabel}`;
  } else {
    st.textContent = `Auto-refresh from sheet: every ${intervalLabel}`;
  }
}

async function refreshAll() {
  await loadState();
  const sig = JSON.stringify({ include: includeTags, exclude: excludeTags });
  if (lastBrowseFilterSig !== null && sig !== lastBrowseFilterSig) {
    browsePage = 0;
  }
  lastBrowseFilterSig = sig;
  fillConstraintSelect(state.uniqueConstraints || []);
  renderChipLists();
  renderNext();
  renderStats();
  renderBrowse();
  updateSyncStatus();
}

async function onRefreshSheet() {
  $("btn-refresh").disabled = true;
  try {
    const res = await fetch(apiUrl("api/refresh"), { method: "POST" });
    if (!res.ok) {
      const j = await res.json().catch(() => ({}));
      throw new Error(j.error || "Refresh failed");
    }
    await refreshAll();
  } catch (e) {
    $("sync-status").textContent = String(e.message || e);
  } finally {
    $("btn-refresh").disabled = false;
  }
}

$("btn-add-include").addEventListener("click", async () => {
  const v = getPickerValue();
  if (!v) {
    setMsg("Pick a constraint from the list or enter custom text.", true);
    return;
  }
  setMsg("", false);
  if (!addTag("include", v)) {
    if (hasTagInsensitive(includeTags, v)) setMsg("Already in include list.", true);
    return;
  }
  try {
    await saveFiltersToDb();
    await refreshAll();
  } catch (e) {
    setMsg(String(e.message || e), true);
  }
});

$("btn-add-exclude").addEventListener("click", async () => {
  const v = getPickerValue();
  if (!v) {
    setMsg("Pick a constraint from the list or enter custom text.", true);
    return;
  }
  setMsg("", false);
  if (!addTag("exclude", v)) {
    if (hasTagInsensitive(excludeTags, v)) setMsg("Already in exclude list.", true);
    return;
  }
  try {
    await saveFiltersToDb();
    await refreshAll();
  } catch (e) {
    setMsg(String(e.message || e), true);
  }
});

$("btn-clear-filters").addEventListener("click", async () => {
  includeTags = [];
  excludeTags = [];
  renderChipLists();
  try {
    await saveFiltersToDb();
    await refreshAll();
  } catch (e) {
    setMsg(String(e.message || e), true);
  }
});

$("btn-refresh").addEventListener("click", onRefreshSheet);

$("btn-solved").addEventListener("click", async () => {
  if (!currentNext) return;
  try {
    const raw = $("t-combined").value.trim();
    const sec = raw ? timeFromCombinedInput() : null;
    if (raw && sec == null) {
      setMsg("Could not parse that time. Use a format like 0:04:50.", true);
      return;
    }
    const body = {
      solved: true,
      videoUsed: getSelectedVideoUsed(),
    };
    if (sec != null) body.timeSeconds = sec;
    await putProgress(currentNext.number, body);
    setMsg("Marked solved.");
    await refreshAll();
  } catch (e) {
    setMsg(String(e.message || e), true);
  }
});

$("btn-skip").addEventListener("click", async () => {
  if (!currentNext) return;
  try {
    await putProgress(currentNext.number, {
      skipped: true,
      videoUsed: getSelectedVideoUsed(),
    });
    setMsg("Marked skip (won’t do).");
    await refreshAll();
  } catch (e) {
    setMsg(String(e.message || e), true);
  }
});

$("btn-active").addEventListener("click", async () => {
  if (!currentNext) return;
  try {
    await putProgress(currentNext.number, { active: true });
    setMsg("Back in queue (not solved, not skipped).");
    await refreshAll();
  } catch (e) {
    setMsg(String(e.message || e), true);
  }
});

$("btn-clear-time").addEventListener("click", async () => {
  if (!currentNext) return;
  try {
    await putProgress(currentNext.number, { clearTime: true });
    setTimeInputs(null);
    setMsg("Time cleared.");
    await refreshAll();
  } catch (e) {
    setMsg(String(e.message || e), true);
  }
});

function formatHMS(totalSeconds) {
  const h = Math.floor(totalSeconds / 3600);
  const m = Math.floor((totalSeconds % 3600) / 60);
  const s = totalSeconds % 60;
  return `${h}:${String(m).padStart(2, "0")}:${String(s).padStart(2, "0")}`;
}

function setImportMsg(text, isErr) {
  const el = $("import-msg");
  el.textContent = text;
  el.classList.toggle("err", !!isErr);
}

$("btn-import").addEventListener("click", async () => {
  const url = $("import-url").value.trim();
  if (!url) {
    setImportMsg("Enter a CSV export URL.", true);
    return;
  }
  $("btn-import").disabled = true;
  setImportMsg("Importing…", false);
  try {
    const res = await fetch(apiUrl("api/import-from-url"), {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        url,
        replaceAll: $("import-replace-all").checked,
      }),
    });
    const data = await res.json().catch(() => ({}));
    if (!res.ok) throw new Error(data.error || res.statusText);
    setImportMsg(`Imported ${data.importedCount} puzzle row(s).`, false);
    await refreshAll();
  } catch (e) {
    setImportMsg(String(e.message || e), true);
  } finally {
    $("btn-import").disabled = false;
  }
});

$("scope-mode").addEventListener("change", () => {
  scopeMode = $("scope-mode").value === "all" ? "all" : "filtered";
  browsePage = 0;
  persistScopeMode();
  renderStats();
  renderBrowse();
});

loadScopeFromStorage();
renderChipLists();
refreshAll().catch((e) => {
  $("next-title").textContent = "Failed to load";
  setMsg(String(e.message || e), true);
});
