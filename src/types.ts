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

export interface Update {
  sessions: SessionSnapshot[]
  alerts: Alert[]
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

// Mirrors src-tauri/src/config.rs.
export interface AppConfig {
  viewMode: string
  pausedThresholdMs: number
  alertNeedsInput: boolean
  alertDied: boolean
  alertFinished: boolean
  sound: boolean
  muteUntilMs: number
  launchAtLogin: boolean
  showBackgroundJobs: boolean
  /** Display key to show the widget on, or null for the primary display. */
  preferredDisplay: string | null
  positions: Record<string, [number, number]>
}

/** `shipped: false` modes are listed but not selectable until their own plan lands. */
export const VIEW_MODES = [
  { id: 'dotRow', label: 'Dot row', shipped: true },
  { id: 'cardStack', label: 'Card stack', shipped: false },
  { id: 'characterBuddy', label: 'Character buddy', shipped: false },
  { id: 'invisible', label: 'Invisible until needed', shipped: false },
] as const

/** One attached display, from the `list_displays` command. */
export interface DisplayInfo {
  key: string
  label: string
  primary: boolean
}
