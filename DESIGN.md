---
name: TaskDesk
description: A Windows 11 flyout that puts today's Google Tasks on screen at boot — system-drawn, glanceable, dismissed through a gate.
colors:
  surface: "#f9f9f9"
  surface-2: "#f3f3f3"
  control: "#ffffff"
  control-hover: "#f6f6f6"
  subtle-hover: "rgba(0, 0, 0, 0.045)"
  stroke: "rgba(0, 0, 0, 0.09)"
  stroke-strong: "rgba(0, 0, 0, 0.18)"
  card-stroke: "rgba(0, 0, 0, 0.07)"
  text: "#1b1b1b"
  text-2: "#5c5c5c"
  text-disabled: "#9d9d9d"
  placeholder: "#757575"
  accent: "#005fb8"
  accent-hover: "#0067c0"
  accent-text: "#ffffff"
  critical: "#c42b1c"
  warn-fill: "#fdf3d7"
  warn-stroke: "rgba(157, 93, 0, 0.25)"
  check-stroke: "#767676"
typography:
  headline:
    fontFamily: "Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "28px"
    fontWeight: 600
  title:
    fontFamily: "Segoe UI Variable Display, Segoe UI, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 600
  section:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 600
  body:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "14px"
    fontWeight: 400
  caption:
    fontFamily: "Segoe UI Variable Text, Segoe UI, system-ui, sans-serif"
    fontSize: "12.5px"
    fontWeight: 400
  icon:
    fontFamily: "Segoe Fluent Icons, Segoe MDL2 Assets"
    fontSize: "12px"
    lineHeight: 1
rounded:
  control: "4px"
  card: "8px"
  switch: "10px"
  circle: "50%"
spacing:
  xs: "6px"
  sm: "8px"
  md: "10px"
  lg: "12px"
  card-margin: "14px"
  card-padding: "18px 20px 16px"
components:
  button-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.accent-text}"
    rounded: "{rounded.control}"
    padding: "8px 14px"
  button-primary-hover:
    backgroundColor: "{colors.accent-hover}"
  button-ghost:
    backgroundColor: "transparent"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "5px 10px"
  button-ghost-hover:
    backgroundColor: "{colors.subtle-hover}"
  button-icon:
    backgroundColor: "transparent"
    textColor: "{colors.text-2}"
    rounded: "{rounded.control}"
    width: "34px"
    height: "34px"
  button-icon-hover:
    backgroundColor: "{colors.subtle-hover}"
    textColor: "{colors.text}"
  input-text:
    backgroundColor: "{colors.control}"
    textColor: "{colors.text}"
    rounded: "{rounded.control}"
    padding: "7px 10px"
  card:
    backgroundColor: "{colors.surface}"
    textColor: "{colors.text}"
    rounded: "{rounded.card}"
    padding: "{spacing.card-padding}"
  strip:
    backgroundColor: "{colors.control}"
    rounded: "{rounded.control}"
    padding: "7px 10px"
  strip-warn:
    backgroundColor: "{colors.warn-fill}"
---

# Design System: TaskDesk

## Overview

**Creative North Star: "Something Windows Drew"**

TaskDesk is a Windows 11 Notification Center flyout, not an app with a brand. Every surface, stroke, radius, and glyph is chosen so the 520×640 undecorated, transparent, always-on-top window reads as chrome the operating system itself would draw: Fluent mica-adjacent surfaces, one system accent per theme, Segoe UI Variable type, Segoe Fluent Icons. There is no logo, no brand color, no decoration — the identity is fidelity to the host OS.

The card is dense and glanceable: today's truth (overdue, due today, worth checking) leads; input sits pinned at the bottom. Both themes are first-class — the entire token set is defined in `:global(:root)` with a full `prefers-color-scheme: dark` override block, and no component references a raw color. The signature chrome is the dismiss gate: a close button whose border becomes a conic-gradient countdown ring, and a refusal shake when the gate is not satisfied.

