import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { HIDE_MODES, type AppConfig, type DisplayInfo } from '../types'
import './settings.css'

/**
 * What the three alert checkboxes become when the sound is switched off: all
 * off, and disabled with it. They are the events that raise a notification, and
 * the notification is the sound, so leaving one armed under a silent parent
 * would be a setting with nothing behind it.
 */
function soundOff(): Partial<AppConfig> {
  return { sound: false, alertNeedsInput: false, alertDied: false, alertFinished: false }
}

/**
 * And what they become when it is switched back on: the defaults, rather than
 * the all-off state the parent just wrote. Switching the group on and getting
 * nothing would read as a broken toggle.
 */
function soundOn(): Partial<AppConfig> {
  return { sound: true, alertNeedsInput: true, alertDied: true, alertFinished: false }
}

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

      <label>
        <input
          type="checkbox"
          checked={config.sound}
          onChange={(e) => update(e.target.checked ? soundOn() : soundOff())}
        />
        Play a sound
      </label>

      {/* Each event reads as off while the sound is off, whatever the file says.
          The parent zeroes them on its way off, but a config hand-edited — or
          written by a version that had no parent here — can still arrive with
          one armed, and delivery already ignores it: `notify::should_deliver`
          gates on the sound. The form has to say the same thing. */}
      <div className="settings-group">
        <label>
          <input
            type="checkbox"
            checked={config.sound && config.alertNeedsInput}
            disabled={!config.sound}
            onChange={(e) => update({ alertNeedsInput: e.target.checked })}
          />
          when a session needs input
        </label>

        <label>
          <input
            type="checkbox"
            checked={config.sound && config.alertDied}
            disabled={!config.sound}
            onChange={(e) => update({ alertDied: e.target.checked })}
          />
          when a session dies
        </label>

        <label>
          <input
            type="checkbox"
            checked={config.sound && config.alertFinished}
            disabled={!config.sound}
            onChange={(e) => update({ alertFinished: e.target.checked })}
          />
          when a session finishes its turn
        </label>
      </div>

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
