import { useEffect, useState } from 'react'
import { formatCountdown } from '../../format'
import type { Usage } from '../../types'

/**
 * How often the countdown re-reads the clock.
 *
 * The label is floored to whole minutes, so anything faster than this re-renders
 * the pill for a glyph that has not changed.
 */
export const TICK_MS = 15_000

/**
 * The five-hour limit, at the end of the collapsed row.
 *
 * A bar rather than a number, and a countdown in fixed-width figures, because
 * the pill animates its box to fit its contents: a label whose width changed as
 * the percentage or the remaining time ticked over would re-run that whole
 * animation, several times an hour, unprompted.
 *
 * Rendered only when there is something trustworthy to render — the caller
 * passes `null` whenever the underlying figure describes a window that has
 * already reset. See `crate::usage` for why that is so often the case.
 */
export function UsageMeter({
  usage,
  show = 'countdown',
}: {
  usage: Usage
  /**
   * What the label reads. The resting row is glanced at, where a share left is
   * quicker to read than a duration; the expanded row is already being looked
   * at deliberately, and there the time is the more useful of the two. Both are
   * spelled out together in the popover.
   */
  show?: 'countdown' | 'percent'
}) {
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    // A percentage does not move with the clock, so the resting row — the one
    // on screen all day — schedules nothing at all.
    if (show === 'percent') return
    const timer = setInterval(() => setNow(Date.now()), TICK_MS)
    return () => clearInterval(timer)
  }, [show])

  return (
    <span
      className="usage"
      data-testid="usage"
      data-usage="true"
      data-severity={usage.severity}
    >
      <span className="usage-track">
        {/* Drains rather than fills, so the bar falls as the figure beside it
            does. With the bar growing on what was spent and the label counting
            what is left, the two moved opposite ways and the pair read as a
            contradiction. The popover states the spent share outright. */}
        <span className="usage-fill" style={{ width: `${100 - usage.percent}%` }} />
      </span>
      <span className="usage-left" data-show={show}>
        {show === 'percent' ? `${100 - usage.percent}%` : formatCountdown(usage.resetsAtMs - now)}
      </span>
    </span>
  )
}
