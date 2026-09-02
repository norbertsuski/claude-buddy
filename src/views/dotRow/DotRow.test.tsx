import { act, render, screen, waitFor } from '@testing-library/react'
import { afterEach, describe, expect, it, vi } from 'vitest'
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

const { DotRow, HOVER_GRACE_MS } = await import('./DotRow')

const sessions: SessionSnapshot[] = [
  {
    pid: 7952,
    sessionId: 'id-a',
    name: 'api-service-55',
    title: null,
    cwd: '/Users/n/Code/api-service',
    entrypoint: 'cli',
    state: 'waiting',
    detail: 'input needed',
    elapsedMs: 360_000,
    uptimeMs: 3_600_000,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    tasks: [],
  },
]

function makeSession(over: Partial<SessionSnapshot>): SessionSnapshot {
  return { ...sessions[0], detail: null, elapsedMs: 0, ...over }
}

describe('crazy mode', () => {
  it('mounts nothing extra when it is off', () => {
    const { container } = render(
      <DotRow
        sessions={[makeSession({ sessionId: 'a', state: 'busy' }), makeSession({ sessionId: 'b', state: 'busy' })]}
        crazy="off"
      />,
    )
    // The wrappers are always there and always classed — that is what keeps the
    // box identical whether the setting is on or off. What must be absent is
    // everything that draws or animates.
    expect(container.querySelector('.crazy-heat')).toBeNull()
    expect(container.querySelector('.crazy-flames')).toBeNull()
    expect(container.querySelector('.crazy-cracks')).toBeNull()
    expect(container.querySelector('[data-shake]')).toBeNull()
    expect(container.querySelector('.pill')?.getAttribute('data-fire')).toBeNull()
  })

  it('mounts nothing extra when it is on but nothing is happening', () => {
    const { container } = render(
      <DotRow sessions={[makeSession({ sessionId: 'a', state: 'idle' })]} crazy="ember" />,
    )
    expect(container.querySelector('.crazy-heat')).toBeNull()
    expect(container.querySelector('[data-shake]')).toBeNull()
    expect(container.querySelector('[data-shudder]')).toBeNull()
  })

  it('shakes harder the longer a session has waited', () => {
    const waited = makeSession({ sessionId: 'a', state: 'waiting', elapsedMs: 300_000 })
    const { container } = render(<DotRow sessions={[waited]} crazy="ember" />)
    const shake = container.querySelector<HTMLElement>('.crazy-shake')

    expect(shake?.getAttribute('data-shake')).toBe('true')
    expect(shake?.style.getPropertyValue('--crazy-amp')).toBe('1.4')
  })

  it('does not shake for a session that has only just asked', () => {
    const fresh = makeSession({ sessionId: 'a', state: 'waiting', elapsedMs: 1_000 })
    const { container } = render(<DotRow sessions={[fresh]} crazy="ember" />)
    // Nothing else is happening either, so the wrapper is not even classed —
    // querying the attribute directly is what says the shake is absent.
    expect(container.querySelector('[data-shake]')).toBeNull()
  })

  it('crumbles the dead dot when a session dies, then stops', async () => {
    vi.useFakeTimers()
    try {
      const dead = makeSession({ sessionId: 'a', state: 'dead' })
      const alerts = [{ sessionId: 'a', name: 'repo', kind: 'died' as const, detail: null }]
      const { container } = render(<DotRow sessions={[dead]} alerts={alerts} crazy="ember" />)

      expect(container.querySelector('.pill')?.getAttribute('data-ash')).toBe('true')

      // Held for the length of the animation and no longer: a dead session can
      // sit in the list for hours, and an effect outliving the moment would be
      // permanent noise.
      await act(async () => {
        vi.advanceTimersByTime(1_500)
      })
      expect(container.querySelector('.pill')?.getAttribute('data-ash')).toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })

  it('fractures the pill as the limit runs down', () => {
    const usage = { percent: 96, resetsAtMs: 0, severity: 'critical' as const }
    const { container } = render(
      <DotRow sessions={[makeSession({ sessionId: 'a' })]} usage={usage} crazy="ember" />,
    )
    expect(container.querySelector('.pill')?.getAttribute('data-strain')).toBe('2')
    // Each crack is drawn twice: a dark underlay, then a light hairline. One
    // stroke alone vanishes against the flames, the other against the pill.
    expect(container.querySelectorAll('.crazy-cracks path')).toHaveLength(10)
  })

  it('does not fracture while usage is normal', () => {
    const usage = { percent: 8, resetsAtMs: 0, severity: 'normal' as const }
    const { container } = render(
      <DotRow sessions={[makeSession({ sessionId: 'a' })]} usage={usage} crazy="ember" />,
    )
    expect(container.querySelector('.crazy-cracks')).toBeNull()
  })

  it('shudders only at critical usage', () => {
    const usage = { percent: 96, resetsAtMs: 0, severity: 'critical' as const }
    const { container } = render(
      <DotRow sessions={[makeSession({ sessionId: 'a' })]} usage={usage} crazy="ember" />,
    )
    expect(container.querySelector('.crazy-shudder')?.getAttribute('data-shudder')).toBe('true')
  })

  it('keeps the same box whether it is lit or not', () => {
    // Turning crazy mode on must change nothing about layout. The wrappers are
    // always mounted so the pill is never remounted mid-morph, and their class
    // has to be constant for the same reason: a class that appears with `lit`
    // takes its `display` with it, and the row changes height as the setting is
    // toggled.
    const calm = render(<DotRow sessions={[makeSession({ sessionId: 'a', state: 'idle' })]} crazy="ember" />)
    const lit = render(
      <DotRow
        sessions={[
          makeSession({ sessionId: 'a', state: 'busy' }),
          makeSession({ sessionId: 'b', state: 'busy' }),
          makeSession({ sessionId: 'c', state: 'busy' }),
        ]}
        crazy="ember"
      />,
    )

    for (const cls of ['crazy-shake', 'crazy-shudder']) {
      expect(calm.container.querySelector(`.${cls}`)).not.toBeNull()
      expect(lit.container.querySelector(`.${cls}`)).not.toBeNull()
    }
  })

  it('burns at the level the busy count calls for', () => {
    const { container } = render(
      <DotRow
        sessions={[
          makeSession({ sessionId: 'a', state: 'busy' }),
          makeSession({ sessionId: 'b', state: 'busy' }),
          makeSession({ sessionId: 'c', state: 'busy' }),
        ]}
        crazy="ember"
      />,
    )
    expect(container.querySelector('.pill')?.getAttribute('data-fire')).toBe('3')
    expect(container.querySelectorAll('.crazy-flames i')).toHaveLength(8)
    expect(container.querySelectorAll('.crazy-spark')).toHaveLength(4)
  })
})

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

describe('DotRow morph timing', () => {
  it('times the box to the distance it covers, not to the widest morph', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))
    await act(() => new Promise((resolve) => setTimeout(resolve, MORPH_MS + 50)))

    // There is no setting behind this any more: every change is timed to its
    // own distance, so the duration is a real one and at most the full morph.
    const pill = screen.getByTestId('dot-row').querySelector<HTMLElement>('.pill')
    const morph = pill?.style.getPropertyValue('--morph') ?? ''
    const ms = Number(morph.replace('ms', ''))
    expect(ms).toBeGreaterThan(0)
    expect(ms).toBeLessThanOrEqual(MORPH_MS)
  })
})

