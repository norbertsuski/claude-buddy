import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'

export interface CursorPosition {
  x: number
  y: number
  inside: boolean
}

export const CURSOR_EVENT = 'ui://cursor'

const OUTSIDE: CursorPosition = { x: -1, y: -1, inside: false }

/**
 * Cursor position relative to the widget, pushed from Rust.
 *
 * The widget is a non-activating NSPanel, so it never becomes the key window
 * and WKWebView never delivers `mousemove` to the page — CSS `:hover` and React
 * `onMouseEnter` simply never fire. Rust samples the cursor and sends
 * window-local coordinates instead.
 */
export function useCursor(): CursorPosition {
  const [cursor, setCursor] = useState<CursorPosition>(OUTSIDE)

  useEffect(() => {
    let stop: (() => void) | undefined
    listen<CursorPosition>(CURSOR_EVENT, (event) => setCursor(event.payload)).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [])

  return cursor
}

/**
 * Which session the cursor is over, given a point in page coordinates.
 *
 * `resolve` is injected so this is testable without a layout engine.
 */
export function sessionAtPoint(
  cursor: CursorPosition,
  resolve: (x: number, y: number) => Element | null,
): string | null {
  if (!cursor.inside) return null
  const element = resolve(cursor.x, cursor.y)
  const entry = element?.closest('[data-session-id]')
  return entry?.getAttribute('data-session-id') ?? null
}

/** What sits under the cursor, when anything does. */
export type HoverTarget = { kind: 'session'; sessionId: string } | { kind: 'usage' } | null

/**
 * What the cursor is over, resolved in one hit-test.
 *
 * One `elementFromPoint` for every kind of target rather than one per kind:
 * each call forces a synchronous layout, and this runs on every cursor sample.
 */
export function targetAtPoint(
  cursor: CursorPosition,
  resolve: (x: number, y: number) => Element | null,
): HoverTarget {
  if (!cursor.inside) return null
  const element = resolve(cursor.x, cursor.y)
  if (!element) return null

  const entry = element.closest('[data-session-id]')
  const sessionId = entry?.getAttribute('data-session-id')
  if (sessionId) return { kind: 'session', sessionId }

  return element.closest('[data-usage]') === null ? null : { kind: 'usage' }
}

/** Popover width, mirrored from `.popover` in dotRow.css. */
export const POPOVER_WIDTH = 335

/**
 * Where to place the popover so it sits centred under its entry.
 *
 * Clamped to the row: the first and last entries would otherwise push the
 * popover past an edge, and it must stay within the window that was sized for
 * it.
 */
export function centredAnchor(
  entryOffset: number,
  entryWidth: number,
  rowWidth: number,
  popoverWidth: number = POPOVER_WIDTH,
): number {
  const centred = entryOffset + entryWidth / 2 - popoverWidth / 2
  const furthest = Math.max(0, rowWidth - popoverWidth)
  return Math.min(Math.max(0, centred), furthest)
}
