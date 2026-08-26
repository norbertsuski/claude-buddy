import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { CHIP_STATES, FLANK_MAX_VISIBLE, FlankCluster } from './FlankCluster'
import type { SessionSnapshot, SessionState } from '../../types'

function session(name: string, state: SessionState): SessionSnapshot {
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
    background: false,
  }
}

function chip(props: Partial<Parameters<typeof FlankCluster>[0]> = {}) {
  return render(
    <FlankCluster
      side="left"
      sessions={[session('api-11', 'waiting')]}
      expanded={false}
      hoveredSessionId={null}
      onHoverSession={vi.fn()}
      {...props}
    />,
  )
}

describe('FlankCluster', () => {
  it('renders nothing when it has no sessions and no fallback', () => {
    chip({ sessions: [] })
    expect(screen.queryByTestId('flank-left')).not.toBeInTheDocument()
  })

  it('draws the fallback total instead of vanishing', () => {
    // The left chip is always present so the notch stays framed on both sides.
    chip({ sessions: [], fallbackTotal: 5 })
    expect(screen.getByTestId('flank-left')).toBeInTheDocument()
    expect(screen.getByTestId('total')).toHaveTextContent('5')
  })

  it('shows no state dot alongside the fallback total', () => {
    // A state count is a dot and a number. Presence no longer signals urgency,
    // so the total must not be mistakable for one.
    const { container } = chip({ sessions: [], fallbackTotal: 3 })
    expect(container.querySelector('.total .dot')).toBeNull()
    expect(screen.queryByTestId('count-idle')).not.toBeInTheDocument()
  })

  it('prefers real counts over the fallback once it has sessions', () => {
    chip({ sessions: [session('a-11', 'waiting')], fallbackTotal: 9 })
    expect(screen.queryByTestId('total')).not.toBeInTheDocument()
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('1')
  })

  it('shows one count per state it carries, collapsed', () => {
    chip({
      sessions: [
        session('a-11', 'waiting'),
        session('b-22', 'waiting'),
        session('c-33', 'dead'),
      ],
    })
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('2')
    expect(screen.getByTestId('count-dead')).toHaveTextContent('1')
  })

  it('omits a state it carries none of', () => {
    chip({ sessions: [session('a-11', 'waiting')] })
    expect(screen.queryByTestId('count-dead')).not.toBeInTheDocument()
  })

  it('orders counts most urgent first', () => {
    // The left chip lays out in reverse, so document order puts the most urgent
    // state nearest the notch on both sides.
    chip({ sessions: [session('a-11', 'dead'), session('b-22', 'waiting')] })
    const rendered = screen.getAllByTestId(/^count-/).map((el) => el.dataset.testid)
    expect(rendered).toEqual(['count-waiting', 'count-dead'])
  })

  it('shows names rather than counts once expanded', () => {
    chip({ sessions: [session('api-service-55', 'waiting')], expanded: true })
    expect(screen.getByText('api-service')).toBeInTheDocument()
    expect(screen.queryByTestId('count-waiting')).not.toBeInTheDocument()
  })

  it('collapses the tail beyond the per-side cap into a count', () => {
    const many = ['a', 'b', 'c', 'd', 'e'].map((n) => session(`${n}-11`, 'waiting'))
    chip({ sessions: many, expanded: true })
    expect(screen.getByTestId('overflow-left')).toHaveTextContent(`+${5 - FLANK_MAX_VISIBLE}`)
  })

  it('shows no overflow count when everything fits', () => {
    chip({ sessions: [session('a-11', 'waiting')], expanded: true })
    expect(screen.queryByTestId('overflow-left')).not.toBeInTheDocument()
  })

  it('counts a state its side does not nominally carry', () => {
    // A background job follows its parent, so the urgent chip can hold a busy
    // one. Counting only waiting and dead here would count that job nowhere.
    chip({ sessions: [session('a-11', 'waiting'), session('job-22', 'busy')] })
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('1')
    expect(screen.getByTestId('count-busy')).toHaveTextContent('1')
  })

  it('marks its side so the CSS can flush it against the notch', () => {
    chip({ side: 'right', sessions: [session('a-11', 'busy')] })
    expect(screen.getByTestId('flank-right')).toHaveAttribute('data-side', 'right')
  })

  it('separates the two sides by urgency without overlap', () => {
    // Every state belongs to exactly one chip, or a session would be counted
    // twice or not at all.
    const all: SessionState[] = ['waiting', 'busy', 'idle', 'paused', 'dead']
    const assigned = [...CHIP_STATES.left, ...CHIP_STATES.right]
    expect([...assigned].sort()).toEqual([...all].sort())
  })
})
