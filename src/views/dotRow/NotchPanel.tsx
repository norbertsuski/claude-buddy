import { useLayoutEffect, useRef, useState } from 'react'
import { shortName, formatElapsed } from '../../format'
import type { SessionSnapshot, Usage } from '../../types'
import './dotRow.css'
import './notchFlanks.css'

/**
 * Rows before the rest collapse into a count.
 *
 * Generous compared with the old flank chips, because a row costs height rather
 * than width here and there is 400pt of reserved space below the bar. The list
 * is what the notch is for.
 */
export const MAX_ROWS = 8

/** What the detail card is anchored to, and where it sits vertically. */
export interface WingTarget {
  kind: 'session' | 'usage'
  sessionId?: string
  /** Offset of the row within the panel, for aligning the card to it. */
  top: number
}

interface Props {
  sessions: SessionSnapshot[]
  usage: Usage | null
  open: boolean
  /** The notch's width: the panel is exactly as wide, and never wider. */
  width: number
  top: number
  wing: WingTarget | null
  /**
   * The height the panel wants, reported as it is measured.
   *
   * The caller needs this to describe the panel to Rust. Its own box cannot be
   * used: the height is animated, so at the moment the panel opens the box is
   * still 0 tall and would be discarded as un-laid-out — leaving the cursor
   * outside every reported rect and shutting the panel it just opened.
   */
  onMeasure?: (height: number) => void
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
export function wingAtPoint(
  x: number,
  y: number,
  resolve: (x: number, y: number) => Element | null,
): WingTarget | null {
  const row = resolve(x, y)?.closest('[data-notch-row]')
  if (!(row instanceof HTMLElement)) return null
  const sessionId = row.getAttribute('data-session-id')
  return sessionId === null
    ? { kind: 'usage', top: row.offsetTop }
    : { kind: 'session', sessionId, top: row.offsetTop }
}

/**
 * The list that drops out of the notch.
 *
 * Exactly the notch's width, which is the whole point: nothing has to flare, so
 * there are no concave corners where a wide panel meets a narrow slot, and the
 * shape reads as the notch extruding downward. The cost is about 159pt of usable
 * row width, so a row is a dot, a truncated name and an elapsed time — the rest
 * of a session's detail goes in the card that wings out beside it.
 *
 * Height is measured and written inline rather than transitioning to `auto`,
 * which does not animate.
 */
export function NotchPanel({
  sessions,
  usage,
  open,
  width,
  top,
  wing,
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
      className="notch-panel"
      data-open={open ? 'true' : 'false'}
      data-testid="notch-panel"
      style={{ left: 0, width, top, height }}
    >
      <div ref={content} className="notch-panel-content">
        {visible.map((session) => (
          <div
            key={session.sessionId}
            className="notch-row"
            data-notch-row="session"
            data-testid={`row-${session.sessionId}`}
            data-session-id={session.sessionId}
            data-hovered={wing?.sessionId === session.sessionId ? 'true' : 'false'}
          >
            <span className={`dot dot-${session.state}`} />
            <span className="notch-name">{shortName(session.name)}</span>
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
            data-hovered={wing?.kind === 'usage' ? 'true' : 'false'}
          >
            <span className="notch-name">{100 - usage.percent}% left</span>
            <span className="notch-elapsed">{resetsIn(usage)}</span>
          </div>
        )}
      </div>
    </div>
  )
}

/** Remaining time on the five-hour window, floored to whole minutes. */
function resetsIn(usage: Usage): string {
  return formatElapsed(Math.max(0, usage.resetsAtMs - Date.now()))
}
