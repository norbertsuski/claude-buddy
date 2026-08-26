import { describe, expect, it } from 'vitest'
import { visibleRects } from './useNotch'

describe('visibleRects', () => {
  it('converts measured boxes into hover rects', () => {
    expect(visibleRects([{ left: 140, top: 0, width: 60, height: 24 }])).toEqual([
      { x: 140, y: 0, width: 60, height: 24 },
    ])
  })

  it('drops the sides that are not rendered', () => {
    // A side with no sessions renders no chip, and the popover is absent unless
    // an entry is hovered, so missing inputs are the normal case rather than an
    // error worth reporting.
    const rects = visibleRects([
      null,
      { left: 390, top: 0, width: 60, height: 24 },
      undefined,
    ])
    expect(rects).toEqual([{ x: 390, y: 0, width: 60, height: 24 }])
  })

  it('drops a box that exists but has not been laid out', () => {
    // Reporting a degenerate rect would put a zero-sized hover target at the
    // window origin, which is inside the notch.
    expect(visibleRects([{ left: 0, top: 0, width: 0, height: 0 }])).toEqual([])
  })

  it('keeps the chips and the popover as separate rects', () => {
    // The whole point: a bounding box across the two chips would bridge the
    // notch, and one across a chip and its popover would cover the desktop
    // between them.
    const rects = visibleRects([
      { left: 140, top: 0, width: 60, height: 24 },
      { left: 390, top: 0, width: 60, height: 24 },
      { left: 200, top: 47, width: 335, height: 100 },
    ])
    expect(rects).toHaveLength(3)
    expect(rects.map((r) => r.x)).toEqual([140, 390, 200])
  })

  it('reports nothing when nothing is on screen', () => {
    expect(visibleRects([null, null, null])).toEqual([])
  })
})
