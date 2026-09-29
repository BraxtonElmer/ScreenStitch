<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { api, type Desk as DeskRect, type Material, type State, type Theme } from './lib/api';
  import { alignRow, diagonalInches, textOn, withDiagonal, type Align } from './lib/geometry';
  import Desk from './lib/Desk.svelte';
  import Switch from './lib/Switch.svelte';
  import Segmented from './lib/Segmented.svelte';
  import TitleBar from './lib/TitleBar.svelte';
  import UpdatePanel from './lib/UpdatePanel.svelte';
  import { exit } from '@tauri-apps/plugin-process';
  import { findUpdate, type Update } from './lib/update';

  let st = $state<State | null>(null);
  let desk = $state<DeskRect[]>([]);
  let selected = $state(0);
  let checking = $state(false);
  let unreachable = $state<string[]>([]);
  let history = $state<DeskRect[][]>([]);
  let error = $state('');

  const media = matchMedia('(prefers-color-scheme: dark)');
  let systemDark = $state(media.matches);
  media.addEventListener('change', (e) => (systemDark = e.matches));

  const ACCENTS = ['#0067c0', '#0f7b6c', '#8764b8', '#c239b3', '#ca5010', '#498205'];

  const screen = $derived(st?.screens[selected]);
  const current = $derived(desk[selected]);
  const count = $derived(st?.screens.length ?? 0);
  const accent = $derived(st ? st.appearance.accent || st.systemAccent : '#0067c0');

  const headline = $derived(
    !st ? '' : !st.enabled ? 'ScreenStitch is off' : count < 2 ? 'Only one screen connected' : 'Your screens are stitched',
  );
  const subline = $derived(
    !st
      ? ''
      : !st.enabled
        ? 'Windows moves the cursor between screens the usual way.'
        : count < 2
          ? 'Connect another screen and ScreenStitch lines it up automatically.'
          : `The cursor crosses at the right height between all ${count} screens.`,
  );

  // Theme, accent and window material: applied to the page and to the window
  // itself (glass tint, title bar).
  $effect(() => {
    if (!st) return;
    const theme = st.appearance.theme;
    const dark = theme === 'system' ? systemDark : theme === 'dark';
    const root = document.documentElement;
    root.dataset.theme = dark ? 'dark' : 'light';
    root.style.setProperty('--accent', accent);
    root.style.setProperty('--on-accent', textOn(accent));
    getCurrentWindow()
      .setTheme(theme === 'system' ? null : theme)
      .catch(() => {});
    api
      .applyMaterial(st.appearance.material, dark)
      .then((m) => (root.dataset.material = m))
      .catch(() => {});
  });

  async function load() {
    try {
      st = await api.state();
      desk = st.screens.map((s) => ({ ...s.desk }));
      unreachable = st.unreachable;
      selected = Math.max(0, st.screens.findIndex((s) => s.primary));
    } catch (e) {
      error = String(e);
    }
  }

  // ---- updates ----

  // "update": opened by the tray app's daily check; only shown if there is one.
  let mode = $state<'settings' | 'update'>('settings');
  let available = $state<Update | null>(null);
  let upToDate = $state(false);

  async function checkNow() {
    upToDate = false;
    available = await findUpdate();
    upToDate = !available;
  }

  onMount(async () => {
    mode = await api.launchMode();
    await load();
    if (mode === 'update') {
      available = st?.checkUpdates ? await findUpdate() : null;
      if (!available) return exit(0);
    }
    await tick();
    await getCurrentWindow().show();
    await getCurrentWindow().setFocus();
    if (mode === 'settings' && st?.checkUpdates) available = await findUpdate();
  });

  // ---- layout edits: every change is undoable and applied live ----

  let saveTimer: ReturnType<typeof setTimeout> | undefined;

  function beforeChange() {
    history = [...history.slice(-49), desk.map((d) => ({ ...d }))];
  }

  function commit() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(async () => {
      if (!st) return;
      try {
        const r = await api.saveLayout(desk.map((d, i) => ({ id: st!.screens[i].id, ...d })));
        unreachable = r.unreachable;
        error = '';
      } catch (e) {
        error = String(e);
      }
    }, 80);
  }

  function undo() {
    const prev = history.at(-1);
    if (!prev) return;
    history = history.slice(0, -1);
    desk = prev;
    commit();
  }

  async function resetToAutomatic() {
    beforeChange();
    desk = await api.autoLayout();
    commit();
  }

  function align(how: Align) {
    beforeChange();
    desk = alignRow(desk, selected, how);
    commit();
  }

  function setDiagonal(value: string) {
    const inches = parseFloat(value);
    if (!current || !(inches >= 5 && inches <= 120)) return;
    beforeChange();
    desk[selected] = withDiagonal(current, inches);
    commit();
  }

  function useReportedSize() {
    if (!screen || !current) return;
    beforeChange();
    const [w, h] = screen.detectedMm;
    desk[selected] = { x: current.x, y: current.y + current.h - h, w, h };
    commit();
  }

  async function toggleCheck() {
    checking = !checking;
    const ok = await api.setAlignmentLine(checking);
    if (!ok && checking) error = "ScreenStitch isn't running in the tray, so the line can't be shown on your screens.";
  }

  // ---- settings ----

  async function setEnabled(on: boolean) {
    if (!st) return;
    st.enabled = on;
    await api.setEnabled(on).catch((e) => (error = String(e)));
  }

  async function setPause(on: boolean) {
    if (!st) return;
    st.pauseInFullscreen = on;
    await api.setPauseInFullscreen(on).catch((e) => (error = String(e)));
  }

  async function setUpdates(on: boolean) {
    if (!st) return;
    st.checkUpdates = on;
    await api.setCheckUpdates(on).catch((e) => (error = String(e)));
  }

  async function setStartup(on: boolean) {
    if (!st) return;
    try {
      st.startWithWindows = await api.setStartWithWindows(on);
    } catch (e) {
      error = String(e);
    }
  }

  async function setTheme(theme: Theme) {
    if (!st) return;
    st.appearance.theme = theme;
    await api.setAppearance({ ...st.appearance });
  }

  async function setMaterial(material: Material) {
    if (!st) return;
    st.appearance.material = material;
    await api.setAppearance({ ...st.appearance });
  }

  async function setAccent(hex: string) {
    if (!st) return;
    st.appearance.accent = hex;
    await api.setAppearance({ ...st.appearance });
  }

  function keydown(e: KeyboardEvent) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'z' && !(e.target instanceof HTMLInputElement)) {
      e.preventDefault();
      undo();
    }
  }

  const lostNames = $derived(
    st ? st.screens.filter((s) => unreachable.includes(s.id)).map((s) => `screen ${s.number}`) : [],
  );
  const sizeChanged = $derived(
    !!screen && !!current && Math.abs(current.w - screen.detectedMm[0]) + Math.abs(current.h - screen.detectedMm[1]) > 1,
  );
