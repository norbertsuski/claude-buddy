import { countByState } from '../../format'
import type { SessionSnapshot, SessionState } from '../../types'
import './dotRow.css'
import './notchFlanks.css'

/**
 * Order the counts appear in, most urgent first.
 *
 * The left chip lays itself out in reverse, so first here is nearest the notch —
 * the count that matters sits closest to where the eye already is.
 */
export const STATE_ORDER: SessionState[] = [
  'waiting',
  'dead',
  'busy',
  'tasking',
  'idle',
  'paused',
]

/**
 * One dot and a number per state present, and nothing for the states absent.
 *
 * Every state is here, on one chip. An earlier design split them across both
 * flanks by urgency, which forced two rules that no longer exist: a background
 * job had to be walked onto its parent's side to keep its continuation arrow
 * meaningful, and a chip had to count states its side did not nominally carry.
 */
export function StateCounts({ sessions }: { sessions: SessionSnapshot[] }) {
  const counts = countByState(sessions)
  return (
    <>
      {STATE_ORDER.filter((state) => counts[state] > 0).map((state) => (
        <span key={state} className="count" data-testid={`count-${state}`}>
          <span className={`dot dot-${state}`} />
          {counts[state]}
        </span>
      ))}
    </>
  )
}
