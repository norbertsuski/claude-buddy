import { act, render, screen, waitFor } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SessionSnapshot, SessionState } from '../../types'

const LAYOUT = { notchLeft: 200, notchRight: 390, barHeight: 37, budget: 200 }

let layout: typeof LAYOUT | null = LAYOUT
const invoke = vi.fn(async (command: string, ..._args: unknown[]) => {
  if (command === 'notch_layout') return layout
  if (command === 'session_detail') {
    return { branch: null, model: null, effort: null, activity: null }
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

const { NotchFlanks, splitByUrgency } = await import('./NotchFlanks')

function session(name: string, state: SessionState, background = false): SessionSnapshot {
  return {
    pid: 1,
    sessionId: `id-${name}`,
    name,
    cwd: `/Users/n/Code/${name}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 60_000,
    uptimeMs: 60_000,
    statusTimeMs: 0,
    startedAtMs: 0,
    background,
  }
}

function moveCursor(position: { x: number; y: number; inside: boolean }) {
  act(() => eventHandlers.get('ui://cursor')!({ payload: position }))
}

beforeEach(() => {
  layout = LAYOUT
  invoke.mockClear()
  eventHandlers.clear()
  // jsdom lays nothing out, so every measured box would be zero-sized and get
  // dropped as un-laid-out. One stub box is enough: the assertions are about
  // how many rects are reported, not where they are.
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue({
    left: 140,
    top: 0,
    width: 60,
    height: 24,
  } as DOMRect)
})

describe('splitByUrgency', () => {
  it('sends what wants you left and what is merely running right', () => {
    const { left, right } = splitByUrgency([
      session('a-11', 'waiting'),
      session('b-22', 'busy'),
      session('c-33', 'dead'),
      session('d-44', 'idle'),
      session('e-55', 'paused'),
    ])
    expect(left.map((s) => s.state)).toEqual(['waiting', 'dead'])
    expect(right.map((s) => s.state)).toEqual(['busy', 'idle', 'paused'])
  })

  it('sends a job to its parent\'s chip, not to its own state\'s', () => {
    // A busy job belonging to a waiting session goes left with its parent. Split
    // by its own state it would land on the opposite chip, and the continuation
    // arrow marking it as a continuation would point at a stranger.
    const { left, right } = splitByUrgency([
      session('api-11', 'waiting'),
      session('subagent', 'busy', true),
      session('web-22', 'busy'),
    ])
    expect(left.map((s) => s.name)).toEqual(['api-11', 'subagent'])
    expect(right.map((s) => s.name)).toEqual(['web-22'])
  })

  it('keeps a job adjacent to the parent it belongs to', () => {
    // Adjacency is what the arrow is drawn from: the job must be the entry
    // immediately after its parent, not merely on the same side.
    const { left } = splitByUrgency([
      session('api-11', 'waiting'),
      session('subagent', 'busy', true),
      session('dead-33', 'dead'),
    ])
    expect(left.map((s) => s.name)).toEqual(['api-11', 'subagent', 'dead-33'])
  })

  it('falls back to its own state for a job with no parent before it', () => {
    // Nothing guarantees an own session comes first, and a job that matched no
    // side would be dropped from both chips.
    const { left, right } = splitByUrgency([session('orphan', 'busy', true)])
    expect(left).toEqual([])
    expect(right.map((s) => s.name)).toEqual(['orphan'])
  })

  it('preserves the incoming order within each side', () => {
    // The list arrives sorted by urgency, which is what lets the chip drop the
    // least urgent when it truncates.
    const { left } = splitByUrgency([
      session('first-11', 'waiting'),
      session('second-22', 'waiting'),
    ])
    expect(left.map((s) => s.name)).toEqual(['first-11', 'second-22'])
  })
})

describe('NotchFlanks', () => {
  it('renders nothing on a display with no notch', async () => {
    layout = null
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('notch_layout'))
    expect(screen.queryByTestId('notch-flanks')).not.toBeInTheDocument()
  })

  it('draws a chip on each side of the notch', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting'), session('b-22', 'busy')]} />)
    expect(await screen.findByTestId('flank-left')).toBeInTheDocument()
    expect(screen.getByTestId('flank-right')).toBeInTheDocument()
  })

  it('keeps the left chip with a total when nothing is urgent', async () => {
    // Both sides are always framed. Colour carries urgency instead of presence.
    render(<NotchFlanks sessions={[session('a-11', 'busy'), session('b-22', 'idle')]} />)
    expect(await screen.findByTestId('flank-right')).toBeInTheDocument()
    expect(screen.getByTestId('flank-left')).toBeInTheDocument()
    expect(screen.getByTestId('total')).toHaveTextContent('2')
  })

  it('drops the right chip when there is nothing ambient to report', async () => {
    // Only the left side holds the shape; two placeholders would be noise.
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} />)
    expect(await screen.findByTestId('flank-left')).toBeInTheDocument()
    expect(screen.queryByTestId('flank-right')).not.toBeInTheDocument()
    expect(screen.queryByTestId('total')).not.toBeInTheDocument()
  })

  it('places each flank so its inner edge is the notch edge', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting'), session('b-22', 'busy')]} />)
    await screen.findByTestId('flank-left')
    const left = screen.getByTestId('flank-left').parentElement!
    const right = screen.getByTestId('flank-right').parentElement!
    // Left flank ends where the notch begins; right flank begins where it ends.
    expect(left.style.left).toBe('0px')
    expect(left.style.width).toBe('200px')
    expect(right.style.left).toBe('390px')
  })

  it('reports the two chips as separate hover rects', async () => {
    // A bounding box across both would bridge the notch, holding the row
    // expanded whenever the cursor crossed it.
    render(<NotchFlanks sessions={[session('a-11', 'waiting'), session('b-22', 'busy')]} />)
    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_hover_rects', {
        rects: [
          { x: 140, y: 0, width: 60, height: 24 },
          { x: 140, y: 0, width: 60, height: 24 },
        ],
      }),
    )
  })

  it('reports one rect when only one side is drawn', async () => {
    // A lone waiting session: the left chip holds it and the right has nothing
    // ambient to report, so exactly one rect is the widget.
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} />)
    await waitFor(() => {
      const calls = invoke.mock.calls.filter(([command]) => command === 'set_hover_rects')
      const last = calls[calls.length - 1]![1] as { rects: unknown[] }
      expect(last.rects).toHaveLength(1)
    })
  })

  it('expands both chips when the cursor is on either of them', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting'), session('b-22', 'busy')]} />)
    await screen.findByTestId('flank-left')

    expect(screen.getByTestId('flank-left')).toHaveAttribute('data-expanded', 'false')
    moveCursor({ x: 170, y: 12, inside: true })
    expect(screen.getByTestId('flank-left')).toHaveAttribute('data-expanded', 'true')
    expect(screen.getByTestId('flank-right')).toHaveAttribute('data-expanded', 'true')
  })

  it('shows names once expanded and counts again once the cursor leaves', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} />)
    await screen.findByTestId('flank-left')

    moveCursor({ x: 170, y: 12, inside: true })
    expect(screen.getByText('api-service')).toBeInTheDocument()

    moveCursor({ x: 800, y: 400, inside: false })
    expect(screen.queryByText('api-service')).not.toBeInTheDocument()
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('1')
  })

  it('draws the continuation arrow between a job and its parent', async () => {
    const { container } = render(
      <NotchFlanks
        sessions={[session('api-11', 'waiting'), session('subagent', 'busy', true)]}
      />,
    )
    await screen.findByTestId('flank-left')
    moveCursor({ x: 170, y: 12, inside: true })

    const chip = screen.getByTestId('flank-left')
    expect(chip.querySelector('.child-arrow')).not.toBeNull()
    // And no plain separator, which is what a peer would get.
    expect(chip.querySelector('.hairline')).toBeNull()
    expect(container.querySelector('[data-side="right"]')).toBeNull()
  })

  it('zeroes the shadow padding so the chips reach the top of the screen', async () => {
    const { unmount } = render(<NotchFlanks sessions={[session('a-11', 'waiting')]} />)
    await screen.findByTestId('flank-left')
    expect(document.body.classList.contains('notch-mode')).toBe(true)
    unmount()
    // Leaving notch mode must give the free-mode pill its shadow room back.
    expect(document.body.classList.contains('notch-mode')).toBe(false)
  })
})
