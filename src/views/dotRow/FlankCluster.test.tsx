import { render, screen } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'
import { FLANK_MAX_VISIBLE, FlankCluster } from './FlankCluster'
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
  it('renders nothing when it has no sessions', () => {
    chip({ sessions: [] })
    expect(screen.queryByTestId('flank-left')).not.toBeInTheDocument()
  })

  it('shows one count per state it holds', () => {
    chip({
      sessions: [
        session('a-11', 'waiting'),
        session('b-22', 'waiting'),
        session('c-33', 'dead'),
        session('d-44', 'busy'),
      ],
    })
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('2')
    expect(screen.getByTestId('count-dead')).toHaveTextContent('1')
    expect(screen.getByTestId('count-busy')).toHaveTextContent('1')
  })

  it('omits a state it holds none of', () => {
    chip({ sessions: [session('a-11', 'waiting')] })
    expect(screen.queryByTestId('count-dead')).not.toBeInTheDocument()
  })

  it('orders counts most urgent first', () => {
    // The left chip lays out in reverse, so document order puts the count that
    // matters nearest the notch.
    chip({
      sessions: [session('a-11', 'idle'), session('b-22', 'dead'), session('c-33', 'waiting')],
    })
    const rendered = screen.getAllByTestId(/^count-/).map((el) => el.dataset.testid)
    expect(rendered).toEqual(['count-waiting', 'count-dead', 'count-idle'])
  })

  it('shows names rather than counts once expanded', () => {
    // Both states stay mounted so the box can be measured and morphed between,
    // so which one is showing is an attribute rather than presence.
    chip({ sessions: [session('api-service-55', 'waiting')], expanded: true })
    expect(screen.getByText('api-service')).toBeInTheDocument()
    expect(screen.getByTestId('flank-left-expanded')).toHaveAttribute('data-show', 'true')
    expect(screen.getByTestId('flank-left-collapsed')).toHaveAttribute('data-show', 'false')
  })

  it('keeps both states mounted so the box has something to morph to', () => {
    chip({ sessions: [session('api-service-55', 'waiting')] })
    expect(screen.getByTestId('flank-left-collapsed')).toHaveAttribute('data-show', 'true')
    expect(screen.getByTestId('flank-left-expanded')).toHaveAttribute('data-show', 'false')
    // The hidden state is still in the DOM, which is what makes it measurable.
    expect(screen.getByText('api-service')).toBeInTheDocument()
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

  it('keeps a background job next to the session it belongs to', () => {
    // With every state on one chip, adjacency comes for free — the continuation
    // arrow is drawn from it, and an earlier design that split the states across
    // both flanks had to carry a rule to preserve this.
    const parent = session('api-11', 'waiting')
    const job = { ...session('subagent', 'busy'), background: true }
    const { container } = chip({ sessions: [parent, job], expanded: true, maxVisible: 2 })
    expect(container.querySelector('.child-arrow')).not.toBeNull()
    expect(container.querySelector('.hairline')).toBeNull()
  })

  it('marks its side so the CSS can flush it against the notch', () => {
    chip({ side: 'right', sessions: [session('a-11', 'busy')] })
    expect(screen.getByTestId('flank-right')).toHaveAttribute('data-side', 'right')
  })
})
