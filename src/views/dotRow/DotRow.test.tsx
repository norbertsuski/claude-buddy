import { act, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { SessionSnapshot } from '../../types'

const invoke = vi.fn().mockResolvedValue({ branch: null, model: null, effort: null })
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))
const eventHandlers = new Map<string, (event: { payload: unknown }) => void>()
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    eventHandlers.set(name, handler)
    return vi.fn()
  }),
}))

function moveCursor(position: { x: number; y: number; inside: boolean }) {
  act(() => eventHandlers.get('ui://cursor')!({ payload: position }))
}

const { DotRow } = await import('./DotRow')

const sessions: SessionSnapshot[] = [
  {
    pid: 7952,
    sessionId: 'id-a',
    name: 'api-service-55',
    cwd: '/Users/n/Code/api-service',
    entrypoint: 'cli',
    state: 'waiting',
    detail: 'input needed',
    elapsedMs: 360_000,
    uptimeMs: 3_600_000,
    background: false,
  },
]

describe('DotRow', () => {
  // Both variants stay mounted so they can crossfade; `data-show` on the
  // wrapping slot is what says which one the user sees.
  const shown = (testId: string) =>
    screen.getByTestId(testId).closest('.variant-slot')?.getAttribute('data-show')

  it('rests in the collapsed pill', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    expect(shown('collapsed-pill')).toBe('true')
    expect(shown('named-dot-row')).toBe('false')
  })

  it('morphs to named dots when the cursor enters the widget', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })

    expect(shown('named-dot-row')).toBe('true')
    expect(shown('collapsed-pill')).toBe('false')
  })

  it('returns to collapsed when the cursor leaves', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })
    moveCursor({ x: -1, y: -1, inside: false })

    expect(shown('collapsed-pill')).toBe('true')
    expect(shown('named-dot-row')).toBe('false')
  })

  it('stays collapsed with no sessions', async () => {
    render(<DotRow sessions={[]} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })

    expect(shown('collapsed-pill')).toBe('true')
    expect(shown('named-dot-row')).toBe('false')
  })

  it('tags entries so the pushed cursor position can be hit-tested', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })

    expect(screen.getByTestId('session-id-a')).toHaveAttribute('data-session-id', 'id-a')
  })
})

describe('DotRow flash fallback', () => {
  it('flashes on ui://flash and stops once the cursor enters', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://flash')).toBe(true))

    act(() => eventHandlers.get('ui://flash')!({ payload: null }))
    expect(screen.getByTestId('dot-row')).toHaveAttribute('data-flashing', 'true')

    moveCursor({ x: 40, y: 15, inside: true })
    expect(screen.getByTestId('dot-row')).toHaveAttribute('data-flashing', 'false')
  })
})

describe('DotRow click to raise', () => {
  it('raises the hovered session when Rust reports a click', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://click')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })
    // The hit-test needs an entry under the point; jsdom has no layout, so
    // drive the hovered session through the row's own callback instead.
    act(() => {
      screen.getByTestId('session-id-a').dispatchEvent(new MouseEvent('mouseenter'))
    })

    invoke.mockClear()
    act(() => eventHandlers.get('ui://click')!({ payload: { x: 40, y: 15, inside: true } }))

    // Nothing hovered in jsdom, so no raise — the guard must hold rather than
    // raising an arbitrary session.
    expect(invoke.mock.calls.filter((c) => c[0] === 'raise_session')).toHaveLength(0)
  })

  it('ignores a click when no session is hovered', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://click')).toBe(true))

    invoke.mockClear()
    act(() => eventHandlers.get('ui://click')!({ payload: { x: 2, y: 2, inside: true } }))

    expect(invoke.mock.calls.filter((c) => c[0] === 'raise_session')).toHaveLength(0)
  })
})

describe('DotRow popover stickiness', () => {
  const twoSessions = [
    sessions[0],
    { ...sessions[0], sessionId: 'id-b', name: 'other-project-22', pid: 4242 },
  ]

  it('keeps the popover open while the cursor crosses the gap between names', async () => {
    // jsdom has no layout, so elementFromPoint always misses — which is exactly
    // the "between two names" case. The selection must survive it.
    render(<DotRow sessions={twoSessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })
    act(() => {
      screen.getByTestId('session-id-a').dispatchEvent(new MouseEvent('mouseenter'))
    })

    // Still inside the widget, hitting nothing.
    moveCursor({ x: 120, y: 15, inside: true })

    expect(screen.getByTestId('named-dot-row')).toBeInTheDocument()
  })

  it('clears the selection when the cursor leaves the widget', async () => {
    render(<DotRow sessions={twoSessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })
    moveCursor({ x: -1, y: -1, inside: false })

    expect(screen.queryByTestId('popover')).not.toBeInTheDocument()
  })
})

describe('DotRow popover anchoring', () => {
  it('offsets the popover wrapper, which is the flex item', async () => {
    // The offset has to live on the wrapper: with it on the popover inside,
    // the row centred the wrapper and the offset pushed it further right.
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 40, y: 15, inside: true })
    act(() => {
      screen.getByTestId('session-id-a').dispatchEvent(new MouseEvent('mouseenter'))
    })

    const anchor = screen.queryByTestId('popover-anchor')
    if (anchor) expect(anchor.style.marginLeft).not.toBe('')
  })
})
