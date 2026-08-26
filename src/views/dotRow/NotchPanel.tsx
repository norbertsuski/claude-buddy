import { useLayoutEffect, useRef, useState } from 'react'
import { shortName, formatElapsed } from '../../format'
import type { SessionSnapshot, SessionState, Usage } from '../../types'
import './dotRow.css'
import './notchFlanks.css'

/**
 * Rows before the rest collapse into a count.
 *
 * A row costs height rather than width here, and there is 400pt reserved below
 * the bar, so this is generous. The list is what the notch is for.
 */
export const MAX_ROWS = 8

/** Which row the cursor is on. */
export interface RowTarget {
  kind: 'session' | 'usage'
  sessionId?: string
}

/**
 * What a row says about a session, beside its name.
 *
 * The slab is wide enough to spell this out, which is why there is no detail
 * card any more: what a popover used to carry, the row carries.
 */
export function stateLabel(session: SessionSnapshot): string {
  const labels: Record<SessionState, string> = {
    waiting: 'needs you',
    busy: 'working',
    idle: 'idle',
    paused: 'paused',
    dead: 'died',
  }
  return session.state === 'waiting' && session.detail !== null
    ? session.detail
    : labels[session.state]
}

/**
 * Which row the cursor is over, given a point in page coordinates.
 *
 * Hit-tested rather than driven by `onMouseEnter`: the widget is a
 * non-activating NSPanel, so it never becomes the key window and WKWebView
 * delivers no mouse events to the page at all. Rust samples the cursor and the
 * page hit-tests for itself — the same reason `sessionAtPoint` exists for the
 * free-mode row.
 *
 * `resolve` is injected so this is testable without a layout engine.
 */
export function rowAtPoint(
  x: number,
  y: number,
  resolve: (x: number, y: number) => Element | null,
): RowTarget | null {
  const row = resolve(x, y)?.closest('[data-notch-row]')
  if (!(row instanceof HTMLElement)) return null
  const sessionId = row.getAttribute('data-session-id')
  return sessionId === null ? { kind: 'usage' } : { kind: 'session', sessionId }
}

interface Props {
  sessions: SessionSnapshot[]
  usage: Usage | null
  open: boolean
  /** The slab's width: the band in the bar and this list are the same. */
  width: number
  /** Menu bar height, reserved at the top so rows start below the bar. */
  barHeight: number
  row: RowTarget | null
  /**
   * The slab's full height, reported as it is measured.
   *
   * The caller needs this to describe the slab to Rust. Its own box cannot be
   * used: the height is animated, so at the moment it opens the box is still 0
   * tall and would be discarded as un-laid-out — leaving the cursor outside
   * every reported rect and shutting the slab it just opened.
   */
  onMeasure?: (height: number) => void
}

/**
 * The slab: black across the menu bar and down into a list, one shape.
 *
 * The band and the list are the same width, so there is no join to treat — no
 * flare, no concave fillets where a wide panel would meet a narrow notch. The
 * notch sits inside it and disappears. Square at the top, where the screen ends,
 * and rounded at the bottom.
 *
 * Height is measured and written inline rather than transitioning to `auto`,
 * which does not animate.
 */
export function NotchPanel({
  sessions,
  usage,
  open,
  width,
  barHeight,
  row,
  onMeasure,
}: Props) {
  const content = useRef<HTMLDivElement>(null)
  const [height, setHeight] = useState(0)

  useLayoutEffect(() => {
    if (content.current === null) return
    const measured = content.current.offsetHeight
    setHeight(open ? measured : 0)
    onMeasure?.(measured)
  }, [open, sessions, usage, onMeasure])

  const visible = sessions.slice(0, MAX_ROWS)
  const hidden = sessions.length - visible.length

  return (
    <div
      className="notch-slab"
      data-open={open ? 'true' : 'false'}
      data-testid="notch-panel"
      style={{ width, height }}
    >
      <div ref={content} className="notch-slab-content" style={{ paddingTop: barHeight }}>
        {visible.map((session) => (
          <div
            key={session.sessionId}
            className="notch-row"
            data-notch-row="session"
            data-testid={`row-${session.sessionId}`}
            data-session-id={session.sessionId}
            data-hovered={row?.sessionId === session.sessionId ? 'true' : 'false'}
          >
            <span className={`dot dot-${session.state}`} />
            <span className="notch-name">{shortName(session.name)}</span>
            <span className="notch-status">{stateLabel(session)}</span>
            <span className="notch-elapsed">{formatElapsed(session.elapsedMs)}</span>
          </div>
        ))}
        {hidden > 0 && (
          <div className="notch-row notch-more" data-testid="notch-more">
            +{hidden} more
          </div>
        )}
        {usage !== null && (
          <div
            className="notch-row notch-foot"
            data-notch-row="usage"
            data-testid="notch-usage-row"
            data-hovered={row?.kind === 'usage' ? 'true' : 'false'}
          >
            <span className="notch-name">{100 - usage.percent}% of the 5h limit left</span>
            <span className="notch-elapsed">{formatElapsed(usage.resetsAtMs - Date.now())}</span>
          </div>
        )}
      </div>
    </div>
  )
}
