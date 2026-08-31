<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    getBootView,
    addTask,
    completeTask,
    ackNudge,
    listNudges,
    addNudge,
    deleteNudge,
    nothingToday,
    dismissWindow,
    updateSettings,
    startGoogleAuth,
    disconnectGoogle,
    syncNow,
    engage,
    type BootView,
    type DismissState,
    type NudgeDef,
    type Settings,
    type TaskDto,
  } from "$lib/api";

  // ---- core view state ----
  let view = $state<BootView | null>(null);
  let loadError = $state("");
  let dateLabel = $state(formatDate());
  let entering = $state(false);

  // ---- composer ----
  let newTitle = $state("");
  let newDue = $state<"today" | "none">("today");
  let composerInput = $state<HTMLInputElement | null>(null);

  // ---- dismiss gate ----
  let blockedMs = $state(0);
  let pauseTotalMs = $state(7000);
  let denied = $state(false);
  let deniedNote = $state("");
  let localEngaged = $state(false);
  let countdownTimer: ReturnType<typeof setInterval> | null = null;

  // ---- optimistic actions with undo ----
  const UNDO_MS = 6000;
  type PendingUndo = {
    key: number;
    kind: "task" | "nudge";
    id: string;
    title: string;
    createTask: boolean;
    timer: ReturnType<typeof setTimeout>;
  };
  let pending = $state<PendingUndo[]>([]);
  let beat = $state<Set<string>>(new Set());
  let undoSeq = 0;

  // ---- errors ----
  let actionError = $state<{ text: string; detail: string } | null>(null);

  // ---- settings panel ----
  let showSettings = $state(false);
  let showUndatedOpen = $state(false);
  let sessionShowUndated = $state(false);
  let somedayFlash = $state(false);
  let somedayTimer: ReturnType<typeof setTimeout> | null = null;

  // ---- nudges management ----
  let allNudges = $state<NudgeDef[]>([]);
  let nudgeTitle = $state("");
  let nudgeInterval = $state<number | null>(14);
  let nudgeMakesTask = $state(false);
  let confirmRemoveId = $state<string | null>(null);
  let confirmRemoveTimer: ReturnType<typeof setTimeout> | null = null;

  // ---- Google auth ----
  let authBusy = $state(false);
  let authError = $state("");
  let authSeq = 0;
  let confirmDisconnect = $state(false);
  let confirmDisconnectTimer: ReturnType<typeof setTimeout> | null = null;

  let unlisteners: UnlistenFn[] = [];

  // ---- formatting helpers ----
  function formatDate(): string {
    return new Date().toLocaleDateString(undefined, {
      weekday: "long",
      month: "long",
      day: "numeric",
    });
  }

  const todayStr = () => {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  };

  /** "due yesterday" / "3 days overdue" from a YYYY-MM-DD due date. */
  function overdueLabel(dueDate: string | null): string {
    if (!dueDate) return "";
    const [y, m, d] = dueDate.split("-").map(Number);
    const due = new Date(y, m - 1, d);
    const now = new Date();
    const today = new Date(now.getFullYear(), now.getMonth(), now.getDate());
    const days = Math.round((today.getTime() - due.getTime()) / 86400000);
    if (days <= 0) return "due today";
    if (days === 1) return "due yesterday";
    return `${days} days overdue`;
  }

  const everyLabel = (n: number) => (n === 1 ? "every day" : `every ${n} days`);
  const lateLabel = (n: number) => (n === 1 ? "1 day late" : `${n} days late`);

  function lastSyncLabel(iso: string | null): string {
    if (!iso) return "never synced";
    const then = new Date(iso).getTime();
    if (Number.isNaN(then)) return iso;
    const mins = Math.max(0, Math.round((Date.now() - then) / 60000));
    if (mins < 1) return "just now";
    if (mins < 60) return `${mins} min ago`;
    const hours = Math.round(mins / 60);
    if (hours < 24) return `${hours}h ago`;
    return new Date(then).toLocaleDateString();
  }

  /** Run a command; on failure surface a friendly error with the raw detail. */
  async function run<T>(label: string, fn: () => Promise<T>): Promise<T | undefined> {
    try {
      const r = await fn();
      actionError = null;
      return r;
    } catch (e) {
      actionError = { text: label, detail: String(e) };
      return undefined;
    }
  }

  /** Like `run`, for void commands (whose success also resolves undefined). */
  async function runOk(label: string, fn: () => Promise<unknown>): Promise<boolean> {
    try {
      await fn();
      actionError = null;
      return true;
    } catch (e) {
      actionError = { text: label, detail: String(e) };
      return false;
    }
  }

  // ---- refresh & countdown ----
  async function refresh() {
    try {
      view = await getBootView();
      loadError = "";
    } catch (e) {
      loadError = String(e);
      return;
    }
    dateLabel = formatDate();
    pauseTotalMs = Math.max(1000, view.settings.pauseSeconds * 1000);
    // Resync the countdown only on real drift so background refreshes
    // don't visibly restart the ring mid-count.
    if (Math.abs(view.dismiss.blockedForMs - blockedMs) > 700) {
      blockedMs = view.dismiss.blockedForMs;
      startCountdown();
    }
  }

  function startCountdown() {
    if (countdownTimer) clearInterval(countdownTimer);
    countdownTimer = null;
    if (blockedMs <= 0) return;
    const startedAt = Date.now();
    const initial = blockedMs;
    countdownTimer = setInterval(() => {
      blockedMs = Math.max(0, initial - (Date.now() - startedAt));
      if (blockedMs === 0 && countdownTimer) {
        clearInterval(countdownTimer);
        countdownTimer = null;
      }
    }, 200);
  }

  // ---- optimistic complete / ack with undo ----
  function queueUndo(kind: "task" | "nudge", id: string, title: string, createTask = false) {
    const key = ++undoSeq;
    const timer = setTimeout(() => commitPending(key), UNDO_MS);
    pending = [...pending, { key, kind, id, title, createTask, timer }];
  }

  function completeOptimistic(t: TaskDto) {
    if (beat.has(t.localId) || pending.some((p) => p.id === t.localId)) return;
    localEngaged = true;
    engage().catch(() => {});
    beat = new Set(beat).add(t.localId);
    // Brief strikethrough beat before the row leaves the list.
    setTimeout(() => {
      beat.delete(t.localId);
      beat = new Set(beat);
      queueUndo("task", t.localId, t.title);
    }, 350);
  }

  function ackOptimistic(id: string, title: string, createTask: boolean) {
    if (pending.some((p) => p.id === id)) return;
    localEngaged = true;
    engage().catch(() => {});
    queueUndo("nudge", id, title, createTask);
  }

  async function commitPending(key: number): Promise<boolean> {
    const p = pending.find((x) => x.key === key);
    if (!p) return true;
    clearTimeout(p.timer);
    pending = pending.filter((x) => x.key !== key);
    const ok =
      p.kind === "task"
        ? await runOk(`Couldn't complete “${p.title}”`, () => completeTask(p.id))
        : await runOk(`Couldn't mark “${p.title}” done`, () => ackNudge(p.id, p.createTask));
    await refresh();
    return ok;
  }

  function undoPending(key: number) {
    const p = pending.find((x) => x.key === key);
    if (!p) return;
    clearTimeout(p.timer);
    pending = pending.filter((x) => x.key !== key);
  }

  async function flushPending(): Promise<boolean> {
    let allOk = true;
    for (const p of [...pending]) {
      if (!(await commitPending(p.key))) allOk = false;
    }
    return allOk;
  }

  function undoAll() {
    for (const p of pending) clearTimeout(p.timer);
    pending = [];
  }

  // ---- dismiss ----
  function refuse(result: DismissState) {
    denied = true;
    if (result.blockedForMs > 0) {
      blockedMs = result.blockedForMs;
      startCountdown();
    }
    deniedNote =
      result.mode === "engage" && !result.engaged
        ? "Add or complete something first — or use “Nothing today”."
        : `You can close this in ${Math.ceil(result.blockedForMs / 1000)}s.`;
    setTimeout(() => (denied = false), 600);
    setTimeout(() => (deniedNote = ""), 4000);
  }

  async function tryDismiss() {
    // A failed commit must not vanish into a hidden window: stay open so the
    // error strip is actually seen.
    if (!(await flushPending())) return;
    const result = await run("Couldn't close the window", () => dismissWindow());
    if (result && !result.allowed) refuse(result);
  }

  async function onNothingToday() {
    if (!(await runOk("Couldn't record that", () => nothingToday()))) return;
    localEngaged = true;
    await tryDismiss();
  }

  // ---- composer ----
  async function submitTask() {
    const title = newTitle.trim();
    if (!title) return;
    const due = newDue === "today" ? todayStr() : undefined;
    const r = await run("Couldn't add the task", () => addTask(title, due));
    if (r === undefined) return;
    newTitle = "";
    localEngaged = true;
    if (!due) {
      // A "Someday" task is undated and normally hidden; acknowledge it
      // instead of letting it silently vanish.
      sessionShowUndated = true;
      somedayFlash = true;
      if (somedayTimer) clearTimeout(somedayTimer);
      somedayTimer = setTimeout(() => (somedayFlash = false), 6000);
    }
    await refresh();
  }

  // ---- settings ----
  async function saveSettings(patch: Partial<Settings>) {
    if (!view) return;
    const next = await run("Couldn't save that setting", () =>
      updateSettings({ ...view!.settings, ...patch })
    );
    if (next === undefined) return;
    view.settings = next;
    await refresh();
  }

  async function refreshNudges() {
    const r = await run("Couldn't load nudges", () => listNudges());
    if (r !== undefined) allNudges = r;
  }

  async function toggleSettings() {
    if (showSettings) {
      showSettings = false;
      return;
    }
    await refreshNudges();
    showSettings = true;
  }

  const intervalValid = $derived(
    typeof nudgeInterval === "number" && Number.isFinite(nudgeInterval) && nudgeInterval >= 1 && nudgeInterval <= 365
  );

  async function submitNudge() {
    const title = nudgeTitle.trim();
    if (!title || !intervalValid) return;
    const r = await run("Couldn't add the nudge", () =>
      addNudge(title, nudgeInterval as number, nudgeMakesTask)
    );
    if (r === undefined) return;
    nudgeTitle = "";
    await refreshNudges();
    await refresh();
  }

  function askRemoveNudge(id: string) {
    confirmRemoveId = id;
    if (confirmRemoveTimer) clearTimeout(confirmRemoveTimer);
    confirmRemoveTimer = setTimeout(() => (confirmRemoveId = null), 4000);
  }

  async function removeNudge(id: string) {
    confirmRemoveId = null;
    if (!(await runOk("Couldn't remove the nudge", () => deleteNudge(id)))) return;
    await refreshNudges();
    await refresh();
  }

  // ---- Google auth ----
  async function connectGoogle() {
    const seq = ++authSeq;
    authBusy = true;
    authError = "";
    try {
      await startGoogleAuth();
      if (seq !== authSeq) return; // user stopped waiting; ignore the late result
      await refresh();
    } catch (e) {
      if (seq === authSeq) authError = String(e);
    } finally {
      if (seq === authSeq) authBusy = false;
    }
  }

  function cancelConnect() {
    // Stops waiting in the app; the browser tab can simply be closed.
    authSeq++;
    authBusy = false;
    authError = "";
  }

  function askDisconnect() {
    confirmDisconnect = true;
    if (confirmDisconnectTimer) clearTimeout(confirmDisconnectTimer);
    confirmDisconnectTimer = setTimeout(() => (confirmDisconnect = false), 4000);
  }

  async function doDisconnect() {
    confirmDisconnect = false;
    if (!(await runOk("Couldn't disconnect Google", () => disconnectGoogle()))) return;
    await refresh();
  }

  async function doSyncNow() {
    authError = "";
    await run("Sync failed", () => syncNow());
    await refresh();
  }

  const syncLabel: Record<string, string> = {
    idle: "synced",
    syncing: "syncing…",
    offline: "offline",
    auth_error: "sign-in needed",
  };

  // ---- keyboard ----
  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (showSettings) {
        showSettings = false;
        return;
      }
      const el = document.activeElement as HTMLElement | null;
      if (el && ["INPUT", "SELECT", "TEXTAREA"].includes(el.tagName)) {
        el.blur(); // first Escape leaves the field; the next one dismisses
        return;
      }
      tryDismiss();
      return;
    }
    // Type-to-add: a printable key outside any field focuses the composer.
    if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey && !showSettings) {
      const t = e.target as HTMLElement | null;
      if (t && ["INPUT", "SELECT", "TEXTAREA"].includes(t.tagName)) return;
      composerInput?.focus();
    }
  }

  // ---- lifecycle ----
  function resetPerShow() {
    showSettings = false;
    localEngaged = false;
    denied = false;
    deniedNote = "";
    entering = true;
    setTimeout(() => (entering = false), 250);
  }

  onMount(async () => {
    resetPerShow();
    await refresh();
    unlisteners.push(
      await listen("window-shown", () => {
        resetPerShow();
        refresh();
      })
    );
    unlisteners.push(await listen("tasks-changed", () => refresh()));
    unlisteners.push(await listen("sync-status-changed", () => refresh()));
    unlisteners.push(
      await listen<DismissState>("dismiss-denied", (e) => refuse(e.payload))
    );
  });

  onDestroy(() => {
    if (countdownTimer) clearInterval(countdownTimer);
    unlisteners.forEach((u) => u());
  });

  // ---- derived view ----
  const pendingIds = $derived(new Set(pending.map((p) => p.id)));
  const overdueVisible = $derived(view ? view.overdue.filter((t) => !pendingIds.has(t.localId)) : []);
  const todayVisible = $derived(view ? view.today.filter((t) => !pendingIds.has(t.localId)) : []);
  const nudgesVisible = $derived(view ? view.nudges.filter((n) => !pendingIds.has(n.id)) : []);
  const undatedVisible = $derived(view ? view.undated.filter((t) => !pendingIds.has(t.localId)) : []);
  const openCount = $derived(overdueVisible.length + todayVisible.length + nudgesVisible.length);
  const trusted = $derived(!!view && view.sync.connected && view.sync.state === "idle");
  const engageSatisfied = $derived(
    !view || view.dismiss.mode !== "engage" || view.dismiss.engaged || localEngaged
  );
  const ringPct = $derived(blockedMs > 0 ? blockedMs / pauseTotalMs : 0);

  const statusText = $derived(
    loadError
      ? "Something went wrong"
      : !view
        ? "Loading your tasks…"
        : openCount === 0
          ? trusted
            ? "All clear."
            : "Nothing cached to show"
          : openCount === 1
            ? "1 thing needs you"
            : `${openCount} things need you`
  );

  const closeLabel = $derived(
    blockedMs > 0
      ? `Close — available in ${Math.ceil(blockedMs / 1000)} seconds`
      : !engageSatisfied
        ? "Close — act on something first, or choose Nothing today"
        : "Close"
  );

  type Strip = {
    tone: "info" | "warn" | "quiet";
    icon: string;
    text: string;
    action?: string;
    onAction?: () => void;
  };
  const trustStrip = $derived.by<Strip | null>(() => {
    if (!view) return null;
    if (authBusy)
      return {
        tone: "info",
        icon: "",
        text: "Finish signing in in your browser, then come back here.",
        action: "Cancel",
        onAction: cancelConnect,
      };
    const s = view.sync;
    if (s.state === "auth_error")
      return {
        tone: "warn",
        icon: "",
        text: "Google sign-in expired — this list may be stale.",
        action: "Sign in again",
        onAction: connectGoogle,
      };
    if (!s.connected)
      return {
        tone: "warn",
        icon: "",
        text: "Google isn't connected — showing local tasks only.",
        action: "Connect",
        onAction: connectGoogle,
      };
    if (s.state === "offline")
      return {
        tone: "info",
        icon: "",
        text: `Offline — tasks as of ${lastSyncLabel(s.lastSyncAt)}.`,
      };
    if (s.state === "syncing")
      return { tone: "quiet", icon: "", text: "Syncing…" };
    if (s.pendingOutbox > 0)
      return {
        tone: "info",
        icon: "",
        text: s.pendingOutbox === 1 ? "1 change waiting to sync." : `${s.pendingOutbox} changes waiting to sync.`,
        action: "Sync now",
        onAction: doSyncNow,
      };
    return null;
  });
