import { useEffect, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { CONFIG_EVENT, HIDE_MODES, type AppConfig, type DisplayInfo } from '../types'
import './settings.css'

/**
 * What the four alert checkboxes become when the sound is switched off: all
 * off, and disabled with it. They are the events that raise a notification, and
 * the notification is the sound, so leaving one armed under a silent parent
 * would be a setting with nothing behind it.
 */
function soundOff(): Partial<AppConfig> {
  return {
    sound: false,
    alertNeedsInput: false,
    alertDied: false,
    alertFinished: false,
    alertTaskDone: false,
  }
}

/**
 * And what they become when it is switched back on: the defaults, rather than
 * the all-off state the parent just wrote. Switching the group on and getting
 * nothing would read as a broken toggle.
 */
function soundOn(): Partial<AppConfig> {
  return {
    sound: true,
    alertNeedsInput: true,
    alertDied: true,
    alertFinished: false,
    alertTaskDone: true,
  }
}

export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [displays, setDisplays] = useState<DisplayInfo[]>([])
  // Whether this Mac has a notch to place against. Rust answers with null when
  // it has not, which is also the answer when the lid is shut.
  const [hasNotch, setHasNotch] = useState(false)
  const [error, setError] = useState<string | null>(null)
  // Saves this form has started and not yet heard back about. The tray menu
  // writes the same file — muting, hiding the widget, the background-jobs tick
  // — and the form has to follow those, or its next save would carry a stale
  // copy of them back over the top. Its own writes come back as the same event,
  // so they are ignored while one is outstanding: without that, two quick
  // clicks let the first save's echo land after the second and undo it.
  const saving = useRef(0)

  useEffect(() => {
    let stop: (() => void) | undefined
    let disposed = false

    listen<AppConfig>(CONFIG_EVENT, (event) => {
      if (saving.current > 0) return
      setConfig(event.payload)
    }).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })

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

    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  // Save on every change: there is no Apply button, so a rejected value must be
  // reported rather than left looking accepted.
  const update = (patch: Partial<AppConfig>) => {
    if (config === null) return
    const next = { ...config, ...patch }
    setConfig(next)
    setError(null)
    saving.current += 1
    invoke('set_config', { config: next })
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
      .finally(() => {
        saving.current -= 1
      })
  }

  if (config === null) {
    return <div className="settings">loading…</div>
  }

  return (
    <div className="settings" data-testid="settings">
      {/* A two-column grid: labels flush right against the controls they name,
          controls flush left in a column of their own. That alignment is the
          strongest single signal that a window belongs to the system rather
          than to a web page, and it is what every AppKit preferences pane has
          looked like for twenty years. The trailing colons are drawn by CSS so
          the accessible name of each control stays the label itself. */}
      <div className="field">
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
      </div>

      <div className="field">
        <label htmlFor="display">Show on display</label>
        <select
          id="display"
          // Notch placement derives its display from where the notch is, so the
          // choice would be silently ignored rather than merely unused.
          disabled={config.placement === 'notch'}
          value={config.preferredDisplay ?? ''}
          onChange={(e) =>
            update({ preferredDisplay: e.target.value === '' ? null : e.target.value })
          }
        >
          <option value="">Primary display</option>
          {displays.map((display) => (
            <option key={display.key} value={display.key}>
              {display.label}
              {display.primary ? ' — primary' : ''}
            </option>
          ))}
        </select>
      </div>

      <div className="field">
        <span className="field-name">Widget</span>
        <div className="checks">
          <label>
            <input
              type="checkbox"
              checked={config.placement === 'notch'}
              disabled={!hasNotch}
              data-testid="placement-notch"
              onChange={(e) => update({ placement: e.target.checked ? 'notch' : 'free' })}
            />
            Sit in the menu bar beside the notch
          </label>
          {!hasNotch && <p className="hint">Needs a MacBook with a notch.</p>}

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

          {/* A checkbox rather than a two-item list, for the same reason notch
              placement is one: `crazy` is a string so later levels can be added
              without migrating anyone's settings file, but with only `off` and
              `ember` in existence a popup button is a checkbox in costume. */}
          <label>
            <input
              type="checkbox"
              checked={config.crazy !== 'off'}
              onChange={(e) => update({ crazy: e.target.checked ? 'ember' : 'off' })}
            />
            Crazy mode
          </label>
          <p className="hint">
            The pill catches fire, shakes and fractures as sessions work, wait and run the
            limit down.
          </p>
        </div>
      </div>

      <div className="field">
        <span className="field-name">Alerts</span>
        <div className="checks">
          <label>
            <input
              type="checkbox"
              checked={config.sound}
              onChange={(e) => update(e.target.checked ? soundOn() : soundOff())}
            />
            Play a sound
          </label>

          {/* Each event reads as off while the sound is off, whatever the file
              says. The parent zeroes them on its way off, but a config
              hand-edited — or written by a version that had no parent here —
              can still arrive with one armed, and delivery already ignores it:
              `notify::should_deliver` gates on the sound. The form has to say
              the same thing. */}
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
                checked={config.sound && config.alertTaskDone}
                disabled={!config.sound}
                onChange={(e) => update({ alertTaskDone: e.target.checked })}
              />
              when a background task finishes
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
        </div>
      </div>

      <div className="field">
        <span className="field-name">General</span>
        <div className="checks">
          <label>
            <input
              type="checkbox"
              checked={config.launchAtLogin}
              onChange={(e) => update({ launchAtLogin: e.target.checked })}
            />
            Launch at login
          </label>
        </div>
      </div>

      {error !== null && (
        <p className="settings-error" data-testid="settings-error">
          {error}
        </p>
      )}

      <div className="settings-foot">
        <button type="button" onClick={onClose}>
          Done
        </button>
      </div>
    </div>
  )
}
