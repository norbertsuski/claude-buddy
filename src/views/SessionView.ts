import type { SessionSnapshot } from '../types'

/** Every view mode takes exactly this. Adding a mode touches no other layer. */
export interface SessionViewProps {
  sessions: SessionSnapshot[]
}
