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

const { NotchFlanks } = await import('./NotchFlanks')

const USAGE = { percent: 40, resetsAtMs: 0, severity: 'normal' as const }

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

describe('NotchFlanks', () => {
  it('renders nothing on a display with no notch', async () => {
    layout = null
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={null} />)
    await waitFor(() => expect(invoke).toHaveBeenCalledWith('notch_layout'))
    expect(screen.queryByTestId('notch-flanks')).not.toBeInTheDocument()
  })

  it('puts the counts on the left and the limit on the right', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={USAGE} />)
    expect(await screen.findByTestId('flank-left')).toBeInTheDocument()
    expect(screen.getByTestId('flank-usage')).toBeInTheDocument()
    // The two carry unrelated things, so the side each lives on never moves.
    expect(screen.getByTestId('count-waiting')).toBeInTheDocument()
  })

  it('keeps every state on the left chip', async () => {
    render(
      <NotchFlanks
        sessions={[session('a-11', 'waiting'), session('b-22', 'busy'), session('c-33', 'idle')]}
        usage={null}
      />,
    )
    await screen.findByTestId('flank-left')
    // No split: an earlier design put urgent left and ambient right, which
    // forced a rule to keep background jobs beside their parent.
    expect(screen.getByTestId('count-waiting')).toBeInTheDocument()
    expect(screen.getByTestId('count-busy')).toBeInTheDocument()
    expect(screen.getByTestId('count-idle')).toBeInTheDocument()
  })

  it('draws no limit chip when there is nothing trustworthy to show', async () => {
    // Rust sends null for a window that has already reset, and the setting can
    // turn it off outright.
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={null} />)
    expect(await screen.findByTestId('flank-left')).toBeInTheDocument()
    expect(screen.queryByTestId('flank-usage')).not.toBeInTheDocument()
  })

  it('shows the share left at rest and the countdown once expanded', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'busy')]} usage={USAGE} />)
    await screen.findByTestId('flank-usage')
    expect(screen.getByTestId('flank-usage-collapsed')).toHaveAttribute('data-show', 'true')
    moveCursor({ x: 170, y: 12, inside: true })
    expect(screen.getByTestId('flank-usage-expanded')).toHaveAttribute('data-show', 'true')
  })

  it('fills the notch span so there is no seam beside it', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting'), session('b-22', 'busy')]} usage={null} />)
    const bridge = await screen.findByTestId('notch-bridge')
    expect(bridge.style.left).toBe('200px')
    expect(bridge.style.width).toBe('190px')
    expect(bridge.style.height).toBe('37px')
  })

  it('does not report the notch bridge as part of the widget', async () => {
    // Hovering the notch must not expand the row: the bridge is decoration
    // behind a physical cutout, not a target.
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('notch-bridge')
    await waitFor(() => {
      const calls = invoke.mock.calls.filter(([command]) => command === 'set_hover_rects')
      const last = calls[calls.length - 1]![1] as { rects: unknown[] }
      expect(last.rects).toHaveLength(2)
    })
  })

  it('places each flank so its inner edge is the notch edge', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')
    const left = screen.getByTestId('flank-left').parentElement!
    const right = screen.getByTestId('flank-usage').parentElement!
    // Left flank ends where the notch begins; right flank begins where it ends.
    expect(left.style.left).toBe('0px')
    expect(left.style.width).toBe('200px')
    expect(right.style.left).toBe('390px')
  })

  it('reports the two chips as separate hover rects', async () => {
    // A bounding box across both would bridge the notch, holding the row
    // expanded whenever the cursor crossed it.
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={USAGE} />)
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
    // Sessions but no limit to show: the left chip is the whole widget.
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={null} />)
    await waitFor(() => {
      const calls = invoke.mock.calls.filter(([command]) => command === 'set_hover_rects')
      const last = calls[calls.length - 1]![1] as { rects: unknown[] }
      expect(last.rects).toHaveLength(1)
    })
  })

  it('expands both chips when the cursor is on either of them', async () => {
    render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={USAGE} />)
    await screen.findByTestId('flank-left')

    expect(screen.getByTestId('flank-left')).toHaveAttribute('data-expanded', 'false')
    moveCursor({ x: 170, y: 12, inside: true })
    expect(screen.getByTestId('flank-left')).toHaveAttribute('data-expanded', 'true')
    expect(screen.getByTestId('flank-usage')).toHaveAttribute('data-expanded', 'true')
  })

  it('shows names once expanded and counts again once the cursor leaves', async () => {
    render(<NotchFlanks sessions={[session('api-service-55', 'waiting')]} usage={null} />)
    await screen.findByTestId('flank-left')

    moveCursor({ x: 170, y: 12, inside: true })
    expect(screen.getByTestId('flank-left-expanded')).toHaveAttribute('data-show', 'true')

    moveCursor({ x: 800, y: 400, inside: false })
    expect(screen.getByTestId('flank-left-collapsed')).toHaveAttribute('data-show', 'true')
    expect(screen.getByTestId('flank-left-expanded')).toHaveAttribute('data-show', 'false')
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('1')
  })

  it('draws the continuation arrow between a job and its parent', async () => {
    const { container } = render(
      <NotchFlanks
        sessions={[session('api-11', 'waiting'), session('subagent', 'busy', true)]}
        usage={null}
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
    const { unmount } = render(<NotchFlanks sessions={[session('a-11', 'waiting')]} usage={null} />)
    await screen.findByTestId('flank-left')
    expect(document.body.classList.contains('notch-mode')).toBe(true)
    unmount()
    // Leaving notch mode must give the free-mode pill its shadow room back.
    expect(document.body.classList.contains('notch-mode')).toBe(false)
  })
})
