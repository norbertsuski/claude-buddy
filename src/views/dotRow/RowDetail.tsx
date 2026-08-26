import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { SessionSnapshot, TranscriptDetail } from '../../types'
import './notchFlanks.css'

const EMPTY: TranscriptDetail = { branch: null, model: null, effort: null, activity: null }

/**
 * What the popover used to say, under the row it belongs to.
 *
 * Fetched per hovered row rather than for every row, for the reason
 * `SessionPopover` gives: reading transcript fields eagerly would tail a file
 * per session twice a second for data nobody is looking at. That is also why
 * this cannot simply be shown under every row at once.
 */
export function RowDetail({
  session,
  agents = 0,
}: {
  session: SessionSnapshot
  /** Background agents this session has running. */
  agents?: number
}) {
  const [detail, setDetail] = useState<TranscriptDetail>(EMPTY)

  useEffect(() => {
    let live = true
    setDetail(EMPTY)
    invoke<TranscriptDetail>('session_detail', {
      cwd: session.cwd,
      sessionId: session.sessionId,
    })
      .then((next) => {
        if (live) setDetail(next)
      })
      .catch(() => {
        // The row above still says everything essential; detail is a bonus.
      })
    return () => {
      live = false
    }
  }, [session.cwd, session.sessionId])

  const where = [detail.branch, detail.model].filter(Boolean).join(' · ')

  return (
    <div className="notch-detail" data-testid={`detail-${session.sessionId}`}>
      {detail.activity !== null && <div className="notch-detail-line">{detail.activity}</div>}
      {agents > 0 && (
        <div className="notch-detail-line" data-testid="agent-count">
          {agents} background {agents === 1 ? 'agent' : 'agents'}
        </div>
      )}
      {where.length > 0 && <div className="notch-detail-line">{where}</div>}
      <div className="notch-detail-line notch-detail-path">{session.cwd}</div>
    </div>
  )
}

/**
 * How long a detail takes to open or close, in ms.
 *
 * Mirrored in `notchFlanks.css`, and read by the list so the row that is being
 * left stays mounted long enough to collapse. Shorter than `--morph`, which the
 * band's own height uses: the band is following this, and a follower that is
 * slower than what it follows reads as lag rather than as one movement.
 */
export const DETAIL_MORPH_MS = 200

/**
 * A detail that opens and closes rather than appearing and vanishing.
 *
 * The height is measured rather than left at `auto`, which cannot be
 * transitioned, and re-measured as the fetched fields land — the activity line
 * arrives well after the row is hovered and changes how tall this is.
 *
 * Mounting with `open` already true is deliberate: the first paint is at height
 * 0 because that is the state's initial value, and the measurement that follows
 * in a layout effect is what the transition runs from.
 */
export function RowDetailSlot({
  session,
  agents = 0,
  open,
}: {
  session: SessionSnapshot
  agents?: number
  open: boolean
}) {
  const inner = useRef<HTMLDivElement>(null)
  const [height, setHeight] = useState(0)

  useLayoutEffect(() => {
    setHeight(inner.current?.offsetHeight ?? 0)
  }, [])

  useEffect(() => {
    const el = inner.current
    if (el === null || typeof ResizeObserver !== 'function') return
    const observer = new ResizeObserver(() => setHeight(el.offsetHeight))
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  return (
    <div
      className="notch-detail-wrap"
      data-open={open ? 'true' : 'false'}
      data-testid={`detail-slot-${session.sessionId}`}
      style={{ height: open ? height : 0 }}
    >
      <div ref={inner}>
        <RowDetail session={session} agents={agents} />
      </div>
    </div>
  )
}
