# TaskDesk

A Windows desktop app that puts your Google Tasks in the middle of the screen every time you log in — so you see what needs doing *before* you start using the computer.

- **Boot window**: centered, always-on-top, shows tasks due today, overdue tasks, and due nudges. Undated tasks behind a toggle.
- **Act in place**: add tasks, check them off; changes sync to Google Tasks (and your phone).
- **Nudges**: recurring prompts you define ("Check in with Mom", every 14 days). Local-only, deterministic.
- **Dismiss modes** (setting): instant · pause (close disabled for a few seconds) · engage (act or click "Nothing today" first).

## Development

Prereqs: Node 20+, Rust (MSVC toolchain), VS C++ Build Tools. Then:

```
npm install
npm run tauri dev     # run in dev mode
npm run tauri build   # NSIS installer under src-tauri/target/release/bundle/nsis/
cargo test            # Rust unit + integration tests (run inside src-tauri/)
```

## Google Tasks setup (one-time, manual)

The app talks to the Google Tasks API with your own OAuth client. Create one:

1. Go to [console.cloud.google.com](https://console.cloud.google.com) → create a project (e.g. `TaskDesk`).
2. **APIs & Services → Library** → enable **Google Tasks API**.
3. **OAuth consent screen**: User type *External*; app name `TaskDesk`; add your Gmail address as a test user. Scopes: `https://www.googleapis.com/auth/tasks`, `openid`, `email`.
4. **Publish the app to Production.** While the consent screen is in *Testing* status, Google expires refresh tokens after 7 days and the app silently breaks weekly. Production mode shows an "unverified app" warning during consent — expected and fine for personal use.
5. **Credentials → Create credentials → OAuth client ID → Desktop app.** Copy the client ID and secret into `src-tauri/src/google/client_config.rs` (gitignored; see `client_config.example.rs`).

Per Google's own docs, a desktop app's client secret is "not treated as a secret" — PKCE protects the flow. The refresh token is stored in Windows Credential Manager, never on disk.

## Architecture

Rust core owns all state; the Svelte frontend only renders.

| Module | Responsibility |
|---|---|
| `store` | SQLite (rusqlite): task cache, nudges, settings, outbox of pending mutations. Knows nothing about Google. |
| `google` | OAuth (PKCE + loopback) and Tasks REST client. Knows nothing about SQLite. |
| `sync` | The only module touching both: incremental pull (`updatedMin` + `showDeleted`), outbox drain with backoff, last-write-wins. |
| `nudges` | Pure scheduling logic, no I/O. |
| `window` | Show/center/always-on-top, dismiss-policy state machine, tray, new-day re-show. |

Known Google Tasks API limits the design works around: due dates are date-only (no times), there are no push notifications (we poll, default every 5 min), and recurrence isn't exposed (nudges are local).
