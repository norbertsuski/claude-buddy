import { invoke } from '@tauri-apps/api/core'

/**
 * Longest the pill's box morph takes, and so the longest the window must wait
 * before shrinking.
 *
 * The window must stay at least as large as its content for the whole
 * animation, or the pill is clipped as it contracts. Growing can happen
 * immediately — the extra area is transparent and invisible.
 *
 * Mirrors the `--morph` fallback in dotRow.css; the real duration is written
 * onto the pill per change by `morphDuration`.
 */
export const MORPH_MS = 300

/**
 * Floor for the morph.
 *
 * A change of a few pixels still has to read as a move rather than a jump, and
 * below roughly this the eye sees a jump anyway.
 */
export const MORPH_MIN_MS = 120

/**
 * Distance `MORPH_MS` is tuned for: the collapsed↔expanded morph, which is the
 * widest the box ever travels.
 */
export const MORPH_FULL_PX = 400

/**
 * How long the pill should take to move from one box to another.
 *
 * Constant velocity rather than constant duration. `MORPH_MS` was chosen for
 * the ~400px collapsed↔expanded morph; spending the same 300ms on the ~130px a
 * status change moves the box is what makes a status change look like a crawl
 * while its content — which is swapped in a single frame — waits for it.
 */
export function morphDuration(
  from: { width: number; height: number } | null,
  to: { width: number; height: number },
): number {
  if (from === null) return MORPH_MS
  const distance = Math.max(Math.abs(to.width - from.width), Math.abs(to.height - from.height))
  const scaled = Math.round((distance / MORPH_FULL_PX) * MORPH_MS)
  return Math.min(MORPH_MS, Math.max(MORPH_MIN_MS, scaled))
}

/** Whether two sizes are the same, either being absent counting as unknown. */
export function sameSize(
  a: { width: number; height: number } | null,
  b: { width: number; height: number } | null,
): boolean {
  return a !== null && b !== null && a.width === b.width && a.height === b.height
}

const FALLBACK_PAD = 24

/** Padding the page reserves for the shadow, read from the CSS. */
export function shadowPad(): number {
  const declared = parseFloat(getComputedStyle(document.body).paddingLeft)
  return Number.isFinite(declared) && declared > 0 ? declared : FALLBACK_PAD
}

/**
 * Resize the widget window.
 *
 * One command, not two: `resize_widget` repositions as part of the same call,
 * because a separate clamp meant a second window move that could land while the
 * pill was still animating.
 */
export function applyWidgetSize(width: number, height: number): Promise<void> {
  return invoke<void>('resize_widget', { width, height }).catch(() => {
    // Sizing is cosmetic; a failure must not break rendering.
  })
}

/**
 * Resolve once the window server has had two frames to settle after a resize.
 *
 * The resize is a round trip to Rust, so it lands well after the call returns.
 * Starting the pill's transition before that put the native resize in the
 * middle of the animation, which dropped frames.
 */
export function afterResizeSettles(): Promise<void> {
  return new Promise((resolve) => {
    requestAnimationFrame(() => requestAnimationFrame(() => resolve()))
  })
}

/**
 * Layout size of an element, ignoring any transform applied to it.
 *
 * `getBoundingClientRect` reports the *transformed* box. The variant slots
 * animate `transform: translateX(-50%) translateY(-4px) scale(0.97)` when
 * hidden, and the sizing effect runs the instant `data-show` flips — while that
 * transition is still at its start. Measuring the rect there returned 0.97 of
 * the true width, so the pill was sized ~3% too narrow: on a five-session row
 * that is 21px, and because the slot is centred with translateX(-50%) the
 * content overflowed both ends equally and `.pill { overflow: hidden }` trimmed
 * the first entry's pulse ring and the last entry's name mid-glyph.
 *
 * `offsetWidth`/`offsetHeight` are pre-transform, which is what the box needs
 * to be sized to.
 */
export function layoutSize(el: HTMLElement): { width: number; height: number } {
  return { width: el.offsetWidth, height: el.offsetHeight }
}

/** Border width of the pill. */
export const PILL_BORDER = 1

/** Outer window size for a given pill and optional popover. */
/**
 * Height reserved for the popover whether or not one is open.
 *
 * Opening a popover must not resize the window: a resize on a transparent panel
 * shows one unpainted frame, and hovering options would flicker on every one.
 * The reserved area is transparent and click-through, so it costs nothing.
 */
export const POPOVER_ALLOWANCE = 400

/**
 * Width of the row: wide enough for the pill and for a popover, always, so that
 * opening one changes nothing about the window.
 */
export function rowWidthFor(pill: { width: number }, popoverWidth: number): number {
  return Math.max(pill.width + PILL_BORDER * 2, popoverWidth)
}

/**
 * Outer window size, held constant across every hover state.
 *
 * `popoverAllowance` is reserved unconditionally rather than measured from an
 * open popover, so hovering an option never resizes the window.
 */
export function widgetWindowSize(
  pill: { width: number; height: number },
  popoverWidth: number,
  popoverAllowance: number,
  gap: number,
  pad: number,
): { width: number; height: number } {
  const border = PILL_BORDER * 2
  const rowWidth = rowWidthFor(pill, popoverWidth)
  const rowHeight = pill.height + border + gap + popoverAllowance
  return {
    width: Math.ceil(rowWidth + pad * 2),
    height: Math.ceil(rowHeight + pad * 2),
  }
}


/**
 * The part of the window that counts as the widget, in window-local pixels.
 *
 * The window is sized to the widest state so hovering never resizes it, which
 * means most of it is empty transparent margin. Rust needs to know which part
 * is actually the widget — both to decide hover, and to leave the margin
 * click-through so it does not swallow clicks meant for the app behind.
 */
export function reportHoverRect(rect: {
  x: number
  y: number
  width: number
  height: number
}): void {
  void invoke('set_hover_rect', rect).catch(() => {
    // Falls back to the whole window, which is only over-eager, not broken.
  })
}

/** Smallest rect containing both inputs; `b` may be absent. */
export function unionRect(
  a: { left: number; top: number; right: number; bottom: number },
  b: { left: number; top: number; right: number; bottom: number } | null,
): { x: number; y: number; width: number; height: number } {
  const left = Math.min(a.left, b?.left ?? a.left)
  const top = Math.min(a.top, b?.top ?? a.top)
  const right = Math.max(a.right, b?.right ?? a.right)
  const bottom = Math.max(a.bottom, b?.bottom ?? a.bottom)
  return { x: left, y: top, width: right - left, height: bottom - top }
}
