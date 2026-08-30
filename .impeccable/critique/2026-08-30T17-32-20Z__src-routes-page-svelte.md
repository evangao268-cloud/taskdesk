---
target: "critique (whole app: src/routes/+page.svelte)"
total_score: 18
max_score: 40
na_heuristics: 
p0_count: 1
p1_count: 4
timestamp: 2026-08-30T17-32-20Z
slug: src-routes-page-svelte
---
# TaskDesk critique — src/routes/+page.svelte

Method: dual-agent (A: design-review agent · B: detector/evidence agent). Detector ran in degraded regex mode (parser modules unavailable; undercount). No browser tool exposed; no visual overlay.

## Design Health Score

| # | Heuristic | Score | Key Issue |
|---|-----------|-------|-----------|
| 1 | Visibility of System Status | 2 | Sync state only rendered inside Settings (:289); no `sync-status-changed` listener (:184-185). |
| 2 | Match System / Real World | 3 | ISO `due 2026-08-12` (:357); "every 1 days" (:386). |
| 3 | User Control and Freedom | 1 | No undo for complete / nudge Done / Remove / Disconnect. Escape in add input hides window (:175-180). No cancel on "Waiting for browser…" (:298-300). |
| 4 | Consistency and Standards | 2 | Tasks complete via left circle (:354), nudges via right "Done" (:388). Same .ghost for destructive and neutral. Row hover implies click (:552). |
| 5 | Error Prevention | 1 | One-click Disconnect/Remove. "Someday" task vanishes (:341-344 vs :398). null nudge interval silent no-op (:98, :315). |
| 6 | Recognition Rather Than Recall | 2 | createTaskOnAck only shown in Settings (:326). Gear no pressed state (:216). Countdown digit unexplained. |
| 7 | Flexibility and Efficiency | 2 | No autofocus/shortcut to add input. Tray "Sync now" dead (window.rs:106-109). |
| 8 | Aesthetic and Minimalist Design | 3 | Stale hint (:332). Raw Rust error strings (:229, :304). |
| 9 | Error Recovery | 1 | loadError raw + no retry (:228-230). Six commands with no catch. Failed complete leaves row struck-through forever (:75-79). |
| 10 | Help and Documentation | 1 | No first-run state; autostart off by default (models.rs:48); only help line is wrong (:332). |
| **Total** | | **18/40** | **Poor** (top of band) |

All 10 heuristics scored (max 40, none n/a).

## Design Specificity Verdict

LLM: Copy is authored for this product; composition and visual language are category-interchangeable dark to-do widget (icon buttons, input+select+primary row, uppercase grey labels, circle checks, space-between settings rows). Default dark palette (#16181d / #3b6fd4 / #9aa0a6 / coral / gold). Product character lives in three places: dismiss gate in the chrome (countdown close :223, shake :446-453), the voice (:212, :380, :395, :417), the 350ms strikethrough beat (:72-80). Missed: Windows 11 native language — no prefers-color-scheme, no accent, Unicode glyphs not Fluent icons, no shadow/elevation on a frameless transparent window, fixed 520x640 regardless of content. Hierarchy says "enter data"; product says "look, then leave".

Deterministic scan: 1 finding — `side-tab` at +page.svelte:589 (`li.nudge { border-left: 3px solid #c9a24b }`). True positive, low weight. Static evidence: 28 hardcoded color literals, 0 custom properties; 0 @media queries (no color-scheme, no reduced-motion); inline style at :229; focus only on input/select, shrunk to 1px (:510-512); 3/24 controls lack accessible name (:309, :336, :341).

## Priority Issues

- [P0] Main view lies when Google is disconnected/expired/offline: sync state only in Settings (:287-302); empty cache → "All clear." + "Go be free." (:212, :394-396). Fix: status strip under header driven by view.sync; subscribe to sync-status-changed; gate "Go be free" on connected && idle. → /impeccable harden
- [P1] Header shows yesterday's date after re-show: dateLabel computed once (:193-197), window hidden not destroyed. Fix: $state + recompute in refresh(); also reset showSettings on window-shown. → /impeccable harden
- [P1] No undo on complete/ack/Remove/Disconnect; last two are one-click ghosts. Fix: 5s inline Undo; two-step confirm for Remove/Disconnect. → /impeccable harden
- [P1] "Someday" tasks vanish on creation (:341-344, :398, models.rs:50). Fix: show undated toggle with "1 new" after add, or drop the select. → /impeccable clarify
- [P1] Google sign-in opens behind always-on-top window, no cancel (:119-130, :298-300, window.rs show_main_window). Fix: drop always-on-top during auth; Cancel; guidance copy. → /impeccable harden

## Persona Red Flags

Alex: pause/7s default; no autofocus; left-circle vs right-Done zig-zag; dead row hover; dead tray Sync now; countdown re-renders on tasks-changed (:49-50).
Sam: check buttons all "Complete" (:354, :369, :406); title-only icon buttons; 1px focus ring, no button :focus-visible; placeholder-as-label (:337), unlabelled select (:341); no aria-live; check border 2.93:1; countdown 0.55 opacity; 11.5px uppercase h2; h2 for both panel title and section labels; Escape hides window.
Riley: stale date; completeTask throw → stuck row; six uncaught commands; null interval no-op; "every 1 days"; long titles clip (no overflow-wrap, main overflow hidden); unclamped notes (:372); 30 overdue → no scroll affordance; engage+empty deadlock feel; Alt+F4 refusal invisible to frontend; Settings persists across re-show.

## Minor Observations

Stale hint :332; autostart off by default and unadvertised; relative dates; gear aria-pressed/Back; Google row wraps; range onchange (:254); duplicate .settings .row (:623, :631); 32px icon buttons; no reduced-motion; no hide transition; no end-note acknowledgement.

## Questions to Consider

1. Why is the add form first on a glance screen? Move to bottom; give top third to the most urgent item?
2. Window sized to content so interruption size is the signal?
3. One adaptive dismiss rule instead of three modes?
4. Trust state ("connected / synced at / offline / pending") as a state of the whole window?
5. Designed as a Windows 11 flyout (Mica, accent, Fluent icons, system light/dark) rather than a dark web card?
