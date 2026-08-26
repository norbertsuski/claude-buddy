import { act, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SessionSnapshot, SessionState } from '../../types'

const LAYOUT = {
  notchLeft: 249,
  notchRight: 428,
  barHeight: 32,
  budget: 160,
  slabWidth: 340,
}
const USAGE = { percent: 36, resetsAtMs: Date.now() + 3_600_000, severity: 'normal' as const }

let layout: typeof LAYOUT | null = LAYOUT
const invoke = vi.fn(async (command: string, ..._args: unknown[]) => {
  if (command === 'notch_layout') return layout
  if (command === 'session_detail') {
    return { branch: 'main', model: 'opus', effort: 'high', activity: 'Edit src/auth.ts' }
  }
  return undefined
})
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: Parameters<typeof invoke>) => invoke(...args),
}))

const eventHandlers = new Map<string, (event: { payload: unknown }) => void>()
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    eventHandlers.set(name, handler)
    return vi.fn()
  }),
}))

const { NotchFlanks } = await import('./NotchFlanks')
const { MAX_ROWS } = await import('./NotchPanel')

function session(name: string, state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: `id-${name}`,
    name,
    cwd: `/Users/n/Code/${name}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 120_000,
    uptimeMs: 600_000,
    statusTimeMs: Date.now() - 120_000,
    startedAtMs: Date.now() - 600_000,
    background: false,
  }
}

function open(inside = true, x = 400, y = 12) {
  act(() => eventHandlers.get('ui://cursor')!({ payload: { x, y, inside } }))
}

/**
 * Put the cursor on a row the way the app does: Rust reports a point, and the
 * page hit-tests it. WKWebView delivers no mouse events to a non-activating
 * panel, so `userEvent.hover` would prove nothing about the real thing.
 */
function pointAt(el: Element | null) {
  // Assigned rather than spied: jsdom has no elementFromPoint to spy on.
  ;(document as unknown as Record<string, unknown>).elementFromPoint = () => el
  open(true, 400, 44)
}

function rectsFromLastCall() {
  const calls = invoke.mock.calls.filter(([command]) => command === 'set_hover_rects')
  return (calls[calls.length - 1]![1] as { rects: unknown[] }).rects
}

beforeEach(() => {
  layout = LAYOUT
  ;(document as unknown as Record<string, unknown>).elementFromPoint = () => null
  // jsdom lays nothing out, so the panel would measure 0 tall and never be
  // reported to Rust — the same failure this measurement replaced.
  Object.defineProperty(HTMLElement.prototype, 'offsetHeight', {
    configurable: true,
    value: 84,
  })
  invoke.mockClear()
  eventHandlers.clear()
  // jsdom lays nothing out, so every measured box would be dropped as
  // un-laid-out. One stub is enough: the assertions are about how many rects
  // are reported, not where they are.
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
    left: 345,
    top: 0,
    width: 179,
    height: 32,
  } as DOMRect)
})

describe('NotchFlanks at rest', () => {
  it('renders nothing on a display with no notch', async () => {
    layout = null
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={USAGE} />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('notch_layout'))
    expect(screen.queryByTestId('notch-flanks')).not.toBeInTheDocument()
  })

  it('puts the counts on the left and the limit on the right', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={USAGE} />)
    expect(await screen.findByTestId('flank-left')).toBeInTheDocument()
    expect(screen.getByTestId('flank-usage')).toBeInTheDocument()
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('1')
  })

  it('leaves the slab shut', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'busy')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    expect(screen.getByTestId('notch-panel')).toHaveAttribute('data-open', 'false')
  })

  it('draws no limit chip when there is nothing trustworthy to show', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'busy')]} usage={null} />)
    await screen.findByTestId('flank-left')
    expect(screen.queryByTestId('flank-usage')).not.toBeInTheDocument()
  })
})

describe('NotchFlanks opening', () => {
  it('opens the slab, one fixed width for the band and the list', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')

    open()
    const slab = screen.getByTestId('notch-panel')
    expect(slab).toHaveAttribute('data-open', 'true')
    expect(slab.style.width).toBe(`${LAYOUT.slabWidth}px`)
    // Centred on the notch, so it covers both chips without either moving.
    const centre = (LAYOUT.notchLeft + LAYOUT.notchRight) / 2
    expect(slab.parentElement!.style.left).toBe(`${centre - LAYOUT.slabWidth / 2}px`)
  })

  it('spells the status out in the row, where a card used to', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    open()
    // The slab's width is chosen so this fits, which is why there is no popover.
    expect(screen.getByText('input needed')).toBeInTheDocument()
    expect(screen.getByText('api-service')).toBeInTheDocument()
  })

  it('lists the sessions and the limit in the slab', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    open()
    expect(screen.getByText('api-service')).toBeInTheDocument()
    expect(screen.getByTestId('notch-usage-row')).toHaveTextContent('64% of the 5h limit left')
  })

  it('collapses the tail beyond the row cap', async () => {
    const many = Array.from({ length: MAX_ROWS + 3 }, (_, i) => session(`s${i}-11`, 'busy'))
    render(<NotchFlanks sessions={many} usage={null} />)
    await screen.findByTestId('flank-left')
    open()
    expect(screen.getByTestId('notch-more')).toHaveTextContent('+3 more')
  })

  it('reports the slab alone once open, spanning the bar it was opened from', async () => {
    // The slab covers the bar as well as the list, so the cursor that opened it
    // is already inside — no separate band is needed to stop it shutting itself.
    render(<NotchFlanks sessions={[session('a-11', 'busy')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    await waitFor(() => expect(rectsFromLastCall()).toHaveLength(2))

    open()
    await waitFor(() => {
      const rects = rectsFromLastCall() as Array<{ x: number; y: number; width: number }>
      expect(rects).toHaveLength(1)
      const centre = (LAYOUT.notchLeft + LAYOUT.notchRight) / 2
      expect(rects[0]!.x).toBe(centre - LAYOUT.slabWidth / 2)
      expect(rects[0]!.y).toBe(0)
      expect(rects[0]!.width).toBe(LAYOUT.slabWidth)
    })
  })
})

describe('rowAtPoint', () => {
  it('picks the row under the point, and nothing outside one', async () => {
    const { rowAtPoint } = await import('./NotchPanel')
    const row = document.createElement('div')
    row.setAttribute('data-notch-row', 'session')
    row.setAttribute('data-session-id', 'id-a')
    document.body.append(row)

    expect(rowAtPoint(0, 0, () => row)).toEqual({ kind: 'session', sessionId: 'id-a' })
    expect(rowAtPoint(0, 0, () => null)).toBeNull()
    // Padding between rows resolves to the panel, which is not a row.
    expect(rowAtPoint(0, 0, () => document.body)).toBeNull()
    row.remove()
  })

  it('reads the footer as the limit rather than a session', async () => {
    const { rowAtPoint } = await import('./NotchPanel')
    const row = document.createElement('div')
    row.setAttribute('data-notch-row', 'usage')
    expect(rowAtPoint(0, 0, () => row)).toEqual({ kind: 'usage' })
  })
})

describe('NotchFlanks rows', () => {
  it('highlights the row under the cursor', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    open()

    pointAt(screen.getByTestId('row-id-api-service-55'))
    await waitFor(() =>
      expect(screen.getByTestId('row-id-api-service-55')).toHaveAttribute('data-hovered', 'true'),
    )
  })

  it('raises the session the cursor is on when Rust reports a click', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    open()
    pointAt(screen.getByTestId('row-id-api-service-55'))
    await waitFor(() =>
      expect(screen.getByTestId('row-id-api-service-55')).toHaveAttribute('data-hovered', 'true'),
    )

    act(() => eventHandlers.get('ui://click')!({ payload: {} }))
    expect(invoke).toHaveBeenCalledWith('raise_session', { pid: 1 })
  })

  it('raises nothing when the cursor is on the limit row', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'busy')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    open()
    pointAt(screen.getByTestId('notch-usage-row'))
    await waitFor(() =>
      expect(screen.getByTestId('notch-usage-row')).toHaveAttribute('data-hovered', 'true'),
    )

    act(() => eventHandlers.get('ui://click')!({ payload: {} }))
    expect(invoke).not.toHaveBeenCalledWith('raise_session', expect.anything())
  })

  it('clears the highlight when the cursor leaves the widget', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    open()
    pointAt(screen.getByTestId('row-id-api-service-55'))
    await waitFor(() =>
      expect(screen.getByTestId('row-id-api-service-55')).toHaveAttribute('data-hovered', 'true'),
    )

    open(false)
    expect(screen.getByTestId('row-id-api-service-55')).toHaveAttribute('data-hovered', 'false')
  })

  it('zeroes the shadow padding so the slab reaches the top of the screen', async () => {
    const { unmount } = render(<NotchFlanks sessions={[session('a-11', 'busy')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    expect(document.body.classList.contains('notch-mode')).toBe(true)
    unmount()
    expect(document.body.classList.contains('notch-mode')).toBe(false)
  })
})
