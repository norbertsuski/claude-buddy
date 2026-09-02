// Mirrors src-tauri/src/watcher/state.rs and alerts.rs. Rust serializes
// camelCase; these names must match exactly.

export type SessionState = 'waiting' | 'busy' | 'tasking' | 'idle' | 'paused' | 'dead'

/** Mirrors watcher::tasks::TaskKind. */
export type TaskKind = 'shell' | 'watch' | 'subagent' | 'job'

/** Mirrors watcher::tasks::TaskStatus. */
export type TaskStatus = 'running' | 'completed' | 'failed' | 'killed' | 'stopped'

/**
 * One piece of background work a session is waiting on; mirrors
 * watcher::tasks::Task.
 *
 * Finished tasks stay in the snapshot for a minute after they end, so a list
 * is not the same thing as a list of running tasks — filter on `status`.
 */
export interface Task {
  id: string
  kind: TaskKind
  /** What the task is, from its notification or the call that started it. */
  label: string | null
  startedAtMs: number
  endedAtMs: number | null
  status: TaskStatus
}

export interface SessionSnapshot {
  pid: number
  sessionId: string
  /** The registry's name, which is always `<folder>-<2 chars>`. */
  name: string
  /** What the session calls itself, from its transcript. Null until titled. */
  title: string | null
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
  /** Background work this session is waiting on, running and just-finished. */
  tasks: Task[]
}

export type AlertKind = 'needsInput' | 'died' | 'finished' | 'taskDone'

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
  alertNeedsInput: boolean
  alertDied: boolean
  alertFinished: boolean
  /** Whether a background task finishing raises a notification. */
  alertTaskDone: boolean
  sound: boolean
  muteUntilMs: number
  launchAtLogin: boolean
  showBackgroundJobs: boolean
  /** Show the five-hour limit meter at the end of the collapsed row. */
  showUsage: boolean
  /** When the widget takes itself off screen; mirrors visibility::HIDE_MODES. */
  hideWhen: string
  /**
   * Put away from the tray menu's "Hide widget", which outranks `hideWhen`.
   *
   * Nothing in the form touches this, but it has to be carried: `set_config`
   * takes the whole object, so a field missing here is a field written back as
   * its default — and the widget would reappear the next time anyone changed a
   * setting.
   */
  hidden: boolean
  /**
   * Hold the display awake while a session is busy or waiting, from the tray
   * menu's "Keep screen awake". Carried for the same reason as `hidden`.
   */
  keepAwake: boolean
  /**
   * How theatrical the widget may be: `off`, or `ember`. Carried for the same
   * reason as `hidden` and `keepAwake` — `set_config` takes the whole object,
   * so a field missing here is a field written back as its default.
   */
  crazy: string
  /** `free` to float where dragged, `notch` to flank the notch in the menu bar. */
  placement: string
  /** Display key to show the widget on, or null for the primary display. */
  preferredDisplay: string | null
  positions: Record<string, [number, number]>
}

/** Mirrors config::CRAZY_LEVELS in Rust. Only levels that exist are listed. */
export const CRAZY_LEVELS = [
  { id: 'off', label: 'Off' },
  { id: 'ember', label: 'Ember — the pill catches fire' },
] as const

/** Mirrors visibility::HIDE_MODES in Rust. */
export const HIDE_MODES = [
  { id: 'never', label: 'Never' },
  { id: 'noSessions', label: 'When there are no sessions' },
  { id: 'nothingActive', label: 'When nothing is waiting, working or on a task' },
] as const

/** One attached display, from the `list_displays` command. */
export interface DisplayInfo {
  key: string
  label: string
  primary: boolean
}
