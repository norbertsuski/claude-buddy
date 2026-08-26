import { formatCountdown } from '../../format'
import type { Usage } from '../../types'
import './dotRow.css'

/** Wall-clock time the window resets, in the viewer's own timezone. */
function resetClock(resetsAtMs: number): string {
  return new Date(resetsAtMs).toLocaleTimeString(undefined, {
    hour: '2-digit',
    minute: '2-digit',
  })
}

/**
 * The five-hour limit in full, opened by hovering the meter.
 *
 * The meter is a bar and a countdown and nothing else — deliberately, since it
 * lives in a row whose width is animated and must not move as the figures tick.
 * Everything that would not fit there is here.
 */
export function UsagePopover({ usage, now }: { usage: Usage; now: number }) {
  return (
    <div className="popover" data-testid="usage-popover">
      <div className="popover-head">
        <span className={`usage-blip usage-blip-${usage.severity}`} />
        <span className="popover-title">5h limit</span>
      </div>
      <dl className="popover-fields">
        <dt>used</dt>
        <dd
          className={usage.severity === 'critical' ? 'hot' : undefined}
          data-testid="usage-popover-percent"
        >
          {usage.percent}%
        </dd>
        <dt>resets</dt>
        <dd data-testid="usage-popover-resets">
          in {formatCountdown(usage.resetsAtMs - now)} · at {resetClock(usage.resetsAtMs)}
        </dd>
      </dl>
      <div className="popover-foot">
        read from Claude Code's own usage cache — refreshed when it fetches usage
      </div>
    </div>
  )
}
