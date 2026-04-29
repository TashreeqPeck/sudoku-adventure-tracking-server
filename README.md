# Sudoku Adventure tracker

**Rust (Axum) + SQLite** service that syncs your **Sudoku Adventure** puzzle list from a Google Sheet CSV export, stores progress in SQLite, and serves the existing **static SPA** in `public/` (next puzzle, filters, stats, browse, import). The stack is tuned for **lower RAM** than a Node runtime.

## Credits & sources

**Sudoku Adventure** (the video series and community puzzle list) is created and hosted by **[Rangsk](https://www.youtube.com/@Rangsk)** on YouTube. Individual puzzles are credited to their setters in the sheet.

- **YouTube:** https://www.youtube.com/@Rangsk  
- **Source spreadsheet (same id as the app default `SHEET_ID`):** https://docs.google.com/spreadsheets/d/1y4BYBEuXbzReb_tx3bTUdKwynL2JveL3ob55g6c-D-Y/edit  
- **CSV export (gid `0`, same as default `SHEET_GID`):** https://docs.google.com/spreadsheets/d/1y4BYBEuXbzReb_tx3bTUdKwynL2JveL3ob55g6c-D-Y/export?format=csv&gid=0  

This repository is an **independent** tracker UI and server for working with that public sheet data; it is not affiliated with Google or YouTube.

## Requirements

- **Rust** 1.83+ (2021 edition) to build locally: https://rustup.rs/
- **Docker** optional for deployment (multi-stage build; runtime image is Debian slim + `ca-certificates` + `libsqlite3-0`).

## Quick start (local)

```bash
cargo run --release
```

Open `http://localhost:3840` (or the port from `PORT`). Static files default to `./public`; override with `STATIC_DIR`.

- **Logging:** `RUST_LOG=info` (default in code) or `RUST_LOG=debug` for more detail.
- The server runs an **initial sheet sync** on boot, then on an interval (`SHEET_SYNC_INTERVAL_MS`, minimum **60s**). Use **Refresh** in the UI or `POST /api/refresh` to pull the sheet sooner.

### Local development

1. **Install Rust** (once): https://rustup.rs/ — then `rustup update` periodically.
2. **Fast iteration** — use a **dev** build (compiles quicker than `--release`):

   ```bash
   cargo run
   ```

   Use `cargo run --release` when you care about performance or to match production.

3. **Auto-restart on save** (optional): `cargo install cargo-watch`, then from the repo root:

   ```bash
   cargo watch -x run
   ```

4. **Shorter sheet sync while developing** — the server enforces a **60s minimum** sync interval. Example (PowerShell):

   ```powershell
   $env:SHEET_SYNC_INTERVAL_MS = "60000"
   cargo run
   ```

   Bash: `SHEET_SYNC_INTERVAL_MS=60000 cargo run`

5. **Smoke-check the API** (with the server running):

   ```bash
   curl -s http://localhost:3840/api/health
   ```

6. **UI** — open `http://localhost:3840` in a browser; SQLite is created under `./data/` by default (`DATA_DIR`).

7. **Lint / format** (optional):

   ```bash
   cargo fmt
   cargo clippy -- -D warnings
   ```

There is **no `cargo test` suite** in this repo yet; testing is mainly manual through the browser and the `/api/*` endpoints.

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `PORT` | `3840` | HTTP listen port |
| `DATA_DIR` | `./data` | Directory for `progress.sqlite` |
| `STATIC_DIR` | `public` | Static site root (`index.html`, `app.js`, …) |
| `SHEET_ID` | Built-in Sudoku Adventure sheet id | Spreadsheet id from `/d/<id>/` |
| `SHEET_GID` | `0` | Tab `gid` for CSV export |
| `SHEET_SYNC_INTERVAL_MS` | `86400000` (24h) | Auto-sync interval in ms (floor **60s**). Alias: `SYNC_INTERVAL_MS` |

The sheet must be reachable as **CSV** (`…/export?format=csv&gid=…`). If Google returns 404, set the sheet to **Anyone with the link can view** (or equivalent) and use a tab URL with the correct `gid` when it is not the first sheet.

## Versioning & commit messages

The app version is **`Cargo.toml` `version`**. Tag releases in git as you prefer (e.g. `v1.0.1`).

**Conventional commits** ([Conventional Commits](https://www.conventionalcommits.org/)) are **recommended** for clear history, but they are **not enforced** in this repo anymore: the previous **Husky + Commitlint** setup was removed with the move to Rust (no `npm`/`package.json` git hooks). Nothing blocks a non-conventional message locally.

To enforce conventions again, typical options are: a **GitHub Action** on pull requests (e.g. commitlint or semantic-PR checks), **pre-commit** / **lefthook** with commitlint, or a policy on your host. This README does not ship those by default.

## Expected CSV columns

Parsed with a header row (see `csv_util::normalize_row`):

- `Puzzle Number`, `Title`, `Setter`, `Constraints`, `Puzzle Link`, `Video Link`

## Docker Compose (recommended)

```bash
docker compose up -d --build
```

SQLite lives in the named volume `tracker-data` (mounted at `/data`). The container serves static files from `/public`.

After each merge to **`main`**, GitHub Actions builds the image and pushes to **GHCR** (`ghcr.io/<owner>/sudoku-adventure-tracking-server:latest` and `sha-<commit>`). Use lowercase in the image path. Set package visibility under **Packages** if you need public pulls.

**Compose overrides:** `HOST_PORT`, `SHEET_SYNC_INTERVAL_MS`, and optionally `SHEET_ID` / `SHEET_GID` in `docker-compose.yml` or a `.env` file.

```bash
docker compose down   # stops container; keeps volume
```

## Docker (without Compose)

```bash
docker build -t sudoku-adventure-tracker .
docker run --rm -p 3840:3840 -e STATIC_DIR=/public -v sa-data:/data sudoku-adventure-tracker
```

Defaults inside the image: `DATA_DIR=/data`, `STATIC_DIR=/public`, `PORT=3840`, `SHEET_SYNC_INTERVAL_MS=86400000`.

## API (summary)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Liveness / puzzle cache size |
| `GET` | `/api/state` | `next`, `puzzles`, `catalog`, `statsAll`, `statsFiltered`, filters, sync metadata. Query: `include`, `exclude` |
| `PUT` | `/api/progress/{number}` | `solved`, `skipped`, `active`, `videoUsed`, `timeSeconds` / `time`, `clearTime`, … |
| `POST` | `/api/refresh` | Re-download configured sheet CSV |
| `POST` | `/api/import-from-url` | JSON `{ "url": "https://…", "replaceAll": false }` |

The UI resolves `/api/...` relative to the page URL (works behind a path prefix).

## Data

- SQLite: `{DATA_DIR}/progress.sqlite`
- `.gitignore` includes `data/` and `target/`.

## License

Add a `LICENSE` file if you distribute the project.
