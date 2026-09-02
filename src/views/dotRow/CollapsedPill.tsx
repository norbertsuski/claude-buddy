import { countByState, ownSessions } from '../../format'
import { UsageMeter } from './UsageMeter'
import type { SessionViewProps } from '../SessionView'
import './dotRow.css'

/**
 * Resting content. Renders the pill's *contents* only — the pill itself is a
 * single persistent element in DotRow so it can animate between states rather
 * than being swapped out.
 */
export function CollapsedPill({ sessions, usage = null }: SessionViewProps) {
  // Background jobs never inflate the volume counts — they belong to a session
  // rather than being one — but urgency is not a volume question. A job blocked
  // on input already fires a notification and already keeps the widget on
  // screen; leaving it out of the amber chip meant the one surface you actually
  // rest your eyes on was the only one that stayed silent about it.
  const own = ownSessions(sessions)
  const counts = countByState(own)
  const urgent = countByState(sessions)
  const idle = counts.idle + counts.paused

  // A job surfaced by its own chip is not also tallied in the quiet summary,
  // or "1 needs you · 2 jobs" would be counting the same entry twice.
  const backgrounded = sessions.filter((s) => s.background)
  const jobs = backgrounded.filter((s) => s.state !== 'waiting' && s.state !== 'dead').length

  // Each state that carries urgency gets its own coloured chip. What is merely
  // sitting there stays as quiet text.
  const summary = sessions.length === 0 ? 'no sessions' : idle > 0 ? `${idle} idle` : null

  return (
    <div className="variant" data-testid="collapsed-pill">
      {urgent.waiting > 0 && (
        <span className="chip chip-waiting" data-testid="needs-you">
          <span className="dot dot-waiting" />
          {urgent.waiting} {urgent.waiting === 1 ? 'needs' : 'need'} you
        </span>
      )}
      {counts.busy > 0 && (
        <span className="chip chip-busy" data-testid="working">
          <span className="dot dot-busy" />
          {counts.busy} working
        </span>
      )}
      {counts.tasking > 0 && (
        <span className="chip chip-tasking" data-testid="tasking">
          <span className="dot dot-tasking" />
          {counts.tasking} on {counts.tasking === 1 ? 'a task' : 'tasks'}
        </span>
      )}
      {urgent.dead > 0 && (
        <span className="chip chip-dead" data-testid="died">
          <span className="dot dot-dead" />
          {urgent.dead} died
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
      {usage !== null && <UsageMeter usage={usage} show="percent" />}
    </div>
  )
}
