import { rowLabel } from '../../format'
import type { SessionSnapshot } from '../../types'
import './dotRow.css'

interface Props {
  session: SessionSnapshot
  /** Whether a separator is drawn before this entry. False for the first one. */
  separated: boolean
  hovered: boolean
  /**
   * The element is handed back alongside the id because each mode anchors its
   * popover differently, and only the caller knows what it is measuring
   * against. Keeping the measurement out of here is what lets the free-mode row
   * and a notch chip share one entry.
   */
  onHover: (sessionId: string | null, element: HTMLElement | null) => void
  /** Whether this session just died and its dot should crumble. */
  ashing?: boolean
}

/**
 * One session in an expanded row: a state dot and the session's own title,
 * falling back to its folder-derived name.
 *
 * Extracted so the free-mode row and the notch chips render entries
 * identically. Two copies drifted the moment one of them gained a data
 * attribute the popover hit-testing depended on.
 */
export function SessionEntry({
  session,
  separated,
  hovered,
  onHover,
  ashing = false,
}: Props) {
  return (
    <span className="entry-group">
      {separated &&
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
        data-hovered={hovered ? 'true' : 'false'}
        onMouseEnter={(e) => onHover(session.sessionId, e.currentTarget)}
        onMouseLeave={() => onHover(null, null)}
      >
        <span
          className={`dot dot-${session.state}`}
          data-ash={ashing ? 'true' : undefined}
        />
        <span className="entry-name">{rowLabel(session)}</span>
      </span>
    </span>
  )
}
