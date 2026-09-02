import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { StateCounts } from './StateCounts'
import type { SessionSnapshot, SessionState } from '../../types'

function session(name: string, state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: `id-${name}`,
    name,
    title: null,
    cwd: `/Users/n/Code/${name}`,
    entrypoint: 'cli',
    state,
    detail: null,
    elapsedMs: 0,
    uptimeMs: 0,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    tasks: [],
  }
}

describe('StateCounts', () => {
  it('shows one count per state present', () => {
    render(
      <StateCounts
        sessions={[
          session('a', 'waiting'),
          session('b', 'waiting'),
          session('c', 'busy'),
        ]}
      />,
    )
    expect(screen.getByTestId('count-waiting')).toHaveTextContent('2')
    expect(screen.getByTestId('count-busy')).toHaveTextContent('1')
  })

  it('omits the states with nothing in them', () => {
    render(<StateCounts sessions={[session('a', 'busy')]} />)
    expect(screen.queryByTestId('count-waiting')).not.toBeInTheDocument()
    expect(screen.queryByTestId('count-dead')).not.toBeInTheDocument()
  })

  it('orders them most urgent first', () => {
    // The chip lays out in reverse, so first here is nearest the notch.
    render(
      <StateCounts
        sessions={[session('a', 'idle'), session('b', 'busy'), session('c', 'dead')]}
      />,
    )
    expect(screen.getAllByTestId(/^count-/).map((el) => el.dataset.testid)).toEqual([
      'count-dead',
      'count-busy',
      'count-idle',
    ])
  })

  it('renders nothing at all with no sessions', () => {
    const { container } = render(<StateCounts sessions={[]} />)
    expect(container.querySelectorAll('.count')).toHaveLength(0)
  })

  it('shows a tasking count between busy and idle', () => {
    render(
      <StateCounts
        sessions={[
          session('a', 'idle'),
          session('b', 'tasking'),
          session('c', 'busy'),
        ]}
      />,
    )
    const rendered = screen
      .getAllByTestId(/^count-/)
      .map((el) => el.getAttribute('data-testid'))
    expect(rendered).toEqual(['count-busy', 'count-tasking', 'count-idle'])
  })
})
