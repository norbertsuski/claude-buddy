import { useEffect, useLayoutEffect, useRef, useState, type ReactNode } from 'react'
import { rowLabel, formatElapsed } from '../../format'
import type { SessionSnapshot, SessionState, Usage } from '../../types'
import { DETAIL_MORPH_MS, RowDetailSlot } from './RowDetail'
import './dotRow.css'
import './notchFlanks.css'

/**
 * Rows before the rest collapse into a count.
 *
 * A row costs height rather than width here, and `notch::POPOVER_ALLOWANCE` is
 * reserved below the bar for the list and one open detail, so this is
 * generous. The list is what the notch is for.
 */
export const MAX_ROWS = 8

/** Which row the cursor is on. */
export interface RowTarget {
  kind: 'session' | 'usage'
  sessionId?: string
}

/** What a row says about a session, beside its name. */
export function stateLabel(session: SessionSnapshot): string {
  const labels: Record<SessionState, string> = {
    waiting: 'needs you',
    busy: 'working',
    tasking: 'running a task',
    idle: 'idle',
    paused: 'paused',
    dead: 'died',
  }
  // `detail` says what a session is waiting on — the reason, for a waiting
  // session, and the task's own name for a tasking one. Either beats the
  // static word for the state.
  return (session.state === 'waiting' || session.state === 'tasking') &&
    session.detail !== null
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
  barHeight: number
  /** The notch's width, for placing the right-hand resting half beyond it. */
  notchWidth: number
  row: RowTarget | null
  /** The resting content: counts on the left of the notch, the limit on the right. */
  restLeft: ReactNode
  restRight: ReactNode
  /**
   * Where the notch's left edge falls inside the band.
   *
   * Anchors the resting halves against the notch rather than the band's own
   * edges, so they stay where the eye already is. The left half is placed with
   * `calc(100% - x)` rather than a computed `left`, deliberately: the band's
   * resting width is measured *from* these halves, and positioning them from it
   * as well made the two circular — the band collapsed to the notch's width and
   * clipped the halves it was supposed to be measuring.
   */
  notchLeftInBand: number
  restLeftRef?: React.Ref<HTMLDivElement>
  restRightRef?: React.Ref<HTMLDivElement>
  /**
   * The list's height as measured, so the caller can size the band.
   *
   * Watched with a ResizeObserver rather than recomputed from a dependency
   * list: a row's detail arrives asynchronously and changes the height after
   * every dependency has already settled.
   */
  onMeasure?: (height: number) => void
}

/**
 * The slab: the resting band in the menu bar and the list it grows into.
 *
 * One element for both, so the black grows out of what is already on screen
 * rather than unrolling down from the top of the display — which is what a
 * separate panel pinned to `top: 0` looked like. The resting halves fade out as
 * the list fades in.
 */
export function NotchPanel({
  sessions,
  usage,
  open,
  barHeight,
  notchWidth,
  row,
  restLeft,
  restRight,
  notchLeftInBand,
  restLeftRef,
  restRightRef,
  onMeasure,
}: Props) {
  const list = useRef<HTMLDivElement>(null)
  const [listHeight, setListHeight] = useState(0)

  useLayoutEffect(() => {
    setListHeight(list.current?.offsetHeight ?? 0)
  }, [sessions, usage, row])

  useEffect(() => {
    const el = list.current
    if (el === null || typeof ResizeObserver !== 'function') return
    const observer = new ResizeObserver(() => setListHeight(el.offsetHeight))
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  useEffect(() => {
    onMeasure?.(listHeight)
  }, [listHeight, onMeasure])

  // Own sessions are the rows; the background work under them is the detail's
  // `tasks` field, not a peer row. Four agents rendered as four more rows
  // buried the three sessions they belonged to.
  const own = sessions.filter((session) => !session.background)
  const visible = own.slice(0, MAX_ROWS)
  const hidden = own.length - visible.length
  const hovered = row?.kind === 'session' ? row.sessionId ?? null : null

  // The row being left stays mounted until its detail has finished collapsing.
  // Unmounting it on the same frame the next one opens made the list jump: the
  // rows below snapped up by the old detail's height and then eased back down
  // by the new one's, from a hover that had only moved by a row.
  const [leaving, setLeaving] = useState<string | null>(null)
  const previous = useRef<string | null>(null)

  useEffect(() => {
    const before = previous.current
    previous.current = hovered
    if (before === null || before === hovered) return
    setLeaving(before)
    const timer = setTimeout(() => setLeaving(null), DETAIL_MORPH_MS)
    return () => clearTimeout(timer)
  }, [hovered])

  return (
    <>
      <div className="slab-rest" data-show={open ? 'false' : 'true'} style={{ height: barHeight }}>
        <div
          className="slab-rest-half"
          ref={restLeftRef}
          data-testid="rest-left"
          style={{ right: `calc(100% - ${notchLeftInBand}px)`, height: barHeight }}
        >
          {restLeft}
        </div>
        <div
          className="slab-rest-half"
          ref={restRightRef}
          data-testid="rest-right"
          style={{ left: notchLeftInBand + notchWidth, height: barHeight }}
        >
          {restRight}
        </div>
      </div>

      <div
        ref={list}
        className="slab-list"
        data-show={open ? 'true' : 'false'}
        data-testid="notch-panel"
        style={{ paddingTop: barHeight }}
      >
        {visible.map((session) => (
          <div key={session.sessionId}>
            <div
              className="notch-row"
              data-notch-row="session"
              data-testid={`row-${session.sessionId}`}
              data-session-id={session.sessionId}
              data-hovered={row?.sessionId === session.sessionId ? 'true' : 'false'}
            >
              <span className={`dot dot-${session.state}`} />
              <span className="notch-name">{rowLabel(session)}</span>
              <span className="notch-status">{stateLabel(session)}</span>
              <span className="notch-elapsed">{formatElapsed(session.elapsedMs)}</span>
            </div>
            {(session.sessionId === hovered || session.sessionId === leaving) && (
              <RowDetailSlot session={session} open={session.sessionId === hovered} />
            )}
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
    </>
  )
}
