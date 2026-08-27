import type { SessionSnapshot, Usage } from '../types'

/** What a view is handed. The dot row is the only one left; `viewMode` in the
 *  config file, which used to pick between several, is parsed and ignored. */
export interface SessionViewProps {
  sessions: SessionSnapshot[]
  /**
   * Five-hour limit usage, or `null` when there is nothing worth showing —
   * which includes the user having turned the meter off.
   */
  usage?: Usage | null
}
