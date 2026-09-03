import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CONFIG_EVENT, type AppConfig } from '@buddy/ui'

/**
 * Settings, for the parts of the widget that draw themselves differently
 * depending on them.
 *
 * Most settings are applied in Rust and reach the widget as a different
 * snapshot; these are the ones that are purely about presentation and so have
 * nowhere else to be applied. `null` until the first read lands — callers fall
 * back to their own defaults rather than rendering nothing, since a widget that
 * waits for disk before drawing is a widget that flashes empty on launch.
 */
export function useConfig(): AppConfig | null {
  const [config, setConfig] = useState<AppConfig | null>(null)
  // A late-resolving initial read must not clobber a newer pushed change.
  const gotEvent = useRef(false)

  useEffect(() => {
    let disposed = false
    let stop: (() => void) | undefined

    listen<AppConfig>(CONFIG_EVENT, (event) => {
      gotEvent.current = true
      setConfig(event.payload)
    }).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })

    invoke<AppConfig>('get_config')
      .then((current) => {
        if (disposed || gotEvent.current) return
        setConfig(current)
      })
      .catch(() => {
        // Defaults are good enough; settings must not be able to break the
        // widget by failing to load.
      })

    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  return config
}
