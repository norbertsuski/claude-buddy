import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  UPDATE_EVENT,
  type Alert,
  type SessionSnapshot,
  type Update,
  type Usage,
} from '@buddy/ui'

/**
 * Subscribe to watcher updates.
 *
 * The backend sends complete snapshots, already sorted and with every derived
 * field computed. This hook therefore replaces state wholesale and derives
 * nothing — no merging, no local state machine.
 *
 * It also fetches the current snapshot on mount. The watcher emits its first
 * snapshot within milliseconds of process start, well before this webview has
 * loaded, and it only re-emits when state actually changes — so a subscription
 * alone can leave the widget empty indefinitely while sessions are running.
 */
export function useSessions(): {
  sessions: SessionSnapshot[]
  usage: Usage | null
  alerts: Alert[]
  ready: boolean
} {
  const [sessions, setSessions] = useState<SessionSnapshot[]>([])
  const [usage, setUsage] = useState<Usage | null>(null)
  // Alerts describe the moment a session changed, not its current state, so
  // they are only ever set from a pushed update — there is no snapshot command
  // that could hand back a transition which has already happened.
  const [alerts, setAlerts] = useState<Alert[]>([])
  const [ready, setReady] = useState(false)
  // A late-resolving initial fetch must not clobber a newer pushed update.
  const gotEvent = useRef(false)

  useEffect(() => {
    let disposed = false
    let stop: (() => void) | undefined

    listen<Update>(UPDATE_EVENT, (event) => {
      gotEvent.current = true
      setSessions(event.payload.sessions)
      setUsage(event.payload.usage)
      setAlerts(event.payload.alerts ?? [])
      setReady(true)
    }).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })

    invoke<SessionSnapshot[]>('get_sessions')
      .then((current) => {
        if (disposed || gotEvent.current) return
        setSessions(current)
        setReady(true)
      })
      .catch(() => {
        // The widget still works off the event stream if this fails.
      })

    // Fetched separately for the same reason the sessions are: the watcher only
    // re-emits on a change, so loading during a quiet stretch would leave the
    // meter missing until something else moved.
    invoke<Usage | null>('get_usage')
      .then((current) => {
        if (disposed || gotEvent.current) return
        setUsage(current ?? null)
      })
      .catch(() => {
        // No meter, which is the same as every other case where the figure
        // cannot be trusted.
      })

    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  return { sessions, usage, alerts, ready }
}
