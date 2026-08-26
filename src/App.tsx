import { useEffect } from 'react'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useConfig } from './useConfig'
import { useSessions } from './useSessions'
import { DotRow } from './views/dotRow/DotRow'
import { NotchFlanks } from './views/dotRow/NotchFlanks'
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

  // Placement is the one setting that cannot be guessed while the read is in
  // flight. Every other default is a matter of degree, but drawing the pill and
  // then replacing it with two chips in the menu bar is a visible wrong answer,
  // and Rust has already sized and positioned the window for one of the two.
  if (config === null) return null

  // Notch placement replaces the row rather than restyling it: the chips are
  // two separate boxes in a 37pt bar, with their own scale, their own sizing,
  // and two hover rects instead of one.
  if (config.placement === 'notch') {
    return (
      <NotchFlanks sessions={sessions} usage={config.showUsage === false ? null : usage} />
    )
  }

  return (
    <DotRow
      sessions={sessions}
      smoothTransitions={config.smoothStatusChanges}
      // Gated here rather than inside the row: "turned off" and "nothing worth
      // showing" render identically, so the row needs only the one case.
      usage={config.showUsage === false ? null : usage}
    />
  )
}
