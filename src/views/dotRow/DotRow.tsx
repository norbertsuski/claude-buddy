import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CollapsedPill } from './CollapsedPill'
import { NamedDotRow } from './NamedDotRow'
import { SessionPopover } from './SessionPopover'
import {
  afterResizeSettles,
  applyWidgetSize,
  layoutSize,
  POPOVER_ALLOWANCE,
  reportHoverRect,
  rowWidthFor,
  shadowPad,
  SHRINK_DELAY_MS,
  unionRect,
  widgetWindowSize,
} from '../../useWidgetSize'
import { centredAnchor, POPOVER_WIDTH, sessionAtPoint, useCursor } from '../../useCursor'
import type { SessionViewProps } from '../SessionView'
import './dotRow.css'

/**
 * Delay before the popover opens. Without it, sweeping the cursor across the
 * row flashes a popover per name.
 */
export const HOVER_GRACE_MS = 180

/** Gap between the pill and the popover, matching `--gap-popover`. */
const POPOVER_GAP = 10

export function DotRow({ sessions }: SessionViewProps) {
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null)
  const [anchorOffset, setAnchorOffset] = useState(0)
  const [flashing, setFlashing] = useState(false)
  const [pillBox, setPillBox] = useState<{ width: number; height: number } | null>(null)
  // The row is held at the widest state so the pill can grow outwards from its
  // centre instead of unrolling from the left edge.
  const [rowWidth, setRowWidth] = useState<number | null>(null)

  const root = useRef<HTMLDivElement>(null)
  const collapsedSlot = useRef<HTMLDivElement>(null)
  const expandedSlot = useRef<HTMLDivElement>(null)
  const popoverSlot = useRef<HTMLDivElement>(null)
  const pillRef = useRef<HTMLDivElement>(null)
  const appliedWindow = useRef<{ width: number; height: number } | null>(null)
  const shrinkTimer = useRef<ReturnType<typeof setTimeout> | null>(null)

  // Hover comes from Rust, not from the DOM: a non-activating NSPanel never
  // becomes the key window, so WKWebView never delivers mousemove to the page.
  const cursor = useCursor()
  const expanded = cursor.inside

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://flash', () => setFlashing(true)).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [])

  useEffect(() => {
    if (cursor.inside) setFlashing(false)
  }, [cursor.inside])

  const showNamed = expanded && sessions.length > 0

  // Hit-testing forces a synchronous layout, so it only runs when there is
  // actually a row to hit — not on every cursor sample.
  const pending = showNamed
    ? sessionAtPoint(cursor, (x, y) =>
        typeof document.elementFromPoint === 'function' ? document.elementFromPoint(x, y) : null,
      )
    : null

  useEffect(() => {
    // Leaving the widget is the only thing that closes the popover. Sweeping
    // between two names crosses the gap between them, where nothing is hit —
    // dropping the selection there made the popover blink out for the length of
    // the grace delay on every pass.
    if (!cursor.inside) {
      setHoveredSessionId(null)
      return
    }
    if (pending === null || pending === hoveredSessionId) return

    const timer = setTimeout(() => setHoveredSessionId(pending), HOVER_GRACE_MS)
    return () => clearTimeout(timer)
  }, [pending, cursor.inside, hoveredSessionId])

  const hovered = sessions.find((s) => s.sessionId === hoveredSessionId) ?? null

  // Clicks arrive from Rust for the same reason hover does.
  const hoveredRef = useRef(hovered)
  hoveredRef.current = hovered

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://click', () => {
      const target = hoveredRef.current
      if (target === null) return
      void invoke('raise_session', { pid: target.pid }).catch(() => {
        // The popover surfaces failures on its own next render.
      })
    }).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [])

  useLayoutEffect(() => {
    if (hoveredSessionId === null || rowWidth === null) return
    const entry = root.current?.querySelector<HTMLElement>(
      `[data-session-id="${hoveredSessionId}"]`,
    )
    const slot = expandedSlot.current
    if (!entry || !slot) return

    // Deliberately offsetLeft/offsetWidth rather than getBoundingClientRect.
    // The slot is centred with translateX(-50%) inside a pill whose width is
    // animating, so on-screen positions move throughout the morph; measuring
    // them there anchored the popover to a position the entry was only passing
    // through. Offsets are relative to the slot and do not move.
    const slotWidth = slot.offsetWidth
    const entryLeftInRow = (rowWidth - slotWidth) / 2 + entry.offsetLeft
    setAnchorOffset(centredAnchor(entryLeftInRow, entry.offsetWidth, rowWidth))
  }, [hoveredSessionId, sessions, rowWidth])

  // Size the pill to the state being morphed into, and the window to hold it.
  // Both variants are mounted, so the target is measurable now rather than
  // after the animation has already clipped.
  useLayoutEffect(() => {
    const slot = (showNamed ? expandedSlot : collapsedSlot).current
    if (!slot) return

    // layoutSize, not getBoundingClientRect: the hidden slot is mid-transition
    // out of scale(0.97) when this runs, and the rect reports that scale.
    const target = layoutSize(slot)

    // Size the window to whichever state is larger, not to the current one, so
    // hovering resizes nothing. Resizing a transparent panel shows one
    // unpainted frame, and it was landing exactly on the start of the morph.
    const collapsedBox = collapsedSlot.current && layoutSize(collapsedSlot.current)
    const expandedBox = expandedSlot.current && layoutSize(expandedSlot.current)
    const widest = {
      width: Math.max(collapsedBox?.width ?? 0, expandedBox?.width ?? 0),
      height: Math.max(collapsedBox?.height ?? 0, expandedBox?.height ?? 0),
    }

    const next = widgetWindowSize(
      widest,
      POPOVER_WIDTH,
      POPOVER_ALLOWANCE,
      POPOVER_GAP,
      shadowPad(),
    )
    const nextRow = rowWidthFor(widest, POPOVER_WIDTH)

    const applied = appliedWindow.current
    const grows = applied === null || next.width > applied.width || next.height > applied.height

    if (shrinkTimer.current !== null) {
      clearTimeout(shrinkTimer.current)
      shrinkTimer.current = null
    }

    if (grows) {
      // Grow the window first and wait for it to land, then let the surface
      // settle for two frames before the pill starts moving. The resize is a
      // round trip to Rust, so without awaiting it the native resize arrived
      // mid-transition and dropped frames.
      appliedWindow.current = next
      setRowWidth(nextRow)
      let cancelled = false
      void applyWidgetSize(next.width, next.height)
        .then(afterResizeSettles)
        .then(() => {
          if (!cancelled) setPillBox(target)
        })
      return () => {
        cancelled = true
      }
    }

    // Contracting animates first and the window follows, so the pill can start
    // moving immediately.
    setPillBox(target)

    // Shrinking has to wait for the morph, or the pill is clipped mid-contract
    // and the row narrows under the still-contracting pill.
    shrinkTimer.current = setTimeout(() => {
      appliedWindow.current = next
      setRowWidth(nextRow)
      applyWidgetSize(next.width, next.height)
      shrinkTimer.current = null
    }, SHRINK_DELAY_MS)
  }, [showNamed, sessions])

  // Tell Rust which part of the window is the widget. Recomputed whenever the
  // pill or popover changes size or position.
  useEffect(() => {
    const pill = pillRef.current?.getBoundingClientRect()
    if (!pill) return
    const popover = popoverSlot.current?.getBoundingClientRect() ?? null
    reportHoverRect(unionRect(pill, popover))
  }, [showNamed, hovered, pillBox, rowWidth])

  useEffect(
    () => () => {
      if (shrinkTimer.current !== null) clearTimeout(shrinkTimer.current)
    },
    [],
  )

  return (
    <div
      ref={root}
      className="dot-row"
      data-testid="dot-row"
      data-flashing={flashing ? 'true' : 'false'}
      style={rowWidth === null ? undefined : { width: rowWidth }}
    >
      <div
        ref={pillRef}
        className="pill"
        style={pillBox === null ? undefined : { width: pillBox.width, height: pillBox.height }}
      >
        <div className="variant-slot" ref={collapsedSlot} data-show={showNamed ? 'false' : 'true'}>
          <CollapsedPill sessions={sessions} />
        </div>
        <div className="variant-slot" ref={expandedSlot} data-show={showNamed ? 'true' : 'false'}>
          <NamedDotRow
            sessions={sessions}
            hoveredSessionId={hoveredSessionId}
            onHoverSession={setHoveredSessionId}
          />
        </div>
      </div>
      {showNamed && hovered !== null && (
        <div
          ref={popoverSlot}
          className="popover-anchor"
          data-testid="popover-anchor"
          style={{ marginLeft: anchorOffset }}
        >
          <SessionPopover session={hovered} />
        </div>
      )}
    </div>
  )
}
