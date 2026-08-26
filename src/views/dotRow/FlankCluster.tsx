import { countByState } from '../../format'
import type { SessionSnapshot, SessionState } from '../../types'
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
  maxVisible?: number
}

/**
 * One chip, flush against its edge of the notch.
 *
 * Collapsed it shows a count per state it carries; expanded it shows names.
 * Either way it grows away from the notch, which is where the free space is:
 * app menu titles fill the left flank from the left edge inward and menu bar
 * extras fill the right flank from the right edge inward.
 *
 * Renders nothing at all when it has no sessions, so a quiet machine reads as
 * deliberately asymmetric rather than as a chip showing zero.
 */
export function FlankCluster({
  side,
  sessions,
  expanded,
  hoveredSessionId,
  onHoverSession,
  chipRef,
  maxVisible = FLANK_MAX_VISIBLE,
}: Props) {
  if (sessions.length === 0) return null

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
    >
      {expanded ? (
        <>
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
        </>
      ) : (
        groups.map((state) => (
          <span key={state} className="count" data-testid={`count-${state}`}>
            <span className={`dot dot-${state}`} />
            {counts[state]}
          </span>
        ))
      )}
    </div>
  )
}
