import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SessionSnapshot, Task } from '../../types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const { SessionPopover } = await import('./SessionPopover')

/** Captured once so the fixture's absolute timestamps stay a fixed age apart. */
const NOW = Date.now()

const session: SessionSnapshot = {
  pid: 7952,
  sessionId: 'id-a',
  name: 'api-service-55',
  title: null,
  cwd: '/Users/dev/Documents/Code/api-service',
  entrypoint: 'cli',
  state: 'waiting',
  detail: 'input needed',
  elapsedMs: 360_000,
  uptimeMs: 24_900_000,
  statusTimeMs: NOW - 360_000,
  startedAtMs: NOW - 24_900_000,
  background: false,
  tasks: [],
}

const runningTask = (id: string, label: string | null): Task => ({
  id,
  kind: 'shell',
  label,
  startedAtMs: NOW - 120_000,
  endedAtMs: null,
  status: 'running',
})

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

  it('heads with the title when the session has one', () => {
    render(<SessionPopover session={{ ...session, title: 'Rate limit bucket key' }} />)
    expect(screen.getByTestId('popover-name')).toHaveTextContent('Rate limit bucket key')
  })

  it('heads with the full registry name when the session has no title', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover-name')).toHaveTextContent('api-service-55')
  })

  it('shows the registry name in the fields, so a title never hides it', () => {
    render(<SessionPopover session={{ ...session, title: 'Rate limit bucket key' }} />)
    expect(screen.getByTestId('popover-session-name')).toHaveTextContent('api-service-55')
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

  it('lists the running tasks with their age', () => {
    render(
      <SessionPopover
        session={{
          ...session,
          state: 'tasking',
          detail: '2 tasks running',
          tasks: [runningTask('t1', 'npm test'), runningTask('t2', 'cargo test')],
        }}
      />,
    )
    const tasks = screen.getByTestId('popover-tasks')
    expect(tasks).toHaveTextContent('npm test')
    expect(tasks).toHaveTextContent('cargo test')
    expect(tasks).toHaveTextContent('2m')
  })

  it('names a task with no label by its id', () => {
    render(
      <SessionPopover
        session={{
          ...session,
          state: 'tasking',
          detail: '1 task running',
          tasks: [runningTask('bmd0i64ke', null)],
        }}
      />,
    )
    expect(screen.getByTestId('popover-tasks')).toHaveTextContent('bmd0i64ke')
  })

  it('leaves out finished tasks', () => {
    // They are in the snapshot for a minute after they end, so the popover
    // would otherwise keep showing a build that is over.
    render(
      <SessionPopover
        session={{
          ...session,
          state: 'idle',
          detail: null,
          tasks: [
            { ...runningTask('t1', 'npm test'), status: 'completed', endedAtMs: NOW - 1_000 },
          ],
        }}
      />,
    )
    expect(screen.queryByTestId('popover-tasks')).toBeNull()
  })

  it('keeps a running task even once it has an endedAtMs', () => {
    // The contract (types.ts) says filter on `status`, not on `endedAtMs`
    // being null — a task can carry a stale/non-null endedAtMs while still
    // `running` (e.g. a restarted watch). An endedAtMs-based filter would
    // wrongly drop this task; only the status-based filter keeps it.
    render(
      <SessionPopover
        session={{
          ...session,
          state: 'tasking',
          detail: '1 task running',
          tasks: [{ ...runningTask('t1', 'npm test'), endedAtMs: NOW - 1_000 }],
        }}
      />,
    )
    expect(screen.getByTestId('popover-tasks')).toHaveTextContent('npm test')
  })

  it('shows no tasks block for a session with none', () => {
    render(<SessionPopover session={{ ...session, tasks: [] }} />)
    expect(screen.queryByTestId('popover-tasks')).toBeNull()
  })

})

describe('SessionPopover hit-testing', () => {
  it('claims its session so the cursor can move onto it without closing it', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover')).toHaveAttribute('data-session-id', 'id-a')
  })
})

describe('SessionPopover five-hour limit', () => {
  /**
   * Built per test, and half a minute past the boundary: the label floors to
   * whole minutes, so a fixture pinned exactly on 2h41m and rendered a few
   * milliseconds later reads as 2h40m.
   */
  const usage = (severity: 'normal' | 'warn' | 'critical' = 'normal') => ({
    percent: 42,
    resetsAtMs: Date.now() + 2 * 3_600_000 + 41 * 60_000 + 30_000,
    severity,
  })

  it('spells out the figure the row only has room to draw as a bar', async () => {
    render(<SessionPopover session={session} usage={usage()} />)

    expect(await screen.findByTestId('popover-usage')).toHaveTextContent(
      '42% used · resets in 2h41m',
    )
  })

  it('marks a spent window hot, as it does a waiting session', async () => {
    render(<SessionPopover session={session} usage={usage('critical')} />)

    expect(await screen.findByTestId('popover-usage')).toHaveClass('hot')
  })

  it('omits the row when there is no figure worth trusting', async () => {
    render(<SessionPopover session={session} usage={null} />)

    await screen.findByTestId('popover-proc')
    expect(screen.queryByTestId('popover-usage')).not.toBeInTheDocument()
  })

  it('omits the row when no usage is passed at all', async () => {
    render(<SessionPopover session={session} />)

    await screen.findByTestId('popover-proc')
    expect(screen.queryByTestId('popover-usage')).not.toBeInTheDocument()
  })
})