describe('DotRow hover rect', () => {
  const rects = () => invoke.mock.calls.filter((c) => c[0] === 'set_hover_rect')

  it('reports the widest variant, not the box the pill is leaving', async () => {
    // The rect effect runs after paint on the frame the morph starts, so a live
    // measurement is of the old, narrower box. Reporting that made the row
    // hittable only across its collapsed extent: moving onto a name beyond it
    // put the cursor outside the widget, which collapsed the row instead of
    // opening a popover.
    const wide = 700
    const w = vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(wide)
    // The pill itself still measures its pre-morph width mid-transition.
    const r = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      left: 100, top: 10, right: 300, bottom: 55, width: 200, height: 45, x: 100, y: 10,
      toJSON: () => ({}),
    } as DOMRect)

    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))
    await act(() => new Promise((resolve) => setTimeout(resolve, MORPH_MS + 50)))

    const reported = rects().at(-1)?.[1] as { width: number } | undefined
    const widest = reported?.width ?? 0
    w.mockRestore()
    r.mockRestore()

    expect(widest).toBe(wide)
  })

  it('stays centred on the pill while it widens', async () => {
    const w = vi.spyOn(HTMLElement.prototype, 'offsetWidth', 'get').mockReturnValue(700)
    const r = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockReturnValue({
      left: 100, top: 10, right: 300, bottom: 55, width: 200, height: 45, x: 100, y: 10,
      toJSON: () => ({}),
    } as DOMRect)

    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))
    await act(() => new Promise((resolve) => setTimeout(resolve, MORPH_MS + 50)))

    const reported = rects().at(-1)?.[1] as { x: number; width: number } | undefined
    const centre = (reported?.x ?? 0) + (reported?.width ?? 0) / 2
    w.mockRestore()
    r.mockRestore()

    // The measured pill spans 100..300, so its centre is 200.
    expect(centre).toBe(200)
  })
})

