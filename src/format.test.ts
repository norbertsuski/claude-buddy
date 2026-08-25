import { describe, expect, it } from 'vitest'
import { formatElapsed } from './format'

describe('formatElapsed', () => {
  it('renders whole minutes under an hour', () => {
    expect(formatElapsed(6 * 60 * 1000)).toBe('6m')
  })

  it('renders seconds under a minute', () => {
    expect(formatElapsed(42 * 1000)).toBe('42s')
  })

  it('renders hours and minutes past an hour', () => {
    expect(formatElapsed(6 * 60 * 60 * 1000 + 55 * 60 * 1000)).toBe('6h55m')
  })

  it('clamps negative input to zero', () => {
    expect(formatElapsed(-5000)).toBe('0s')
  })
})

import { countByState, shortName } from './format'
import type { SessionSnapshot, SessionState } from './types'

function s(state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: `id-${state}-${Math.random()}`,
    name: 'proj-a1',
    cwd: '/Users/n/Code/proj',
    entrypoint: 'cli',
    state,
    detail: null,
    elapsedMs: 0,
    uptimeMs: 0,
    background: false,
  }
}

describe('shortName', () => {
  it('strips the two-character suffix Claude Code appends', () => {
    expect(shortName('api-service-55')).toBe('api-service')
    expect(shortName('clawde-buddy-1f')).toBe('clawde-buddy')
    expect(shortName('web-app-e2')).toBe('web-app')
  })

  it('leaves a name without that suffix alone', () => {
    expect(shortName('api-service')).toBe('api-service')
  })

  it('does not strip a longer trailing segment', () => {
    expect(shortName('my-app-staging')).toBe('my-app-staging')
  })

  it('never returns an empty string', () => {
    expect(shortName('a1')).toBe('a1')
    expect(shortName('')).toBe('')
  })
})

describe('countByState', () => {
  it('counts each state', () => {
    const counts = countByState([s('waiting'), s('busy'), s('busy'), s('paused')])
    expect(counts.waiting).toBe(1)
    expect(counts.busy).toBe(2)
    expect(counts.paused).toBe(1)
    expect(counts.idle).toBe(0)
    expect(counts.dead).toBe(0)
  })

  it('returns all zeroes for no sessions', () => {
    const counts = countByState([])
    expect(Object.values(counts).every((n) => n === 0)).toBe(true)
  })
})
