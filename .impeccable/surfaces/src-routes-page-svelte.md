---
version: 1
slug: "src-routes-page-svelte"
primary_target: "src/routes/+page.svelte"
related_targets: []
---

## Scope & mode
The whole app UI: `src/routes/+page.svelte` (boot window: lists + settings panel). Mode: Operate.

## Audience, job, task
Single user (Evan) at Windows login. Job: glance at overdue/today/nudges, optionally act (add, complete, ack), dismiss within seconds. Trust in the list outranks everything.

## Important states
Signed-out / auth-expired / offline / syncing / pending-outbox (trust strip under header — never show "All clear" untrusted); loading; empty-trusted ("Go be free"); dismiss gate (pause countdown ring in close button, engage footer); undo window (6s deferred commit for complete/ack); two-step confirm (Remove nudge, Disconnect).

## Chosen direction
Windows 11 flyout (user-pinned 2026-08-30, code-led): system light/dark tokens, Fluent surfaces + elevation (inset card, 8px radius, soft shadow), Segoe UI Variable Display/Text ramp, Segoe Fluent Icons, one accent (#005FB8 / #60CDFF). Glance-first: lists lead, composer pinned at bottom. Direction contract lives in src/app.html body comment.

## Memorable moment
The close button morphs into an accent countdown ring during pause mode; refusal shakes the card (reduced-motion guarded).

## Unresolved
Content-sized window (vs fixed 520×640) and adaptive dismiss policy — raised in critique, deliberately not taken.
