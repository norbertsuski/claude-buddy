import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SessionSnapshot } from '../../types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const { SessionPopover } = await import('./SessionPopover')

/** Captured once so the fixture's absolute timestamps stay a fixed age apart. */
const NOW = Date.now()

const session: SessionSnapshot = {
  pid: 7952,
  sessionId: 'id-a',
  name: 'api-service-55',
  cwd: '/Users/dev/Documents/Code/api-service',
  entrypoint: 'cli',
  state: 'waiting',
  detail: 'input needed',
  elapsedMs: 360_000,
  uptimeMs: 24_900_000,
  statusTimeMs: NOW - 360_000,
  startedAtMs: NOW - 24_900_000,
  background: false,
}

describe('SessionPopover', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockResolvedValue({
      branch: 'feat/rate-limiting',
      model: 'claude-opus-5',
      effort: 'xhigh',
      activity: 'Bash',
    })
  })

  it('shows the full session name, not the shortened one', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover-name')).toHaveTextContent('api-service-55')
  })

  it('shows the waiting detail with elapsed time', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover-state')).toHaveTextContent('input needed · 6m')
  })

  it('shows the state name when there is no detail', () => {
    render(<SessionPopover session={{ ...session, state: 'busy', detail: null }} />)
    expect(screen.getByTestId('popover-state')).toHaveTextContent('busy · 6m')
  })

  it('shows the cwd', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover-cwd')).toHaveTextContent(
      '/Users/dev/Documents/Code/api-service',
    )
  })

  it('fetches and shows transcript fields', async () => {
    render(<SessionPopover session={session} />)

    await waitFor(() => {
      expect(screen.getByTestId('popover-branch')).toHaveTextContent(
        'feat/rate-limiting',
      )
    })
    expect(screen.getByTestId('popover-model')).toHaveTextContent('claude-opus-5')
    expect(screen.getByTestId('popover-model')).toHaveTextContent('xhigh')
    expect(invoke).toHaveBeenCalledWith('session_detail', {
      cwd: session.cwd,
      sessionId: session.sessionId,
    })
  })

  it('renders an em dash for transcript fields that are absent', async () => {
    invoke.mockResolvedValue({ branch: null, model: null, effort: null, activity: null })
    render(<SessionPopover session={session} />)

    await waitFor(() => expect(screen.getByTestId('popover-branch')).toHaveTextContent('—'))
    expect(screen.getByTestId('popover-model')).toHaveTextContent('—')
  })

  it('still opens when the transcript read fails', async () => {
    invoke.mockRejectedValue(new Error('unreadable'))
    render(<SessionPopover session={session} />)

    await waitFor(() => expect(screen.getByTestId('popover-branch')).toHaveTextContent('—'))
    expect(screen.getByTestId('popover-name')).toBeInTheDocument()
  })

  it('raises the session on click', async () => {
    render(<SessionPopover session={session} />)

    await userEvent.click(screen.getByTestId('popover'))

    expect(invoke).toHaveBeenCalledWith('raise_session', { pid: 7952 })
  })

  it('shows the failure in place when raising fails', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'raise_session'
        ? Promise.reject(new Error('no host application found for pid 7952'))
        : Promise.resolve({ branch: null, model: null, effort: null, activity: null }),
    )
    render(<SessionPopover session={session} />)

    await userEvent.click(screen.getByTestId('popover'))

    await waitFor(() => {
      expect(screen.getByTestId('popover-error')).toHaveTextContent('no host application')
    })
  })


  it('shows the transcript activity line', async () => {
    invoke.mockResolvedValue({
      branch: 'main',
      model: 'claude-opus-5',
      effort: 'high',
      activity: 'Bash',
    })
    render(<SessionPopover session={session} />)
    expect(await screen.findByTestId('popover-activity')).toHaveTextContent('Bash')
  })

  it('dashes the activity line when the transcript has nothing', async () => {
    invoke.mockResolvedValue({ branch: null, model: null, effort: null, activity: null })
    render(<SessionPopover session={session} />)
    expect(await screen.findByTestId('popover-activity')).toHaveTextContent('—')
  })

  it('advances elapsed and uptime as time passes, without new props', () => {
    // Regression: the watcher only re-emits when state changes, so a snapshot's
    // elapsedMs is the age at the moment state last changed. A session blocked
    // for twenty minutes reported the two seconds it took to notice.
    vi.useFakeTimers()
    const now = 1_700_000_000_000
    vi.setSystemTime(now)

    render(
      <SessionPopover
        session={{ ...session, statusTimeMs: now - 65_000, startedAtMs: now - 5 * 60_000 }}
      />,
    )
    expect(screen.getByTestId('popover-state')).toHaveTextContent('input needed · 1m')

    act(() => {
      vi.advanceTimersByTime(60_000)
    })

    expect(screen.getByTestId('popover-state')).toHaveTextContent('input needed · 2m')
    expect(screen.getByTestId('popover-proc')).toHaveTextContent('6m')
    vi.useRealTimers()
  })

})

describe('SessionPopover hit-testing', () => {
  it('claims its session so the cursor can move onto it without closing it', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover')).toHaveAttribute('data-session-id', 'id-a')
  })
})
