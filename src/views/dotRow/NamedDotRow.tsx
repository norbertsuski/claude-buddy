import type { SessionSnapshot, Usage } from '../../types'
import { SessionEntry } from './SessionEntry'
import { UsageMeter } from './UsageMeter'
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
  /**
   * Five-hour limit usage, drawn here as well as on the collapsed row so that
   * hovering the widget does not make it vanish — the popover only opens over a
   * name, so between two of them there would be nowhere left showing it.
   */
  usage?: Usage | null
  /** Sessions crumbling right now, so only the one that died animates. */
  ashing?: readonly string[]
}

export function NamedDotRow({
  sessions,
  hoveredSessionId,
  onHoverSession,
  onHoverOffset,
  usage = null,
  ashing = [],
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
          ashing={ashing.includes(session.sessionId)}
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
      {usage !== null && <UsageMeter usage={usage} />}
    </div>
  )
}
