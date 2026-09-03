import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Update } from '@buddy/ui'

const listeners = new Map<string, (event: { payload: unknown }) => void>()
const unlisten = vi.fn()
const invoke = vi.fn()

vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: (event: { payload: unknown }) => void) => {
    listeners.set(name, handler)
    return unlisten
  }),
}))

const { useSessions } = await import('./useSessions')

function emit(update: Update) {
  const handler = listeners.get('sessions://update')
  if (!handler) throw new Error('nothing subscribed to sessions://update')
  act(() => handler({ payload: update }))
}

function session(id: string, state: Update['sessions'][number]['state']) {
  return {
    pid: 1,
    sessionId: id,
    name: `name-${id}`,
    title: null,
    cwd: '/Users/n/Code/x',
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 0,
    uptimeMs: 0,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    tasks: [],
  }
}

describe('useSessions', () => {
  beforeEach(() => {
    listeners.clear()
    unlisten.mockClear()
    invoke.mockReset()
    invoke.mockResolvedValue([])
  })

  it('starts empty and not ready', () => {
    const { result } = renderHook(() => useSessions())
    expect(result.current.sessions).toEqual([])
    expect(result.current.ready).toBe(false)
  })

  it('exposes sessions from an update and becomes ready', async () => {
    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    emit({ sessions: [session('a', 'waiting')], alerts: [], usage: null })

    expect(result.current.sessions).toHaveLength(1)
    expect(result.current.sessions[0].sessionId).toBe('a')
    expect(result.current.ready).toBe(true)
  })

  it('replaces state wholesale rather than merging', async () => {
    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    emit({ sessions: [session('a', 'busy'), session('b', 'busy')], alerts: [], usage: null })
    emit({ sessions: [session('b', 'waiting')], alerts: [], usage: null })

    expect(result.current.sessions.map((s) => s.sessionId)).toEqual(['b'])
    expect(result.current.sessions[0].state).toBe('waiting')
  })

  it('becomes ready on an empty update', async () => {
    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    emit({ sessions: [], alerts: [], usage: null })

    expect(result.current.sessions).toEqual([])
    expect(result.current.ready).toBe(true)
  })

  it('unsubscribes on unmount', async () => {
    const { unmount } = renderHook(() => useSessions())
    await act(async () => {})

    unmount()
    await act(async () => {})

    expect(unlisten).toHaveBeenCalled()
  })
})

describe('useSessions initial fetch', () => {
  it('shows sessions that already existed before the webview subscribed', async () => {
    // Regression: the watcher emits its first snapshot before this webview
    // loads and only re-emits on change, so without an explicit fetch the
    // widget rendered "no sessions" while three sessions were running.
    invoke.mockResolvedValue([session('a', 'busy')])

    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    expect(invoke).toHaveBeenCalledWith('get_sessions')
    expect(result.current.sessions).toHaveLength(1)
    expect(result.current.ready).toBe(true)
  })

  it('does not let a late fetch overwrite a newer pushed update', async () => {
    let release: (value: unknown) => void = () => {}
    invoke.mockReturnValue(new Promise((resolve) => { release = resolve }))

    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    emit({ sessions: [session('fresh', 'waiting')], alerts: [], usage: null })
    await act(async () => { release([session('stale', 'busy')]) })

    expect(result.current.sessions.map((s) => s.sessionId)).toEqual(['fresh'])
  })

  it('survives a failing fetch', async () => {
    invoke.mockRejectedValue(new Error('command unavailable'))

    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    expect(result.current.sessions).toEqual([])
    emit({ sessions: [session('a', 'busy')], alerts: [], usage: null })
    expect(result.current.sessions).toHaveLength(1)
  })

  it('starts with no alerts', () => {
    const { result } = renderHook(() => useSessions())
    expect(result.current.alerts).toEqual([])
  })

  it('exposes the alerts that arrive with an update', () => {
    const { result } = renderHook(() => useSessions())

    emit({
      sessions: [],
      usage: null,
      alerts: [{ sessionId: 'a', name: 'api', kind: 'died', detail: null }],
    })

    expect(result.current.alerts).toEqual([
      { sessionId: 'a', name: 'api', kind: 'died', detail: null },
    ])
  })
})
