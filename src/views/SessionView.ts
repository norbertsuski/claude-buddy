import type { SessionSnapshot } from '../types'

/** Every view mode takes exactly this. Adding a mode touches no other layer. */
export interface SessionViewProps {
  sessions: SessionSnapshot[]
  /**
   * Whether the view may pick its own animation timings rather than using the
   * one duration every change used to share. Off restores the fixed morph.
   */
  smoothTransitions?: boolean
}
