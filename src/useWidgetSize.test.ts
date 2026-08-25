import { describe, expect, it } from 'vitest'
import { rowWidthFor, unionRect, widgetWindowSize } from './useWidgetSize'

const PAD = 30
const GAP = 10
const ALLOW = 400
const POPOVER_W = 335

describe('widgetWindowSize', () => {
  it('reserves the popover allowance whether or not one is open', () => {
    // Constant across hover states: resizing on a transparent panel shows an
    // unpainted frame, so opening a popover must not change the window.
    const size = widgetWindowSize({ width: 600, height: 40 }, POPOVER_W, ALLOW, GAP, PAD)
    expect(size).toEqual({ width: 602 + 60, height: 42 + GAP + ALLOW + 60 })
  })

  it('widens to the popover when the pill is narrower', () => {
    const size = widgetWindowSize({ width: 100, height: 40 }, POPOVER_W, ALLOW, GAP, PAD)
    expect(size.width).toBe(POPOVER_W + 60)
  })

  it('does not depend on the pill being expanded or collapsed', () => {
    const collapsed = widgetWindowSize({ width: 200, height: 40 }, POPOVER_W, ALLOW, GAP, PAD)
    const expanded = widgetWindowSize({ width: 200, height: 40 }, POPOVER_W, ALLOW, GAP, PAD)
    expect(collapsed).toEqual(expanded)
  })

  it('never returns a fractional size', () => {
    const size = widgetWindowSize({ width: 100.4, height: 40.7 }, POPOVER_W, ALLOW, GAP, PAD)
    expect(Number.isInteger(size.width)).toBe(true)
    expect(Number.isInteger(size.height)).toBe(true)
  })
})

describe('rowWidthFor', () => {
  it('follows the pill plus its border when the pill is wider', () => {
    expect(rowWidthFor({ width: 600 }, POPOVER_W)).toBe(602)
  })

  it('follows the popover width when the pill is narrower', () => {
    expect(rowWidthFor({ width: 100 }, POPOVER_W)).toBe(POPOVER_W)
  })
})

describe('unionRect', () => {
  const pill = { left: 100, top: 30, right: 300, bottom: 80 }

  it('is just the pill when no popover is open', () => {
    expect(unionRect(pill, null)).toEqual({ x: 100, y: 30, width: 200, height: 50 })
  })

  it('grows to cover a popover below the pill', () => {
    // So moving the cursor onto the popover still counts as being on the widget.
    const popover = { left: 140, top: 90, right: 475, bottom: 300 }
    expect(unionRect(pill, popover)).toEqual({ x: 100, y: 30, width: 375, height: 270 })
  })

  it('grows to cover a popover that starts left of the pill', () => {
    const popover = { left: 40, top: 90, right: 375, bottom: 300 }
    expect(unionRect(pill, popover)).toEqual({ x: 40, y: 30, width: 335, height: 270 })
  })
})
