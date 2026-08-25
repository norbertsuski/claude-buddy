import { countByState, ownSessions } from '../../format'
import type { SessionViewProps } from '../SessionView'
import './dotRow.css'

/**
 * Resting content. Renders the pill's *contents* only — the pill itself is a
 * single persistent element in DotRow so it can animate between states rather
 * than being swapped out.
 */
export function CollapsedPill({ sessions }: SessionViewProps) {
  // Background jobs never inflate the counts — they belong to a session rather
  // than being one — but they are worth acknowledging.
  const own = ownSessions(sessions)
  const jobs = sessions.length - own.length
  const counts = countByState(own)
  const idle = counts.idle + counts.paused

  // Each state that carries urgency gets its own coloured chip. What is merely
  // sitting there stays as quiet text.
  const summary = sessions.length === 0 ? 'no sessions' : idle > 0 ? `${idle} idle` : null

  return (
    <div className="variant" data-testid="collapsed-pill">
      {counts.waiting > 0 && (
        <span className="chip chip-waiting" data-testid="needs-you">
          <span className="dot dot-waiting" />
          {counts.waiting} {counts.waiting === 1 ? 'needs' : 'need'} you
        </span>
      )}
      {counts.busy > 0 && (
        <span className="chip chip-busy" data-testid="working">
          <span className="dot dot-busy" />
          {counts.busy} working
        </span>
      )}
      {counts.dead > 0 && (
        <span className="chip chip-dead" data-testid="died">
          <span className="dot dot-dead" />
          {counts.dead} died
        </span>
      )}
      {summary !== null && (
        <span className="summary" data-testid="summary">
          {summary}
        </span>
      )}
      {jobs > 0 && (
        <span className="summary summary-jobs" data-testid="jobs">
          {jobs} {jobs === 1 ? 'job' : 'jobs'}
        </span>
      )}
    </div>
  )
}