</script>

<svelte:window onkeydown={onKeydown} />

<main class:denied class:entering data-tauri-drag-region>
  <header data-tauri-drag-region>
    <div class="masthead" data-tauri-drag-region>
      <p class="date" data-tauri-drag-region>{dateLabel}</p>
      <h1 aria-live="polite" data-tauri-drag-region>{statusText}</h1>
    </div>
    <div class="header-actions">
      <button
        class="icon-btn"
        aria-label={showSettings ? "Back to tasks" : "Settings"}
        aria-pressed={showSettings}
        onclick={toggleSettings}
      >
        <span class="fluent" aria-hidden="true">{showSettings ? "" : ""}</span>
      </button>
      <button
        class="icon-btn close"
        class:counting={blockedMs > 0}
        class:gated={!engageSatisfied}
        style={`--pct:${ringPct}`}
        aria-label={closeLabel}
        aria-disabled={blockedMs > 0 || !engageSatisfied}
        disabled={blockedMs > 0}
        onclick={tryDismiss}
      >
        {#if blockedMs > 0}
          <span class="count">{Math.ceil(blockedMs / 1000)}</span>
        {:else}
          <span class="fluent" aria-hidden="true">{""}</span>
        {/if}
      </button>
    </div>
  </header>

  {#if trustStrip && !showSettings}
    <div class="strip {trustStrip.tone}" role="status">
      <span class="fluent strip-icon" aria-hidden="true">{trustStrip.icon}</span>
      <span class="strip-text">{trustStrip.text}</span>
      {#if trustStrip.action}
        <button class="ghost small" onclick={trustStrip.onAction}>{trustStrip.action}</button>
      {/if}
    </div>
  {/if}

  {#if loadError}
    <div class="strip warn" role="alert">
      <span class="fluent strip-icon" aria-hidden="true">{""}</span>
      <span class="strip-text">TaskDesk couldn't load your tasks.</span>
      <button class="ghost small" onclick={refresh}>Try again</button>
    </div>
    <details class="error-detail">
      <summary>Details</summary>
      <p>{loadError}</p>
    </details>
  {/if}

  {#if actionError}
    <div class="strip warn" role="alert">
      <span class="fluent strip-icon" aria-hidden="true">{""}</span>
      <span class="strip-text">{actionError.text} — {actionError.detail}</span>
      <button class="ghost small" onclick={() => (actionError = null)}>Dismiss</button>
    </div>
  {/if}

  {#if view}
    {#if showSettings}
      <section class="settings" aria-label="Settings">
        <h2 class="panel-title">Settings</h2>

        <h3>Window</h3>
        <label class="row">
          <span>Dismiss mode</span>
          <select
            value={view.settings.dismissMode}
            onchange={(e) => saveSettings({ dismissMode: e.currentTarget.value as Settings["dismissMode"] })}
          >
            <option value="instant">Instant — close any time</option>
            <option value="pause">Pause — wait a few seconds</option>
            <option value="engage">Engage — act before closing</option>
          </select>
        </label>
        {#if view.settings.dismissMode === "pause"}
          <label class="row">
            <span>Pause length: {view.settings.pauseSeconds}s</span>
            <input
              type="range"
              min="3"
              max="30"
              value={view.settings.pauseSeconds}
              oninput={(e) => {
                if (view) view.settings.pauseSeconds = +e.currentTarget.value;
              }}
              onchange={(e) => saveSettings({ pauseSeconds: +e.currentTarget.value })}
            />
          </label>
        {/if}
        <label class="row">
          <span>Start when I sign in to Windows</span>
          <input
            type="checkbox"
            checked={view.settings.autostartEnabled}
            onchange={(e) => saveSettings({ autostartEnabled: e.currentTarget.checked })}
          />
        </label>
        <label class="row">
          <span>Show undated tasks</span>
          <input
            type="checkbox"
            checked={view.settings.showUndated}
            onchange={(e) => saveSettings({ showUndated: e.currentTarget.checked })}
          />
        </label>

        <h3>Google Tasks</h3>
        {#if view.sync.connected}
          <div class="g-account">
            <div class="g-id">
              <span class="g-email">{view.sync.email ?? "Connected"}</span>
              <span class="g-state">
                {syncLabel[view.sync.state]}{view.sync.pendingOutbox > 0
                  ? ` · ${view.sync.pendingOutbox} pending`
                  : ""} · last sync {lastSyncLabel(view.sync.lastSyncAt)}
              </span>
            </div>
            <div class="g-actions">
              <button class="ghost" onclick={doSyncNow}>Sync now</button>
              {#if confirmDisconnect}
                <button class="ghost danger" onclick={doDisconnect}>Disconnect?</button>
              {:else}
                <button class="ghost" onclick={askDisconnect}>Disconnect</button>
              {/if}
            </div>
          </div>
        {:else if authBusy}
          <div class="row">
            <span>Finish signing in in your browser, then come back here.</span>
            <button class="ghost" onclick={cancelConnect}>Cancel</button>
          </div>
        {:else}
          <div class="row">
            <span>Not connected</span>
            <button class="primary" onclick={connectGoogle}>Connect Google</button>
          </div>
        {/if}
        {#if authError}
          <p class="auth-error" role="alert">Sign-in failed: {authError}</p>
        {/if}
        <label class="row">
          <span>Sync every</span>
          <select
            value={String(view.settings.syncIntervalSecs)}
            onchange={(e) => saveSettings({ syncIntervalSecs: +e.currentTarget.value })}
          >
            <option value="120">2 minutes</option>
            <option value="300">5 minutes</option>
            <option value="900">15 minutes</option>
            <option value="1800">30 minutes</option>
          </select>
        </label>

        <h3>Nudges</h3>
        <p class="hint">Recurring prompts that appear under “Worth checking” when due.</p>
        <div class="add">
          <label class="visually-hidden" for="nudge-title">New nudge</label>
          <input
            id="nudge-title"
            placeholder="e.g. Check in with Mom"
            bind:value={nudgeTitle}
            onkeydown={(e) => e.key === "Enter" && submitNudge()}
          />
          <label class="days-label">
            every
            <input
              class="days"
              type="number"
              min="1"
              max="365"
              bind:value={nudgeInterval}
              aria-label="Interval in days"
            />
            days
          </label>
          <button class="primary" onclick={submitNudge} disabled={!nudgeTitle.trim() || !intervalValid}>
            Add
          </button>
        </div>
        {#if nudgeTitle.trim() && !intervalValid}
          <p class="field-error">The interval must be between 1 and 365 days.</p>
        {/if}
        <label class="row">
          <span>New nudge also creates a task when marked done</span>
          <input type="checkbox" bind:checked={nudgeMakesTask} />
        </label>
        <ul class="plain-list">
          {#each allNudges as n (n.id)}
            <li>
              <div class="task-text">
                <span>{n.title}</span>
                <small>{everyLabel(n.intervalDays)}{n.createTaskOnAck ? " · adds a task when done" : ""}</small>
              </div>
              {#if confirmRemoveId === n.id}
                <button class="ghost danger" onclick={() => removeNudge(n.id)}>Remove?</button>
              {:else}
                <button class="ghost" aria-label={`Remove nudge “${n.title}”`} onclick={() => askRemoveNudge(n.id)}>
                  Remove
                </button>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {:else}
      <section class="lists" aria-label="Tasks">
        {#if overdueVisible.length}
          <h2 class="critical">Overdue</h2>
          <ul class="plain-list">
            {#each overdueVisible as t (t.localId)}
              <li class:done={beat.has(t.localId)}>
                <button
                  class="check"
                  aria-label={`Complete “${t.title}”`}
                  onclick={() => completeOptimistic(t)}
                ><span class="fluent check-glyph" aria-hidden="true">{""}</span></button>
                <div class="task-text">
                  <span>{t.title}</span>
                  <small class="overdue-tag">{overdueLabel(t.dueDate)}</small>
                </div>
              </li>
            {/each}
          </ul>
        {/if}

        {#if todayVisible.length}
          <h2>Today</h2>
          <ul class="plain-list">
            {#each todayVisible as t (t.localId)}
              <li class:done={beat.has(t.localId)}>
                <button
                  class="check"
                  aria-label={`Complete “${t.title}”`}
                  onclick={() => completeOptimistic(t)}
                ><span class="fluent check-glyph" aria-hidden="true">{""}</span></button>
                <div class="task-text">
                  <span>{t.title}</span>
                  {#if t.notes}<small class="notes">{t.notes}</small>{/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}

        {#if nudgesVisible.length}
          <h2>Worth checking</h2>
          <ul class="plain-list">
            {#each nudgesVisible as n (n.id)}
              <li class="nudge">
                <span class="fluent nudge-glyph" aria-hidden="true">{""}</span>
                <div class="task-text">
                  <span>{n.title}</span>
                  <small>
                    {everyLabel(n.intervalDays)}{n.daysOverdue > 0 ? ` · ${lateLabel(n.daysOverdue)}` : ""}{n.createTaskOnAck
                      ? " · adds a task when done"
                      : ""}
                  </small>
                </div>
                <button
                  class="ghost"
                  aria-label={`Mark “${n.title}” done`}
                  onclick={() => ackOptimistic(n.id, n.title, n.createTaskOnAck)}
                >
                  Done
                </button>
              </li>
            {/each}
          </ul>
        {/if}

        {#if openCount === 0 && pending.length === 0}
          <div class="empty">
            {#if trusted}
              <span class="fluent empty-glyph" aria-hidden="true">{""}</span>
              <p>Nothing due. Go be free.</p>
            {:else}
              <p>No cached tasks to show.</p>
            {/if}
          </div>
        {/if}

        {#if (view.settings.showUndated || sessionShowUndated) && undatedVisible.length}
          <button class="ghost undated-toggle" aria-expanded={showUndatedOpen} onclick={() => (showUndatedOpen = !showUndatedOpen)}>
            <span class="fluent" aria-hidden="true">{showUndatedOpen ? "" : ""}</span>
            {showUndatedOpen ? "Hide" : "Show"}
            {undatedVisible.length} undated
          </button>
          {#if showUndatedOpen}
            <ul class="plain-list">
              {#each undatedVisible as t (t.localId)}
                <li class:done={beat.has(t.localId)}>
                  <button
                    class="check"
                    aria-label={`Complete “${t.title}”`}
                    onclick={() => completeOptimistic(t)}
                  ><span class="fluent check-glyph" aria-hidden="true">{""}</span></button>
                  <div class="task-text"><span>{t.title}</span></div>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>

      {#if pending.length === 1}
        <div class="undo" role="status">
          <span class="undo-text">
            {pending[0].kind === "task" ? `Completed “${pending[0].title}”` : `“${pending[0].title}” marked done`}
          </span>
          <button class="ghost small" onclick={() => undoPending(pending[0].key)}>Undo</button>
        </div>
      {:else if pending.length > 1}
        <div class="undo" role="status">
          <span class="undo-text">{pending.length} marked done</span>
          <button class="ghost small" onclick={undoAll}>Undo all</button>
        </div>
      {/if}

      {#if somedayFlash}
        <div class="undo" role="status">
          <span class="undo-text">Added to Someday.</span>
          <button
            class="ghost small"
            onclick={() => {
              showUndatedOpen = true;
              somedayFlash = false;
            }}
          >
            Show
          </button>
        </div>
      {/if}

      {#if deniedNote}
        <p class="denied-note" role="status">{deniedNote}</p>
      {/if}

      <div class="composer">
        <label class="visually-hidden" for="new-task">New task</label>
        <input
          id="new-task"
          bind:this={composerInput}
          placeholder="Add a task…"
          bind:value={newTitle}
          onkeydown={(e) => e.key === "Enter" && submitTask()}
        />
        <div class="seg" role="group" aria-label="Due date">
          <button
            type="button"
            class="seg-btn"
            class:active={newDue === "today"}
            aria-pressed={newDue === "today"}
            onclick={() => (newDue = "today")}
          >
            Today
          </button>
          <button
            type="button"
            class="seg-btn"
            class:active={newDue === "none"}
            aria-pressed={newDue === "none"}
            onclick={() => (newDue = "none")}
          >
            Someday
          </button>
        </div>
        <button class="primary" onclick={submitTask} disabled={!newTitle.trim()}>Add</button>
      </div>

      {#if view.dismiss.mode === "engage" && !engageSatisfied}
        <footer>
          <button class="ghost" onclick={onNothingToday}>Nothing today — let me through</button>
        </footer>
      {/if}
    {/if}
  {/if}
</main>

<style>
  /* ---- Windows 11 flyout world: tokens ---- */
  :global(:root) {
    --surface: #f9f9f9;
    --surface-2: #f3f3f3;
    --control: #ffffff;
    --control-hover: #f6f6f6;
    --subtle-hover: rgba(0, 0, 0, 0.045);
    --stroke: rgba(0, 0, 0, 0.09);
    --stroke-strong: rgba(0, 0, 0, 0.18);
    --card-stroke: rgba(0, 0, 0, 0.07);
    --text: #1b1b1b;
    --text-2: #5c5c5c;
    --text-disabled: #9d9d9d;
    --accent: #005fb8;
    --accent-hover: #0067c0;
    --accent-text: #ffffff;
    --critical: #c42b1c;
    --warn-fill: #fdf3d7;
    --warn-stroke: rgba(157, 93, 0, 0.25);
    --check-stroke: #767676;
    --shadow:
      0 2px 6px rgba(0, 0, 0, 0.08),
      0 14px 32px rgba(0, 0, 0, 0.16);
    --placeholder: #757575;
    --scroll-shadow: rgba(0, 0, 0, 0.16);
  }
  @media (prefers-color-scheme: dark) {
    :global(:root) {
      --surface: #262626;
      --surface-2: #2d2d2d;
      --control: rgba(255, 255, 255, 0.061);
      --control-hover: rgba(255, 255, 255, 0.09);
      --subtle-hover: rgba(255, 255, 255, 0.06);
      --stroke: rgba(255, 255, 255, 0.093);
      --stroke-strong: rgba(255, 255, 255, 0.22);
      --card-stroke: rgba(255, 255, 255, 0.09);
      --text: #ffffff;
      --text-2: #cfcfcf;
      --text-disabled: #8a8a8a;
      --accent: #60cdff;
      --accent-hover: #7fd7ff;
      --accent-text: #003553;
      --critical: #ff99a4;
      --warn-fill: #3a3117;
      --warn-stroke: rgba(255, 212, 108, 0.25);
      --check-stroke: #a0a0a0;
      --shadow:
        0 2px 6px rgba(0, 0, 0, 0.28),
        0 16px 40px rgba(0, 0, 0, 0.45);
      --placeholder: #a6a6a6;
      --scroll-shadow: rgba(0, 0, 0, 0.55);
    }
  }

  :global(html, body) {
    margin: 0;
    height: 100%;
    /* Fixed-size window: the page must never scroll — .lists scrolls inside.
       Without this, 1px of overflow summons the OS scrollbar. */
    overflow: hidden;
  }
  :global(body) {
    font-family: "Segoe UI Variable Text", "Segoe UI", system-ui, sans-serif;
    background: transparent;
    color: var(--text);
  }
  :global(::selection) {
    background: var(--accent);
    color: var(--accent-text);
  }

  .fluent {
    font-family: "Segoe Fluent Icons", "Segoe MDL2 Assets";
    font-size: 12px;
    line-height: 1;
    font-style: normal;
  }

  .visually-hidden {
    position: absolute;
    width: 1px;
    height: 1px;
    margin: -1px;
    padding: 0;
    overflow: hidden;
    clip: rect(0 0 0 0);
    white-space: nowrap;
    border: 0;
  }

  /* ---- card ---- */
  /* The card fills the window edge-to-edge: WebView2 renders "transparent"
     margins as a milky veil on some machines, so only the 8px corner curves
     stay transparent. No CSS drop shadow — Windows shades borderless windows. */
  main {
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    height: 100vh;
    padding: 18px 20px 16px;
    background: var(--surface);
    color: var(--text);
    border: 1px solid var(--card-stroke);
    border-radius: 8px;
    overflow: hidden;
  }
  @media (prefers-reduced-motion: no-preference) {
    main.entering {
      animation: enter 0.2s ease-out;
    }
    main.denied {
      animation: shake 0.4s;
    }
  }
  @keyframes enter {
    from {
      opacity: 0.5;
      transform: scale(0.985) translateY(4px);
    }
  }
  @keyframes shake {
    0%,
    100% {
      transform: translateX(0);
    }
    25% {
      transform: translateX(-6px);
    }
    75% {
      transform: translateX(6px);
    }
  }

  /* ---- header ---- */
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 10px;
  }
  .date {
    margin: 0;
    color: var(--text-2);
    font-size: 12.5px;
  }
  h1 {
    font-family: "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
    font-size: 28px;
    font-weight: 600;
    line-height: 1.15;
    margin: 2px 0 0;
    overflow-wrap: anywhere;
  }
  .header-actions {
    display: flex;
    gap: 6px;
  }
  .icon-btn {
    background: transparent;
    border: 1px solid transparent;
    color: var(--text-2);
    width: 34px;
    height: 34px;
    border-radius: 4px;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }
  .icon-btn .fluent {
    font-size: 13px;
  }
  .icon-btn:hover:not(:disabled) {
    background: var(--subtle-hover);
    color: var(--text);
  }
  .icon-btn[aria-pressed="true"] {
    background: var(--subtle-hover);
    color: var(--accent);
  }
  .icon-btn:disabled {
    color: var(--text-disabled);
    cursor: not-allowed;
  }
  /* Engage-gated close: still clickable so the refusal (shake + note) fires. */
  .icon-btn.gated:not(:disabled) {
    color: var(--text-disabled);
  }
  .icon-btn.close.counting {
    border-radius: 50%;
    border: 2px solid transparent;
    background:
      linear-gradient(var(--surface), var(--surface)) padding-box,
      conic-gradient(var(--accent) calc(var(--pct) * 1turn), var(--stroke) 0) border-box;
    color: var(--text);
  }
  .count {
    font-size: 12px;
    font-weight: 600;
    font-variant-numeric: tabular-nums;
  }

  /* ---- trust / error strips ---- */
  .strip {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border-radius: 4px;
    border: 1px solid var(--stroke);
    background: var(--control);
    font-size: 12.5px;
    margin-bottom: 8px;
  }
  .strip.warn {
    background: var(--warn-fill);
    border-color: var(--warn-stroke);
  }
  .strip.quiet {
    border-color: transparent;
    background: transparent;
    color: var(--text-2);
    padding: 2px 10px;
  }
  .strip-icon {
    color: var(--accent);
    flex: none;
  }
  .strip.warn .strip-icon {
    color: var(--critical);
  }
  .strip-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .error-detail {
    font-size: 12px;
    color: var(--text-2);
    margin: -4px 0 8px;
  }
  .error-detail p {
    overflow-wrap: anywhere;
    margin: 4px 0 0;
  }

  /* ---- lists ---- */
  .lists {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
    /* scroll shadows: only visible when content actually hides */
    background:
      linear-gradient(var(--surface) 30%, transparent) center top,
      linear-gradient(transparent, var(--surface) 70%) center bottom,
      radial-gradient(farthest-side at 50% 0, var(--scroll-shadow), transparent) center top,
      radial-gradient(farthest-side at 50% 100%, var(--scroll-shadow), transparent) center bottom;
    background-repeat: no-repeat;
    background-size:
      100% 32px,
      100% 32px,
      100% 10px,
      100% 10px;
    background-attachment: local, local, scroll, scroll;
  }
  .lists::-webkit-scrollbar,
  .settings::-webkit-scrollbar {
    width: 6px;
  }
  .lists::-webkit-scrollbar-thumb,
  .settings::-webkit-scrollbar-thumb {
    background: var(--stroke-strong);
    border-radius: 3px;
  }
  .lists::-webkit-scrollbar-track,
  .settings::-webkit-scrollbar-track {
    background: transparent;
  }

  h2 {
    font-size: 14px;
    font-weight: 600;
    color: var(--text-2);
    margin: 12px 0 4px;
  }
  h2.critical {
    color: var(--critical);
  }
  .plain-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 7px 6px;
    border-radius: 4px;
  }
  .lists li:hover {
    background: var(--subtle-hover);
  }
  li.done .task-text {
    text-decoration: line-through;
    opacity: 0.5;
  }
  @media (prefers-reduced-motion: no-preference) {
    li.done .task-text {
      transition: opacity 0.2s;
    }
    :global(input[type="checkbox"]::before) {
      transition: transform 0.12s ease-out;
    }
  }

  .check {
    box-sizing: border-box;
    width: 24px;
    height: 24px;
    min-width: 24px;
    border-radius: 50%;
    border: 2px solid var(--check-stroke);
    background: transparent;
    cursor: pointer;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0;
  }
  .check-glyph {
    font-size: 10px;
    color: transparent;
  }
  .check:hover {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .check:hover .check-glyph {
    color: var(--accent);
  }

  .task-text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .task-text span {
    font-size: 14px;
    overflow-wrap: anywhere;
  }
  .task-text small {
    color: var(--text-2);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .task-text small.notes {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  small.overdue-tag {
    color: var(--critical);
  }
  .nudge-glyph {
    color: var(--text-2);
    width: 24px;
    min-width: 24px;
    text-align: center;
  }

  .undated-toggle {
    margin-top: 12px;
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  .empty {
    color: var(--text-2);
    text-align: center;
    margin-top: 48px;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .empty-glyph {
    font-size: 26px;
    color: var(--accent);
  }
  .empty p {
    margin: 0;
    font-size: 14px;
  }

  /* ---- undo & notes ---- */
  .undo {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-radius: 4px;
    border: 1px solid var(--stroke);
    background: var(--control);
    font-size: 12.5px;
    margin-top: 6px;
  }
  .undo-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .denied-note {
    margin: 6px 0 0;
    font-size: 12.5px;
    color: var(--text-2);
    text-align: center;
  }

  /* ---- composer ---- */
  .composer {
    display: flex;
    gap: 8px;
    align-items: center;
    padding-top: 12px;
    margin-top: 6px;
    border-top: 1px solid var(--stroke);
  }
  .composer input {
    flex: 1;
    min-width: 0;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--stroke);
    border-radius: 4px;
    overflow: hidden;
    flex: none;
  }
  .seg-btn {
    background: var(--control);
    border: none;
    color: var(--text-2);
    padding: 7px 10px;
    font-size: 12px;
    font-family: inherit;
    cursor: pointer;
  }
  .seg-btn + .seg-btn {
    border-left: 1px solid var(--stroke);
  }
  .seg-btn:hover {
    background: var(--control-hover);
  }
  .seg-btn.active {
    background: color-mix(in srgb, var(--accent) 18%, var(--control));
    color: var(--text);
    font-weight: 600;
  }

  /* ---- inputs & buttons ---- */
  input,
  select {
    box-sizing: border-box;
    background: var(--control);
    border: 1px solid var(--stroke);
    border-bottom-color: var(--stroke-strong);
    color: var(--text);
    border-radius: 4px;
    padding: 7px 10px;
    font-size: 13px;
    font-family: inherit;
  }
  input::placeholder {
    color: var(--placeholder);
  }
  input:not([type="checkbox"]):not([type="range"]):focus,
  select:focus {
    outline: none;
    border-bottom: 2px solid var(--accent);
    padding-bottom: 6px;
  }

  /* Fluent toggle switch (Windows draws a switch, not a Chromium checkbox) */
  input[type="checkbox"] {
    appearance: none;
    -webkit-appearance: none;
    box-sizing: border-box;
    width: 40px;
    height: 20px;
    border-radius: 10px;
    border: 1px solid var(--stroke-strong);
    background: var(--control);
    position: relative;
    cursor: pointer;
    flex: none;
    padding: 0;
    margin: 0;
  }
  input[type="checkbox"]::before {
    content: "";
    position: absolute;
    top: 3px;
    left: 3px;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: var(--text-2);
  }
  input[type="checkbox"]:checked {
    background: var(--accent);
    border-color: var(--accent);
  }
  input[type="checkbox"]:checked::before {
    transform: translateX(20px);
    background: var(--accent-text);
  }

  /* Fluent slider */
  input[type="range"] {
    appearance: none;
    -webkit-appearance: none;
    height: 20px;
    padding: 0;
    border: none;
    background: transparent;
    cursor: pointer;
  }
  input[type="range"]::-webkit-slider-runnable-track {
    height: 4px;
    border-radius: 2px;
    background: var(--stroke-strong);
  }
  input[type="range"]::-webkit-slider-thumb {
    -webkit-appearance: none;
    width: 16px;
    height: 16px;
    margin-top: -6px;
    border-radius: 50%;
    background: var(--accent);
    border: none;
  }

  button {
    font-family: inherit;
  }
  /* Text fields and selects carry focus in the accent underline; the ring is
     for buttons and the appearance-less controls. */
  button:focus-visible,
  input[type="checkbox"]:focus-visible,
  input[type="range"]:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
  }

  .primary {
    background: var(--accent);
    color: var(--accent-text);
    border: none;
    border-radius: 4px;
    padding: 8px 14px;
    cursor: pointer;
    font-size: 13px;
    font-weight: 600;
    flex: none;
  }
  .primary:hover:not(:disabled) {
    background: var(--accent-hover);
  }
  .primary:disabled {
    opacity: 0.5;
    cursor: default;
  }

  .ghost {
    background: transparent;
    border: 1px solid var(--stroke);
    color: var(--text);
    border-radius: 4px;
    padding: 5px 10px;
    cursor: pointer;
    font-size: 12px;
    flex: none;
  }
  .ghost:hover {
    background: var(--subtle-hover);
  }
  .ghost.small {
    padding: 4px 8px;
  }
  .ghost.danger {
    color: var(--critical);
    border-color: color-mix(in srgb, var(--critical) 45%, transparent);
  }

  /* ---- footer ---- */
  footer {
    padding-top: 10px;
    text-align: center;
  }

  /* ---- settings ---- */
  .settings {
    flex: 1;
    overflow-y: auto;
    min-height: 0;
  }
  .panel-title {
    font-family: "Segoe UI Variable Display", "Segoe UI", system-ui, sans-serif;
    font-size: 17px;
    font-weight: 600;
    color: var(--text);
    margin: 4px 0 2px;
  }
  h3 {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-2);
    margin: 16px 0 2px;
  }
  .settings .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 9px 0;
    font-size: 13px;
  }
  .settings .row > span {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .g-account {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 9px 0;
  }
  .g-id {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .g-email {
    font-size: 13px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .g-state {
    font-size: 12px;
    color: var(--text-2);
  }
  .g-actions {
    display: flex;
    gap: 8px;
  }
  .add {
    display: flex;
    gap: 8px;
    align-items: center;
    margin: 6px 0;
  }
  .add input:not(.days) {
    flex: 1;
    min-width: 0;
  }
  .days-label {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--text-2);
    flex: none;
  }
  .days {
    width: 56px;
  }
  .auth-error {
    color: var(--critical);
    font-size: 12px;
    overflow-wrap: anywhere;
    margin: 2px 0 6px;
  }
  .field-error {
    color: var(--critical);
    font-size: 12px;
    margin: 2px 0 6px;
  }
  .hint {
    color: var(--text-2);
    font-size: 12px;
    margin: 2px 0 6px;
  }
</style>
