import { describe, expect, it, vi } from 'vitest'
import { centredAnchor, sessionAtPoint, type CursorPosition } from './useCursor'

function inside(x: number, y: number): CursorPosition {
  return { x, y, inside: true }
}

describe('sessionAtPoint', () => {
  it('returns the session id of the entry under the cursor', () => {
    const entry = document.createElement('span')
    entry.setAttribute('data-session-id', 'id-a')
    const dot = document.createElement('span')
    entry.appendChild(dot)

    expect(sessionAtPoint(inside(40, 15), () => dot)).toBe('id-a')
  })

  it('walks up from a nested child', () => {
    const entry = document.createElement('span')
    entry.setAttribute('data-session-id', 'id-b')
    const name = document.createElement('span')
    const inner = document.createElement('em')
    name.appendChild(inner)
    entry.appendChild(name)

    expect(sessionAtPoint(inside(10, 10), () => inner)).toBe('id-b')
  })

  it('returns null when the cursor is outside the widget', () => {
    const resolve = vi.fn()
    expect(sessionAtPoint({ x: 5, y: 5, inside: false }, resolve)).toBeNull()
    expect(resolve).not.toHaveBeenCalled()
  })

  it('returns null over the pill but not over an entry', () => {
    const pill = document.createElement('div')
    expect(sessionAtPoint(inside(3, 3), () => pill)).toBeNull()
  })

  it('returns null when nothing resolves at the point', () => {
    expect(sessionAtPoint(inside(3, 3), () => null)).toBeNull()
  })
})

describe('centredAnchor', () => {
  it('centres the popover under a middle entry', () => {
    // Entry spans 200..300, so its centre is 250; a 335-wide popover starts at
    // 250 - 167.5.
    expect(centredAnchor(200, 100, 900)).toBeCloseTo(82.5)
  })

  it('pins to the left edge for the first entry', () => {
    expect(centredAnchor(10, 80, 900)).toBe(0)
  })

  it('pins to the right edge for the last entry', () => {
    expect(centredAnchor(800, 90, 900)).toBe(900 - 335)
  })

  it('pins to the left when the popover is wider than the row', () => {
    expect(centredAnchor(40, 60, 300)).toBe(0)
  })
})
