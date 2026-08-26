import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { SessionSnapshot, Usage } from '../../types'
import { useCursor } from '../../useCursor'
import { reportHoverRects, useNotchLayout, visibleRects } from '../../useNotch'
import { FlankChip } from './FlankChip'
import { NotchPanel, rowAtPoint, type RowTarget } from './NotchPanel'
import { StateCounts } from './StateCounts'
import { UsageMeter } from './UsageMeter'
import './notchFlanks.css'

/**
 * Delay before the highlight moves rows.
 *
 * Without it, sweeping down the list flickers a highlight per row on the way
 * past — and the click target moves with it.
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
 * The widget as black in the menu bar that grows into a slab.
 *
 * At rest: session counts on the left of the notch, the five-hour limit on the
 * right, both flush against it and reading as one shape with it. On hover a slab
 * of a single fixed width takes over — black across the bar and down into a list
 * of every session with its status and elapsed time spelled out.
 *
 * One width throughout, so there is no join to treat: no flare, no concave
 * fillets where a wide panel would meet a narrow notch. The notch sits inside
 * the slab and disappears.
 *
 * There is no detail card. The slab is wide enough that a row carries what a
 * popover used to, which is the whole reason for choosing this width.
 */
export function NotchFlanks({ sessions, usage }: Props) {
  const layout = useNotchLayout()
  const cursor = useCursor()
  const open = cursor.inside

  const [row, setRow] = useState<RowTarget | null>(null)
  const [slabHeight, setSlabHeight] = useState(0)

  const leftRef = useRef<HTMLDivElement>(null)
  const rightRef = useRef<HTMLDivElement>(null)

  // The chips sit flush at y = 0, but `body` carries --shadow-pad on every side
  // so the free-mode pill has somewhere to drop a shadow. Notch mode zeroes it,
  // and does so from here rather than from App so that turning the mode off
  // restores the padding without App having to know why it was gone.
  useEffect(() => {
    document.body.classList.add('notch-mode')
    return () => document.body.classList.remove('notch-mode')
  }, [])

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

  // Tell Rust which parts of the window are the widget.
  //
  // While open that is the slab alone, and because the slab spans the bar as
  // well as the list, the cursor that opened it is already inside — no separate
  // band is needed to stop it shutting itself.
  useLayoutEffect(() => {
    if (layout === null) return
    const slabLeft = (layout.notchLeft + layout.notchRight) / 2 - layout.slabWidth / 2
    // Described from the geometry rather than from its own box, which is
    // mid-animation and would measure 0 tall exactly when it matters.
    const slab =
      open && slabHeight > 0
        ? { left: slabLeft, top: 0, width: layout.slabWidth, height: slabHeight }
        : null
    reportHoverRects(
      visibleRects(
        open
          ? [slab]
          : [leftRef.current?.getBoundingClientRect(), rightRef.current?.getBoundingClientRect()],
      ),
    )
  }, [open, sessions, usage, row, layout, slabHeight])

  // Every hook runs before this: a display without a notch must not change the
  // order they are called in.
  if (layout === null) return null

  const notchWidth = layout.notchRight - layout.notchLeft
  const slabLeft = (layout.notchLeft + layout.notchRight) / 2 - layout.slabWidth / 2

  return (
    <div className="notch-flanks" data-testid="notch-flanks">
      <div
        className="flank flank-left"
        style={{
          left: layout.notchLeft - layout.budget,
          width: layout.budget,
          height: layout.barHeight,
        }}
      >
        <FlankChip side="left" chipRef={leftRef} testId="flank-left">
          <StateCounts sessions={sessions} />
        </FlankChip>
      </div>
      <div
        className="flank flank-right"
        style={{ left: layout.notchRight, width: layout.budget, height: layout.barHeight }}
      >
        {usage !== null && (
          <FlankChip side="right" chipRef={rightRef} testId="flank-usage">
            <UsageMeter usage={usage} show="percent" />
          </FlankChip>
        )}
      </div>

      {/* Black across the notch at rest, so the chips and the notch read as one
          shape before the slab takes over. */}
      <div
        className="notch-bridge"
        data-testid="notch-bridge"
        aria-hidden="true"
        style={{ left: layout.notchLeft, width: notchWidth, height: layout.barHeight }}
      />

      <div className="notch-slab-slot" style={{ left: slabLeft }}>
        <NotchPanel
          sessions={sessions}
          usage={usage}
          open={open}
          width={layout.slabWidth}
          barHeight={layout.barHeight}
          row={row}
          onMeasure={setSlabHeight}
        />
      </div>
    </div>
  )
}
