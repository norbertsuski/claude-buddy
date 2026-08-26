import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { SessionSnapshot, Usage } from '../../types'
import { centredAnchor, POPOVER_WIDTH, useCursor } from '../../useCursor'
import { reportHoverRects, useNotchLayout, visibleRects } from '../../useNotch'
import { FlankChip } from './FlankChip'
import { FlankCluster } from './FlankCluster'
import { SessionPopover } from './SessionPopover'
import { UsageMeter } from './UsageMeter'
import './notchFlanks.css'

/** Gap between the menu bar and the popover, matching `--gap-popover`. */
const POPOVER_GAP = 10

interface Props {
  sessions: SessionSnapshot[]
  /** The five-hour limit, or null when there is nothing trustworthy to show. */
  usage: Usage | null
}

/**
 * The widget as two boxes either side of the notch, reading as one shape with it.
 *
 * Counts on the left, the five-hour limit on the right. The two carry unrelated
 * things, so neither has to make room for the other, and the side each lives on
 * never changes — which is what makes the pair glanceable rather than something
 * to read.
 *
 * Hovering either box expands both. One `cursor.inside` boolean already drives
 * the whole thing, and per-side hover would need a rule for what happens as the
 * cursor crosses the notch between them.
 */
export function NotchFlanks({ sessions, usage }: Props) {
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

  // Tell Rust which parts of the window are the widget: the two chips and, when
  // one is open, the popover. Three disjoint rects, never merged into a bounding
  // box — that would bridge the notch and hold the row expanded whenever the
  // cursor crossed it.
  useLayoutEffect(() => {
    reportHoverRects(
      visibleRects([
        leftRef.current?.getBoundingClientRect(),
        rightRef.current?.getBoundingClientRect(),
        popoverRef.current?.getBoundingClientRect(),
      ]),
    )
  }, [expanded, sessions, usage, hoveredSessionId, layout])

  // Every hook runs before this: a display without a notch must not change the
  // order they are called in.
  if (layout === null) return null

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
        className="notch-bridge"
        data-testid="notch-bridge"
        aria-hidden="true"
        style={{
          left: layout.notchLeft,
          width: layout.notchRight - layout.notchLeft,
          height: layout.barHeight,
        }}
      />
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
          sessions={sessions}
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
        {usage !== null && (
          <FlankChip
            side="right"
            showExpanded={expanded}
            chipRef={rightRef}
            testId="flank-usage"
            // A share left is quicker to glance at; the countdown is the more
            // useful of the two once the row is being looked at deliberately.
            // UsageMeter documents the same split for the free-mode row.
            collapsed={<UsageMeter usage={usage} show="percent" />}
            expanded={<UsageMeter usage={usage} show="countdown" />}
          />
        )}
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
