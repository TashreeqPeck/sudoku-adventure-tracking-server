# Sudoku Adventure tracker

Small **Node + Express** app that keeps your **Sudoku Adventure** puzzle list in sync with a Google Sheet CSV export, stores progress in **SQLite**, and serves a **single-page UI** for next puzzle, filters, stats, browse, and import.

## Requirements

- **Node.js** 20+ (local run). The Docker image uses Node 22 Alpine with build tools for `better-sqlite3`.

## Quick start

```bash
npm install
npm start
```

Open `http://localhost:3840` (or the port you set with `PORT`).

- **Dev** (restart on file changes): `npm run dev`
- The server tries an **initial sheet sync** on boot, then on a timer (see `SHEET_SYNC_INTERVAL_MS`). Use **Refresh** in the UI or `POST /api/refresh` to pull the sheet sooner.

## Commits and versioning

- **Commit messages** follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `chore:`, `docs:`, `refactor:`, `BREAKING CHANGE:` in footer or `!` after type, etc.). After `npm install`, a **Husky** `commit-msg` hook runs [**Commitlint**](https://commitlint.js.org/) so non-compliant messages are rejected.
- **Releases** use [Semantic Versioning](https://semver.org/). The version in `package.json` is the source of truth for the app. To cut a release from conventional commits since the last git tag (bumps `package.json`, updates `CHANGELOG.md`, commits, and tags):

  ```bash
  npm run release
  ```

  To force a bump level: `npm run release:patch`, `npm run release:minor`, or `npm run release:major`. Push with tags: `git push --follow-tags`.

- **First release** (repo has no semver tag yet): `npx commit-and-tag-version --first-release` tags the current `package.json` version without changing it, then add `CHANGELOG.md` entries on the next `npm run release`.

## Environment variables

| Variable | Default | Purpose |
|----------|---------|---------|
| `PORT` | `3840` | HTTP port |
| `DATA_DIR` | `./data` (next to `server.mjs`) | Directory for `progress.sqlite` |
| `SHEET_ID` | Built-in Sudoku Adventure sheet id | Google Spreadsheet id (`/d/<id>/` in the URL) |
| `SHEET_GID` | `0` | Sheet tab gid for CSV export |
| `SHEET_SYNC_INTERVAL_MS` | `86400000` (24h) | Minimum interval between automatic syncs (floor **60s** in code). Alias: `SYNC_INTERVAL_MS` |

The sheet must be reachable as **CSV** (typical `…/export?format=csv&gid=…`). If Google returns 404, set the sheet to **Anyone with the link can view** (or equivalent) and use a tab URL that includes the correct `gid` when it is not the first tab.

## Expected CSV columns

Rows are parsed with headers (see `normalizeRow` in `server.mjs`):

- `Puzzle Number`, `Title`, `Setter`, `Constraints`, `Puzzle Link`, `Video Link`

## Docker Compose (recommended)

From the project root:

```bash
docker compose up -d --build
```

Open `http://localhost:3840`. SQLite lives in the named volume `tracker-data` (mounted at `/data` in the container).

After each merge to **`main`**, GitHub Actions builds the image and pushes it to **GitHub Container Registry** as `ghcr.io/<owner>/sudoku-adventure-tracking-server:latest` (and a `sha-<commit>` tag). Pull with `docker pull ghcr.io/<your-github-username>/sudoku-adventure-tracking-server:latest` (use lowercase). The first time, you may need to set the package visibility under **Packages** in your GitHub profile or org settings.

**Useful overrides** (shell env or a `.env` file beside `docker-compose.yml`):

- `HOST_PORT` — host port mapped to the app (default `3840`).
- `SHEET_SYNC_INTERVAL_MS` — same meaning as in the table above (default `86400000`).

Set `SHEET_ID` / `SHEET_GID` in `docker-compose.yml` under `environment`, or use Compose’s `env_file` pointing at a file you keep out of git.

Stop and remove the container (volume kept):

```bash
docker compose down
```

## Docker (without Compose)

```bash
docker build -t sudoku-adventure-tracker .
docker run --rm -p 3840:3840 -v sa-data:/data sudoku-adventure-tracker
```

Image defaults: `DATA_DIR=/data`, `PORT=3840`, `SHEET_SYNC_INTERVAL_MS=86400000`. Override with `-e` as needed.

## API (summary)

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/api/health` | Liveness / puzzle cache size |
| `GET` | `/api/state` | Full UI state: `next`, `puzzles` (filtered), `catalog` (all rows + `matchesFilter`), `statsAll`, `statsFiltered`, filters echo, sync metadata. Query: `include`, `exclude` (constraint substring filters, newline- or comma-separated in the query string as stored by the client) |
| `PUT` | `/api/progress/:number` | Update progress: `solved`, `skipped`, `active`, `videoUsed` (`none` / `partial` / `full`), `timeSeconds` or `time` (human-readable), `clearTime`, etc. |
| `POST` | `/api/refresh` | Re-download puzzle list from the configured export URL |
| `POST` | `/api/import-from-url` | JSON `{ "url": "https://…", "replaceAll": false }` — import puzzles from another sheet CSV URL |

Static files are served from `public/`; the UI calls `/api/...` relative to the page URL so it works behind a path prefix when reverse-proxied.

## Data

- SQLite file: `{DATA_DIR}/progress.sqlite` (solved, skipped, optional time and video usage per puzzle number).
- The repo’s `.gitignore` excludes `data/` and `node_modules/` so local DB and dependencies are not committed by default.

## License

Private project (`"private": true` in `package.json`); add a license file if you intend to distribute it.
