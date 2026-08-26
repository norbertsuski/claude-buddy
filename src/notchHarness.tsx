import { useEffect, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { FlankCluster } from './views/dotRow/FlankCluster'
import { splitByUrgency } from './views/dotRow/NotchFlanks'
import type { SessionSnapshot, SessionState } from './types'

/**
 * Visual harness for notch mode, served by `vite dev` at /notch.html.
 *
 * Not the app. It renders the real FlankCluster against the real stylesheets so
 * chip metrics, the notch scale, ordering and truncation can be judged at 1:1 —
 * but the menu bar, the notch and the widget window are drawn here rather than
 * being the real ones, and hover comes from the DOM rather than from Rust.
 *
 * The numbers are what `notch::notch_layout` returns on a 14-inch MacBook Pro,
 * so the simulated notch and the flanks are positioned by the same arithmetic
 * the app uses. If they do not line up on screen, the arithmetic is wrong.
 */

const SCREEN = 1512
const NOTCH_WIDTH = 190
const BAR = 37
const BUDGET = 240

const NOTCH_X = (SCREEN - NOTCH_WIDTH) / 2
const WINDOW_W = NOTCH_WIDTH + BUDGET * 2
const WINDOW_X = NOTCH_X + NOTCH_WIDTH / 2 - WINDOW_W / 2
const NOTCH_LEFT = BUDGET
const NOTCH_RIGHT = BUDGET + NOTCH_WIDTH
const POPOVER_WIDTH = 335

let seq = 0
function session(name: string, state: SessionState, background = false): SessionSnapshot {
  seq += 1
  return {
    pid: seq,
    sessionId: `id-${seq}-${name}`,
    name,
    cwd: `/Users/n/Code/${name}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 90_000,
    uptimeMs: 900_000,
    statusTimeMs: Date.now() - 90_000,
    startedAtMs: Date.now() - 900_000,
    background,
  }
}

const SCENARIOS: Record<string, SessionSnapshot[]> = {
  quiet: [
    session('api-service-55', 'busy'),
    session('web-app-e2', 'busy'),
    session('infra-tf-b1', 'idle'),
  ],
  urgent: [
    session('api-service-55', 'waiting'),
    session('cli-tools-a9', 'waiting'),
    session('web-app-e2', 'dead'),
    session('docs-site-3f', 'busy'),
    session('infra-tf-b1', 'busy'),
    session('scratch-11', 'idle'),
  ],
  overflow: [
    session('api-service-55', 'waiting'),
    session('cli-tools-a9', 'waiting'),
    session('web-app-e2', 'waiting'),
    session('billing-7c', 'waiting'),
    session('search-2d', 'dead'),
    session('docs-site-3f', 'busy'),
    session('infra-tf-b1', 'busy'),
    session('scratch-11', 'busy'),
    session('etl-jobs-9a', 'busy'),
    session('paused-one-4b', 'paused'),
  ],
  'one side': [session('api-service-55', 'waiting')],
  jobs: [
    session('api-service-55', 'waiting'),
    session('subagent-review', 'busy', true),
    session('infra-tf-b1', 'idle'),
  ],
}

/** Clamp a popover so it stays inside the window, as `centredAnchor` does. */
function anchor(entryLeft: number, entryWidth: number): number {
  const centred = entryLeft + entryWidth / 2 - POPOVER_WIDTH / 2
  return Math.min(Math.max(0, centred), Math.max(0, WINDOW_W - POPOVER_WIDTH))
}

function Harness() {
  const [scenario, setScenario] = useState<keyof typeof SCENARIOS>('urgent')
  const [expanded, setExpanded] = useState(false)
  const [light, setLight] = useState(false)
  const [hovered, setHovered] = useState<{ id: string; x: number } | null>(null)

  useEffect(() => {
    // What the real component does: --shadow-pad would push the chips out of
    // the menu bar entirely.
    document.body.classList.add('notch-mode')
  }, [])

  const sessions = SCENARIOS[scenario]!
  const { left, right } = splitByUrgency(sessions)

  const onHoverSession = (sessionId: string | null, element: HTMLElement | null) => {
    if (sessionId === null || element === null) {
      setHovered(null)
      return
    }
    const box = element.getBoundingClientRect()
    const windowBox = element.closest('.harness-window')!.getBoundingClientRect()
    setHovered({ id: sessionId, x: anchor(box.left - windowBox.left, box.width) })
  }

  return (
    <div style={{ font: '400 13px -apple-system, system-ui, sans-serif', color: '#e8ecf5' }}>
      <div
        style={{
          display: 'flex',
          gap: 18,
          alignItems: 'center',
          padding: '14px 20px',
          background: '#15171d',
          borderBottom: '1px solid rgba(255,255,255,.1)',
          position: 'sticky',
          top: 0,
          zIndex: 10,
        }}
      >
        <span style={{ display: 'flex', gap: 6 }}>
          {Object.keys(SCENARIOS).map((name) => (
            <button
              key={name}
              onClick={() => setScenario(name as keyof typeof SCENARIOS)}
              style={{
                padding: '5px 11px',
                borderRadius: 6,
                cursor: 'pointer',
                border: '1px solid rgba(255,255,255,.16)',
                background: scenario === name ? '#2f3644' : 'transparent',
                color: '#e8ecf5',
                font: 'inherit',
              }}
            >
              {name}
            </button>
          ))}
        </span>
        <label style={{ display: 'flex', gap: 6, alignItems: 'center', cursor: 'pointer' }}>
          <input
            type="checkbox"
            checked={expanded}
            onChange={(e) => setExpanded(e.target.checked)}
          />
          expanded
        </label>
        <label style={{ display: 'flex', gap: 6, alignItems: 'center', cursor: 'pointer' }}>
          <input type="checkbox" checked={light} onChange={(e) => setLight(e.target.checked)} />
          light wallpaper
        </label>
      </div>

      <div style={{ padding: 20, overflowX: 'auto' }}>
        <div
          style={{
            position: 'relative',
            width: SCREEN,
            height: 260,
            background: light ? '#cfd6e4' : '#3b4252',
            borderRadius: 10,
            overflow: 'hidden',
          }}
        >
          <div
            style={{
              position: 'absolute',
              inset: `0 0 auto 0`,
              height: BAR,
              background: light ? 'rgba(255,255,255,.55)' : 'rgba(0,0,0,.45)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              padding: '0 14px',
              font: '400 12px -apple-system, system-ui, sans-serif',
              color: light ? '#1c1c1e' : '#dfe4ee',
            }}
          >
            <span style={{ display: 'flex', gap: 15 }}>
              <b style={{ fontWeight: 600 }}>Xcode</b>
              <span>File</span>
              <span>Edit</span>
              <span>View</span>
              <span>Find</span>
              <span>Navigate</span>
              <span>Editor</span>
              <span>Product</span>
              <span>Debug</span>
            </span>
            <span style={{ display: 'flex', gap: 14 }}>
              <span>100%</span>
              <span>Wi-Fi</span>
              <span>Mon 09:41</span>
            </span>
          </div>

          <div
            style={{
              position: 'absolute',
              top: 0,
              left: NOTCH_X,
              width: NOTCH_WIDTH,
              height: BAR,
              background: '#000',
              borderRadius: '0 0 11px 11px',
            }}
          />

          <div
            className="harness-window notch-flanks"
            style={{ position: 'absolute', top: 0, left: WINDOW_X, width: WINDOW_W, height: 220 }}
          >
            <div
              className="flank flank-left"
              style={{ left: NOTCH_LEFT - BUDGET, width: BUDGET, height: BAR }}
            >
              <FlankCluster
                side="left"
                sessions={left}
                expanded={expanded}
                hoveredSessionId={hovered?.id ?? null}
                onHoverSession={onHoverSession}
              />
            </div>
            <div
              className="flank flank-right"
              style={{ left: NOTCH_RIGHT, width: BUDGET, height: BAR }}
            >
              <FlankCluster
                side="right"
                sessions={right}
                expanded={expanded}
                hoveredSessionId={hovered?.id ?? null}
                onHoverSession={onHoverSession}
              />
            </div>
            {expanded && hovered !== null && (
              <div
                style={{
                  position: 'absolute',
                  left: hovered.x,
                  top: BAR + 10,
                  width: POPOVER_WIDTH,
                  height: 96,
                  boxSizing: 'border-box',
                  padding: '10px 13px',
                  borderRadius: 12,
                  background: 'rgba(24,28,38,.97)',
                  border: '1px solid rgba(255,255,255,.14)',
                  font: '400 12px -apple-system, system-ui, sans-serif',
                  color: '#98a1b5',
                }}
              >
                popover lands here — 335pt wide, clear of the bar
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}

createRoot(document.getElementById('root')!).render(<Harness />)
