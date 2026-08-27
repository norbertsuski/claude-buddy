import type { SessionSnapshot, Usage } from '../types'

/** Every view mode takes exactly this. Adding a mode touches no other layer. */
export interface SessionViewProps {
  sessions: SessionSnapshot[]
  /**
   * Five-hour limit usage, or `null` when there is nothing worth showing —
   * which includes the user having turned the meter off.
   */
  usage?: Usage | null
}
