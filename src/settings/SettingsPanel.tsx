import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { HIDE_MODES, type AppConfig, type DisplayInfo } from '../types'
import './settings.css'

export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [displays, setDisplays] = useState<DisplayInfo[]>([])
  // Whether this Mac has a notch to place against. Rust answers with null when
  // it has not, which is also the answer when the lid is shut.
  const [hasNotch, setHasNotch] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    invoke<AppConfig>('get_config')
      .then(setConfig)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
    invoke<DisplayInfo[]>('list_displays')
      // A malformed response must not blank the whole form.
      .then((list) => setDisplays(Array.isArray(list) ? list : []))
      .catch(() => setDisplays([]))
    invoke<unknown>('notch_layout')
      // Boolean, not `!== null`: Rust sends null for no notch, but a command
      // that resolves with nothing at all must read the same way.
      .then((layout) => setHasNotch(Boolean(layout)))
      // No notch is the safe reading: the control stays disabled rather than
      // offering a placement the widget cannot take up.
      .catch(() => setHasNotch(false))
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
      <label htmlFor="hide-when">Hide the widget</label>
      <select
        id="hide-when"
        value={config.hideWhen}
        onChange={(e) => update({ hideWhen: e.target.value })}
      >
        {HIDE_MODES.map((mode) => (
          <option key={mode.id} value={mode.id}>
            {mode.label}
          </option>
        ))}
      </select>

      <label>
        <input
          type="checkbox"
          checked={config.placement === 'notch'}
          disabled={!hasNotch}
          data-testid="placement-notch"
          onChange={(e) => update({ placement: e.target.checked ? 'notch' : 'free' })}
        />
        Sit in the menu bar beside the notch
        {!hasNotch && ' — needs a MacBook with a notch'}
      </label>

      <label htmlFor="display">Show on display</label>
      <select
        id="display"
        // Notch placement derives its display from where the notch is, so the
        // choice would be silently ignored rather than merely unused.
        disabled={config.placement === 'notch'}
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
          checked={config.smoothStatusChanges}
          onChange={(e) => update({ smoothStatusChanges: e.target.checked })}
        />
        Smooth transitions when a status changes
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.showUsage}
          onChange={(e) => update({ showUsage: e.target.checked })}
        />
        Show the 5h limit at the end of the row
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
