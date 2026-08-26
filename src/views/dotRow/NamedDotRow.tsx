import type { SessionSnapshot } from '../../types'
import { SessionEntry } from './SessionEntry'
import './dotRow.css'

/**
 * Expanded content. Like CollapsedPill this renders the pill's contents only;
 * the pill element itself lives in DotRow so it can animate between states.
 *
 * Beyond MAX_VISIBLE the row is wider than any sane corner of the screen, so the
 * remainder collapses into a count. Sessions are already sorted with whatever
 * needs the user first, so the hidden tail is the least urgent by construction.
 */
export const MAX_VISIBLE = 8

interface Props {
  sessions: SessionSnapshot[]
  hoveredSessionId: string | null
  onHoverSession: (sessionId: string | null) => void
  onHoverOffset?: (offsetPx: number) => void
}

export function NamedDotRow({
  sessions,
  hoveredSessionId,
  onHoverSession,
  onHoverOffset,
}: Props) {
  const visible = sessions.slice(0, MAX_VISIBLE)
  const hidden = sessions.length - visible.length

  return (
    <div className="variant variant-expanded" data-testid="named-dot-row">
      {visible.map((session, index) => (
        <SessionEntry
          key={session.sessionId}
          session={session}
          separated={index > 0}
          hovered={hoveredSessionId === session.sessionId}
          onHover={(sessionId, element) => {
            onHoverSession(sessionId)
            // Only an enter carries an element, and only an enter has an offset
            // worth reporting.
            if (element === null) return
            const entry = element.getBoundingClientRect()
            const row = element.closest('.pill')?.getBoundingClientRect()
            onHoverOffset?.(row ? entry.left - row.left : 0)
          }}
        />
      ))}
      {hidden > 0 && (
        <span className="summary" data-testid="overflow">
          +{hidden} more
        </span>
      )}
    </div>
  )
}
