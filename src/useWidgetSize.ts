import { invoke } from '@tauri-apps/api/core'

/**
 * How long to wait before shrinking the window.
 *
 * The window must stay at least as large as its content for the whole
 * animation, or the pill is clipped as it contracts. Growing can happen
 * immediately — the extra area is transparent and invisible.
 *
 * Keep in step with the `--morph` duration in dotRow.css.
 */
export const SHRINK_DELAY_MS = 300

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
