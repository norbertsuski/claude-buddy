import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { UPDATE_EVENT, type SessionSnapshot, type Update } from './types'

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
export function useSessions(): { sessions: SessionSnapshot[]; ready: boolean } {
  const [sessions, setSessions] = useState<SessionSnapshot[]>([])
  const [ready, setReady] = useState(false)
  // A late-resolving initial fetch must not clobber a newer pushed update.
  const gotEvent = useRef(false)

  useEffect(() => {
    let disposed = false
    let stop: (() => void) | undefined

    listen<Update>(UPDATE_EVENT, (event) => {
      gotEvent.current = true
      setSessions(event.payload.sessions)
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

    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  return { sessions, ready }
}