</script>

<svelte:window onkeydown={keydown} />

<TitleBar />
{#if st && mode === 'update' && available}
  <UpdatePanel update={available} current={st.version} variant="window" onlater={() => exit(0)} />
{:else if st}
  <main>
    <header>
      <div class="titles">
        <h1>{headline}</h1>
        <p class="muted">{subline}</p>
      </div>
      <span class="state">{st.enabled ? 'On' : 'Off'}</span>
      <Switch big checked={st.enabled} label="Turn ScreenStitch on or off" onchange={setEnabled} />
    </header>

    {#if error}
      <div class="banner error" role="alert">
        <span>{error}</span>
        <button class="link" onclick={() => (error = '')}>Dismiss</button>
      </div>
    {/if}

    {#if available}
      <UpdatePanel update={available} current={st.version} variant="banner" onlater={() => (available = null)} />
    {/if}

    <div class="grid">
      <section class="card desk">
        <div class="section-head">
          <h2>Your desk</h2>
          <span class="muted">Drag the screens so they match where they really are. The stitches show where the cursor crosses.</span>
        </div>

        <Desk
          screens={st.screens}
          bind:desk
          bind:selected
          {checking}
          {unreachable}
          onbeforechange={beforeChange}
          oncommit={commit}
        />

        {#if checking}
          <div class="banner info">
            <span class="swatch-line"></span>
            <span>A line is now drawn across all your screens at the same height. If it looks broken where two screens meet, drag that screen here until the line looks straight.</span>
          </div>
        {:else if lostNames.length}
          <div class="banner warn">
            <span>The cursor can't reach {lostNames.join(' and ')}. Drag it so it touches another screen.</span>
          </div>
        {/if}

        <div class="toolbar">
          <button class="btn primary" onclick={toggleCheck} aria-pressed={checking}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round"><path d="M1.5 8h13M4.5 5v6M11.5 5v6" /></svg>
            {checking ? 'Done checking' : 'Check alignment'}
          </button>
          <span class="spacer"></span>
          <button class="btn" onclick={undo} disabled={history.length === 0}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M5.5 9.5 2 6l3.5-3.5" /><path d="M2 6h8a4 4 0 0 1 0 8H8" /></svg>
            Undo
          </button>
          <button class="btn" onclick={resetToAutomatic}>
            <svg width="16" height="16" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M2.5 8a5.5 5.5 0 1 0 1.6-3.9L2.5 5.7" /><path d="M2.5 2.5v3.2h3.2" /></svg>
            Reset to automatic
          </button>
        </div>
      </section>

      <aside>
        {#if screen && current}
          <section class="card pad">
            <div class="screen-head">
              <span class="badge">{screen.number}</span>
              <div>
                <h2>{screen.name}</h2>
                <span class="muted">{screen.primary ? 'Main display' : 'Selected screen'}</span>
              </div>
            </div>
            <dl>
              <dt>Resolution</dt>
              <dd>{screen.px.w} × {screen.px.h}</dd>
              <dt>Scaling</dt>
              <dd>{screen.scale}%</dd>
              <dt>Size</dt>
              <dd class="size">
                <label class="diag">
                  <input
                    type="number"
                    min="5"
                    max="120"
                    step="0.1"
                    value={diagonalInches(current.w, current.h).toFixed(1)}
                    onchange={(e) => setDiagonal(e.currentTarget.value)}
                    aria-label="Screen size in inches, measured diagonally"
                  />
                  <span>inch</span>
                </label>
              </dd>
              <dt></dt>
              <dd class="muted small">{Math.round(current.w)} × {Math.round(current.h)} mm</dd>
            </dl>
            <p class="note muted">
              {#if sizeChanged}
                You changed this size. <button class="link" onclick={useReportedSize}>Use the size the monitor reports</button>
              {:else if screen.sizeFromEdid}
                Read from the monitor.
              {:else}
                This monitor doesn't report its size, so it's estimated. Enter the real size for the best result.
              {/if}
            </p>
            {#if count > 1}
              <div class="align" role="group" aria-label="Line up this screen's row">
                <span class="muted">Line up the row with this screen</span>
                <div class="align-buttons">
                  <button class="btn" onclick={() => align('bottom')}>Bottoms</button>
                  <button class="btn" onclick={() => align('centre')}>Centres</button>
                  <button class="btn" onclick={() => align('top')}>Tops</button>
                </div>
              </div>
            {/if}
          </section>
        {/if}

        <section class="card options">
          <div class="row">
            <div class="text">
              <span>Start with Windows</span>
              <span class="muted small">Waits quietly in the tray, about 2 MB</span>
            </div>
            <Switch checked={st.startWithWindows} label="Start with Windows" onchange={setStartup} />
          </div>
          <div class="row">
            <div class="text">
              <span>Pause in fullscreen games</span>
              <span class="muted small">Fully off while a game is in front</span>
            </div>
            <Switch checked={st.pauseInFullscreen} label="Pause in fullscreen games" onchange={setPause} />
          </div>
          <div class="row">
            <div class="text">
              <span>Check for updates</span>
              <span class="muted small">Asks before installing anything</span>
            </div>
            <Switch checked={st.checkUpdates} label="Check for updates automatically" onchange={setUpdates} />
          </div>
          <div class="row stack">
            <span>Appearance</span>
            <Segmented
              label="Theme"
              value={st.appearance.theme}
              options={[
                { value: 'system', label: 'Windows' },
                { value: 'light', label: 'Light' },
                { value: 'dark', label: 'Dark' },
              ]}
              onchange={setTheme}
            />
            <Segmented
              label="Window background"
              value={st.appearance.material}
              options={[
                { value: 'acrylic', label: 'Frosted glass' },
                ...(st.mica ? [{ value: 'mica' as Material, label: 'Mica' }] : []),
                { value: 'solid', label: 'Solid' },
              ]}
              onchange={setMaterial}
            />
          </div>
          <div class="row stack">
            <span>Accent colour</span>
            <div class="swatches" role="radiogroup" aria-label="Accent colour">
              <button
                class="swatch system"
                role="radio"
                aria-checked={st.appearance.accent === ''}
                aria-label="Windows accent colour"
                title="Same as Windows"
                style:--c={st.systemAccent}
                onclick={() => setAccent('')}
              ></button>
              {#each ACCENTS as c (c)}
                <button
                  class="swatch"
                  role="radio"
                  aria-checked={st.appearance.accent === c}
                  aria-label="Accent {c}"
                  style:--c={c}
                  onclick={() => setAccent(c)}
                ></button>
              {/each}
            </div>
          </div>
        </section>
      </aside>
    </div>

    <footer class="muted">
      <span>ScreenStitch {st.version} · Free and open source</span>
      {#if upToDate}
        <span>You have the latest version.</span>
      {:else if !available}
        <button class="link" onclick={checkNow}>Check for updates now</button>
      {/if}
      <span class="spacer"></span>
      <button class="link" onclick={() => api.open('source')}>Source code</button>
      <button class="link" onclick={() => api.open('issues')}>Report a problem</button>
      <button class="link" onclick={() => api.open('folder')}>Settings folder</button>
    </footer>
  </main>
{/if}

<style>
  main {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 6px 24px 14px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 14px;
  }
  .titles {
    flex: 1;
    min-width: 0;
  }
  h1 {
    margin: 0;
    font: 600 26px/1.2 var(--font-display);
  }
  h1 + p {
    margin: 4px 0 0;
  }
  h2 {
    margin: 0;
    font-size: 15px;
    font-weight: 600;
  }
  .state {
    font-weight: 600;
    min-width: 24px;
    text-align: right;
  }
  .grid {
    flex: 1;
    min-height: 0;
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 16px;
  }
  .desk {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 16px;
    min-height: 0;
  }
  .section-head {
    display: flex;
    align-items: baseline;
    gap: 12px;
    flex-wrap: wrap;
  }
  .section-head .muted {
    font-size: 13px;
  }
  .toolbar {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }
  .align {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 14px;
    padding-top: 14px;
    border-top: 1px solid var(--stroke);
  }
  .align-buttons {
    display: flex;
    gap: 6px;
  }
  .align-buttons .btn {
    flex: 1;
    justify-content: center;
    padding: 0 8px;
  }
  .spacer {
    flex: 1;
  }
  aside {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-height: 0;
    overflow-y: auto;
  }
  .pad {
    padding: 16px;
  }
  .screen-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .badge {
    width: 36px;
    height: 36px;
    flex-shrink: 0;
    border-radius: 8px;
    display: grid;
    place-items: center;
    background: var(--accent);
    color: var(--on-accent);
    font: 600 17px var(--font-display);
  }
  dl {
    display: grid;
    grid-template-columns: 84px 1fr;
    row-gap: 10px;
    margin: 16px 0 0;
    align-items: center;
  }
  dt {
    color: var(--text-2);
  }
  dd {
    margin: 0;
  }
  .size {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .diag {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  .diag input {
    width: 64px;
    height: 30px;
    padding: 0 8px;
    border-radius: var(--radius-sm);
    border: 1px solid var(--stroke-strong);
    border-bottom-color: var(--text-2);
    background: var(--control);
  }
  .diag input:focus {
    outline: none;
    border-bottom: 2px solid var(--accent);
  }
  .note {
    margin: 12px 0 0;
    font-size: 12px;
  }
  .options {
    padding: 4px 16px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 10px 0;
    border-bottom: 1px solid var(--stroke);
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row > .text {
    flex: 1;
    display: flex;
    flex-direction: column;
  }
  .row.stack {
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
  }
  .small {
    font-size: 12px;
  }
  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: 10px;
    padding: 2px;
  }
  .swatch {
    width: 26px;
    height: 26px;
    border-radius: 50%;
    border: 0;
    padding: 0;
    background: var(--c);
    cursor: pointer;
    box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.15);
    transition: box-shadow var(--fast);
  }
  .swatch[aria-checked='true'] {
    box-shadow:
      0 0 0 2px var(--surface),
      0 0 0 4px var(--c);
  }
  .swatch.system {
    background: conic-gradient(var(--c) 0 50%, transparent 50% 100%), var(--control);
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 9px 12px;
    border-radius: var(--radius-sm);
    font-size: 13px;
  }
  .banner.info {
    background: var(--surface-2);
    border: 1px solid var(--stroke);
  }
  .banner.warn,
  .banner.error {
    background: var(--warn-bg);
    color: var(--warn-text);
  }
  .banner.error {
    justify-content: space-between;
  }
  .swatch-line {
    width: 18px;
    height: 2px;
    flex-shrink: 0;
    background: var(--line);
  }
  footer {
    display: flex;
    align-items: center;
    gap: 16px;
    font-size: 12px;
  }
  footer .link {
    font-size: 12px;
  }
</style>
