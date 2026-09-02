import { Fragment, useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { formatCountdown, formatElapsed } from '../../format'
import type { SessionSnapshot, Task, TaskKind, TranscriptDetail, Usage } from '../../types'

const EMPTY: TranscriptDetail = { branch: null, model: null, effort: null, activity: null }

/**
 * How each kind of task is introduced.
 *
 * A word rather than an icon: the block is a list of lines of text, and one
 * glyph in a column of prose reads as a bullet rather than as a category.
 */
const TASK_KIND_LABEL: Record<TaskKind, string> = {
  shell: 'shell',
  watch: 'watch',
  subagent: 'agent',
  job: 'job',
}

/** How often a surface showing an age recomputes it. */
const TICK_MS = 1000

/**
 * Wall-clock now, refreshed on an interval.
 *
 * The watcher deliberately does not re-emit for the passage of time — its
 * change fingerprint ignores clock-derived fields so the row does not re-render
 * twice a second. That means `elapsedMs` on a snapshot is the age at the moment
 * state last changed, which for anything sitting still is wrong and stays
 * wrong. The clock therefore lives here, where the value is displayed, and only
 * the surface that is open re-renders.
 */
export function useNow(): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), TICK_MS)
    return () => clearInterval(timer)
  }, [])
  return now
}

/**
 * The transcript-derived fields for one session, fetched while it is hovered.
 *
 * Fetched per hover rather than for every session on every tick: reading these
 * eagerly would tail a file per session twice a second for data the user is
 * usually not looking at. That is also why neither surface can show them for
 * every row at once.
 */
export function useSessionDetail(session: SessionSnapshot): TranscriptDetail {
  const [detail, setDetail] = useState<TranscriptDetail>(EMPTY)

  useEffect(() => {
    let live = true
    setDetail(EMPTY)
    invoke<TranscriptDetail>('session_detail', {
      cwd: session.cwd,
      sessionId: session.sessionId,
    })
      .then((result) => {
        if (live) setDetail(result)
      })
      .catch(() => {
        // Whatever surface this is, the row above it still says the essentials.
        if (live) setDetail(EMPTY)
      })
    return () => {
      live = false
    }
  }, [session.cwd, session.sessionId])

  return detail
}

/** Which fields a surface asks for. Rendered in the canonical order below. */
export type FieldName =
  | 'state'
  | 'doing'
  | 'tasks'
  | 'session'
  | 'cwd'
  | 'branch'
  | 'model'
  | 'proc'
  | 'usage'

/**
 * Every field, in the order they are drawn.
 *
 * The order is the component's, not the caller's: the two surfaces should read
 * the same way down the page, and a caller that could reorder them would
 * eventually make them differ for no reason anybody chose.
 */
const ORDER: FieldName[] = [
  'state',
  'doing',
  'tasks',
  'session',
  'cwd',
  'branch',
  'model',
  'proc',
  'usage',
]

function dash(value: string | null | undefined): string {
  return value && value.length > 0 ? value : '—'
}

interface Props {
  session: SessionSnapshot
  detail: TranscriptDetail
  /** Wall-clock now, from `useNow` in whichever surface is drawing this. */
  now: number
  /** Which fields to draw. Anything not named is omitted entirely. */
  fields: FieldName[]
  /**
   * Five-hour limit usage, for the `usage` field. Not a property of this
   * session — it is the whole account's — but the free-mode row's meter has
   * room for a bar and a countdown and nothing else, so the figure behind it
   * belongs on a surface that can carry it. `null` whenever the meter is
   * absent, for all the same reasons.
   */
  usage?: Usage | null
}

/**
 * The `dt`/`dd` pairs describing one session, shared by both surfaces.
 *
 * The popover asks for all of them; the notch row's detail asks for the seven
 * its own row cannot already say. Only the type scale differs, and that is the
 * containing `dl`'s to set.
 *
 * The `data-testid`s keep their `popover-` prefix on both surfaces: they name
 * the field rather than the surface that happens to be drawing it.
 */
export function SessionFields({ session, detail, now, fields, usage = null }: Props) {
  const wanted = new Set(fields)
  const elapsedMs = Math.max(0, now - session.statusTimeMs)
  const uptimeMs = Math.max(0, now - session.startedAtMs)
  // Finished tasks stay in the snapshot for a minute so the alert diff can see
  // them end. A field about what is happening now shows only what is running.
  const running = session.tasks.filter((t) => t.status === 'running')
  const modelLine = detail.model
    ? `${detail.model}${detail.effort ? ` · ${detail.effort}` : ''}`
    : '—'

  const field = (name: FieldName): React.ReactNode => {
    if (!wanted.has(name)) return null
    switch (name) {
      case 'state':
        return (
          <>
            <dt>state</dt>
            <dd
              className={session.state === 'waiting' ? 'hot' : undefined}
              data-testid="popover-state"
            >
              {`${session.detail ?? session.state} · ${formatElapsed(elapsedMs)}`}
            </dd>
          </>
        )
      case 'doing':
        return (
          <>
            <dt>doing</dt>
            <dd className="popover-activity" data-testid="popover-activity">
              {dash(detail.activity)}
            </dd>
          </>
        )
      case 'tasks':
        // Absent rather than dashed: a session with nothing running has no
        // task story to tell, and an empty field is a line of noise on a
        // surface that is already dense.
        if (running.length === 0) return null
        return (
          <>
            <dt>tasks</dt>
            <dd className="popover-tasks-field" data-testid="popover-tasks">
              <ul className="popover-tasks">
                {running.map((task: Task) => (
                  <li key={task.id}>
                    <span className="popover-task-kind">{TASK_KIND_LABEL[task.kind]}</span>
                    {/* The name is the part that can be arbitrarily long — a
                        whole shell command — and the age is the part you came
                        for. Only the name shrinks, so a long one is clipped
                        rather than pushing the age off the line. */}
                    <span className="popover-task-label">{task.label ?? task.id}</span>
                    <span className="popover-task-age">
                      {formatElapsed(now - task.startedAtMs)}
                    </span>
                  </li>
                ))}
              </ul>
            </dd>
          </>
        )
      case 'session':
        return (
          <>
            <dt>session</dt>
            <dd data-testid="popover-session-name">{session.name}</dd>
          </>
        )
      case 'cwd':
        return (
          <>
            <dt>cwd</dt>
            <dd data-testid="popover-cwd">{session.cwd}</dd>
          </>
        )
      case 'branch':
        return (
          <>
            <dt>branch</dt>
            <dd data-testid="popover-branch">{dash(detail.branch)}</dd>
          </>
        )
      case 'model':
        return (
          <>
            <dt>model</dt>
            <dd data-testid="popover-model">{modelLine}</dd>
          </>
        )
      case 'proc':
        return (
          <>
            <dt>proc</dt>
            <dd data-testid="popover-proc">
              {session.entrypoint} · pid {session.pid} · {formatElapsed(uptimeMs)}
            </dd>
          </>
        )
      case 'usage':
        if (usage === null) return null
        return (
          <>
            <dt>5h limit</dt>
            <dd
              className={usage.severity === 'critical' ? 'hot' : undefined}
              data-testid="popover-usage"
            >
              {usage.percent}% used · resets in {formatCountdown(usage.resetsAtMs - now)}
            </dd>
          </>
        )
    }
  }

  return (
    <>
      {/* Keyed fragments rather than wrapper elements: the `dl` is a grid and
          its `dt`/`dd` must be its own children, so anything in between would
          have to be `display: contents` to stay out of the layout. */}
      {ORDER.map((name) => (
        <Fragment key={name}>{field(name)}</Fragment>
      ))}
    </>
  )
}
