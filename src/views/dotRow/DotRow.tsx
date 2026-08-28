import { useEffect, useLayoutEffect, useRef, useState, type CSSProperties } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CollapsedPill } from './CollapsedPill'
import { NamedDotRow } from './NamedDotRow'
import { SessionPopover } from './SessionPopover'
import { UsagePopover } from './UsagePopover'
import {
  afterResizeSettles,
  applyWidgetSize,
  layoutSize,
  morphDuration,
  MORPH_MS,
  POPOVER_ALLOWANCE,
  reportHoverRect,
  rowWidthFor,
  sameSize,
  shadowPad,
  unionRect,
  widgetWindowSize,
} from '../../useWidgetSize'
import { centredAnchor, POPOVER_WIDTH, targetAtPoint, useCursor } from '../../useCursor'
import { CALM, deriveHeat, isCalm } from './heat'
import type { SessionViewProps } from '../SessionView'
import './dotRow.css'
import './crazy.css'

/**
 * Delay before the popover opens. Without it, sweeping the cursor across the
 * row flashes a popover per name.
 */
export const HOVER_GRACE_MS = 180

/** Gap between the pill and the popover, matching `--gap-popover`. */
const POPOVER_GAP = 10

/** How long the crumble runs, matching `crazy-fall-a` in crazy.css. */
const ASH_MS = 1400

/** Where each flame sits along the pill, and how far into its cycle it starts.
 *  Fixed, so changing level never remounts DOM — and the staggered delays are
 *  what make one keyframe track look like a fire rather than like eight things
 *  doing the same thing at the same moment. */
const FLAME_OFFSETS = [
  { left: '3%', delay: '0s' },
  { left: '15%', delay: '-0.3s' },
  { left: '27%', delay: '-0.7s' },
  { left: '39%', delay: '-0.15s' },
  { left: '51%', delay: '-0.55s' },
  { left: '63%', delay: '-0.9s' },
  { left: '75%', delay: '-0.25s' },
  { left: '88%', delay: '-0.65s' },
]

const SPARK_OFFSETS = [
  { left: '18%', bottom: '9px', delay: '0s' },
  { left: '38%', bottom: '6px', delay: '-0.5s' },
  { left: '58%', bottom: '8px', delay: '-0.9s' },
  { left: '80%', bottom: '5px', delay: '-1.3s' },
]

/** Five fractures across the pill, each drawn twice so it reads over fire and
 *  over the dark background alike. Stretched to whatever width the pill is. */
const CRACKS = [
  'M52 0 L60 15 L48 24 L56 42',
  'M148 0 L140 13 L152 22 L144 42',
  'M96 5 L104 19 L90 27',
  'M228 0 L221 17 L234 25 L226 42',
  'M18 9 L27 21 L15 31',
]

