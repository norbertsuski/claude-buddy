export function formatElapsed(ms: number): string {
  const clamped = Math.max(0, ms)
  const totalSeconds = Math.floor(clamped / 1000)
  if (totalSeconds < 60) return `${totalSeconds}s`
  const totalMinutes = Math.floor(totalSeconds / 60)
  if (totalMinutes < 60) return `${totalMinutes}m`
  const hours = Math.floor(totalMinutes / 60)
  return `${hours}h${totalMinutes % 60}m`
}

/**
 * Time left until the five-hour window resets.
 *
 * Floored to whole minutes and never showing seconds, so the meter can tick on
 * a slow interval: a countdown that ticked every second would re-render the
 * pill sixty times a minute for a glyph nobody is watching that closely.
 */
export function formatCountdown(ms: number): string {
  const minutes = Math.floor(Math.max(0, ms) / 60_000)
  if (minutes < 1) return '<1m'
  if (minutes < 60) return `${minutes}m`
  return `${Math.floor(minutes / 60)}h${String(minutes % 60).padStart(2, '0')}m`
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

/**
 * Longest label the collapsed row will draw for one session.
 *
 * The pill sizes itself to its contents, so nothing else stops a title from
 * widening the widget across the screen — and titles are sentences where names
 * were single words. Chosen to hold a short sentence: five sessions at this
 * width still fit a laptop menu bar.
 */
export const ROW_LABEL_MAX_CHARS = 22

/**
 * What one session is called in the row.
 *
 * The title is what the user recognises — three sessions in one repository are
 * three identical names and three different titles. It falls back to the
 * registry name, because a session is only titled once Claude Code has seen
 * enough of it to name it, and a nameless dot for the first minute is worse
 * than the folder it is running in.
 */
export function rowLabel(session: SessionSnapshot): string {
  const title = session.title?.trim()
  if (!title) return shortName(session.name)
  if (title.length <= ROW_LABEL_MAX_CHARS) return title
  return `${title.slice(0, ROW_LABEL_MAX_CHARS).trimEnd()}\u2026`
}

/** Sessions you answer, as opposed to background jobs belonging to them. */
export function ownSessions(sessions: SessionSnapshot[]): SessionSnapshot[] {
  return sessions.filter((s) => !s.background)
}

export function countByState(sessions: SessionSnapshot[]): Record<SessionState, number> {
  const counts: Record<SessionState, number> = {
    waiting: 0,
    busy: 0,
    tasking: 0,
    idle: 0,
    paused: 0,
    dead: 0,
  }
  for (const session of sessions) counts[session.state] += 1
  return counts
}
