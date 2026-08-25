import { shortName } from '../../format'
import type { SessionSnapshot } from '../../types'
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
        <span key={session.sessionId} className="entry-group">
          {index > 0 &&
            (session.background ? (
              // A job belongs to the session before it, so it reads as a
              // continuation rather than a peer.
              <span className="child-arrow" aria-hidden="true">
                →
              </span>
            ) : (
              <span className="hairline" />
            ))}
          <span
            className="entry"
            data-testid={`session-${session.sessionId}`}
            data-session-id={session.sessionId}
            data-state={session.state}
            data-background={session.background ? 'true' : 'false'}
            data-hovered={hoveredSessionId === session.sessionId ? 'true' : 'false'}
            onMouseEnter={(e) => {
              onHoverSession(session.sessionId)
              const entry = e.currentTarget.getBoundingClientRect()
              const row = e.currentTarget.closest('.pill')?.getBoundingClientRect()
              onHoverOffset?.(row ? entry.left - row.left : 0)
            }}
            onMouseLeave={() => onHoverSession(null)}
          >
            <span className={`dot dot-${session.state}`} />
            <span className="entry-name">{shortName(session.name)}</span>
          </span>
        </span>
      ))}
      {hidden > 0 && (
        <span className="summary" data-testid="overflow">
          +{hidden} more
        </span>
      )}
    </div>
  )
}
