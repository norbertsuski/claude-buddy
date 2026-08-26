import { countByState } from '../../format'
import type { SessionSnapshot, SessionState } from '../../types'
import { FlankChip, type FlankSide } from './FlankChip'
import { SessionEntry } from './SessionEntry'
import './dotRow.css'
import './notchFlanks.css'

export type { FlankSide }

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
 * Order the collapsed counts appear in, most urgent first.
 *
 * Because the left chip lays itself out in reverse, first here is nearest the
 * notch — so the count that matters is the one closest to the eye.
 */
export const STATE_ORDER: SessionState[] = ['waiting', 'dead', 'busy', 'idle', 'paused']

interface Props {
  side: FlankSide
  sessions: SessionSnapshot[]
  expanded: boolean
  hoveredSessionId: string | null
  onHoverSession: (sessionId: string | null, element: HTMLElement | null) => void
  chipRef?: React.Ref<HTMLDivElement>
  maxVisible?: number
}

/**
 * The session counts, and the names they expand into.
 *
 * Every state lives on this one chip. An earlier design split them across both
 * flanks by urgency, which forced two awkward rules — a background job had to
 * follow its parent across the split to keep its continuation arrow meaningful,
 * and a chip had to count states its side did not nominally carry. With one
 * chip both problems simply do not arise: order is the order it was given, and
 * adjacency is preserved for free.
 *
 * Renders nothing when it has no sessions.
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
    <FlankChip
      side={side}
      showExpanded={expanded}
      chipRef={chipRef}
      testId={`flank-${side}`}
      collapsed={groups.map((state) => (
        <span key={state} className="count" data-testid={`count-${state}`}>
          <span className={`dot dot-${state}`} />
          {counts[state]}
        </span>
      ))}
      expanded={
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
      }
    />
  )
}
