import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { SessionSnapshot } from '../../types'
import { centredAnchor, POPOVER_WIDTH, useCursor } from '../../useCursor'
import { reportHoverRects, useNotchLayout, visibleRects } from '../../useNotch'
import { CHIP_STATES, FlankCluster } from './FlankCluster'
import { SessionPopover } from './SessionPopover'
import './notchFlanks.css'

/** Gap between the menu bar and the popover, matching `--gap-popover`. */
const POPOVER_GAP = 10

interface Props {
  sessions: SessionSnapshot[]
}

/**
 * Sessions divided between the two chips.
 *
 * Order within each side is preserved from the incoming list, which is already
 * sorted by urgency, so `slice` in the chip drops the least urgent.
 *
 * A background job goes wherever its parent went, not where its own state would
 * send it. `SessionSnapshot` carries no parent field — free mode reads parentage
 * from list order, where a background entry belongs to the nearest own session
 * before it — so splitting a busy job by its own state put it on the opposite
 * chip from the waiting session it belongs to, and the continuation arrow that
 * marks it as a continuation pointed at a stranger, or vanished because the job
 * had become the first entry in its chip.
 */
export function splitByUrgency(sessions: SessionSnapshot[]): {
  left: SessionSnapshot[]
  right: SessionSnapshot[]
} {
  const left: SessionSnapshot[] = []
  const right: SessionSnapshot[] = []
  const sideFor = (session: SessionSnapshot) =>
    CHIP_STATES.left.includes(session.state) ? left : right

  // The side of the most recent own session, which every job after it inherits.
  let parentSide: SessionSnapshot[] | null = null

  for (const session of sessions) {
    if (session.background) {
      // A job before any own session has no parent to follow, so it falls back
      // to its own state rather than being dropped.
      ;(parentSide ?? sideFor(session)).push(session)
      continue
    }
    parentSide = sideFor(session)
    parentSide.push(session)
  }

  return { left, right }
}

/**
 * The widget as two chips in the menu bar, flanking the notch.
 *
 * Hovering either chip expands both. One `cursor.inside` boolean already drives
 * the whole thing, and per-side hover would need a rule for what happens as the
 * cursor crosses the notch between them.
 */
export function NotchFlanks({ sessions }: Props) {
  const layout = useNotchLayout()
  const cursor = useCursor()
  const expanded = cursor.inside

  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null)
  const [anchorX, setAnchorX] = useState(0)

  const leftRef = useRef<HTMLDivElement>(null)
  const rightRef = useRef<HTMLDivElement>(null)
  const popoverRef = useRef<HTMLDivElement>(null)

  // The chips sit flush at y = 0, but `body` carries --shadow-pad on every side
  // so the free-mode pill has somewhere to drop a shadow. Notch mode zeroes it,
  // and does so from here rather than from App so that turning the mode off
  // restores the padding without App having to know why it was gone.
  useEffect(() => {
    document.body.classList.add('notch-mode')
    return () => document.body.classList.remove('notch-mode')
  }, [])

  // Clicks arrive from Rust for the same reason hover does: a non-activating
  // NSPanel never becomes the key window, so the page never sees its own.
  const hoveredRef = useRef<string | null>(null)
  hoveredRef.current = hoveredSessionId

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://click', () => {
      const target = sessions.find((s) => s.sessionId === hoveredRef.current)
      if (target === undefined) return
      void invoke('raise_session', { pid: target.pid }).catch(() => {
        // The popover surfaces failures on its own next render.
      })
    }).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [sessions])

  useEffect(() => {
    if (!cursor.inside) setHoveredSessionId(null)
  }, [cursor.inside])

  // Tell Rust which parts of the window are the widget. Both chips and, when
  // one is open, the popover — three disjoint rects, none of which may be
  // merged into a bounding box that would swallow the notch between them.
  useLayoutEffect(() => {
    reportHoverRects(
      visibleRects([
        leftRef.current?.getBoundingClientRect(),
        rightRef.current?.getBoundingClientRect(),
        popoverRef.current?.getBoundingClientRect(),
      ]),
    )
  }, [expanded, sessions, hoveredSessionId, layout])

  // Every hook runs before this: a display without a notch must not change the
  // order they are called in.
  if (layout === null) return null

  const { left, right } = splitByUrgency(sessions)
  const hovered = sessions.find((s) => s.sessionId === hoveredSessionId) ?? null
  const windowWidth = layout.notchRight + layout.budget

  const onHoverSession = (sessionId: string | null, element: HTMLElement | null) => {
    setHoveredSessionId(sessionId)
    if (element === null) return
    const box = element.getBoundingClientRect()
    setAnchorX(centredAnchor(box.left, box.width, windowWidth))
  }

  return (
    <div className="notch-flanks" data-testid="notch-flanks">
      <div
        className="flank flank-left"
        style={{
          left: layout.notchLeft - layout.budget,
          width: layout.budget,
          height: layout.barHeight,
        }}
      >
        <FlankCluster
          side="left"
          sessions={left}
          expanded={expanded}
          hoveredSessionId={hoveredSessionId}
          onHoverSession={onHoverSession}
          chipRef={leftRef}
        />
      </div>
      <div
        className="flank flank-right"
        style={{ left: layout.notchRight, width: layout.budget, height: layout.barHeight }}
      >
        <FlankCluster
          side="right"
          sessions={right}
          expanded={expanded}
          hoveredSessionId={hoveredSessionId}
          onHoverSession={onHoverSession}
          chipRef={rightRef}
        />
      </div>
      {expanded && hovered !== null && (
        <div
          ref={popoverRef}
          className="notch-popover"
          data-testid="notch-popover"
          style={{ left: anchorX, top: layout.barHeight + POPOVER_GAP, width: POPOVER_WIDTH }}
        >
          <SessionPopover session={hovered} />
        </div>
      )}
    </div>
  )
}
