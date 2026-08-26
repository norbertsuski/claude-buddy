import { useEffect } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useConfig } from './useConfig'
import { useSessions } from './useSessions'
import { DotRow } from './views/dotRow/DotRow'
import { SettingsPanel } from './settings/SettingsPanel'

/** The settings window loads the same bundle with this fragment. */
export const SETTINGS_ROUTE = '#settings'

export function isSettingsRoute(hash: string): boolean {
  return hash === SETTINGS_ROUTE
}

export function App() {
  const settings = isSettingsRoute(window.location.hash)

  // The settings window is an ordinary opaque window, unlike the transparent
  // widget, so it needs a background of its own.
  useEffect(() => {
    document.body.classList.toggle('settings-window', settings)
  }, [settings])

  if (settings) {
    return <SettingsPanel onClose={() => void getCurrentWindow().close()} />
  }
  return <WidgetView />
}

function WidgetView() {
  const { sessions, usage } = useSessions()
  const config = useConfig()
  // Defaulting to on while settings load keeps the widget from drawing its
  // first frames with one timing and the rest with another.
  return (
    <DotRow
      sessions={sessions}
      smoothTransitions={config?.smoothStatusChanges ?? true}
      // Gated here rather than inside the row: "turned off" and "nothing worth
      // showing" render identically, so the row needs only the one case.
      usage={config?.showUsage === false ? null : usage}
    />
  )
}
