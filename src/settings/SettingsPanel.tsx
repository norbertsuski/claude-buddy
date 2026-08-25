import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { VIEW_MODES, type AppConfig, type DisplayInfo } from '../types'
import './settings.css'

export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [displays, setDisplays] = useState<DisplayInfo[]>([])
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    invoke<AppConfig>('get_config')
      .then(setConfig)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
    invoke<DisplayInfo[]>('list_displays')
      // A malformed response must not blank the whole form.
      .then((list) => setDisplays(Array.isArray(list) ? list : []))
      .catch(() => setDisplays([]))
  }, [])

  // Save on every change: there is no Apply button, so a rejected value must be
  // reported rather than left looking accepted.
  const update = (patch: Partial<AppConfig>) => {
    if (config === null) return
    const next = { ...config, ...patch }
    setConfig(next)
    setError(null)
    invoke('set_config', { config: next }).catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    )
  }

  if (config === null) {
    return <div className="settings">loading…</div>
  }

  return (
    <div className="settings" data-testid="settings">
      <label htmlFor="view-mode">View mode</label>
      <select
        id="view-mode"
        value={config.viewMode}
        onChange={(e) => update({ viewMode: e.target.value })}
      >
        {VIEW_MODES.map((mode) => (
          <option key={mode.id} value={mode.id} disabled={!mode.shipped}>
            {mode.label}
          </option>
        ))}
      </select>

      <label htmlFor="display">Show on display</label>
      <select
        id="display"
        value={config.preferredDisplay ?? ''}
        onChange={(e) => update({ preferredDisplay: e.target.value === '' ? null : e.target.value })}
      >
        <option value="">Primary display</option>
        {displays.map((display) => (
          <option key={display.key} value={display.key}>
            {display.label}
            {display.primary ? ' — primary' : ''}
          </option>
        ))}
      </select>

      <label htmlFor="paused-after">Paused after (minutes)</label>
      <input
        id="paused-after"
        type="number"
        min={1}
        value={Math.round(config.pausedThresholdMs / 60_000)}
        onChange={(e) => update({ pausedThresholdMs: Number(e.target.value) * 60_000 })}
      />

      <label>
        <input
          type="checkbox"
          checked={config.alertNeedsInput}
          onChange={(e) => update({ alertNeedsInput: e.target.checked })}
        />
        Alert when a session needs input
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.alertDied}
          onChange={(e) => update({ alertDied: e.target.checked })}
        />
        Alert when a session dies
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.alertFinished}
          onChange={(e) => update({ alertFinished: e.target.checked })}
        />
        Alert when a session finishes its turn
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.sound}
          onChange={(e) => update({ sound: e.target.checked })}
        />
        Play a sound
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.showBackgroundJobs}
          onChange={(e) => update({ showBackgroundJobs: e.target.checked })}
        />
        Show background jobs and subagents
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.launchAtLogin}
          onChange={(e) => update({ launchAtLogin: e.target.checked })}
        />
        Launch at login
      </label>

      {error !== null && (
        <p className="settings-error" data-testid="settings-error">
          {error}
        </p>
      )}

      <button type="button" onClick={onClose}>
        Done
      </button>
    </div>
  )
}
