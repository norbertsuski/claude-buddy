import { render, screen } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SessionSnapshot, Task } from '../../types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const { RowDetail } = await import('./RowDetail')

/** Captured once so the fixture's absolute timestamps stay a fixed age apart. */
const NOW = Date.now()

const session: SessionSnapshot = {
  pid: 7952,
  sessionId: 'id-a',
  name: 'api-service-55',
  title: 'Rate limit bucket key',
  cwd: '/Users/dev/Documents/Code/api-service',
  entrypoint: 'cli',
  state: 'tasking',
  detail: 'Run the suite',
  elapsedMs: 360_000,
  uptimeMs: 24_900_000,
  statusTimeMs: NOW - 360_000,
  startedAtMs: NOW - 24_900_000,
  background: false,
  tasks: [],
}

const task = (over: Partial<Task>): Task => ({
  id: 'b1',
  kind: 'shell',
  label: 'Run the suite',
  startedAtMs: NOW - 120_000,
  endedAtMs: null,
  status: 'running',
  ...over,
})

describe('RowDetail', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockResolvedValue({
      branch: 'feat/rate-limiting',
      model: 'claude-opus-5',
      effort: 'xhigh',
      activity: 'Bash',
    })
  })

  it('carries the popover fields the row cannot already say', async () => {
    render(<RowDetail session={session} />)

    expect(await screen.findByTestId('popover-activity')).toHaveTextContent('Bash')
    expect(screen.getByTestId('popover-session-name')).toHaveTextContent('api-service-55')
    expect(screen.getByTestId('popover-cwd')).toHaveTextContent(
      '/Users/dev/Documents/Code/api-service',
    )
    expect(screen.getByTestId('popover-branch')).toHaveTextContent('feat/rate-limiting')
    expect(screen.getByTestId('popover-model')).toHaveTextContent('claude-opus-5 · xhigh')
    expect(screen.getByTestId('popover-proc')).toHaveTextContent('cli · pid 7952')
  })

  it('leaves out what the row and the footer already say', async () => {
    // The head is the row's own name, `state` is its status and elapsed
    // columns, and the 5h limit is the list's footer row.
    render(<RowDetail session={session} />)
    await screen.findByTestId('popover-branch')

    expect(screen.queryByTestId('popover-name')).not.toBeInTheDocument()
    expect(screen.queryByTestId('popover-state')).not.toBeInTheDocument()
    expect(screen.queryByTestId('popover-usage')).not.toBeInTheDocument()
  })

  it('lists the running tasks with kind, name and age', async () => {
    render(
      <RowDetail
        session={{
          ...session,
          tasks: [task({}), task({ id: 'a1', kind: 'subagent', label: 'B3 unapplied field guard' })],
        }}
      />,
    )

    const tasks = await screen.findByTestId('popover-tasks')
    expect(tasks).toHaveTextContent('shell')
    expect(tasks).toHaveTextContent('Run the suite')
    expect(tasks).toHaveTextContent('agent')
    expect(tasks).toHaveTextContent('B3 unapplied field guard')
    expect(tasks).toHaveTextContent('2m')
  })

  it('leaves out tasks that have finished', async () => {
    // Finished tasks stay in a snapshot for a minute so the alert diff can see
    // them end. The detail is about what is happening now.
    render(
      <RowDetail
        session={{
          ...session,
          tasks: [task({ id: 'done', status: 'completed', endedAtMs: NOW })],
        }}
      />,
    )

    await screen.findByTestId('popover-branch')
    expect(screen.queryByTestId('popover-tasks')).not.toBeInTheDocument()
  })

  it('omits the tasks field entirely when nothing is running', async () => {
    render(<RowDetail session={session} />)
    await screen.findByTestId('popover-branch')
    expect(screen.queryByTestId('popover-tasks')).not.toBeInTheDocument()
  })

  it('says the row can be clicked', async () => {
    render(<RowDetail session={session} />)
    await screen.findByTestId('popover-branch')
    expect(screen.getByTestId('notch-detail-hint')).toHaveTextContent('raise this window')
  })

  it('names a task by its id when it has no label', async () => {
    render(<RowDetail session={{ ...session, tasks: [task({ label: null })] }} />)
    expect(await screen.findByTestId('popover-tasks')).toHaveTextContent('b1')
  })
})
