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
export function UsageMeter({ usage }: { usage: Usage }) {
  const [now, setNow] = useState(() => Date.now())

  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), TICK_MS)
    return () => clearInterval(timer)
  }, [])

  return (
    <span
      className="usage"
      data-testid="usage"
      data-severity={usage.severity}
      title={`${usage.percent}% of the 5h limit used`}
    >
      <span className="usage-track">
        <span className="usage-fill" style={{ width: `${usage.percent}%` }} />
      </span>
      <span className="usage-left">{formatCountdown(usage.resetsAtMs - now)}</span>
    </span>
  )
}
