import { useLayoutEffect, useRef, useState } from 'react'
import { countByState } from '../../format'
import type { SessionSnapshot, SessionState } from '../../types'
import { layoutSize } from '../../useWidgetSize'
import { SessionEntry } from './SessionEntry'
import './dotRow.css'
import './notchFlanks.css'

export type FlankSide = 'left' | 'right'

/**
 * Names per chip before the rest collapse into a count.
 *
 * Two, measured rather than estimated: against the real stylesheet an expanded
 * entry group is 72-91pt, so three of them plus the overflow marker wanted
 * 313pt of the 224pt a 240pt chip has to give. Sessions arrive sorted with
 * whatever needs the user first, so the hidden tail is the least urgent by
 * construction.
 */
export const FLANK_MAX_VISIBLE = 2

/** Chip padding and border, mirroring `.flank-chip` in notchFlanks.css. */
export const CHIP_PAD = 10
export const CHIP_BORDER = 0

/**
 * Which side each state sends a session to.
 *
 * The split is by urgency, not by count: the left chip is the one that lights up
 * when something wants you, the right is ambient.
 *
 * This is the split rule only. It is deliberately not what a chip renders —
 * a background job follows its parent, so the urgent chip can hold a busy job,
 * and a chip that counted only the states it nominally carries would count that
 * job nowhere at all.
 */
export const CHIP_STATES: Record<FlankSide, SessionState[]> = {
  left: ['waiting', 'dead'],
  right: ['busy', 'idle', 'paused'],
}

/**
 * Order the collapsed counts appear in, most urgent first.
 *
 * Applied to whatever the chip was actually given rather than to its side's
 * nominal states. Because the left chip lays itself out in reverse, first here
 * means nearest the notch on both sides.
 */
export const STATE_ORDER: SessionState[] = ['waiting', 'dead', 'busy', 'idle', 'paused']

interface Props {
  side: FlankSide
  /** Only the sessions belonging to this side; the parent does the splitting. */
  sessions: SessionSnapshot[]
  expanded: boolean
  hoveredSessionId: string | null
  onHoverSession: (sessionId: string | null, element: HTMLElement | null) => void
  /**
   * Set on the chip itself, not on the flank around it. The flank spans the
   * whole budget; reporting that as the hover target would make empty menu bar
   * beside the chip read as hovering the widget.
   */
  chipRef?: React.Ref<HTMLDivElement>
  /**
   * Drawn in place of nothing when this side has no sessions of its own.
   *
   * The left chip passes the total session count so it is always present and
   * the notch stays framed on both sides. The right chip passes nothing and
   * vanishes when it has nothing to say — only one side needs to hold the
   * shape, and two placeholders either side of a quiet machine is just noise.
   */
  fallbackTotal?: number
  maxVisible?: number
}

/**
 * One chip, flush against its edge of the notch.
 *
 * Collapsed it shows a count per state it holds; expanded it shows names. Both
 * are mounted at once and the chip's own width is animated between the two, so
 * the box morphs rather than jumping — the same approach `DotRow` takes for the
 * pill, minus the window choreography, because the notch window is a fixed size
 * big enough for either state and never resizes.
 *
 * With no sessions it renders `fallbackTotal` if it was given one and nothing
 * at all otherwise.
 */
export function FlankCluster({
  side,
  sessions,
  expanded,
  hoveredSessionId,
  onHoverSession,
  chipRef,
  fallbackTotal,
  maxVisible = FLANK_MAX_VISIBLE,
}: Props) {
  const collapsedSlot = useRef<HTMLDivElement>(null)
  const expandedSlot = useRef<HTMLDivElement>(null)
  const [boxWidth, setBoxWidth] = useState<number | null>(null)

  const empty = sessions.length === 0
  const showNames = expanded && !empty

  // Size the chip to the state being morphed into. Both slots are mounted, so
  // the target is measurable now rather than after the transition has already
  // clipped it.
  useLayoutEffect(() => {
    const slot = (showNames ? expandedSlot : collapsedSlot).current
    if (slot === null) return
    // layoutSize, not getBoundingClientRect: the hidden slot is mid-transition
    // and the rect would report its animating state rather than its layout.
    setBoxWidth(layoutSize(slot).width + CHIP_PAD * 2 + CHIP_BORDER * 2)
  }, [showNames, sessions, fallbackTotal, hoveredSessionId])

  // After the hooks: a chip that renders nothing must not change their order.
  if (empty && fallbackTotal === undefined) return null

  const counts = countByState(sessions)
  const groups = STATE_ORDER.filter((state) => counts[state] > 0)
  const visible = sessions.slice(0, maxVisible)
  const hidden = sessions.length - visible.length

  return (
    <div
      ref={chipRef}
      className="flank-chip"
      data-side={side}
      data-expanded={expanded ? 'true' : 'false'}
      data-testid={`flank-${side}`}
      style={boxWidth === null ? undefined : { width: boxWidth }}
    >
      <div
        className="flank-variant"
        ref={collapsedSlot}
        data-show={showNames ? 'false' : 'true'}
        data-testid={`collapsed-${side}`}
      >
        {empty ? (
          // Deliberately no dot. A state count is a dot and a number, so a bare
          // muted number cannot be misread as one — which matters here, because
          // this chip being present no longer means anything on its own.
          <span className="total" data-testid="total">
            {fallbackTotal}
          </span>
        ) : (
          groups.map((state) => (
            <span key={state} className="count" data-testid={`count-${state}`}>
              <span className={`dot dot-${state}`} />
              {counts[state]}
            </span>
          ))
        )}
      </div>
      <div
        className="flank-variant"
        ref={expandedSlot}
        data-show={showNames ? 'true' : 'false'}
        data-testid={`expanded-${side}`}
      >
        {visible.map((session, index) => (
          <SessionEntry
            key={session.sessionId}
            session={session}
            separated={index > 0}
            hovered={hoveredSessionId === session.sessionId}
            onHover={onHoverSession}
          />
        ))}
        {hidden > 0 && (
          <span className="summary" data-testid={`overflow-${side}`}>
            +{hidden}
          </span>
        )}
      </div>
    </div>
  )
}
