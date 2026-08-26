import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { formatCountdown, formatElapsed } from '../../format'
import type { SessionSnapshot, TranscriptDetail, Usage } from '../../types'
import './dotRow.css'

const EMPTY: TranscriptDetail = { branch: null, model: null, effort: null, activity: null }

/** How often the popover recomputes its ages. */
const TICK_MS = 1000

/**
 * Wall-clock now, refreshed on an interval.
 *
 * The watcher deliberately does not re-emit for the passage of time — its
 * change fingerprint ignores clock-derived fields so the row does not re-render
 * twice a second. That means `elapsedMs` on a snapshot is the age at the moment
 * state last changed, which for anything sitting still is wrong and stays
 * wrong. The clock therefore lives here, where the value is displayed, and only
 * the open popover re-renders.
 */
function useNow(): number {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), TICK_MS)
    return () => clearInterval(timer)
  }, [])
  return now
}

function dash(value: string | null | undefined): string {
  return value && value.length > 0 ? value : '—'
}

export function SessionPopover({
  session,
  usage = null,
}: {
  session: SessionSnapshot
  /**
   * Five-hour limit usage. Not a property of this session — it is the whole
   * account's — but the row's meter has room for a bar and a countdown and
   * nothing else, so the figure behind it belongs on the one surface that can
   * carry it. `null` whenever the meter is absent, for all the same reasons.
   */
  usage?: Usage | null
}) {
  const [detail, setDetail] = useState<TranscriptDetail>(EMPTY)
  const [error, setError] = useState<string | null>(null)
  const now = useNow()

  // Transcript fields are fetched per hover rather than for every session on
  // every tick: reading them eagerly would tail a file per session twice a
  // second for data the user is usually not looking at.
  useEffect(() => {
    let live = true
    invoke<TranscriptDetail>('session_detail', {
      cwd: session.cwd,
      sessionId: session.sessionId,
    })
      .then((result) => live && setDetail(result))
      .catch(() => live && setDetail(EMPTY))
    return () => {
      live = false
    }
  }, [session.cwd, session.sessionId])

  const raise = () => {
    setError(null)
    invoke<string>('raise_session', { pid: session.pid }).catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    )
  }

  const elapsedMs = Math.max(0, now - session.statusTimeMs)
  const uptimeMs = Math.max(0, now - session.startedAtMs)
  const stateLine = `${session.detail ?? session.state} · ${formatElapsed(elapsedMs)}`
  const modelLine = detail.model
    ? `${detail.model}${detail.effort ? ` · ${detail.effort}` : ''}`
    : '—'

  return (
    <div
      className="popover"
      data-testid="popover"
      // Hit-testing keys off this attribute, so the popover must claim the same
      // session as its entry — otherwise moving onto it reads as "no session
      // hovered" and it closes under the cursor before it can be clicked.
      data-session-id={session.sessionId}
      onClick={raise}
    >
      <div className="popover-head">
        <span className={`dot dot-${session.state}`} />
        <span className="popover-title" data-testid="popover-name">
          {session.name}
        </span>
      </div>
      <dl className="popover-fields">
        <dt>state</dt>
        <dd className={session.state === 'waiting' ? 'hot' : undefined} data-testid="popover-state">
          {stateLine}
        </dd>
        <dt>doing</dt>
        <dd className="popover-activity" data-testid="popover-activity">
          {dash(detail.activity)}
        </dd>
        <dt>cwd</dt>
        <dd data-testid="popover-cwd">{session.cwd}</dd>
        <dt>branch</dt>
        <dd data-testid="popover-branch">{dash(detail.branch)}</dd>
        <dt>model</dt>
        <dd data-testid="popover-model">{modelLine}</dd>
        <dt>proc</dt>
        <dd data-testid="popover-proc">
          {session.entrypoint} · pid {session.pid} · {formatElapsed(uptimeMs)}
        </dd>
        {usage !== null && (
          <>
            <dt>5h limit</dt>
            <dd
              className={usage.severity === 'critical' ? 'hot' : undefined}
              data-testid="popover-usage"
            >
              {usage.percent}% used · resets in {formatCountdown(usage.resetsAtMs - now)}
            </dd>
          </>
        )}
      </dl>
      {error === null ? (
        <div className="popover-foot">click → raise this window</div>
      ) : (
        <div className="popover-foot popover-foot-error" data-testid="popover-error">
          {error}
        </div>
      )}
    </div>
  )
}
