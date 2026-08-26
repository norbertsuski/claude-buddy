import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import type { SessionSnapshot, TranscriptDetail } from '../../types'
import './notchFlanks.css'

const EMPTY: TranscriptDetail = { branch: null, model: null, effort: null, activity: null }

/**
 * What the popover used to say, under the row it belongs to.
 *
 * Fetched per hovered row rather than for every row, for the reason
 * `SessionPopover` gives: reading transcript fields eagerly would tail a file
 * per session twice a second for data nobody is looking at. That is also why
 * this cannot simply be shown under every row at once.
 */
export function RowDetail({
  session,
  agents = 0,
}: {
  session: SessionSnapshot
  /** Background agents this session has running. */
  agents?: number
}) {
  const [detail, setDetail] = useState<TranscriptDetail>(EMPTY)

  useEffect(() => {
    let live = true
    setDetail(EMPTY)
    invoke<TranscriptDetail>('session_detail', {
      cwd: session.cwd,
      sessionId: session.sessionId,
    })
      .then((next) => {
        if (live) setDetail(next)
      })
      .catch(() => {
        // The row above still says everything essential; detail is a bonus.
      })
    return () => {
      live = false
    }
  }, [session.cwd, session.sessionId])

  const where = [detail.branch, detail.model].filter(Boolean).join(' · ')

  return (
    <div className="notch-detail" data-testid={`detail-${session.sessionId}`}>
      {detail.activity !== null && <div className="notch-detail-line">{detail.activity}</div>}
      {agents > 0 && (
        <div className="notch-detail-line" data-testid="agent-count">
          {agents} background {agents === 1 ? 'agent' : 'agents'}
        </div>
      )}
      {where.length > 0 && <div className="notch-detail-line">{where}</div>}
      <div className="notch-detail-line notch-detail-path">{session.cwd}</div>
    </div>
  )
}
