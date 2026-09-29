<script lang="ts">
  import type { Desk, Screen } from './api';
  import { alignmentLineMm, bounds, diagonalInches, snap, stitches } from './geometry';

  let {
    screens,
    desk = $bindable(),
    selected = $bindable(),
    checking,
    unreachable,
    onbeforechange,
    oncommit,
  }: {
    screens: Screen[];
    desk: Desk[];
    selected: number;
    checking: boolean;
    unreachable: string[];
    onbeforechange: () => void;
    oncommit: () => void;
  } = $props();

  const PAD = 36;
  const SNAP_PX = 10;
  /** Stitches: short diagonal strokes across the seam, like thread. */
  const STITCH_SPACING = 11;
  const STITCH_HALF = 6;

  let width = $state(0);
  let height = $state(0);

  // While dragging, the view is frozen so the screen doesn't slide under the pointer.
  let frozen: { scale: number; ox: number; oy: number } | null = $state(null);

  const view = $derived.by(() => {
    if (frozen) return frozen;
    const b = bounds(desk);
    const scale = Math.max(0.05, Math.min((width - PAD * 2) / b.w, (height - PAD * 2) / b.h));
    return { scale, ox: (width - b.w * scale) / 2 - b.x * scale, oy: (height - b.h * scale) / 2 - b.y * scale };
  });

  const seams = $derived(stitches(desk));
  const lineY = $derived(alignmentLineMm(desk));
  const box = $derived(bounds(desk));

  let drag: { i: number; px: number; py: number; start: Desk; moved: boolean } | null = null;
  // Stitches are hidden while a screen is being moved and sewn back in once it's placed.
  let dragging = $state(false);

  function down(e: PointerEvent, i: number) {
    if (e.button !== 0) return;
    selected = i;
    drag = { i, px: e.clientX, py: e.clientY, start: { ...desk[i] }, moved: false };
    frozen = { ...view };
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    const dx = (e.clientX - drag.px) / view.scale;
    const dy = (e.clientY - drag.py) / view.scale;
    if (!drag.moved) {
      if (Math.hypot(dx, dy) * view.scale < 3) return;
      drag.moved = true;
      dragging = true;
      onbeforechange();
    }
    const m = { ...drag.start, x: drag.start.x + dx, y: drag.start.y + dy };
    const others = desk.filter((_, j) => j !== drag!.i);
    const s = e.altKey ? m : snap(m, others, SNAP_PX / view.scale);
    desk[drag.i] = { ...m, x: s.x, y: s.y };
  }

  function up() {
    if (drag?.moved) oncommit();
    drag = null;
    dragging = false;
    frozen = null;
  }

  function key(e: KeyboardEvent, i: number) {
    const step = e.shiftKey ? 10 : 1;
    const d = { ArrowLeft: [-step, 0], ArrowRight: [step, 0], ArrowUp: [0, -step], ArrowDown: [0, step] }[e.key];
    if (!d) return;
    e.preventDefault();
    onbeforechange();
    desk[i] = { ...desk[i], x: desk[i].x + d[0], y: desk[i].y + d[1] };
    oncommit();
  }

  const px = (mm: number) => `${mm * view.scale}px`;
</script>

