import { render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

const close = vi.fn()
const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn(async () => vi.fn()) }))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ close }),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}))

const { App, isSettingsRoute } = await import('./App')

describe('isSettingsRoute', () => {
  it('matches the settings fragment', () => {
    expect(isSettingsRoute('#settings')).toBe(true)
  })

  it('does not match the widget route', () => {
    expect(isSettingsRoute('')).toBe(false)
    expect(isSettingsRoute('#')).toBe(false)
    expect(isSettingsRoute('#other')).toBe(false)
  })
})

describe('App routing', () => {
  it('renders the widget by default', async () => {
    window.location.hash = ''
    invoke.mockResolvedValue([])

    render(<App />)

    await waitFor(() => expect(screen.getByTestId('dot-row')).toBeInTheDocument())
    expect(screen.queryByTestId('settings')).not.toBeInTheDocument()
  })

  it('renders the notch chips instead of the row when placement is notch', async () => {
    window.location.hash = ''
    invoke.mockImplementation(async (command: string) => {
      if (command === 'get_config') return { placement: 'notch', smoothStatusChanges: true }
      if (command === 'notch_layout') {
        return { notchLeft: 240, notchRight: 430, barHeight: 37, budget: 240 }
      }
      return []
    })

    render(<App />)

    await waitFor(() => expect(screen.getByTestId('notch-flanks')).toBeInTheDocument())
    expect(screen.queryByTestId('dot-row')).not.toBeInTheDocument()
  })

  it('renders settings on the settings route and marks the body opaque', async () => {
    window.location.hash = '#settings'
    const config = {
      hideWhen: 'noSessions',
      pausedThresholdMs: 600_000,
      alertNeedsInput: true,
      alertDied: true,
      alertFinished: false,
      sound: false,
      muteUntilMs: 0,
      launchAtLogin: false,
      showBackgroundJobs: true,
      placement: 'free',
      preferredDisplay: null,
      positions: {},
    }
    invoke.mockImplementation((cmd: string) =>
      cmd === 'list_displays' ? Promise.resolve([]) : Promise.resolve(config),
    )

    render(<App />)

    await waitFor(() => expect(screen.getByTestId('settings')).toBeInTheDocument())
    expect(document.body.classList.contains('settings-window')).toBe(true)
    expect(screen.queryByTestId('dot-row')).not.toBeInTheDocument()
  })
})
