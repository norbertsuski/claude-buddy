import { render, screen } from '@testing-library/react'
import { describe, expect, it } from 'vitest'
import { CollapsedPill } from './CollapsedPill'
import type { SessionSnapshot, SessionState } from '../../types'

function session(id: string, state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: id,
    name: `${id}-a1`,
    cwd: `/Users/n/Code/${id}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 0,
    uptimeMs: 0,
    background: false,
  }
}

describe('CollapsedPill', () => {
  it('shows the needs-you chip with a count when sessions are waiting', () => {
    render(<CollapsedPill sessions={[session('a', 'waiting'), session('b', 'busy')]} />)
    expect(screen.getByTestId('needs-you')).toHaveTextContent('1 needs you')
  })

  it('shows working as its own chip, like waiting and died', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), session('b', 'busy')]} />)
    expect(screen.getByTestId('working')).toHaveTextContent('2 working')
  })

  it('shows waiting, working and died chips side by side', () => {
    render(
      <CollapsedPill
        sessions={[session('a', 'waiting'), session('b', 'busy'), session('c', 'dead')]}
      />,
    )
    expect(screen.getByTestId('needs-you')).toHaveTextContent('1 needs you')
    expect(screen.getByTestId('working')).toHaveTextContent('1 working')
    expect(screen.getByTestId('died')).toHaveTextContent('1 died')
  })

  it('omits the needs-you chip entirely when nothing is waiting', () => {
    render(<CollapsedPill sessions={[session('a', 'busy')]} />)
    expect(screen.queryByTestId('needs-you')).not.toBeInTheDocument()
  })

  it('counts waiting sessions across several at once', () => {
    render(<CollapsedPill sessions={[session('a', 'waiting'), session('b', 'waiting')]} />)
    expect(screen.getByTestId('needs-you')).toHaveTextContent('2 need you')
  })

  it('keeps the idle count as quiet text', () => {
    render(<CollapsedPill sessions={[session('a', 'idle'), session('b', 'paused')]} />)
    expect(screen.getByTestId('summary')).toHaveTextContent('2 idle')
    expect(screen.queryByTestId('working')).not.toBeInTheDocument()
  })

  it('reports dead sessions', () => {
    render(<CollapsedPill sessions={[session('a', 'dead')]} />)
    expect(screen.getByTestId('died')).toHaveTextContent('1 died')
  })

  it('shows a resting label when there are no sessions at all', () => {
    render(<CollapsedPill sessions={[]} />)
    expect(screen.getByTestId('summary')).toHaveTextContent('no sessions')
  })
})


describe('CollapsedPill background jobs', () => {
  const job = (id: string): SessionSnapshot => ({ ...session(id, 'busy'), background: true })

  it('does not let jobs inflate the session counts', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), job('j1'), job('j2')]} />)
    expect(screen.getByTestId('working')).toHaveTextContent('1 working')
  })

  it('acknowledges jobs separately', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), job('j1'), job('j2')]} />)
    expect(screen.getByTestId('jobs')).toHaveTextContent('2 jobs')
  })

  it('says nothing about jobs when there are none', () => {
    render(<CollapsedPill sessions={[session('a', 'busy')]} />)
    expect(screen.queryByTestId('jobs')).not.toBeInTheDocument()
  })
})
