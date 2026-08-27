import type { Alert, SessionSnapshot, Usage } from '../types'

/** What a view is handed. The dot row is the only one left; `viewMode` in the
 *  config file, which used to pick between several, is parsed and ignored. */
export interface SessionViewProps {
  sessions: SessionSnapshot[]
  /**
   * Five-hour limit usage, or `null` when there is nothing worth showing —
   * which includes the user having turned the meter off.
   */
  usage?: Usage | null
  /**
   * Transitions from this update. Crazy mode's one-shot effects key off these
   * rather than off session state, because a session stays `dead` for as long
   * as it is listed but only dies once.
   */
  alerts?: Alert[]
  /** Crazy-mode level; `off` when absent. */
  crazy?: string
}