export function DotRow({
  sessions,
  usage = null,
  alerts = [],
  crazy = 'off',
}: SessionViewProps) {
  // Off is free: nothing is derived, no attribute is written and no element is
  // mounted, so the widget is what it was before crazy mode existed.
  const heat = crazy === 'ember' ? deriveHeat(sessions, usage, alerts) : CALM
  const lit = !isCalm(heat)
  const flames = lit && heat.fire > 0 ? FLAME_OFFSETS : []
  const sparks = lit && heat.fire >= 2 ? SPARK_OFFSETS : []
  const cracked = lit && heat.strain > 0

  // `died` alerts arrive on one update and are gone by the next. The crumble
  // takes 1.4s, so the ids are held for that long — without this the attribute
  // is removed on the following render and the animation stops halfway.
  const [ashing, setAshing] = useState<readonly string[]>([])
  const ashKey = heat.ash.join(',')
  useEffect(() => {
    if (ashKey === '') return
    setAshing(ashKey.split(','))
    const timer = setTimeout(() => setAshing([]), ASH_MS)
    return () => clearTimeout(timer)
  }, [ashKey])

  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null)
  const [hoveredUsage, setHoveredUsage] = useState(false)
  const [anchorOffset, setAnchorOffset] = useState(0)
  const [flashing, setFlashing] = useState(false)
  const [pillBox, setPillBox] = useState<{ width: number; height: number } | null>(null)
  // Written onto the pill alongside each new box, so the box moves at a
  // constant speed instead of always taking the full morph.
  const [morphMs, setMorphMs] = useState(MORPH_MS)
  // The row is held at the widest state so the pill can grow outwards from its
  // centre instead of unrolling from the left edge.
  const [rowWidth, setRowWidth] = useState<number | null>(null)

  const root = useRef<HTMLDivElement>(null)
  const collapsedSlot = useRef<HTMLDivElement>(null)
  const expandedSlot = useRef<HTMLDivElement>(null)
  const popoverSlot = useRef<HTMLDivElement>(null)
  const pillRef = useRef<HTMLDivElement>(null)
  const appliedWindow = useRef<{ width: number; height: number } | null>(null)
  const shrinkTimer = useRef<ReturnType<typeof setTimeout> | null>(null)
  // The sizing effect must not depend on `pillBox` — setting it there would
  // re-run the effect — so the box it is moving from is mirrored in a ref.
  const appliedPill = useRef<{ width: number; height: number } | null>(null)
  // The larger of the two variants, which is what the window is sized to and
  // what the hover rect is reported as.
  const [widestPill, setWidestPill] = useState<{ width: number; height: number } | null>(null)

  // Hover comes from Rust, not from the DOM: a non-activating NSPanel never
  // becomes the key window, so WKWebView never delivers mousemove to the page.
  const cursor = useCursor()
  const expanded = cursor.inside

  // The shake stops while the pointer is over the widget. Entries have hover
  // states and open popovers, and a pill shaking under the cursor makes hovering
  // a moving target — by the time you are pointing at it, it has done its job.
  //
  // `cursor.inside`, not CSS :hover: the widget is a non-activating NSPanel, so
  // it never becomes the key window and WKWebView never delivers mouse events to
  // the page. :hover would simply never fire.
  const shaking = lit && heat.jitter > 0 && !cursor.inside
  const shuddering = lit && heat.strain === 2 && !cursor.inside

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://flash', () => setFlashing(true)).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [])

  useEffect(() => {
    if (cursor.inside) setFlashing(false)
  }, [cursor.inside])

  const showNamed = expanded && sessions.length > 0

  // Hit-testing forces a synchronous layout, so it only runs when there is
  // actually a row to hit — not on every cursor sample.
  const target = showNamed
    ? targetAtPoint(cursor, (x, y) =>
        typeof document.elementFromPoint === 'function' ? document.elementFromPoint(x, y) : null,
      )
    : null
  // Kept as primitives, not the object: the effect below would otherwise see a
  // fresh dependency on every cursor sample.
  const pending = target?.kind === 'session' ? target.sessionId : null
  const pendingUsage = target?.kind === 'usage'

  useEffect(() => {
    // Leaving the widget is the only thing that closes the popover. Sweeping
    // between two names crosses the gap between them, where nothing is hit —
    // dropping the selection there made the popover blink out for the length of
    // the grace delay on every pass.
    if (!cursor.inside) {
      setHoveredSessionId(null)
      setHoveredUsage(false)
      return
    }

    if (pendingUsage) {
      if (hoveredUsage) return
      const timer = setTimeout(() => {
        setHoveredUsage(true)
        setHoveredSessionId(null)
      }, HOVER_GRACE_MS)
      return () => clearTimeout(timer)
    }

    if (pending === null || pending === hoveredSessionId) return

    const timer = setTimeout(() => {
      setHoveredSessionId(pending)
      setHoveredUsage(false)
    }, HOVER_GRACE_MS)
    return () => clearTimeout(timer)
  }, [pending, pendingUsage, cursor.inside, hoveredSessionId, hoveredUsage])

  const hovered = sessions.find((s) => s.sessionId === hoveredSessionId) ?? null

  // Clicks arrive from Rust for the same reason hover does.
  const hoveredRef = useRef(hovered)
  hoveredRef.current = hovered

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://click', () => {
      const target = hoveredRef.current
      if (target === null) return
      void invoke('raise_session', { pid: target.pid }).catch(() => {
        // The popover surfaces failures on its own next render.
      })
    }).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [])

  useLayoutEffect(() => {
    if (rowWidth === null) return
    // The meter anchors its popover the same way an entry does — it is just a
    // different element to measure from.
    const selector =
      hoveredSessionId !== null ? `[data-session-id="${hoveredSessionId}"]` : '[data-usage]'
    if (hoveredSessionId === null && !hoveredUsage) return
    const slot = expandedSlot.current
    // Scoped to the expanded slot, not the whole row. Both variants stay
    // mounted and both draw a meter, so a row-wide lookup found the collapsed
    // one first — and its offsets are relative to a slot that is not the one
    // the offsets below are measured against, which put the popover somewhere
    // the meter had never been.
    const entry = slot?.querySelector<HTMLElement>(selector)
    if (!entry || !slot) return

    // Deliberately offsetLeft/offsetWidth rather than getBoundingClientRect.
    // The slot is centred with translateX(-50%) inside a pill whose width is
    // animating, so on-screen positions move throughout the morph; measuring
    // them there anchored the popover to a position the entry was only passing
    // through. Offsets are relative to the slot and do not move.
    const slotWidth = slot.offsetWidth
    const entryLeftInRow = (rowWidth - slotWidth) / 2 + entry.offsetLeft
    setAnchorOffset(centredAnchor(entryLeftInRow, entry.offsetWidth, rowWidth))
  }, [hoveredSessionId, hoveredUsage, sessions, rowWidth])

  // Size the pill to the state being morphed into, and the window to hold it.
  // Both variants are mounted, so the target is measurable now rather than
  // after the animation has already clipped.
  useLayoutEffect(() => {
    const slot = (showNamed ? expandedSlot : collapsedSlot).current
    if (!slot) return

    // layoutSize, not getBoundingClientRect: the hidden slot is mid-transition
    // out of scale(0.97) when this runs, and the rect reports that scale.
    const target = layoutSize(slot)

    // Size the window to whichever state is larger, not to the current one, so
    // hovering resizes nothing. Resizing a transparent panel shows one
    // unpainted frame, and it was landing exactly on the start of the morph.
    const collapsedBox = collapsedSlot.current && layoutSize(collapsedSlot.current)
    const expandedBox = expandedSlot.current && layoutSize(expandedSlot.current)
    const widest = {
      width: Math.max(collapsedBox?.width ?? 0, expandedBox?.width ?? 0),
      height: Math.max(collapsedBox?.height ?? 0, expandedBox?.height ?? 0),
    }

    const next = widgetWindowSize(
      widest,
      POPOVER_WIDTH,
      POPOVER_ALLOWANCE,
      POPOVER_GAP,
      shadowPad(),
    )
    const nextRow = rowWidthFor(widest, POPOVER_WIDTH)
    setWidestPill(widest)

    const applied = appliedWindow.current
    const grows = applied === null || next.width > applied.width || next.height > applied.height

    // How far the pill itself has to travel, which is not how far the window
    // does: the window is held at the widest state and mostly does not move at
    // all while the pill morphs beneath it.
    const duration = morphDuration(appliedPill.current, target)
    const movePill = () => {
      appliedPill.current = target
      setMorphMs(duration)
      setPillBox(target)
    }

    if (shrinkTimer.current !== null) {
      clearTimeout(shrinkTimer.current)
      shrinkTimer.current = null
    }

    if (grows) {
      // Grow the window first and wait for it to land, then let the surface
      // settle for two frames before the pill starts moving. The resize is a
      // round trip to Rust, so without awaiting it the native resize arrived
      // mid-transition and dropped frames.
      appliedWindow.current = next
      setRowWidth(nextRow)
      let cancelled = false
      void applyWidgetSize(next.width, next.height)
        .then(afterResizeSettles)
        .then(() => {
          if (!cancelled) movePill()
        })
      return () => {
        cancelled = true
      }
    }

    // Contracting animates first and the window follows, so the pill can start
    // moving immediately.
    movePill()

    // A status change on the collapsed row leaves the window alone: it is sized
    // to the widest of the two states and the expanded row is nearly always the
    // wider one. Resizing to the size it already has still costs a window-server
    // round trip and the one unpainted frame that comes with it, and the delay
    // below lands that frame on the last frame of the morph.
    if (sameSize(applied, next)) return

    // Shrinking has to wait for the morph, or the pill is clipped mid-contract
    // and the row narrows under the still-contracting pill.
    shrinkTimer.current = setTimeout(() => {
      appliedWindow.current = next
      setRowWidth(nextRow)
      applyWidgetSize(next.width, next.height)
      shrinkTimer.current = null
    }, duration)
  }, [showNamed, sessions])

  // The usage popover counts down, and nothing else in this component needs a
  // clock, so it only runs while that popover is open.
  const [usageNow, setUsageNow] = useState(() => Date.now())
  useEffect(() => {
    if (!hoveredUsage) return
    setUsageNow(Date.now())
    const timer = setInterval(() => setUsageNow(Date.now()), 1000)
    return () => clearInterval(timer)
  }, [hoveredUsage])

  // Tell Rust which part of the window is the widget.
  //
  // Reported at the widest of the two variants rather than at whatever the pill
  // measures right now, for the same reason the window is sized that way: this
  // runs after paint on the frame the morph *starts*, so a live measurement is
  // of the box the pill is leaving, not the one it is going to. The row would
  // then be hittable only across its old, narrower extent — and moving onto a
  // name beyond that put the cursor outside the widget, collapsing the row
  // instead of opening a popover.
  useEffect(() => {
    const pill = pillRef.current?.getBoundingClientRect()
    if (!pill) return

    // The pill is centred and stays centred, so widening it about its own
    // centre is the box it will settle into.
    const centre = pill.left + pill.width / 2
    const width = Math.max(pill.width, widestPill?.width ?? 0)
    const height = Math.max(pill.height, widestPill?.height ?? 0)
    const settled = {
      left: centre - width / 2,
      top: pill.top,
      right: centre + width / 2,
      bottom: pill.top + height,
    }

    const popover = popoverSlot.current?.getBoundingClientRect() ?? null
    reportHoverRect(unionRect(settled, popover))
  }, [showNamed, hovered, hoveredUsage, pillBox, rowWidth, widestPill])

  useEffect(
    () => () => {
      if (shrinkTimer.current !== null) clearTimeout(shrinkTimer.current)
    },
    [],
  )

  return (
    <div
      ref={root}
      className="dot-row"
      data-testid="dot-row"
      data-flashing={flashing ? 'true' : 'false'}
      style={rowWidth === null ? undefined : { width: rowWidth }}
    >
      {/* Two wrappers, one transform each. `.pill` already owns an animation —
          the attention flash — and the CSS shorthand does not compose across
          rules, so jitter and shudder cannot live there.
          Always mounted *and* always classed, whatever the setting says. The
          class was conditional at first and that was a bug: it carries
          `display` with it, so turning crazy mode on relaid out the row and the
          widget visibly jumped. Only the data attributes vary now, and they
          drive animation alone. */}
      <div
        className="crazy-shake"
        data-shake={shaking ? 'true' : undefined}
        style={shaking ? ({ '--crazy-amp': heat.jitter * 1.4 } as CSSProperties) : undefined}
      >
        <div
          className="crazy-shudder"
          data-shudder={shuddering ? 'true' : undefined}
        >
          <div
            ref={pillRef}
            className="pill"
            data-fire={lit && heat.fire > 0 ? String(heat.fire) : undefined}
            data-strain={cracked ? String(heat.strain) : undefined}
            data-ash={ashing.length > 0 ? 'true' : undefined}
            // `--morph` is written per change rather than left to the
            // stylesheet: the duration the box needs depends on how far it is
            // going.
            style={
              {
                '--morph': `${morphMs}ms`,
                ...(pillBox === null ? {} : { width: pillBox.width, height: pillBox.height }),
              } as CSSProperties
            }
          >
            {lit && heat.fire > 0 && <span className="crazy-heat" />}
            {flames.length > 0 && (
              <span className="crazy-flames" aria-hidden="true">
                {flames.map((flame) => (
                  <i key={flame.left} style={{ left: flame.left, animationDelay: flame.delay }} />
                ))}
              </span>
            )}
            {sparks.map((spark) => (
              <span
                key={spark.left}
                className="crazy-spark"
                aria-hidden="true"
                style={{ left: spark.left, bottom: spark.bottom, animationDelay: spark.delay }}
              />
            ))}
            {cracked && (
              <span className="crazy-cracks" aria-hidden="true">
                <svg viewBox="0 0 300 42" preserveAspectRatio="none">
                  {CRACKS.map((d) => (
                    <path key={d} className="crack-dark" d={d} />
                  ))}
                  {CRACKS.map((d) => (
                    <path key={d} className="crack-lite" d={d} />
                  ))}
                </svg>
              </span>
            )}
            <div
              className="variant-slot"
              ref={collapsedSlot}
              data-show={showNamed ? 'false' : 'true'}
            >
              <CollapsedPill sessions={sessions} usage={usage} />
            </div>
            <div
              className="variant-slot"
              ref={expandedSlot}
              data-show={showNamed ? 'true' : 'false'}
            >
              <NamedDotRow
                usage={usage}
                sessions={sessions}
                hoveredSessionId={hoveredSessionId}
                onHoverSession={setHoveredSessionId}
                ashing={ashing}
              />
            </div>
          </div>
        </div>
      </div>
      {showNamed && (hovered !== null || (hoveredUsage && usage !== null)) && (
        <div
          ref={popoverSlot}
          className="popover-anchor"
          data-testid="popover-anchor"
          style={{ marginLeft: anchorOffset }}
        >
          {hovered !== null ? (
            <SessionPopover session={hovered} usage={usage} />
          ) : (
            usage !== null && <UsagePopover usage={usage} now={usageNow} />
          )}
        </div>
      )}
    </div>
  )
}
