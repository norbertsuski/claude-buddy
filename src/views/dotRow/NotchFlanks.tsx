import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { SessionSnapshot, Usage } from '../../types'
import { useCursor } from '../../useCursor'
import { reportHoverRects, useNotchLayout, visibleRects } from '../../useNotch'
import { NotchPanel, rowAtPoint, type RowTarget } from './NotchPanel'
import { StateCounts } from './StateCounts'
import { UsageMeter } from './UsageMeter'
import './notchFlanks.css'

/**
 * Delay before the highlight moves rows.
 *
 * Without it, sweeping down the list flickers a highlight per row on the way
 * past — and the click target and the open detail move with it.
 */
export const ROW_GRACE_MS = 120

interface Props {
  sessions: SessionSnapshot[]
  /** The five-hour limit, or null when there is nothing trustworthy to show. */
  usage: Usage | null
}

/** Identity of a row, for comparing without allocating. */
function rowKey(target: RowTarget | null): string | null {
  return target === null ? null : `${target.kind}:${target.sessionId ?? ''}`
}

/**
 * The widget as one black shape beside the notch that grows into a slab.
 *
 * At rest it is the menu bar's height and hugs its content: session counts to
 * the left of the notch, the five-hour limit's bar to the right, the notch held
 * open between them. On hover the same element widens to a fixed 340pt and grows
 * downward into a list of every session, its status, and its elapsed time — the
 * resting halves fading out as the list fades in.
 *
 * One element for both states, so the black grows out of what is already on
 * screen. A separate panel pinned to the top of the display looked like it was
 * unrolling from the screen edge rather than opening from the notch.
 */
export function NotchFlanks({ sessions, usage }: Props) {
  const layout = useNotchLayout()
  const cursor = useCursor()
  const open = cursor.inside

  const [row, setRow] = useState<RowTarget | null>(null)
  const [listHeight, setListHeight] = useState(0)
  const [restWidths, setRestWidths] = useState({ left: 0, right: 0 })

  const restLeftRef = useRef<HTMLDivElement>(null)
  const restRightRef = useRef<HTMLDivElement>(null)

  // The band sits flush at y = 0, but `body` carries --shadow-pad on every side
  // so the free-mode pill has somewhere to drop a shadow. Notch mode zeroes it,
  // and does so from here rather than from App so that turning the mode off
  // restores the padding without App having to know why it was gone.
  useEffect(() => {
    document.body.classList.add('notch-mode')
    return () => document.body.classList.remove('notch-mode')
  }, [])

  // The resting band hugs its content, so its width has to be measured. It
  // cannot animate from `auto` to a fixed width, and the two halves are not the
  // same size as each other.
  useLayoutEffect(() => {
    setRestWidths({
      left: restLeftRef.current?.offsetWidth ?? 0,
      right: restRightRef.current?.offsetWidth ?? 0,
    })
  }, [sessions, usage])

  // Which row the cursor is on. Hit-testing forces a synchronous layout, so it
  // only runs while the slab is actually open.
  const pending = open
    ? rowAtPoint(cursor.x, cursor.y, (x, y) =>
        typeof document.elementFromPoint === 'function' ? document.elementFromPoint(x, y) : null,
      )
    : null
  const pendingRef = useRef(pending)
  pendingRef.current = pending

  useEffect(() => {
    if (!open) {
      setRow(null)
      return
    }
    // Leaving the widget is the only thing that clears the highlight. Sweeping
    // between rows crosses their padding, where nothing is hit — dropping the
    // selection there would flicker it out on every pass.
    const next = rowKey(pendingRef.current)
    if (next === null || next === rowKey(row)) return
    const timer = setTimeout(() => setRow(pendingRef.current), ROW_GRACE_MS)
    return () => clearTimeout(timer)
    // Compared by key rather than by object, which is freshly allocated every
    // render and would re-run this effect forever.
  }, [open, rowKey(pending), rowKey(row)])

  // Clicks arrive from Rust for the same reason hover does: a non-activating
  // NSPanel never becomes the key window, so the page never sees its own.
  const rowRef = useRef<RowTarget | null>(null)
  rowRef.current = row

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://click', () => {
      const target = rowRef.current
      if (target?.kind !== 'session') return
      const session = sessions.find((s) => s.sessionId === target.sessionId)
      if (session === undefined) return
      void invoke('raise_session', { pid: session.pid }).catch(() => {
        // The row surfaces failures on its own next render.
      })
    }).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [sessions])

  const notchWidth = layout === null ? 0 : layout.notchRight - layout.notchLeft
  const restWidth = restWidths.left + notchWidth + restWidths.right
  const band =
    layout === null
      ? null
      : open
        ? {
            left: (layout.notchLeft + layout.notchRight) / 2 - layout.slabWidth / 2,
            width: layout.slabWidth,
            // Not barHeight + listHeight: the list reserves the bar's height as
            // its own top padding, so adding it again left exactly one bar
            // height of dead black below the footer.
            height: listHeight,
          }
        : {
            left: layout.notchLeft - restWidths.left,
            width: restWidth,
            height: layout.barHeight,
          }

  // Tell Rust which parts of the window are the widget.
  //
  // Open, that is the band alone: it spans the bar as well as the list, so the
  // cursor that opened it is already inside. At rest it is the two resting
  // halves and not the band, so that crossing the notch does not open it.
  useLayoutEffect(() => {
    if (layout === null || band === null) return
    reportHoverRects(
      visibleRects(
        open
          ? [{ left: band.left, top: 0, width: band.width, height: band.height }]
          : [
              restLeftRef.current?.getBoundingClientRect(),
              restRightRef.current?.getBoundingClientRect(),
            ],
      ),
    )
  }, [open, sessions, usage, row, layout, band?.left, band?.width, band?.height])

  // Every hook runs before this: a display without a notch must not change the
  // order they are called in.
  if (layout === null || band === null) return null

  return (
    <div className="notch-flanks" data-testid="notch-flanks">
      <div
        className="notch-slab"
        data-open={open ? 'true' : 'false'}
        data-testid="notch-slab"
        style={{ left: band.left, width: band.width, height: band.height }}
      >
        <NotchPanel
          sessions={sessions}
          usage={usage}
          open={open}
          barHeight={layout.barHeight}
          row={row}
          notchWidth={notchWidth}
          restLeft={<StateCounts sessions={sessions} />}
          restRight={usage === null ? null : <UsageMeter usage={usage} show="percent" />}
          restLeftRef={restLeftRef}
          restRightRef={restRightRef}
          onMeasure={setListHeight}
        />
      </div>
    </div>
  )
}
