# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(Tauri v2 desktop shell; the design language target is Windows 11 Fluent — see Brand Commitments.)

## Users

Evan — a single personal user on a Windows 11 desktop. The window appears centered and always-on-top at every login (and on the first tick of each new day). The job: glance at what needs doing, maybe act (add / complete / acknowledge), and dismiss within seconds — before starting to use the computer.

## Product Purpose

Put Google Tasks in the middle of the screen at boot so the day's tasks are seen *before* the computer gets used. Success means the user trusts the list (it is honest about sync state) and acts on it instead of dismissing it on reflex.

## Positioning

A boot-time interrupt with an enforceable dismissal policy (instant / pause / engage) expressed in the window chrome itself, plus local recurring "nudges" ("Check in with Mom", every 14 days). Offline-first SQLite cache with an outbox that syncs to Google Tasks; changes propagate to the user's phone.

## Operating Context

- Windows login / morning wake; 520×640 undecorated, transparent, always-on-top, non-resizable window; tray icon with Show / Sync now / Quit.
- Rust core owns all state; the Svelte frontend only renders (BootView over `invoke`).
- Google Tasks API via the user's own OAuth client (PKCE + loopback; browser-based consent). Sync states: idle / syncing / offline / auth_error.
- Dismiss gate: pause mode counts down in the close button; engage mode requires an action or "Nothing today".

## Capabilities and Constraints

- Capabilities: tasks due today / overdue / undated (behind a toggle), add + complete tasks, nudge CRUD + acknowledge (optionally creating a task), settings (dismiss mode, pause length, autostart, sync interval), Google connect/disconnect, manual + scheduled sync.
- Constraints: fixed window size; WebView2 rendering; window is hidden on dismiss, never destroyed (state persists across shows); no server; single user.
- Terminology: "nudge" (recurring prompt), "Worth checking" (due-nudge section), "engage" (dismiss mode).

## Brand Commitments

- Name: TaskDesk.
- Voice: short, human, direct — "3 things need you", "Worth checking", "Nothing due. Go be free.", "Nothing today — let me through". Confirmed; preserve.
- **Pinned visual direction (user, 2026-08-30): Windows 11 flyout.** System light/dark, Fluent surfaces and elevation like Notification Center, Segoe UI Variable type ramp, Segoe Fluent Icons, one accent. This pin beats any concept roll.

## Evidence on Hand

README.md (setup + architecture), privacy-policy for the OAuth consent screen. No marketing surface exists or is planned.

## Product Principles

1. Trust before cheer — never show "All clear" without sync truth to back it.
2. Glance first, input second — the day's state leads; entry is secondary.
3. Rules are felt, not explained — the dismiss gate lives in the chrome.
4. Native to Windows — the window should read as something Windows drew.
5. Every action is recoverable or confirmed — undo over interrogation.

## Accessibility & Inclusion

Keyboard-complete operation, visible focus, screen-reader names on all controls, WCAG AA contrast in both themes, reduced-motion respected. (Standard adopted this session; no user-specific need recorded.)
