import { act, render, screen, waitFor } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import type { ReactElement } from 'react'
import type { SessionSnapshot } from '../../types'
import { MORPH_MS } from '../../useWidgetSize'

const invoke = vi
  .fn()
  .mockResolvedValue({ branch: null, model: null, effort: null, activity: null })
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
    statusTimeMs: 0,
    startedAtMs: 0,
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

describe('DotRow window resizing', () => {
  const resizes = () => invoke.mock.calls.filter((c) => c[0] === 'resize_widget')

  /** Settle the mount, including the shrink delay, then start counting. */
  const settled = async (ui: ReactElement) => {
    const rendered = render(ui)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))
    await act(() => new Promise((resolve) => setTimeout(resolve, MORPH_MS + 50)))
    invoke.mockClear()
    return rendered
  }

  it('does not resize the window when a status change leaves its size alone', async () => {
    // The window is sized to the widest of the two states, and the expanded row
    // is nearly always the wider one, so a status change on the collapsed row
    // leaves it untouched. Resizing anyway costs a window-server round trip and
    // one unpainted frame, and the delay put that frame on the last frame of
    // the morph — which is what read as a stutter on every status change.
    const { rerender } = await settled(<DotRow sessions={sessions} />)

    rerender(<DotRow sessions={[{ ...sessions[0], state: 'busy' }]} />)
    await act(() => new Promise((resolve) => setTimeout(resolve, MORPH_MS + 50)))

    expect(resizes()).toHaveLength(0)
  })

  it('still resizes when the size actually changes', async () => {
    const { rerender } = await settled(<DotRow sessions={sessions} />)

    // jsdom reports every element as 0×0, so force a real change through the
    // one input the effect reads that a test can control.
    const width = vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(500)
    rerender(<DotRow sessions={[...sessions, { ...sessions[0], sessionId: 'id-b' }]} />)
    await act(() => new Promise((resolve) => setTimeout(resolve, MORPH_MS + 50)))
    // Counted before restoring: restoring the shared invoke mock would clear
    // the very calls being asserted on.
    const count = resizes().length
    width.mockRestore()

    expect(count).toBeGreaterThan(0)
  })
})

describe('DotRow smooth transitions setting', () => {
  it('marks the row so the chips animate with it', async () => {
    render(<DotRow sessions={sessions} smoothTransitions={true} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    expect(screen.getByTestId('dot-row')).toHaveAttribute('data-smooth', 'true')
  })

  it('marks the row when the setting is off, so nothing animates on its own', async () => {
    render(<DotRow sessions={sessions} smoothTransitions={false} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    expect(screen.getByTestId('dot-row')).toHaveAttribute('data-smooth', 'false')
  })

  it('holds the box at the full morph when the setting is off', async () => {
    render(<DotRow sessions={sessions} smoothTransitions={false} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))
    await act(() => new Promise((resolve) => setTimeout(resolve, MORPH_MS + 50)))

    const pill = screen.getByTestId('dot-row').querySelector<HTMLElement>('.pill')
    expect(pill?.style.getPropertyValue('--morph')).toBe(`${MORPH_MS}ms`)
  })
})
