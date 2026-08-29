<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import {
    getBootView,
    addTask,
    completeTask,
    ackNudge,
    nothingToday,
    dismissWindow,
    updateSettings,
    type BootView,
    type Settings,
  } from "$lib/api";

  let view = $state<BootView | null>(null);
  let newTitle = $state("");
  let newDue = $state<"today" | "none">("today");
  let showSettings = $state(false);
  let showUndatedOpen = $state(false);
  let blockedMs = $state(0);
  let denied = $state(false);
  let completing = $state<Set<string>>(new Set());

  let countdownTimer: ReturnType<typeof setInterval> | null = null;
  let unlisteners: UnlistenFn[] = [];

  const todayStr = () => {
    const d = new Date();
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;
  };

  async function refresh() {
    view = await getBootView();
    blockedMs = view.dismiss.blockedForMs;
    startCountdown();
  }

  function startCountdown() {
    if (countdownTimer) clearInterval(countdownTimer);
    if (blockedMs <= 0) return;
    const startedAt = Date.now();
    const initial = blockedMs;
    countdownTimer = setInterval(() => {
      blockedMs = Math.max(0, initial - (Date.now() - startedAt));
      if (blockedMs === 0 && countdownTimer) clearInterval(countdownTimer);
    }, 200);
  }

  async function submitTask() {
    const title = newTitle.trim();
    if (!title) return;
    await addTask(title, newDue === "today" ? todayStr() : undefined);
    newTitle = "";
    await refresh();
  }

  async function complete(localId: string) {
    completing = new Set(completing).add(localId);
    // Brief strikethrough beat before the row leaves the list.
    setTimeout(async () => {
      await completeTask(localId);
      completing.delete(localId);
      await refresh();
    }, 350);
  }

  async function onAckNudge(id: string) {
    await ackNudge(id);
    await refresh();
  }

  async function tryDismiss() {
    const result = await dismissWindow();
    if (!result.allowed) {
      denied = true;
      blockedMs = result.blockedForMs;
      startCountdown();
      setTimeout(() => (denied = false), 600);
    }
  }

  async function onNothingToday() {
    await nothingToday();
    await dismissWindow();
  }

  async function saveSettings(patch: Partial<Settings>) {
    if (!view) return;
    view.settings = await updateSettings({ ...view.settings, ...patch });
    await refresh();
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === "Escape") {
      if (showSettings) showSettings = false;
      else tryDismiss();
    }
  }

  onMount(async () => {
    await refresh();
    unlisteners.push(await listen("window-shown", () => refresh()));
    unlisteners.push(await listen("tasks-changed", () => refresh()));
  });

  onDestroy(() => {
    if (countdownTimer) clearInterval(countdownTimer);
    unlisteners.forEach((u) => u());
  });

  const dateLabel = new Date().toLocaleDateString(undefined, {
    weekday: "long",
    month: "long",
    day: "numeric",
  });

  let openCount = $derived(
    view ? view.today.length + view.overdue.length + view.nudges.length : 0
  );
  let engageSatisfied = $derived(view?.dismiss.mode !== "engage" || view?.dismiss.engaged);
</script>

<svelte:window onkeydown={onKeydown} />

