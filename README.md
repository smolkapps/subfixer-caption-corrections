# SubFixer — Crowd-sourced Closed-Caption Corrections

SubFixer lets people submit corrected closed-caption text for any video, keyed
by **video URL + timestamp**. Each submission is stored as a **word-level diff
against the original / auto-generated caption**, and surfaced for other viewers.
A **global leaderboard** ranks the top "fixers" by total words changed, with an
opt-out **anonymous** setting for people who want their stats counted but their
name hidden.

The core product is the caption-correction CRUD plus the leaderboard. The
gift-card reward economy described in the original concept (pay per *N* words
changed, manual verification, ad-funded) is an optional monetization layer
sitting on top of the same word-change metric this app already computes — it is
**not** implemented here.

## Architecture

| Layer | Stack | What it does |
|-------|-------|--------------|
| Backend | **Rust** (Axum + SQLite via `rusqlite`, bundled) | Diff engine, video-key normalization, correction/fixer store, leaderboard ranking, JSON API |
| Frontend | **TypeScript + React** (Vite, React Router) | Submit-correction form with a live diff preview, per-video correction browser, leaderboard + privacy controls |

The backend is a Rust library (`subfixer`) with a thin `subfixer-server` binary.
The same word-level diff definition is implemented on both sides so the live
preview the user sees matches what the server stores.

### Key design decisions

- **Video keying.** Two people pointing at the same moment of the same video
  must collide on one key regardless of URL noise. `youtu.be/<id>`,
  `youtube.com/watch?v=<id>`, `shorts/`, `embed/`, `m.youtube.com`, tracking
  params (`utm_*`, `si`, `feature`, `fbclid`, …), `http` vs `https`, and `www.`
  all normalize to the bare 11-char YouTube video id. Non-YouTube URLs fall back
  to a normalized canonical URL. Timestamps are floored to whole seconds.
- **Change counting.** A word substitution (`teh` → `the`) counts as **one**
  changed word, not an insert plus a delete. Pure insertions/deletions count one
  each. Formally `words_changed = max(inserted, deleted)`. This is the metric the
  leaderboard ranks on and the natural unit for any future pay-per-word reward.
- **No-op rejection.** A "correction" identical to the original is rejected
  (`400`) — you cannot farm the leaderboard by re-submitting unchanged text.
- **Anonymity.** Opting out replaces the display name with `Anonymous Fixer`
  **and withholds the fixer id** from the API response, so anonymous rows cannot
  be correlated across the board. Stats still count and the rank is unchanged.

## API

| Method | Path | Body / Query | Description |
|--------|------|--------------|-------------|
| `GET` | `/api/health` | — | Liveness check |
| `POST` | `/api/corrections` | `{video_url, start_sec, original_text, corrected_text, fixer_name}` | Submit a correction; returns the stored row with `words_changed` |
| `GET` | `/api/corrections?url=<videoUrl>` | — | All corrections for a video (any URL form), ordered by timestamp |
| `GET` | `/api/leaderboard?limit=<n>` | — | Top fixers, ranked by words changed |
| `POST` | `/api/anonymity` | `{fixer_name, anonymous}` | Toggle a fixer's public anonymity |
| `POST` | `/api/preview-diff` | `{original_text, corrected_text}` | Stateless diff preview (no persistence) |

Validation errors and no-op corrections return `400` with `{"error": "..."}`;
unknown fixers on `/api/anonymity` return `404`.

## Development

This repo follows a develop-locally / build-remotely workflow; everything below
runs anywhere with the listed toolchains.

### Backend (Rust)

```bash
cd backend
cargo test            # unit + API integration tests (in-memory SQLite)
cargo run             # serves on 0.0.0.0:8080 (SUBFIXER_ADDR to override)
```

Environment variables: `SUBFIXER_DB` (default `subfixer.db`), `SUBFIXER_ADDR`
(default `0.0.0.0:8080`), `SUBFIXER_STATIC` (default `static` — drop the built
frontend here to serve the SPA from the same origin).

### Frontend (React + TypeScript)

```bash
cd frontend
npm install
npm run dev           # Vite dev server, proxies /api -> localhost:8080
npm run build         # type-check + production build into dist/
npm test              # vitest unit + component tests
```

### Run the whole thing together

```bash
# 1. build the SPA and hand it to the backend
cd frontend && npm install && npm run build
cp -r dist ../backend/static

# 2. run the server (serves API + SPA on one origin)
cd ../backend && cargo run --release
# open http://localhost:8080
```

## Testing

- **Backend:** `diff` (word diff + change counting), `videokey` (URL/timestamp
  normalization, incl. error paths), `leaderboard` (ranking, tie-breaks,
  anonymity), `store` (submit/list/leaderboard/anonymity over in-memory SQLite,
  incl. no-op and validation rejections), and `api` (HTTP integration via
  `tower::oneshot`, asserting status codes for the happy path and the `400`/error
  paths).
- **Frontend:** `timestamp` and `worddiff` pure-logic suites (including the
  `max(inserted, deleted)` rule and round-tripping), a `DiffView` rendering test,
  and a `SubmitPage` integration test that drives the real form with a mocked API
  and asserts the parsed timestamp, the success status, and error surfacing.

## License

MIT — see [LICENSE](LICENSE).
