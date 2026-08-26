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

  // The resting band hugs its content, so its width has to be measured: it
  // cannot animate from `auto`, and the two halves are not the same size as each
  // other. Only the open width is fixed.
  //
  // Watched rather than measured against a dependency list. The counts render
  // empty until the first snapshot arrives and the meter until the first usage
  // read, so a measurement taken when a dependency changed caught them at zero —
  // and a zero-width band clipped the halves out of sight, which is where the
  // measurement came from.
  useEffect(() => {
    const halves = [restLeftRef.current, restRightRef.current]
    const read = () =>
      setRestWidths({
        left: restLeftRef.current?.offsetWidth ?? 0,
        right: restRightRef.current?.offsetWidth ?? 0,
      })
    read()
    if (typeof ResizeObserver !== 'function') return
    const observer = new ResizeObserver(read)
    for (const half of halves) if (half !== null) observer.observe(half)
    return () => observer.disconnect()
    // Keyed on the layout, not empty: this component renders nothing until the
    // layout arrives from Rust, so an effect that ran once on mount ran before
    // the halves existed — it read zero and had nothing to observe, and a
    // zero-width band then clipped away the very content it was measuring.
  }, [layout])

  // Which row the cursor is on.
  //
  // Driven by the cursor's own coordinates and nothing else. Selecting from a
  // hit-test on every render made the list fight itself: opening a detail
  // pushes the rows below it down, closing one pulls them back up, so a
  // stationary cursor kept finding a different row under it as the layout
  // settled — which moved the detail again. Two rows was enough to oscillate.
  //
  // Rust only reports a position that has actually changed, so the effect
  // re-runs when the pointer moves and not when the layout does.
  useEffect(() => {
    if (!open) {
      setRow(null)
      return
    }
    // Hit-tested at the end of the grace rather than at the start: it forces a
    // synchronous layout, and this way it reads the list as it has settled.
    // Sweeping between rows crosses their padding, where nothing is hit —
    // dropping the selection there would flicker it out on every pass, so a
    // miss leaves the highlight where it is. Only leaving the widget clears it.
    const timer = setTimeout(() => {
      const next = rowAtPoint(cursor.x, cursor.y, (x, y) =>
        typeof document.elementFromPoint === 'function' ? document.elementFromPoint(x, y) : null,
      )
      if (next === null) return
      setRow((current) => (rowKey(current) === rowKey(next) ? current : next))
    }, ROW_GRACE_MS)
    return () => clearTimeout(timer)
  }, [open, cursor.x, cursor.y])

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
  // Open is one fixed width, derived from the display rather than from how much
  // there is to say, so the slab is the same size every time. At rest the band
  // hugs its content and stays out of the menu bar's way — widening it there
  // would put black under the menu bar extras permanently.
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
            width: restWidths.left + notchWidth + restWidths.right,
            height: layout.barHeight,
          }

  // Tell Rust which part of the window is the widget: the band, in both states.
  //
  // Whatever is painted black is the widget. Reporting the two resting halves
  // instead — which is what this did, to keep the notch itself from opening the
  // slab — left the black either side of their content dead to the cursor, so
  // the band responded only where it had something to say.
  //
  // While open the height only ever grows: a detail closing shortens the list,
  // and a rect that shrinks under a stationary cursor put it outside the widget
  // — so moving from the first row towards the second shut the whole slab as
  // the first row's detail collapsed. The latch is released on close.
  const openHeight = useRef(0)

  useLayoutEffect(() => {
    if (layout === null || band === null) return
    if (!open) {
      openHeight.current = 0
      reportHoverRects(
        visibleRects([{ left: band.left, top: 0, width: band.width, height: band.height }]),
      )
      return
    }
    openHeight.current = Math.max(openHeight.current, band.height)
    reportHoverRects(
      visibleRects([{ left: band.left, top: 0, width: band.width, height: openHeight.current }]),
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
          notchWidth={notchWidth}
          row={row}
          notchLeftInBand={open ? (band.width - notchWidth) / 2 : restWidths.left}
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
