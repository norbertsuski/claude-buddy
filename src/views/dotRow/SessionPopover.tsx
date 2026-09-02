import { useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { SessionSnapshot, Usage } from '../../types'
import { SessionFields, useNow, useSessionDetail, type FieldName } from './SessionFields'
import './dotRow.css'

/**
 * Everything there is to say. The popover is the surface with room for it —
 * the notch's row detail asks for the subset its own row cannot already say.
 */
const ALL_FIELDS: FieldName[] = [
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
  const [error, setError] = useState<string | null>(null)
  const detail = useSessionDetail(session)
  const now = useNow()

  const raise = () => {
    setError(null)
    invoke<string>('raise_session', { pid: session.pid }).catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    )
  }

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
          {session.title ?? session.name}
        </span>
      </div>
      <dl className="popover-fields">
        <SessionFields
          session={session}
          detail={detail}
          now={now}
          fields={ALL_FIELDS}
          usage={usage}
        />
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