<div class="canvas" bind:clientWidth={width} bind:clientHeight={height}>
  {#if width > 0}
    <div class="world" style:transform="translate({view.ox}px, {view.oy}px)">
      {#each desk as r, i (screens[i].id)}
        {@const s = screens[i]}
        {@const lost = unreachable.includes(s.id)}
        <button
          class="tile"
          class:selected={selected === i}
          class:lost
          style:left={px(r.x)}
          style:top={px(r.y)}
          style:width={px(r.w)}
          style:height={px(r.h)}
          aria-label="Screen {s.number}, {s.name}. Drag to move, arrow keys to nudge."
          onpointerdown={(e) => down(e, i)}
          onpointermove={move}
          onpointerup={up}
          onpointercancel={up}
          onkeydown={(e) => key(e, i)}
          onfocus={() => (selected = i)}
        >
          <span class="num">{s.number}</span>
          {#if r.w * view.scale > 96}
            <span class="name">{s.name}</span>
            <span class="meta">{diagonalInches(r.w, r.h).toFixed(1)}″ · {s.scale}%</span>
          {/if}
        </button>
      {/each}

      {#each dragging ? [] : seams as st, k (k)}
        {@const len = st.length * view.scale}
        {@const count = Math.max(1, Math.floor(len / STITCH_SPACING))}
        {@const step = len / count}
        <svg
          class="stitch"
          aria-hidden="true"
          style:left="{st.vertical ? st.x * view.scale - STITCH_HALF : st.x * view.scale}px"
          style:top="{st.vertical ? st.y * view.scale : st.y * view.scale - STITCH_HALF}px"
          width={st.vertical ? STITCH_HALF * 2 : len}
          height={st.vertical ? len : STITCH_HALF * 2}
        >
          {#each { length: count } as _, n (n)}
            {@const c = step * (n + 0.5)}
            {@const delay = `${Math.min(n * 14, 280)}ms`}
            {#if st.vertical}
              <line pathLength="1" style:--delay={delay} x1="1.5" y1={c + 2.5} x2={STITCH_HALF * 2 - 1.5} y2={c - 2.5} />
            {:else}
              <line pathLength="1" style:--delay={delay} x1={c - 2.5} y1={STITCH_HALF * 2 - 1.5} x2={c + 2.5} y2="1.5" />
            {/if}
          {/each}
        </svg>
      {/each}

      {#if checking}
        <div class="line" style:left={px(box.x - 12)} style:top={px(lineY)} style:width={px(box.w + 24)}></div>
      {/if}
    </div>
  {/if}
</div>

<style>
  .canvas {
    position: relative;
    flex: 1;
    min-height: 0;
    border-radius: var(--radius-sm);
    background-color: var(--well);
    background-image: radial-gradient(var(--stroke-strong) 1px, transparent 1px);
    background-size: 18px 18px;
    overflow: hidden;
  }
  .world {
    position: absolute;
    inset: 0 auto auto 0;
  }
  .tile {
    position: absolute;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 2px;
    padding: 4px;
    border-radius: 6px;
    border: 1px solid var(--stroke-strong);
    background: var(--tile);
    box-shadow: var(--shadow);
    cursor: grab;
    overflow: hidden;
    touch-action: none;
    transition: border-color var(--fast), box-shadow var(--fast), background var(--fast);
  }
  .tile:active {
    cursor: grabbing;
  }
  .tile.selected {
    border: 2px solid var(--accent);
    box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent);
    z-index: 1;
  }
  .tile.lost {
    border-style: dashed;
    border-color: var(--line);
  }
  .num {
    font: 600 28px/1 var(--font-display);
  }
  .selected .num {
    color: var(--accent);
  }
  .name {
    max-width: 100%;
    font-weight: 600;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .meta {
    font-size: 11px;
    color: var(--text-2);
  }
  .stitch {
    position: absolute;
    z-index: 2;
    pointer-events: none;
    overflow: visible;
  }
  /* A faint dark edge keeps each stitch crisp on top of the screens' borders. */
  .stitch line {
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linecap: round;
    filter: drop-shadow(0 0 1px rgba(0, 0, 0, 0.55));
    /* Sewn in one stitch after another along the seam. */
    stroke-dasharray: 1;
    stroke-dashoffset: 1;
    animation: sew 160ms cubic-bezier(0.2, 0, 0, 1) var(--delay) forwards;
  }
  @keyframes sew {
    to {
      stroke-dashoffset: 0;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .stitch line {
      animation: none;
      stroke-dashoffset: 0;
    }
  }
  .line {
    position: absolute;
    z-index: 3;
    height: 2px;
    background: var(--line);
    pointer-events: none;
    transform: translateY(-1px);
  }
</style>
