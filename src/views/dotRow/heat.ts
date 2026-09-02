import type { Alert, SessionSnapshot, Usage } from '../../types'

/**
 * How intense the widget should look, derived from what it is already showing.
 *
 * Four separate figures rather than one blended number. A single "intensity"
 * would say the widget is agitated without saying why, which is less than the
 * five dots already tell you — the point of crazy mode is to add information,
 * not to trade it for spectacle.
 *
 * Pure and clock-free, in the shape of `visibility::should_hide` and
 * `watcher::state::snapshot`, so every question about when the widget burns is
 * answered and tested here rather than in a component.
 */
export interface Heat {
  /** Busy foreground sessions, capped at 3. */
  fire: 0 | 1 | 2 | 3
  /** 0 at 30s of waiting, 1 at five minutes, linear between. */
  jitter: number
  /** 0 normal, 1 warn, 2 critical. */
  strain: 0 | 1 | 2
  /** Sessions that died in this update. */
  ash: readonly string[]
}

/**
 * A session that has only just asked a question does not need the widget to
 * panic on its behalf, so nothing moves for the first half minute.
 */
export const JITTER_START_MS = 30_000

/** Where the shake reaches full amplitude. */
export const JITTER_FULL_MS = 300_000

export const CALM: Heat = { fire: 0, jitter: 0, strain: 0, ash: [] }

const SEVERITY: Record<Usage['severity'], 0 | 1 | 2> = {
  normal: 0,
  warn: 1,
  critical: 2,
}

export function deriveHeat(
  sessions: readonly SessionSnapshot[],
  usage: Usage | null,
  alerts: readonly Alert[],
): Heat {
  // Background jobs are already demoted to 0.55 opacity because they are work
  // you did not start. Setting the widget alight for a subagent would be the
  // same mistake in a louder voice.
  const own = sessions.filter((s) => !s.background)

  const busy = own.filter((s) => s.state === 'busy').length

  // A task you launched yourself is work in progress and stokes the fire. A
  // registry job is not: the comment above stands, and a session whose only
  // running task is a job would be the same exclusion dodged by one hop.
  const tasking = own.filter(
    (s) =>
      s.state === 'tasking' &&
      s.tasks.some((t) => t.status === 'running' && t.kind !== 'job'),
  ).length

  const fire = Math.min(3, busy + tasking) as Heat['fire']

  const waited = own
    .filter((s) => s.state === 'waiting')
    .reduce((longest, s) => Math.max(longest, s.elapsedMs), 0)
  const span = JITTER_FULL_MS - JITTER_START_MS
  const jitter = Math.min(1, Math.max(0, (waited - JITTER_START_MS) / span))

  const strain = usage === null ? 0 : SEVERITY[usage.severity]

  // Keyed off the alert, not the state: a session stays `dead` for as long as
  // it is listed, which can be hours, but dying happens once and the alert is
  // that moment.
  const ash = alerts.filter((a) => a.kind === 'died').map((a) => a.sessionId)

  return { fire, jitter, strain, ash }
}

/** Whether anything at all is worth drawing. Nothing mounts when this is true. */
export function isCalm(heat: Heat): boolean {
  return heat.fire === 0 && heat.jitter === 0 && heat.strain === 0 && heat.ash.length === 0
}