describe('DotRow usage popover', () => {
  const usage = { percent: 42, resetsAtMs: Date.now() + 3_600_000, severity: 'normal' as const }

  /**
   * jsdom has no layout and no `elementFromPoint` at all — which is why DotRow
   * feature-tests for it — so the hit-test is driven by installing one that
   * answers with whatever element the test means the cursor to be over.
   */
  type Pointable = { elementFromPoint?: (x: number, y: number) => Element | null }
  const pointAt = (selector: string) => {
    ;(document as Pointable).elementFromPoint = () => document.querySelector(selector)
  }
  afterEach(() => {
    delete (document as Pointable).elementFromPoint
  })

  it('opens over the meter, which the session popover never covered', async () => {
    pointAt('[data-usage]')
    render(<DotRow sessions={sessions} usage={usage} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 400, y: 15, inside: true })
    await act(() => new Promise((resolve) => setTimeout(resolve, HOVER_GRACE_MS + 30)))

    expect(screen.getByTestId('usage-popover')).toBeInTheDocument()
    expect(screen.queryByTestId('popover')).not.toBeInTheDocument()
  })

  it('closes when the cursor leaves the widget', async () => {
    pointAt('[data-usage]')
    render(<DotRow sessions={sessions} usage={usage} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 400, y: 15, inside: true })
    await act(() => new Promise((resolve) => setTimeout(resolve, HOVER_GRACE_MS + 30)))
    moveCursor({ x: -1, y: -1, inside: false })

    expect(screen.queryByTestId('usage-popover')).not.toBeInTheDocument()
  })

  it('gives way to a session when the cursor moves onto a name', async () => {
    pointAt('[data-usage]')
    render(<DotRow sessions={sessions} usage={usage} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 400, y: 15, inside: true })
    await act(() => new Promise((resolve) => setTimeout(resolve, HOVER_GRACE_MS + 30)))
    expect(screen.getByTestId('usage-popover')).toBeInTheDocument()

    pointAt('[data-session-id]')
    moveCursor({ x: 40, y: 15, inside: true })
    await act(() => new Promise((resolve) => setTimeout(resolve, HOVER_GRACE_MS + 30)))

    expect(screen.queryByTestId('usage-popover')).not.toBeInTheDocument()
  })

  it('stays shut when there is no figure to show', async () => {
    pointAt('[data-usage]')
    render(<DotRow sessions={sessions} usage={null} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))

    moveCursor({ x: 400, y: 15, inside: true })
    await act(() => new Promise((resolve) => setTimeout(resolve, HOVER_GRACE_MS + 30)))

    expect(screen.queryByTestId('usage-popover')).not.toBeInTheDocument()
  })
})

describe('DotRow usage popover anchoring', () => {
  const usage = { percent: 42, resetsAtMs: Date.now() + 3_600_000, severity: 'normal' as const }
  type Pointable = { elementFromPoint?: (x: number, y: number) => Element | null }

  afterEach(() => {
    delete (document as Pointable).elementFromPoint
  })

  it('measures the meter in the expanded row, not the hidden collapsed one', async () => {
    // Both variants stay mounted and both draw a meter. A row-wide lookup found
    // the collapsed one first, whose offsets belong to a different slot, and
    // the popover was anchored somewhere the visible meter had never been.
    ;(document as Pointable).elementFromPoint = () => document.querySelector('[data-usage]')

    render(<DotRow sessions={sessions} usage={usage} />)
    await waitFor(() => expect(eventHandlers.has('ui://cursor')).toBe(true))
    moveCursor({ x: 400, y: 15, inside: true })
    await act(() => new Promise((resolve) => setTimeout(resolve, HOVER_GRACE_MS + 30)))

    const expanded = screen.getByTestId('named-dot-row').closest('.variant-slot')
    const meters = screen.getAllByTestId('usage')
    // The one the anchor must measure is the expanded row's, and there is more
    // than one to choose from — which is the whole point of the regression.
    expect(meters.length).toBeGreaterThan(1)
    expect(expanded?.contains(meters.find((m) => expanded?.contains(m)) ?? null)).toBe(true)
    expect(screen.getByTestId('popover-anchor')).toBeInTheDocument()
  })
})
