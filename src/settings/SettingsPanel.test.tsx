import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppConfig } from '../types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const { SettingsPanel } = await import('./SettingsPanel')

const config: AppConfig = {
  hideWhen: 'noSessions',
  pausedThresholdMs: 600_000,
  alertNeedsInput: true,
  alertDied: true,
  alertFinished: false,
  sound: false,
  muteUntilMs: 0,
  launchAtLogin: false,
  showBackgroundJobs: true,
  smoothStatusChanges: true,
  preferredDisplay: null,
  positions: {},
}

const displays = [
  { key: 'Built-in@1470x956', label: 'Built-in (1470×956)', primary: true },
  { key: 'Studio@3840x2160', label: 'Studio (3840×2160)', primary: false },
]

describe('SettingsPanel', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_config') return Promise.resolve({ ...config })
      if (cmd === 'list_displays') return Promise.resolve(displays)
      return Promise.resolve()
    })
  })

  it('loads current settings into the form', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    await waitFor(() =>
      expect(screen.getByLabelText('Alert when a session needs input')).toBeChecked(),
    )
    expect(screen.getByLabelText('Paused after (minutes)')).toHaveValue(10)
  })

  it('saves a toggled alert setting', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    await waitFor(() => expect(screen.getByLabelText('Play a sound')).toBeInTheDocument())

    await userEvent.click(screen.getByLabelText('Play a sound'))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ sound: true }),
      }),
    )
  })

  it('turns smooth transitions off', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    const box = await screen.findByLabelText('Smooth transitions when a status changes')
    expect(box).toBeChecked()

    await userEvent.click(box)

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ smoothStatusChanges: false }),
      }),
    )
  })

  it('toggles the finished alert', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    const box = await screen.findByLabelText('Alert when a session finishes its turn')
    expect(box).not.toBeChecked()

    await userEvent.click(box)

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ alertFinished: true }),
      }),
    )
  })

  it('converts the paused threshold from minutes to milliseconds', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    const field = await screen.findByLabelText('Paused after (minutes)')

    await userEvent.clear(field)
    await userEvent.type(field, '25')

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ pausedThresholdMs: 1_500_000 }),
      }),
    )
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
      return Promise.reject(new Error('paused threshold must be greater than zero'))
    })
    render(<SettingsPanel onClose={vi.fn()} />)
    const field = await screen.findByLabelText('Paused after (minutes)')

    await userEvent.clear(field)
    await userEvent.type(field, '0')

    await waitFor(() =>
      expect(screen.getByTestId('settings-error')).toHaveTextContent('greater than zero'),
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