<main class:denied data-tauri-drag-region>
  <header data-tauri-drag-region>
    <div data-tauri-drag-region>
      <h1 data-tauri-drag-region>{dateLabel}</h1>
      <p class="subtitle" data-tauri-drag-region>
        {openCount === 0 ? "All clear." : `${openCount} thing${openCount === 1 ? "" : "s"} need you`}
      </p>
    </div>
    <div class="header-actions">
      <button class="icon-btn" title="Settings" onclick={() => (showSettings = !showSettings)}>⚙</button>
      <button
        class="icon-btn close"
        title="Dismiss"
        disabled={blockedMs > 0 || !engageSatisfied}
        onclick={tryDismiss}
      >
        {#if blockedMs > 0}{Math.ceil(blockedMs / 1000)}{:else}✕{/if}
      </button>
    </div>
  </header>

  {#if view}
    {#if showSettings}
      <section class="settings">
        <h2>Settings</h2>
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
              onchange={(e) => saveSettings({ pauseSeconds: +e.currentTarget.value })}
            />
          </label>
        {/if}
        <label class="row">
          <span>Show undated tasks</span>
          <input
            type="checkbox"
            checked={view.settings.showUndated}
            onchange={(e) => saveSettings({ showUndated: e.currentTarget.checked })}
          />
        </label>
        <p class="hint">Google sync and start-at-login arrive in later milestones.</p>
      </section>
    {:else}
      <section class="add">
        <input
          placeholder="Add a task…"
          bind:value={newTitle}
          onkeydown={(e) => e.key === "Enter" && submitTask()}
        />
        <select bind:value={newDue}>
          <option value="today">Today</option>
          <option value="none">Someday</option>
        </select>
        <button class="primary" onclick={submitTask} disabled={!newTitle.trim()}>Add</button>
      </section>

      <section class="lists">
        {#if view.overdue.length}
          <h2 class="overdue-h">Overdue</h2>
          <ul>
            {#each view.overdue as t (t.localId)}
              <li class:done={completing.has(t.localId)}>
                <button class="check" onclick={() => complete(t.localId)} aria-label="Complete"></button>
                <div class="task-text">
                  <span>{t.title}</span>
                  <small class="overdue-tag">due {t.dueDate}</small>
                </div>
              </li>
            {/each}
          </ul>
        {/if}

        {#if view.today.length}
          <h2>Today</h2>
          <ul>
            {#each view.today as t (t.localId)}
              <li class:done={completing.has(t.localId)}>
                <button class="check" onclick={() => complete(t.localId)} aria-label="Complete"></button>
                <div class="task-text">
                  <span>{t.title}</span>
                  {#if t.notes}<small>{t.notes}</small>{/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}

        {#if view.nudges.length}
          <h2>Worth checking</h2>
          <ul>
            {#each view.nudges as n (n.id)}
              <li class="nudge">
                <div class="task-text">
                  <span>{n.title}</span>
                  <small>every {n.intervalDays} days{n.daysOverdue > 0 ? ` · ${n.daysOverdue}d late` : ""}</small>
                </div>
                <button class="ghost" onclick={() => onAckNudge(n.id)}>Done</button>
              </li>
            {/each}
          </ul>
        {/if}

        {#if openCount === 0}
          <p class="empty">Nothing due. Go be free.</p>
        {/if}

        {#if view.settings.showUndated && view.undated.length}
          <button class="ghost undated-toggle" onclick={() => (showUndatedOpen = !showUndatedOpen)}>
            {showUndatedOpen ? "Hide" : "Show"} {view.undated.length} undated
          </button>
          {#if showUndatedOpen}
            <ul>
              {#each view.undated as t (t.localId)}
                <li class:done={completing.has(t.localId)}>
                  <button class="check" onclick={() => complete(t.localId)} aria-label="Complete"></button>
                  <div class="task-text"><span>{t.title}</span></div>
                </li>
              {/each}
            </ul>
          {/if}
        {/if}
      </section>

      {#if view.dismiss.mode === "engage" && !view.dismiss.engaged}
        <footer>
          <button class="ghost" onclick={onNothingToday}>Nothing today — let me through</button>
        </footer>
      {/if}
    {/if}
  {/if}
</main>

<style>
  :global(html, body) {
    margin: 0;
    height: 100%;
  }
  :global(body) {
    font-family: "Segoe UI Variable", "Segoe UI", system-ui, sans-serif;
    background: transparent;
  }
  main {
    display: flex;
    flex-direction: column;
    height: 100vh;
    box-sizing: border-box;
    padding: 20px 22px;
    background: #16181d;
    color: #e8eaed;
    border: 1px solid #2c2f36;
    border-radius: 12px;
    overflow: hidden;
    transition: transform 0.1s;
  }
  main.denied {
    animation: shake 0.4s;
  }
  @keyframes shake {
    0%, 100% { transform: translateX(0); }
    25% { transform: translateX(-6px); }
    75% { transform: translateX(6px); }
  }
  header {
    display: flex;
    justify-content: space-between;
    align-items: flex-start;
    margin-bottom: 14px;
  }
  h1 {
    font-size: 1.25rem;
    margin: 0;
  }
  .subtitle {
    margin: 2px 0 0;
    color: #9aa0a6;
    font-size: 0.85rem;
  }
  .header-actions {
    display: flex;
    gap: 6px;
  }
  .icon-btn {
    background: #22252c;
    border: none;
    color: #c8ccd2;
    width: 32px;
    height: 32px;
    border-radius: 8px;
    cursor: pointer;
    font-size: 0.9rem;
  }
  .icon-btn:hover:not(:disabled) {
    background: #2c3038;
  }
  .icon-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .icon-btn.close:disabled {
    font-variant-numeric: tabular-nums;
  }
  .add {
    display: flex;
    gap: 8px;
    margin-bottom: 14px;
  }
  .add input:not([type]) {
    flex: 1;
  }
  input, select {
    background: #22252c;
    border: 1px solid #2c2f36;
    color: #e8eaed;
    border-radius: 8px;
    padding: 8px 10px;
    font-size: 0.9rem;
    font-family: inherit;
  }
  input:focus, select:focus {
    outline: 1px solid #4d7dd6;
  }
  button.primary {
    background: #3b6fd4;
    color: white;
    border: none;
    border-radius: 8px;
    padding: 8px 14px;
    cursor: pointer;
    font-size: 0.9rem;
  }
  button.primary:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .lists {
    flex: 1;
    overflow-y: auto;
  }
  h2 {
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: #9aa0a6;
    margin: 14px 0 6px;
  }
  h2.overdue-h {
    color: #e07a6a;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 6px;
    border-radius: 8px;
  }
  li:hover {
    background: #1c1f25;
  }
  li.done .task-text {
    text-decoration: line-through;
    opacity: 0.5;
  }
  .check {
    width: 18px;
    height: 18px;
    min-width: 18px;
    border-radius: 50%;
    border: 2px solid #5f6368;
    background: transparent;
    cursor: pointer;
  }
  .check:hover {
    border-color: #3b6fd4;
    background: #3b6fd422;
  }
  .task-text {
    display: flex;
    flex-direction: column;
    flex: 1;
    min-width: 0;
  }
  .task-text span {
    font-size: 0.95rem;
  }
  .task-text small {
    color: #9aa0a6;
    font-size: 0.75rem;
  }
  small.overdue-tag {
    color: #e07a6a;
  }
  li.nudge {
    border-left: 3px solid #c9a24b;
    padding-left: 10px;
  }
  .ghost {
    background: transparent;
    border: 1px solid #2c2f36;
    color: #c8ccd2;
    border-radius: 8px;
    padding: 5px 10px;
    cursor: pointer;
    font-size: 0.8rem;
  }
  .ghost:hover {
    background: #22252c;
  }
  .undated-toggle {
    margin-top: 12px;
  }
  .empty {
    color: #9aa0a6;
    text-align: center;
    margin-top: 40px;
  }
  footer {
    padding-top: 10px;
    text-align: center;
  }
  .settings {
    flex: 1;
  }
  .settings .row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 10px 0;
    font-size: 0.9rem;
  }
  .hint {
    color: #9aa0a6;
    font-size: 0.8rem;
    margin-top: 16px;
  }
</style>
