import { describe, expect, it } from 'vitest'
import {
  layoutSize,
  MORPH_FULL_PX,
  MORPH_MIN_MS,
  MORPH_MS,
  morphDuration,
  rowWidthFor,
  sameSize,
  unionRect,
  widgetWindowSize,
} from './useWidgetSize'

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

describe('layoutSize', () => {
  it('measures pre-transform size, ignoring a scale on the element', () => {
    // Regression: the pill was sized from getBoundingClientRect while the
    // hidden variant slot was still at scale(0.97), so it came out ~3% narrow
    // and overflow:hidden clipped both ends of a wide row.
    const el = {
      offsetWidth: 800,
      offsetHeight: 46,
      getBoundingClientRect: () => ({ width: 776.5, height: 44.6 }),
    } as unknown as HTMLElement

    expect(layoutSize(el)).toEqual({ width: 800, height: 46 })
  })
})

describe('morphDuration', () => {
  it('takes the full duration for the widest morph there is', () => {
    // The collapsed↔expanded change, which is what MORPH_MS was tuned against.
    const duration = morphDuration({ width: 180, height: 45 }, { width: 180 + MORPH_FULL_PX, height: 45 })
    expect(duration).toBe(MORPH_MS)
  })

  it('is quicker for the shorter hop a status change makes', () => {
    // Measured: a three-session row going busy → waiting moves the box 129px.
    // Giving that the same 300ms as a 400px morph is what read as a crawl.
    const duration = morphDuration({ width: 183, height: 45 }, { width: 312, height: 45 })
    expect(duration).toBeLessThan(MORPH_MS)
    expect(duration).toBeGreaterThanOrEqual(MORPH_MIN_MS)
  })

  it('never drops below the floor, so a tiny change is still a move', () => {
    expect(morphDuration({ width: 300, height: 45 }, { width: 302, height: 45 })).toBe(MORPH_MIN_MS)
  })

  it('never exceeds the full duration, however far the box travels', () => {
    expect(morphDuration({ width: 0, height: 0 }, { width: 4000, height: 45 })).toBe(MORPH_MS)
  })

  it('is measured on whichever axis moves further', () => {
    const wide = morphDuration({ width: 100, height: 45 }, { width: 300, height: 45 })
    const tall = morphDuration({ width: 100, height: 45 }, { width: 100, height: 245 })
    expect(tall).toBe(wide)
  })

  it('takes the full duration for the first box, having nothing to compare to', () => {
    expect(morphDuration(null, { width: 200, height: 45 })).toBe(MORPH_MS)
  })

  it('shrinks and grows at the same speed', () => {
    const grow = morphDuration({ width: 183, height: 45 }, { width: 312, height: 45 })
    const shrink = morphDuration({ width: 312, height: 45 }, { width: 183, height: 45 })
    expect(shrink).toBe(grow)
  })
})

describe('sameSize', () => {
  it('matches identical sizes', () => {
    expect(sameSize({ width: 383, height: 460 }, { width: 383, height: 460 })).toBe(true)
  })

  it('separates sizes differing on either axis', () => {
    expect(sameSize({ width: 383, height: 460 }, { width: 384, height: 460 })).toBe(false)
    expect(sameSize({ width: 383, height: 460 }, { width: 383, height: 461 })).toBe(false)
  })

  it('treats an unknown size as not matching, so the first resize still runs', () => {
    expect(sameSize(null, { width: 383, height: 460 })).toBe(false)
  })
})
