import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import type { SessionSnapshot, Usage } from '../../types'
import { POPOVER_WIDTH, useCursor } from '../../useCursor'
import { reportHoverRects, useNotchLayout, visibleRects } from '../../useNotch'
import { FlankChip } from './FlankChip'
import { NotchPanel, wingAtPoint, type WingTarget } from './NotchPanel'
import { SessionPopover } from './SessionPopover'
import { StateCounts } from './StateCounts'
import { UsageMeter } from './UsageMeter'
import { UsagePopover } from './UsagePopover'
import './notchFlanks.css'

/** Gap between the panel's edge and the detail card, matching `--gap-popover`. */
const WING_GAP = 10

/**
 * Delay before the card switches rows.
 *
 * Without it, sweeping down the list flashes a card per row on the way past.
 */
export const WING_GRACE_MS = 140

/** Identity of a card target, for comparing without allocating. */
function wingKey(target: WingTarget | null): string | null {
  return target === null ? null : `${target.kind}:${target.sessionId ?? ''}`
}

interface Props {
  sessions: SessionSnapshot[]
  /** The five-hour limit, or null when there is nothing trustworthy to show. */
  usage: Usage | null
}

/**
 * The widget as two boxes either side of the notch that retract into it.
 *
 * At rest: session counts on the left, the five-hour limit on the right, both
 * flush against the notch and reading as one black shape with it. On hover both
 * slide under the notch and a list exactly the notch's width drops out of it, so
 * the menu bar is emptier while the widget is open than while it is closed.
 *
 * Pointing at a row wings a detail card out beside the panel. The window is
 * centred on the notch and sized to hold that card, so it always fits on the
 * right and there is no side to choose.
 */
export function NotchFlanks({ sessions, usage }: Props) {
  const layout = useNotchLayout()
  const cursor = useCursor()
  const open = cursor.inside

  const [wing, setWing] = useState<WingTarget | null>(null)

  const leftRef = useRef<HTMLDivElement>(null)
  const rightRef = useRef<HTMLDivElement>(null)
  const wingRef = useRef<HTMLDivElement>(null)
  const [panelHeight, setPanelHeight] = useState(0)

  // The chips sit flush at y = 0, but `body` carries --shadow-pad on every side
  // so the free-mode pill has somewhere to drop a shadow. Notch mode zeroes it,
  // and does so from here rather than from App so that turning the mode off
  // restores the padding without App having to know why it was gone.
  useEffect(() => {
    document.body.classList.add('notch-mode')
    return () => document.body.classList.remove('notch-mode')
  }, [])

  // Clicks arrive from Rust for the same reason hover does: a non-activating
  // NSPanel never becomes the key window, so the page never sees its own.
  const wingRefCurrent = useRef<WingTarget | null>(null)
  wingRefCurrent.current = wing

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://click', () => {
      const target = wingRefCurrent.current
      if (target?.kind !== 'session') return
      const session = sessions.find((s) => s.sessionId === target.sessionId)
      if (session === undefined) return
      void invoke('raise_session', { pid: session.pid }).catch(() => {
        // The popover surfaces failures on its own next render.
      })
    }).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [sessions])

  // Which row the cursor is on. Hit-testing forces a synchronous layout, so it
  // only runs while the panel is actually open.
  const pending = open
    ? wingAtPoint(cursor.x, cursor.y, (x, y) =>
        typeof document.elementFromPoint === 'function' ? document.elementFromPoint(x, y) : null,
      )
    : null
  const pendingRef = useRef(pending)
  pendingRef.current = pending

  useEffect(() => {
    if (!open) {
      setWing(null)
      return
    }
    // Leaving the widget is the only thing that closes the card. Sweeping
    // between rows crosses their padding, where nothing is hit — dropping the
    // selection there would blink the card out on every pass.
    const next = wingKey(pendingRef.current)
    if (next === null || next === wingKey(wing)) return
    const timer = setTimeout(() => setWing(pendingRef.current), WING_GRACE_MS)
    return () => clearTimeout(timer)
    // Compared by key rather than by object, which is freshly allocated every
    // render and would re-run this effect forever.
  }, [open, wingKey(pending), wingKey(wing)])

  // Tell Rust which parts of the window are the widget.
  //
  // While open this has to include a band across the bar, not just the panel:
  // the cursor is up in the bar when the panel opens — that is what opened it —
  // and reporting only the panel would put the cursor outside every rect, close
  // the panel, and reopen it on the next sample, forever.
  useLayoutEffect(() => {
    if (layout === null) return
    const band = open
      ? {
          left: layout.notchLeft - layout.budget,
          top: 0,
          width: layout.budget * 2 + (layout.notchRight - layout.notchLeft),
          height: layout.barHeight,
        }
      : null
    // The panel is described from the geometry rather than from its own box,
    // which is mid-animation and would measure 0 tall exactly when it matters.
    const panel =
      open && panelHeight > 0
        ? {
            left: layout.notchLeft,
            top: layout.barHeight,
            width: layout.notchRight - layout.notchLeft,
            height: panelHeight,
          }
        : null
    reportHoverRects(
      visibleRects([
        band,
        open ? panel : leftRef.current?.getBoundingClientRect(),
        open ? wingRef.current?.getBoundingClientRect() : rightRef.current?.getBoundingClientRect(),
      ]),
    )
  }, [open, sessions, usage, wing, layout, panelHeight])

  // Every hook runs before this: a display without a notch must not change the
  // order they are called in.
  if (layout === null) return null

  const notchWidth = layout.notchRight - layout.notchLeft
  const hovered =
    wing?.kind === 'session' ? sessions.find((s) => s.sessionId === wing.sessionId) ?? null : null

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
        <FlankChip
          side="left"
          retracted={open}
          maxWidth={notchWidth}
          chipRef={leftRef}
          testId="flank-left"
        >
          <StateCounts sessions={sessions} />
        </FlankChip>
      </div>
      <div
        className="flank flank-right"
        style={{ left: layout.notchRight, width: layout.budget, height: layout.barHeight }}
      >
        {usage !== null && (
          <FlankChip
            side="right"
            retracted={open}
            maxWidth={notchWidth}
            chipRef={rightRef}
            testId="flank-usage"
          >
            <UsageMeter usage={usage} show="percent" />
          </FlankChip>
        )}
      </div>

      {/* Over the notch, and above the chips, so they slide out of sight under
          it. Also covers the notch band while the panel is open. */}
      <div
        className="notch-bridge"
        data-testid="notch-bridge"
        aria-hidden="true"
        style={{ left: layout.notchLeft, width: notchWidth, height: layout.barHeight }}
      />

      <div
        className="notch-panel-slot"
        style={{ left: layout.notchLeft, width: notchWidth }}
      >
        <NotchPanel
          sessions={sessions}
          usage={usage}
          open={open}
          width={notchWidth}
          top={layout.barHeight}
          wing={wing}
          onMeasure={setPanelHeight}
        />
      </div>

      {open && wing !== null && (
        <div
          ref={wingRef}
          className="notch-wing"
          data-testid="notch-wing"
          style={{
            left: layout.notchRight + WING_GAP,
            top: layout.barHeight + wing.top,
            width: POPOVER_WIDTH,
          }}
        >
          {wing.kind === 'usage' && usage !== null ? (
            <UsagePopover usage={usage} now={Date.now()} />
          ) : hovered !== null ? (
            <SessionPopover session={hovered} />
          ) : null}
        </div>
      )}
    </div>
  )
}
