import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type { SessionSnapshot } from '../../types'
import { SessionFields, useNow, useSessionDetail, type FieldName } from './SessionFields'
// The task list's own classes are the popover's, since the markup is. Loaded
// here rather than relied on through `NotchPanel`, which is only ever the
// caller by convention.
import './dotRow.css'
import './notchFlanks.css'

/**
 * What the row cannot already say.
 *
 * The popover's own head is this row's name, its `state` field is this row's
 * status and elapsed columns, and its `5h limit` is the list's footer row. The
 * rest is what the detail is for.
 */
const NOTCH_FIELDS: FieldName[] = ['doing', 'tasks', 'session', 'cwd', 'branch', 'model', 'proc']

/**
 * What the popover says, under the row it belongs to.
 *
 * The same fields drawn by the same component, so the two surfaces cannot drift
 * apart and a field added to one arrives in both. Only the type scale differs,
 * and that is `.notch-fields`'s to set.
 */
export function RowDetail({ session }: { session: SessionSnapshot }) {
  const detail = useSessionDetail(session)
  const now = useNow()

  return (
    <div className="notch-detail" data-testid={`detail-${session.sessionId}`}>
      <dl className="notch-fields">
        <SessionFields session={session} detail={detail} now={now} fields={NOTCH_FIELDS} />
      </dl>
      {/* The click already raises the session — `NotchFlanks` listens for
          `ui://click` and acts on whichever row the cursor is over. It has
          simply never been advertised, which the popover always was. */}
      <div className="notch-detail-hint" data-testid="notch-detail-hint">
        click → raise this window
      </div>
    </div>
  )
}

/**
 * How long a detail takes to open or close, in ms.
 *
 * Mirrored in `notchFlanks.css`, and read by the list so the row that is being
 * left stays mounted long enough to collapse. Shorter than `--morph`, which the
 * band's own height uses: the band is following this, and a follower that is
 * slower than what it follows reads as lag rather than as one movement.
 */
export const DETAIL_MORPH_MS = 200

/**
 * A detail that opens and closes rather than appearing and vanishing.
 *
 * The height is measured rather than left at `auto`, which cannot be
 * transitioned, and re-measured as the fetched fields land — the activity line
 * arrives well after the row is hovered and changes how tall this is. The
 * one-second clock inside changes nothing about the height, since every age it
 * redraws is one line either way.
 *
 * Mounting with `open` already true is deliberate: the first paint is at height
 * 0 because that is the state's initial value, and the measurement that follows
 * in a layout effect is what the transition runs from.
 */
export function RowDetailSlot({ session, open }: { session: SessionSnapshot; open: boolean }) {
  const inner = useRef<HTMLDivElement>(null)
  const [height, setHeight] = useState(0)

  useLayoutEffect(() => {
    setHeight(inner.current?.offsetHeight ?? 0)
  }, [])

  useEffect(() => {
    const el = inner.current
    if (el === null || typeof ResizeObserver !== 'function') return
    const observer = new ResizeObserver(() => setHeight(el.offsetHeight))
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  return (
    <div
      className="notch-detail-wrap"
      data-open={open ? 'true' : 'false'}
      data-testid={`detail-slot-${session.sessionId}`}
      style={{ height: open ? height : 0 }}
    >
      <div ref={inner}>
        <RowDetail session={session} />
      </div>
    </div>
  )
}
