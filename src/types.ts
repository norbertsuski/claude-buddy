// Mirrors src-tauri/src/watcher/state.rs and alerts.rs. Rust serializes
// camelCase; these names must match exactly.

export type SessionState = 'waiting' | 'busy' | 'idle' | 'paused' | 'dead'

export interface SessionSnapshot {
  pid: number
  sessionId: string
  name: string
  cwd: string
  entrypoint: string
  state: SessionState
  /** The registry's waitingFor, present only while waiting. */
  detail: string | null
  elapsedMs: number
  uptimeMs: number
  /** Absolute epoch ms the current state began; the popover ticks from this. */
  statusTimeMs: number
  /** Absolute epoch ms the session started. */
  startedAtMs: number
  /** A background job or subagent, not a session you answer. */
  background: boolean
}

export type AlertKind = 'needsInput' | 'died' | 'finished'

export interface Alert {
  sessionId: string
  name: string
  kind: AlertKind
  detail: string | null
}

/** Mirrors usage::Severity. */
export type UsageSeverity = 'normal' | 'warn' | 'critical'

/**
 * How much of the rolling five-hour limit is spent; mirrors usage::Usage.
 *
 * Absent far more often than present. It comes from a cache Claude Code only
 * refreshes when it actually fetches usage, and anything describing a window
 * that has already reset is dropped rather than shown as if it were current.
 */
export interface Usage {
  /** Whole percent of the window spent, 0–100. */
  percent: number
  /** Absolute epoch ms the window resets; the meter counts down from this. */
  resetsAtMs: number
  severity: UsageSeverity
}

export interface Update {
  sessions: SessionSnapshot[]
  alerts: Alert[]
  usage: Usage | null
}

/** Fields that live only in the session transcript, fetched lazily on hover. */
export interface TranscriptDetail {
  branch: string | null
  model: string | null
  effort: string | null
  /** What the session is doing: the newest tool use, or what it last said. */
  activity: string | null
}

export const UPDATE_EVENT = 'sessions://update'

/** Settings changed; mirrors commands::CONFIG_EVENT. */
export const CONFIG_EVENT = 'config://update'

// Mirrors src-tauri/src/config.rs.
export interface AppConfig {
  pausedThresholdMs: number
  alertNeedsInput: boolean
  alertDied: boolean
  alertFinished: boolean
  sound: boolean
  muteUntilMs: number
  launchAtLogin: boolean
  showBackgroundJobs: boolean
  /** Time animations to the distance they cover, and fade chips in and out. */
  smoothStatusChanges: boolean
  /** Show the five-hour limit meter at the end of the collapsed row. */
  showUsage: boolean
  /** When the widget takes itself off screen; mirrors visibility::HIDE_MODES. */
  hideWhen: string
  /** `free` to float where dragged, `notch` to flank the notch in the menu bar. */
  placement: string
  /** Display key to show the widget on, or null for the primary display. */
  preferredDisplay: string | null
  positions: Record<string, [number, number]>
}

/** Mirrors visibility::HIDE_MODES in Rust. */
export const HIDE_MODES = [
  { id: 'never', label: 'Never' },
  { id: 'noSessions', label: 'When there are no sessions' },
  { id: 'nothingActive', label: 'When nothing is waiting or working' },
] as const

/** One attached display, from the `list_displays` command. */
export interface DisplayInfo {
  key: string
  label: string
  primary: boolean
}
