import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { act } from 'react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { CONFIG_EVENT, type AppConfig } from '../types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

/// Keeps the handlers the panel registers, so a test can push a settings
/// change the way the tray menu does.
const { listeners } = vi.hoisted(() => ({
  listeners: new Map<string, (event: { payload: unknown }) => void>(),
}))
vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return () => listeners.delete(name)
  },
}))

const { SettingsPanel } = await import('./SettingsPanel')

const config: AppConfig = {
  hideWhen: 'noSessions',
  hidden: false,
  keepAwake: false,
  crazy: 'off',
  alertNeedsInput: true,
  alertDied: true,
  alertFinished: false,
  alertTaskDone: false,
  sound: true,
  muteUntilMs: 0,
  launchAtLogin: false,
  showBackgroundJobs: true,
  showUsage: true,
  placement: 'free',
  preferredDisplay: null,
  positions: {},
}

const displays = [
  { key: 'Built-in@1470x956', label: 'Built-in (1470×956)', primary: true },
  { key: 'Studio@3840x2160', label: 'Studio (3840×2160)', primary: false },
]

describe('SettingsPanel', () => {
  beforeEach(() => {
    listeners.clear()
    invoke.mockReset()
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve({ ...config })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.resolve()
    })
  })

  it('offers notch placement only where there is a notch', async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve({ ...config })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      if (cmd === 'notch_layout') {
        return Promise.resolve({ notchLeft: 240, notchRight: 430, barHeight: 37, budget: 240 })
      }
      return Promise.resolve()
    })
    render(<SettingsPanel onClose={vi.fn()} />)
    await waitFor(() => expect(screen.getByTestId('placement-notch')).toBeEnabled())
  })

  it('disables notch placement when no notch is attached', async () => {
    // Rust answers null with the lid shut or on a Mac without one. A command
    // resolving with nothing at all has to read the same way.
    render(<SettingsPanel onClose={vi.fn()} />)
    await waitFor(() => expect(screen.getByTestId('placement-notch')).toBeDisabled())
    expect(screen.getByText(/Needs a MacBook with a notch/)).toBeInTheDocument()
  })

  it('ignores the display picker under notch placement', async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve({ ...config, placement: 'notch' })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      if (cmd === 'notch_layout') return Promise.resolve({ notchLeft: 240, notchRight: 430, barHeight: 37, budget: 240 })
      return Promise.resolve()
    })
    render(<SettingsPanel onClose={vi.fn()} />)
    // Notch placement derives its display, so the choice would be silently
    // ignored rather than merely unused.
    await waitFor(() => expect(screen.getByLabelText('Show on display')).toBeDisabled())
  })

  it('loads current settings into the form', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    await waitFor(() => expect(screen.getByLabelText('when a session needs input')).toBeChecked())
    expect(screen.getByLabelText('when a session finishes its turn')).not.toBeChecked()
  })

  it('silences every alert when the sound goes off', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    await waitFor(() => expect(screen.getByLabelText('Play a sound')).toBeChecked())

    await userEvent.click(screen.getByLabelText('Play a sound'))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({
          sound: false,
          alertNeedsInput: false,
          alertDied: false,
          alertFinished: false,
        }),
      }),
    )
  })

  it('disables the alert events while the sound is off', async () => {
    invoke.mockImplementation((cmd: string) => {
      // Armed events under a silent parent: what an older config file, or a
      // hand-edited one, actually looks like.
      if (cmd === 'get_config') return Promise.resolve({ ...config, sound: false })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.resolve()
    })
    render(<SettingsPanel onClose={vi.fn()} />)

    // Disabled *and* unchecked: delivery ignores them while the sound is off,
    // so the form must not show them armed.
    await waitFor(() =>
      expect(screen.getByLabelText('when a session needs input')).toBeDisabled(),
    )
    for (const label of [
      'when a session needs input',
      'when a session dies',
      'when a session finishes its turn',
    ]) {
      expect(screen.getByLabelText(label)).toBeDisabled()
      expect(screen.getByLabelText(label)).not.toBeChecked()
    }
  })

  it('restores the default alerts when the sound comes back on', async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') {
        return Promise.resolve({
          ...config,
          sound: false,
          alertNeedsInput: false,
          alertDied: false,
          alertFinished: false,
        })
      }
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.resolve()
    })
    render(<SettingsPanel onClose={vi.fn()} />)
    await waitFor(() => expect(screen.getByLabelText('Play a sound')).not.toBeChecked())

    await userEvent.click(screen.getByLabelText('Play a sound'))

    // Switching the group on and getting nothing would read as a broken toggle.
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({
          sound: true,
          alertNeedsInput: true,
          alertDied: true,
          alertFinished: false,
        }),
      }),
    )
  })

  it('turns the 5h meter off', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    const box = await screen.findByLabelText('Show the 5h limit at the end of the row')
    expect(box).toBeChecked()

    await userEvent.click(box)

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ showUsage: false }),
      }),
    )
  })

  it('turns crazy mode on and off', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    const box = await screen.findByLabelText('Crazy mode')
    expect(box).not.toBeChecked()

    await userEvent.click(box)

    // A checkbox over a string field: the levels stay open for `blaze` and
    // `inferno` without anyone's settings file needing to change.
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ crazy: 'ember' }),
      }),
    )
  })

  it('reads any level other than off as on', async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve({ ...config, crazy: 'ember' })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.resolve()
    })
    render(<SettingsPanel onClose={vi.fn()} />)
    const box = await screen.findByLabelText('Crazy mode')
    expect(box).toBeChecked()

    await userEvent.click(box)

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ crazy: 'off' }),
      }),
    )
  })

  it('toggles the finished alert', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    const box = await screen.findByLabelText('when a session finishes its turn')
    expect(box).not.toBeChecked()

    await userEvent.click(box)

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ alertFinished: true }),
      }),
    )
  })

  it('follows a change made from the tray menu', async () => {
    // The menu writes the same file. Without this the form would keep showing
    // the old value and carry it back over the top on its next save.
    render(<SettingsPanel onClose={vi.fn()} />)
    const box = await screen.findByLabelText('Show background jobs and subagents')
    expect(box).toBeChecked()

    act(() => {
      listeners.get(CONFIG_EVENT)?.({ payload: { ...config, showBackgroundJobs: false } })
    })

    expect(box).not.toBeChecked()
  })

  it('keeps hidden untouched when the form saves', async () => {
    // Nothing in the form draws it, but `set_config` takes the whole object:
    // dropping the field would put the widget back on screen the next time
    // anyone changed a setting.
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve({ ...config, hidden: true })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.resolve()
    })
    render(<SettingsPanel onClose={vi.fn()} />)

    await userEvent.click(await screen.findByLabelText('Launch at login'))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ hidden: true }),
      }),
    )
  })

  it('has no paused-after control', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    await screen.findByTestId('settings')
    expect(screen.queryByLabelText('Paused after (minutes)')).toBeNull()
  })

  it('has no view mode control', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    await screen.findByTestId('settings')
    expect(screen.queryByLabelText('View mode')).toBeNull()
  })

  it('changes when the widget hides', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    const select = await screen.findByLabelText('Hide the widget')
    await userEvent.selectOptions(select, 'nothingActive')

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ hideWhen: 'nothingActive' }),
      }),
    )
  })

  it('surfaces a rejected save instead of silently dropping it', async () => {
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve({ ...config })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.reject(new Error('unknown hide mode: sometimes'))
    })
    render(<SettingsPanel onClose={vi.fn()} />)

    // Any save will do: what is under test is that a rejection from Rust
    // reaches the form rather than leaving the change looking accepted.
    await userEvent.click(await screen.findByLabelText('Play a sound'))

    await waitFor(() =>
      expect(screen.getByTestId('settings-error')).toHaveTextContent('unknown hide mode'),
    )
  })

  it('closes on request', async () => {
    const onClose = vi.fn()
    render(<SettingsPanel onClose={onClose} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Done' }))

    expect(onClose).toHaveBeenCalled()
  })
})

describe('SettingsPanel display picker', () => {
  it('lists every attached display plus a primary default', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    await waitFor(() => expect(screen.getByLabelText('Show on display')).toBeInTheDocument())
    expect(screen.getByRole('option', { name: 'Primary display' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: /Studio \(3840×2160\)/ })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: /Built-in .* primary/ })).toBeInTheDocument()
  })

  it('saves the chosen display', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    const picker = await screen.findByLabelText('Show on display')

    await userEvent.selectOptions(picker, 'Studio@3840x2160')

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ preferredDisplay: 'Studio@3840x2160' }),
      }),
    )
  })

  it('saves null when returning to the primary default', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    const picker = await screen.findByLabelText('Show on display')

    await userEvent.selectOptions(picker, 'Studio@3840x2160')
    await userEvent.selectOptions(picker, '')

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ preferredDisplay: null }),
      }),
    )
  })
})
