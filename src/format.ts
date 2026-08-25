export function formatElapsed(ms: number): string {
  const clamped = Math.max(0, ms)
  const totalSeconds = Math.floor(clamped / 1000)
  if (totalSeconds < 60) return `${totalSeconds}s`
  const totalMinutes = Math.floor(totalSeconds / 60)
  if (totalMinutes < 60) return `${totalMinutes}m`
  const hours = Math.floor(totalMinutes / 60)
  return `${hours}h${totalMinutes % 60}m`
}

import type { SessionSnapshot, SessionState } from './types'

/**
 * Claude Code derives session names as `<project>-<2 chars>`, e.g.
 * `api-service-55`. The suffix disambiguates two sessions in one repo but
 * costs horizontal space the pill does not have, so the row drops it. The
 * popover shows the full name.
 */
export function shortName(name: string): string {
  const stripped = name.replace(/-[a-z0-9]{2}$/i, '')
  return stripped.length > 0 ? stripped : name
}

/** Sessions you answer, as opposed to background jobs belonging to them. */
export function ownSessions(sessions: SessionSnapshot[]): SessionSnapshot[] {
  return sessions.filter((s) => !s.background)
}

export function countByState(sessions: SessionSnapshot[]): Record<SessionState, number> {
  const counts: Record<SessionState, number> = {
    waiting: 0,
    busy: 0,
    idle: 0,
    paused: 0,
    dead: 0,
  }
  for (const session of sessions) counts[session.state] += 1
  return counts
}