**Key Characteristics:**
- System-native Fluent light/dark, driven entirely by CSS custom properties on `:root`
- One accent per theme (#005FB8 light / #60CDFF dark); critical red is the only other chromatic voice
- Segoe UI Variable ramp (Display for headings, Text for body); Segoe Fluent Icons PUA glyphs, no image assets
- One elevated card; everything inside is flat, separated by hairline strokes and subtle control fills
- Motion is rare, brief, and always `prefers-reduced-motion` guarded

## Colors

A neutral Fluent surface stack with a single system accent; every value exists in a light and a dark form and is only ever consumed via `var(--token)`.

### Primary
- **System Accent** (`--accent`, #005fb8 light / #60cdff dark): the one voice of interactivity — primary buttons, toggle-on fill, slider thumb, focus rings and focus underlines, selection background, the countdown ring, the pressed icon-button tint, hover tint on task checks (via `color-mix(... 14%, transparent)`), the empty-state glyph, and the healthy trust-strip icon. `--accent-hover` (#0067c0 / #7fd7ff) is its only variant; `--accent-text` (#ffffff / #003553) rides on top of it.

### Neutral
- **Surface** (`--surface`, #f9f9f9 / #262626): the card itself and the fade layer of scroll shadows.
- **Surface-2** (`--surface-2`, #f3f3f3 / #2d2d2d): secondary surface step (rarely used; keep for layering).
- **Control** (`--control`, #ffffff / rgba(255,255,255,0.061)): fill for inputs, strips, undo bars, and segmented buttons. `--control-hover` is its hover step.
- **Subtle Hover** (`--subtle-hover`, black 4.5% / white 6% alpha): rest-state-invisible hover wash for list rows, icon buttons, and ghost buttons.
- **Strokes** (`--stroke` 9% alpha, `--stroke-strong` 18–22% alpha, `--card-stroke` 7–9% alpha): hairline borders on controls, the input bottom edge, the card outline, scrollbar thumbs, and the slider track.
- **Text** (`--text` #1b1b1b / #ffffff; `--text-2` #5c5c5c / #cfcfcf; `--text-disabled` #9d9d9d / #8a8a8a; `--placeholder` #757575 / #a6a6a6): three-step text hierarchy plus a placeholder tone.
- **Check Stroke** (`--check-stroke`, #767676 / #a0a0a0): the resting ring of the task check circle.

### Status
- **Critical** (`--critical`, #c42b1c / #ff99a4): overdue headings and tags, warn-strip icons, danger ghost buttons, error text. Never used as an accent.
- **Warn Fill / Warn Stroke** (`--warn-fill` #fdf3d7 / #3a3117; `--warn-stroke` amber 25% alpha): background and border of warning strips only.

### Named Rules
**The One Accent Rule.** Each theme has exactly one accent. Anything interactive that needs color uses `--accent`; anything wrong uses `--critical`. No third chromatic voice exists.

**The Token-Only Rule.** No component states a raw color. Both themes live entirely in the `:root` custom-property blocks; adding a color means adding it to both blocks.

## Typography

**Display Font:** Segoe UI Variable Display (fallback Segoe UI, system-ui, sans-serif)
**Body Font:** Segoe UI Variable Text (fallback Segoe UI, system-ui, sans-serif)
**Icon Font:** Segoe Fluent Icons (fallback Segoe MDL2 Assets)

**Character:** The Windows 11 system voice — optical-size-correct Segoe UI Variable, semibold headings, small quiet metadata. Nothing decorative; hierarchy comes from weight and the two-tone text colors, not size jumps.

### Hierarchy
- **Headline** (600, 28px, Display, tight 1.15 line height): the card's single `h1` — the status line ("3 things need you" / "All clear.") at Fluent's Title step; the thesis at full typographic strength. The date sits above it as a 12.5px `--text-2` caption (bolder pass, user-approved 2026-08-30).
- **Title** (600, 17px, Display): panel titles (Settings).
- **Section** (600, 14px, `--text-2`): list section headings (`h2`); Overdue variant switches color to `--critical`. Settings sub-headings (`h3`) drop to 13px.
- **Body** (400, 14px): task titles, empty-state copy.
- **Caption** (400, 12–12.5px, mostly `--text-2`): subtitle, strips, undo bars, task metadata, hints, field errors; the countdown numeral is 12px/600 with `font-variant-numeric: tabular-nums`.
- **Icon** (12px glyphs, 13px inside icon buttons, 26px empty-state): Segoe Fluent Icons PUA codepoints, always paired with `aria-hidden="true"` and a text label or `aria-label`.

### Named Rules
**The Two-Face Rule.** Segoe UI Variable Display for 17px+ headings, Segoe UI Variable Text for everything else. No other family ever appears except the icon font.

## Layout

One fixed, non-resizable 520×640 window (undecorated, transparent, always-on-top, centered). The visible world is a single card: `margin: 14px` inside the transparent window (room for the shadow), `height: calc(100vh - 28px)`, `padding: 18px 20px 16px`, flex column. There is no responsive behavior and no breakpoints.

Vertical order is fixed by the thesis: masthead (date caption + status `h1`, `data-tauri-drag-region`) with icon actions top-right → trust strip → scrollable lists region (`flex: 1; overflow-y: auto`) → undo bars → composer pinned at the bottom above a `--stroke` top border → footer. Settings replaces the lists region in place; the frame does not change.

Rhythm is a tight 6/8/10/12px scale: 6px between header icons, 8px control gaps, 10px in-row gaps, 12px composer top padding. List rows are `padding: 7px 6px` with 4px radii; controls are `padding: 7px 10px`. The scroll region uses the layered-gradient scroll-shadow technique (`--surface` fades over `--scroll-shadow` radial shadows, `background-attachment: local, scroll`) so edges only shadow when content is actually hidden; scrollbars are 6px, thumb `--stroke-strong`, track transparent.

## Elevation & Depth

Exactly one elevated object: the card, carrying `--shadow` — a two-layer soft shadow (`0 2px 6px rgba(0,0,0,0.08), 0 14px 32px rgba(0,0,0,0.16)` light; deeper `0.28/0.45` alphas and 16/40px in dark) over the transparent window. Everything inside the card is flat: depth is conveyed by hairline strokes, the `--control` fill step, and the scroll shadows. No inner element ever casts a shadow.

### Shadow Vocabulary
- **Card elevation** (`box-shadow: var(--shadow)`): the flyout's only shadow; belongs to `main` alone.
- **Scroll shadow** (`--scroll-shadow`, rgba(0,0,0,0.16) light / 0.55 dark): 10px radial gradients at the scroll region's edges, revealed only by hidden content.

### Named Rules
**The One Shadow Rule.** The card floats; nothing inside it does. New surfaces get strokes and control fills, never box-shadows.

## Shapes

Fluent's dual-radius language: **8px** for the window-level card, **4px** for everything inside it (buttons, inputs, strips, list-row hover, segmented control, scrollbar thumbs at 3px). Circles are reserved for meaning: the 24px task check ring (2px `--check-stroke` border), the close button while counting down, the toggle-switch knob, and the 16px slider thumb. The toggle switch is a 40×20px pill (10px radius). Borders are 1px hairlines from the stroke tokens; text inputs additionally darken their bottom edge (`border-bottom-color: var(--stroke-strong)`) in the Fluent underline idiom.

## Components

### Buttons
- **Shape:** gently rounded (4px), 13px text.
- **Primary** (`.primary`): `--accent` fill, `--accent-text` text, 600 weight, `padding: 8px 14px`; hover swaps to `--accent-hover`; disabled drops to `opacity: 0.5`.
- **Ghost** (`.ghost`): transparent with 1px `--stroke` border, `--text`, `padding: 5px 10px`, 12px text; hover fills `--subtle-hover`. `.small` tightens to 4px 8px. `.danger` recolors text to `--critical` with a 45% critical border — used for two-step destructive confirms ("Disconnect" → "Disconnect?").
- **Icon** (`.icon-btn`): 34×34px, transparent, `--text-2` glyph at 13px; hover `--subtle-hover` + `--text`; `aria-pressed` state adds the wash and turns the glyph `--accent`; disabled/gated fades to `--text-disabled`.
- **Focus:** all buttons take a 2px `--accent` outline ring, offset 1px, on `:focus-visible`.

### Cards / Containers
- **The card** (`main`): `--surface`, 8px radius, 1px `--card-stroke`, `var(--shadow)`; enter animation `enter 0.2s ease-out` (fade + scale 0.985 + 4px rise), refusal animation `shake 0.4s` (±6px translateX) — both only under `prefers-reduced-motion: no-preference`.
- **Strips** (`.strip`): status rows — `--control` fill, 1px `--stroke`, 4px radius, 12.5px text, leading Fluent glyph in `--accent`. `.warn` swaps to `--warn-fill`/`--warn-stroke` with a `--critical` glyph; `.quiet` drops fill and border entirely. Undo bars (`.undo`) reuse the same recipe.

### Inputs / Fields
- **Text/select:** `--control` fill, 1px `--stroke` with stronger bottom edge, 4px radius, `padding: 7px 10px`, 13px; placeholder in `--placeholder`.
- **Focus:** the Fluent underline — `border-bottom: 2px solid var(--accent)` with `padding-bottom: 6px` to keep the box stable; no outline ring on text fields.
- **Toggle switch:** checkboxes are `appearance: none`, CSS-drawn as a 40×20 Fluent switch; off = `--control` pill, `--stroke-strong` border, 12px `--text-2` knob; on = `--accent` fill, knob slides 20px (0.12s ease-out, motion-guarded) and recolors `--accent-text`.
- **Slider:** custom `input[type="range"]` — 4px `--stroke-strong` track, 16px round `--accent` thumb.
- **Segmented control** (`.seg`): bordered group of `.seg-btn`s (12px, `--control` fill, `--text-2`); the active segment mixes 18% accent into the control fill and goes 600/`--text`.
- **Errors:** inline 12px `--critical` text (`.field-error`, `.auth-error`); hints in `--text-2`.

### Navigation
No navigation. Settings toggles in place via the masthead's pressed icon button; the frame (masthead, composer) persists.

### The Dismiss Gate (signature)
The close icon button is the enforcement surface. In pause mode it becomes a circle whose 2px border is a live conic-gradient ring — `conic-gradient(var(--accent) calc(var(--pct) * 1turn), var(--stroke) 0)` layered over a `--surface` padding-box fill — with a tabular-nums seconds count inside. In engage mode it stays clickable but faded (`--text-disabled`); refusing produces the card shake plus a `.denied-note` caption, never a modal. Both animations sit behind `prefers-reduced-motion: no-preference`.

### Task Row (signature)
A flat list row (`padding: 7px 6px`, 4px radius, hover `--subtle-hover`): a 24px circular check button whose Fluent check glyph is transparent until hover (then `--accent` glyph over a 14% accent wash), title at 14px, metadata small in `--text-2`, overdue tag in `--critical`, notes clamped to two lines. Completion strikes through and fades the text (0.2s, motion-guarded) with a 6-second undo bar.

## Do's and Don'ts

### Do:
- **Do** draw every control the way Windows 11 would: Segoe Fluent Icons glyphs (with `aria-hidden` + a real label), CSS-drawn Fluent switches, the accent-underline focus idiom on text fields, 4px/8px radii.
- **Do** define any new color in both `:root` blocks and consume it only via `var()`; test every surface in light and dark.
- **Do** guard all motion behind `prefers-reduced-motion: no-preference` and keep it ≤0.4s.
- **Do** use two-step inline confirms (ghost → danger-ghost "…?") and undo bars instead of dialogs.
- **Do** keep status in strips (role="status"/"alert") between the masthead and the lists.

### Don't:
- **Don't** add a second accent, gradients-as-decoration, or brand color; `--accent` and `--critical` are the entire chromatic vocabulary.
- **Don't** put a box-shadow on anything inside the card (The One Shadow Rule).
- **Don't** introduce any font family beyond Segoe UI Variable (Display/Text) and Segoe Fluent Icons, or any raster/SVG image asset — this build ships zero images.
- **Don't** add chrome Windows would not draw: no title bar, no modal dialogs, no custom window buttons beyond the gate-aware close.
- **Don't** restyle the select's popup — its Chromium-drawn dropdown list is a documented, accepted exception (WebView2 residue), not a license for other non-Fluent chrome.
