import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { formatElapsed } from '../../format'
import type { SessionSnapshot, TranscriptDetail } from '../../types'
import './dotRow.css'

const EMPTY: TranscriptDetail = { branch: null, model: null, effort: null }

function dash(value: string | null | undefined): string {
  return value && value.length > 0 ? value : '—'
}

export function SessionPopover({ session }: { session: SessionSnapshot }) {
  const [detail, setDetail] = useState<TranscriptDetail>(EMPTY)
  const [error, setError] = useState<string | null>(null)

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

  const stateLine = `${session.detail ?? session.state} · ${formatElapsed(session.elapsedMs)}`
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
        <dt>cwd</dt>
        <dd data-testid="popover-cwd">{session.cwd}</dd>
        <dt>branch</dt>
        <dd data-testid="popover-branch">{dash(detail.branch)}</dd>
        <dt>model</dt>
        <dd data-testid="popover-model">{modelLine}</dd>
        <dt>proc</dt>
        <dd data-testid="popover-proc">
          {session.entrypoint} · pid {session.pid} · {formatElapsed(session.uptimeMs)}
        </dd>
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
