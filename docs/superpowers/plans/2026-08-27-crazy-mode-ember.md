# Crazy Mode (ember level) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship the `ember` level of crazy mode — an opt-in setting that makes the widget's pill catch fire, shake, fracture and crumble in response to signals it already receives.

**Architecture:** A pure `deriveHeat()` function turns the existing `Update` payload into a `Heat` value. `DotRow` writes that value onto the DOM as data attributes and CSS custom properties; a new `crazy.css` does every visual effect from there. No `requestAnimationFrame`, no canvas, no per-frame React work. Rust gains one config field and nothing else.

**Tech Stack:** React 19 + TypeScript, Vitest + @testing-library/react, Rust (Tauri v2), plain CSS.

## Global Constraints

- macOS only. No cross-platform branches.
- Run before every commit: `npm run typecheck && npm test`, and for Rust changes `cd src-tauri && cargo fmt && cargo test -- --test-threads=1`. `--test-threads=1` is mandatory.
- Stage explicit paths. Never `git add -A`, never `git commit -a` — the tree may be shared with another agent session. Run `git status` before staging and leave unrelated modified files alone.
- Conventional commit subjects, no scopes, body explaining reasoning, and keep the `Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>` trailer.
- Comments explain *why*, never *what*.
- Do not reformat files you are not changing.
- Spec: `docs/superpowers/specs/2026-08-27-crazy-mode-design.md`.
- Config field name: `crazy` (Rust `crazy: String`, JSON `crazy`), values `"off"` and `"ember"`, default `"off"`.
- **Notch placement is out of scope.** `NotchFlanks` is a different view with its own markup and scale; crazy mode applies to the free-floating `DotRow` only. This is stated in the README in Task 9.

---

### Task 1: The setting exists end to end

Adds the config field in Rust, mirrors it in TypeScript, and puts a `<select>` in the settings panel. Nothing is drawn differently yet — this task is done when the choice persists across a restart.

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src/types.ts`
- Modify: `src/settings/SettingsPanel.tsx`
- Test: `src-tauri/src/config.rs` (inline `#[cfg(test)]` module, existing)
- Test: `src/settings/SettingsPanel.test.tsx`

**Interfaces:**
- Consumes: nothing.
- Produces: `Config::crazy: String` (Rust), `AppConfig['crazy']: string` (TS), `CRAZY_LEVELS: readonly { id: string; label: string }[]` (TS), `CRAZY_LEVELS: [&str; 2]` (Rust).

- [ ] **Step 1: Write the failing Rust test**

Add to the existing `#[cfg(test)]` module in `src-tauri/src/config.rs`:

```rust
    #[test]
    fn crazy_defaults_to_off_and_round_trips() {
        assert_eq!(Config::default().crazy, "off");

        // A settings file written before crazy mode existed must load with the
        // feature off rather than failing to parse.
        let older = r#"{"placement":"free","showUsage":true}"#;
        let loaded: Config = serde_json::from_str(older).expect("older config parses");
        assert_eq!(loaded.crazy, "off");

        let mut config = Config::default();
        config.crazy = "ember".into();
        let json = serde_json::to_string(&config).expect("serialises");
        let back: Config = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back.crazy, "ember");
        assert!(CRAZY_LEVELS.contains(&back.crazy.as_str()));
    }
```

- [ ] **Step 2: Run it and watch it fail**

```bash
cd src-tauri && cargo test crazy_defaults -- --test-threads=1
```

Expected: FAIL, `no field 'crazy' on type 'Config'`.

- [ ] **Step 3: Add the field**

In `src-tauri/src/config.rs`, add to `struct Config` after `keep_awake`:

```rust
    /// How theatrical the widget is allowed to be: `off`, or `ember` to let the
    /// pill catch fire, shake and fracture in response to what it is already
    /// showing. Off by default and deliberately so — the calm widget is the
    /// correct default for something that lives in the menu bar.
    pub crazy: String,
```

Add to `impl Default for Config`, after `keep_awake: false,`:

```rust
            crazy: "off".into(),
```

Add beside the existing `PLACEMENTS` const:

```rust
/// Crazy-mode levels that exist. `blaze` and `inferno` are designed in the spec
/// but not built, and are deliberately absent so the settings form cannot offer
/// a level that does nothing.
pub const CRAZY_LEVELS: [&str; 2] = ["off", "ember"];
```

- [ ] **Step 4: Run it and watch it pass**

```bash
cd src-tauri && cargo fmt && cargo test -- --test-threads=1
```

Expected: PASS, whole suite green.

- [ ] **Step 5: Mirror it in TypeScript**

In `src/types.ts`, add to `interface AppConfig` after `keepAwake`:

```ts
  /**
   * How theatrical the widget may be: `off`, or `ember`. Carried for the same
   * reason as `hidden` and `keepAwake` — `set_config` takes the whole object,
   * so a field missing here is a field written back as its default.
   */
  crazy: string
```

And after `HIDE_MODES`:

```ts
/** Mirrors config::CRAZY_LEVELS in Rust. Only levels that exist are listed. */
export const CRAZY_LEVELS = [
  { id: 'off', label: 'Off' },
  { id: 'ember', label: 'Ember — the pill catches fire' },
] as const
```

- [ ] **Step 6: Write the failing settings-form test**

Add to `src/settings/SettingsPanel.test.tsx`, following the file's existing setup for a rendered panel:

```tsx
  it('offers every crazy level and saves the chosen one', async () => {
    render(<SettingsPanel onClose={() => {}} />)
    const select = await screen.findByLabelText('Crazy mode')

    expect([...select.querySelectorAll('option')].map((o) => o.textContent)).toEqual(
      CRAZY_LEVELS.map((level) => level.label),
    )

    fireEvent.change(select, { target: { value: 'ember' } })

    expect(saved()).toMatchObject({ crazy: 'ember' })
  })
```

Import `CRAZY_LEVELS` from `../types`. `saved()` is whatever helper the file already uses to read the last `set_config` payload — reuse it rather than adding a second one; if the file asserts on the mocked `invoke` directly, assert the same way here.

- [ ] **Step 7: Run it and watch it fail**

```bash
npx vitest run src/settings/SettingsPanel.test.tsx
```

Expected: FAIL, unable to find a label "Crazy mode".

- [ ] **Step 8: Add the control**

In `src/settings/SettingsPanel.tsx`, import `CRAZY_LEVELS` from `../types` and insert immediately after the "Show the 5h limit at the end of the row" label block:

```tsx
      <label htmlFor="crazy">Crazy mode</label>
      <select id="crazy" value={config.crazy} onChange={(e) => update({ crazy: e.target.value })}>
        {CRAZY_LEVELS.map((level) => (
          <option key={level.id} value={level.id}>
            {level.label}
          </option>
        ))}
      </select>
```

- [ ] **Step 9: Run the whole front-end suite**

```bash
npm run typecheck && npm test
```

Expected: PASS.

- [ ] **Step 10: Commit**

```bash
git status --short
git add src-tauri/src/config.rs src/types.ts src/settings/SettingsPanel.tsx src/settings/SettingsPanel.test.tsx
git commit -m "feat: add the crazy mode setting"
```

---

### Task 2: Alerts and the level reach the row

`useSessions` currently drops `alerts` from the update payload, and `SessionViewProps` carries neither alerts nor the level. Both are needed before anything can burn.

**Files:**
- Modify: `src/useSessions.ts`
- Modify: `src/views/SessionView.ts`
- Modify: `src/App.tsx`
- Test: `src/useSessions.test.ts`

**Interfaces:**
- Consumes: `AppConfig['crazy']` from Task 1.
- Produces: `useSessions(): { sessions, usage, alerts, ready }` where `alerts: Alert[]`; `SessionViewProps` gains `alerts?: Alert[]` and `crazy?: string`.

- [ ] **Step 1: Write the failing test**

Add to `src/useSessions.test.ts`, matching the file's existing pattern for emitting an update:

```ts
  it('exposes the alerts that arrive with an update', async () => {
    const { result } = renderHook(() => useSessions())

    await act(async () => {
      emit({
        sessions: [],
        usage: null,
        alerts: [{ sessionId: 'a', name: 'api', kind: 'died', detail: null }],
      })
    })

    expect(result.current.alerts).toEqual([
      { sessionId: 'a', name: 'api', kind: 'died', detail: null },
    ])
  })

  it('starts with no alerts', () => {
    const { result } = renderHook(() => useSessions())
    expect(result.current.alerts).toEqual([])
  })
```

`emit` is whatever the file already uses to push an `UPDATE_EVENT` payload — reuse it.

- [ ] **Step 2: Run it and watch it fail**

```bash
npx vitest run src/useSessions.test.ts
```

Expected: FAIL, `result.current.alerts` is undefined.

- [ ] **Step 3: Carry the alerts**

In `src/useSessions.ts`, change the import to include `Alert`, add state, set it in the listener, and return it:

```ts
import { UPDATE_EVENT, type Alert, type SessionSnapshot, type Update, type Usage } from './types'

export function useSessions(): {
  sessions: SessionSnapshot[]
  usage: Usage | null
  alerts: Alert[]
  ready: boolean
} {
  const [sessions, setSessions] = useState<SessionSnapshot[]>([])
  const [usage, setUsage] = useState<Usage | null>(null)
  // Alerts describe the moment a session changed, not its current state, so
  // they are only ever set from a pushed update — there is no snapshot command
  // that could hand back a transition that has already happened.
  const [alerts, setAlerts] = useState<Alert[]>([])
```

In the `listen` callback, after `setUsage(event.payload.usage)`:

```ts
      setAlerts(event.payload.alerts ?? [])
```

And the return:

```ts
  return { sessions, usage, alerts, ready }
```

- [ ] **Step 4: Run it and watch it pass**

```bash
npx vitest run src/useSessions.test.ts
```

Expected: PASS.

- [ ] **Step 5: Widen the view props**

In `src/views/SessionView.ts`:

```ts
import type { Alert, SessionSnapshot, Usage } from '../types'

/** What a view is handed. The dot row is the only one left; `viewMode` in the
 *  config file, which used to pick between several, is parsed and ignored. */
export interface SessionViewProps {
  sessions: SessionSnapshot[]
  /**
   * Five-hour limit usage, or `null` when there is nothing worth showing —
   * which includes the user having turned the meter off.
   */
  usage?: Usage | null
  /**
   * Transitions from this update. Crazy mode's one-shot effects key off these
   * rather than off session state, because a session stays `dead` for as long
   * as it is listed but only dies once.
   */
  alerts?: Alert[]
  /** Crazy-mode level; `off` when absent. */
  crazy?: string
}
```

- [ ] **Step 6: Pass them from App**

In `src/App.tsx`, take `alerts` from the hook and hand both to the row:

```tsx
  const { sessions, usage, alerts } = useSessions()
```

```tsx
  return (
    <DotRow
      sessions={sessions}
      // Gated here rather than inside the row: "turned off" and "nothing worth
      // showing" render identically, so the row needs only the one case.
      usage={config.showUsage === false ? null : usage}
      alerts={alerts}
      crazy={config.crazy}
    />
  )
```

Leave the `NotchFlanks` branch untouched — crazy mode does not apply there.

- [ ] **Step 7: Run everything**

```bash
npm run typecheck && npm test
```

Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git status --short
git add src/useSessions.ts src/useSessions.test.ts src/views/SessionView.ts src/App.tsx
git commit -m "feat: carry alerts and the crazy level into the dot row"
```

---

### Task 3: `deriveHeat`

The pure function every effect reads from. This is where the behaviour lives and where future questions about it get answered.

**Files:**
- Create: `src/views/dotRow/heat.ts`
- Test: `src/views/dotRow/heat.test.ts`

**Interfaces:**
- Consumes: `SessionSnapshot`, `Usage`, `Alert` from `src/types.ts`; `alerts` plumbing from Task 2.
- Produces:
  ```ts
  export interface Heat {
    fire: 0 | 1 | 2 | 3
    jitter: number
    strain: 0 | 1 | 2
    ash: readonly string[]
  }
  export const JITTER_START_MS = 30_000
  export const JITTER_FULL_MS = 300_000
  export function deriveHeat(
    sessions: readonly SessionSnapshot[],
    usage: Usage | null,
    alerts: readonly Alert[],
  ): Heat
  export const CALM: Heat
  export function isCalm(heat: Heat): boolean
  ```

- [ ] **Step 1: Write the failing tests**

Create `src/views/dotRow/heat.test.ts`:

```ts
import { describe, expect, it } from 'vitest'
import type { Alert, SessionSnapshot, Usage } from '../../types'
import { deriveHeat, isCalm, JITTER_FULL_MS, JITTER_START_MS } from './heat'

function session(over: Partial<SessionSnapshot>): SessionSnapshot {
  return {
    pid: 1,
    sessionId: 's',
    name: 'repo',
    cwd: '/tmp',
    entrypoint: 'cli',
    state: 'idle',
    detail: null,
    elapsedMs: 0,
    uptimeMs: 0,
    statusTimeMs: 0,
    startedAtMs: 0,
    background: false,
    ...over,
  }
}

function usage(over: Partial<Usage>): Usage {
  return { percent: 10, resetsAtMs: 0, severity: 'normal', ...over }
}

function died(sessionId: string): Alert {
  return { sessionId, name: 'repo', kind: 'died', detail: null }
}

describe('fire', () => {
  it('counts busy sessions', () => {
    const sessions = [
      session({ sessionId: 'a', state: 'busy' }),
      session({ sessionId: 'b', state: 'busy' }),
      session({ sessionId: 'c', state: 'idle' }),
    ]
    expect(deriveHeat(sessions, null, []).fire).toBe(2)
  })

  it('caps at three however many are working', () => {
    const sessions = Array.from({ length: 7 }, (_, i) =>
      session({ sessionId: `s${i}`, state: 'busy' }),
    )
    expect(deriveHeat(sessions, null, []).fire).toBe(3)
  })

  it('ignores background jobs entirely', () => {
    const sessions = [
      session({ sessionId: 'a', state: 'busy', background: true }),
      session({ sessionId: 'b', state: 'busy', background: true }),
      session({ sessionId: 'c', state: 'busy' }),
    ]
    expect(deriveHeat(sessions, null, []).fire).toBe(1)
  })

  it('is zero when only background jobs are working', () => {
    const sessions = [session({ sessionId: 'a', state: 'busy', background: true })]
    expect(deriveHeat(sessions, null, []).fire).toBe(0)
  })
})

describe('jitter', () => {
  it('is zero below the threshold', () => {
    const waiting = session({ state: 'waiting', elapsedMs: JITTER_START_MS - 1 })
    expect(deriveHeat([waiting], null, []).jitter).toBe(0)
  })

  it('is one at and beyond five minutes', () => {
    const at = session({ state: 'waiting', elapsedMs: JITTER_FULL_MS })
    const beyond = session({ state: 'waiting', elapsedMs: JITTER_FULL_MS * 4 })
    expect(deriveHeat([at], null, []).jitter).toBe(1)
    expect(deriveHeat([beyond], null, []).jitter).toBe(1)
  })

  it('ramps linearly between the two', () => {
    const half = JITTER_START_MS + (JITTER_FULL_MS - JITTER_START_MS) / 2
    const waiting = session({ state: 'waiting', elapsedMs: half })
    expect(deriveHeat([waiting], null, []).jitter).toBeCloseTo(0.5, 5)
  })

  it('reads the longest wait, not the first', () => {
    const sessions = [
      session({ sessionId: 'a', state: 'waiting', elapsedMs: JITTER_START_MS }),
      session({ sessionId: 'b', state: 'waiting', elapsedMs: JITTER_FULL_MS }),
    ]
    expect(deriveHeat(sessions, null, []).jitter).toBe(1)
  })

  it('ignores sessions that are not waiting', () => {
    const busy = session({ state: 'busy', elapsedMs: JITTER_FULL_MS })
    expect(deriveHeat([busy], null, []).jitter).toBe(0)
  })

  it('ignores background jobs', () => {
    const job = session({ state: 'waiting', elapsedMs: JITTER_FULL_MS, background: true })
    expect(deriveHeat([job], null, []).jitter).toBe(0)
  })
})

describe('strain', () => {
  it('maps each severity', () => {
    expect(deriveHeat([], usage({ severity: 'normal' }), []).strain).toBe(0)
    expect(deriveHeat([], usage({ severity: 'warn' }), []).strain).toBe(1)
    expect(deriveHeat([], usage({ severity: 'critical' }), []).strain).toBe(2)
  })

  it('is zero when there is no usage to read', () => {
    expect(deriveHeat([], null, []).strain).toBe(0)
  })
})

describe('ash', () => {
  it('lists the sessions that died in this update', () => {
    expect(deriveHeat([], null, [died('a'), died('b')]).ash).toEqual(['a', 'b'])
  })

  it('ignores alerts of other kinds', () => {
    const alerts: Alert[] = [
      { sessionId: 'a', name: 'repo', kind: 'needsInput', detail: null },
      { sessionId: 'b', name: 'repo', kind: 'finished', detail: null },
    ]
    expect(deriveHeat([], null, alerts).ash).toEqual([])
  })

  it('is empty for a session that is merely dead without a fresh alert', () => {
    const dead = session({ sessionId: 'a', state: 'dead' })
    expect(deriveHeat([dead], null, []).ash).toEqual([])
  })
})

describe('isCalm', () => {
  it('is true when nothing is happening', () => {
    expect(isCalm(deriveHeat([session({})], null, []))).toBe(true)
  })

  it('is false as soon as anything is', () => {
    expect(isCalm(deriveHeat([session({ state: 'busy' })], null, []))).toBe(false)
    expect(isCalm(deriveHeat([], usage({ severity: 'warn' }), []))).toBe(false)
    expect(isCalm(deriveHeat([], null, [died('a')]))).toBe(false)
  })
})
```

- [ ] **Step 2: Run and watch it fail**

```bash
npx vitest run src/views/dotRow/heat.test.ts
```

Expected: FAIL, cannot resolve `./heat`.

- [ ] **Step 3: Write the implementation**

Create `src/views/dotRow/heat.ts`:

```ts
import type { Alert, SessionSnapshot, Usage } from '../../types'

/**
 * How intense the widget should look, derived from what it is already showing.
 *
 * Four separate figures rather than one blended number. A single "intensity"
 * would say the widget is agitated without saying why, which is less than the
 * five dots already tell you — the point of crazy mode is to add information,
 * not to trade it for spectacle.
 *
 * Pure and clock-free, in the shape of `visibility::should_hide` and
 * `watcher::state::snapshot`, so every question about when the widget burns is
 * answered and tested here rather than in a component.
 */
export interface Heat {
  /** Busy foreground sessions, capped at 3. */
  fire: 0 | 1 | 2 | 3
  /** 0 at 30s of waiting, 1 at five minutes, linear between. */
  jitter: number
  /** 0 normal, 1 warn, 2 critical. */
  strain: 0 | 1 | 2
  /** Sessions that died in this update. */
  ash: readonly string[]
}

/**
 * A session that has only just asked a question does not need the widget to
 * panic on its behalf, so nothing moves for the first half minute.
 */
export const JITTER_START_MS = 30_000

/** Where the shake reaches full amplitude. */
export const JITTER_FULL_MS = 300_000

export const CALM: Heat = { fire: 0, jitter: 0, strain: 0, ash: [] }

const SEVERITY: Record<Usage['severity'], 0 | 1 | 2> = {
  normal: 0,
  warn: 1,
  critical: 2,
}

export function deriveHeat(
  sessions: readonly SessionSnapshot[],
  usage: Usage | null,
  alerts: readonly Alert[],
): Heat {
  // Background jobs are already demoted to 0.55 opacity because they are work
  // you did not start. Setting the widget alight for a subagent would be the
  // same mistake in a louder voice.
  const own = sessions.filter((s) => !s.background)

  const busy = own.filter((s) => s.state === 'busy').length
  const fire = Math.min(3, busy) as Heat['fire']

  const waited = own
    .filter((s) => s.state === 'waiting')
    .reduce((longest, s) => Math.max(longest, s.elapsedMs), 0)
  const span = JITTER_FULL_MS - JITTER_START_MS
  const jitter = Math.min(1, Math.max(0, (waited - JITTER_START_MS) / span))

  const strain = usage === null ? 0 : SEVERITY[usage.severity]

  // Keyed off the alert, not the state: a session stays `dead` for as long as
  // it is listed, which can be hours, but dying happens once and the alert is
  // that moment.
  const ash = alerts.filter((a) => a.kind === 'died').map((a) => a.sessionId)

  return { fire, jitter, strain, ash }
}

/** Whether anything at all is worth drawing. Nothing mounts when this is true. */
export function isCalm(heat: Heat): boolean {
  return heat.fire === 0 && heat.jitter === 0 && heat.strain === 0 && heat.ash.length === 0
}
```

- [ ] **Step 4: Run and watch it pass**

```bash
npx vitest run src/views/dotRow/heat.test.ts
```

Expected: PASS, 17 tests.

- [ ] **Step 5: Commit**

```bash
git status --short
git add src/views/dotRow/heat.ts src/views/dotRow/heat.test.ts
git commit -m "feat: derive how hot the widget should look"
```

---

### Task 4: Fire

Mounts the wrappers and the fire layers, and wires `DotRow` to `deriveHeat`. At the end of this task the pill glows and burns as sessions go busy.

**Files:**
- Create: `src/views/dotRow/crazy.css`
- Modify: `src/views/dotRow/DotRow.tsx`
- Test: `src/views/dotRow/DotRow.test.tsx`

**Interfaces:**
- Consumes: `deriveHeat`, `isCalm`, `Heat`, `CALM` from Task 3; `alerts` and `crazy` props from Task 2.
- Produces: the DOM contract every later task styles against —
  `.crazy-shake > .crazy-shudder > .pill`, with `.pill[data-fire]`, `.pill[data-strain]`, `.pill[data-ash]` and the child layers `.crazy-heat`, `.crazy-flames`, `.crazy-spark`, `.crazy-cracks`.

- [ ] **Step 1: Write the failing tests**

Add to `src/views/dotRow/DotRow.test.tsx`:

```tsx
  it('mounts nothing extra when crazy mode is off', () => {
    const { container } = render(
      <DotRow sessions={[busySession('a'), busySession('b')]} crazy="off" />,
    )
    expect(container.querySelector('.crazy-shake')).toBeNull()
    expect(container.querySelector('.crazy-heat')).toBeNull()
    expect(container.querySelector('.crazy-flames')).toBeNull()
    expect(container.querySelector('.pill')?.getAttribute('data-fire')).toBeNull()
  })

  it('mounts nothing extra when crazy mode is on but nothing is happening', () => {
    const { container } = render(<DotRow sessions={[idleSession('a')]} crazy="ember" />)
    expect(container.querySelector('.crazy-heat')).toBeNull()
    expect(container.querySelector('.crazy-shake')).toBeNull()
  })

  it('burns at the level the busy count calls for', () => {
    const { container } = render(
      <DotRow
        sessions={[busySession('a'), busySession('b'), busySession('c')]}
        crazy="ember"
      />,
    )
    expect(container.querySelector('.pill')?.getAttribute('data-fire')).toBe('3')
    expect(container.querySelectorAll('.crazy-flames i')).toHaveLength(8)
    expect(container.querySelectorAll('.crazy-spark')).toHaveLength(4)
  })
```

Add these helpers near the top of the file if it has no equivalent, otherwise reuse whatever session factory it already defines:

```tsx
function busySession(id: string) {
  return makeSession({ sessionId: id, state: 'busy' })
}

function idleSession(id: string) {
  return makeSession({ sessionId: id, state: 'idle' })
}
```

- [ ] **Step 2: Run and watch it fail**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: FAIL, `data-fire` is null on the burning case.

- [ ] **Step 3: Write the fire CSS**

Create `src/views/dotRow/crazy.css`:

```css
/* Crazy mode.
   Kept out of dotRow.css deliberately: that file is load-bearing layout —
   window sizing, morph timing, the notch anchor — and burying fire keyframes in
   it would make both harder to read. Nothing here is mounted unless the setting
   is on and something is actually happening, so an idle machine animates
   nothing at all.

   Every animation below lives on a wrapper or a child layer, never on .pill.
   .pill already owns an animation — flash-attention, when a session needs input
   — and the CSS animation shorthand does not compose across rules. The flash
   outranks everything here and keeps its element. */

.crazy-shake,
.crazy-shudder {
  display: inline-block;
}

/* ── Fire ─────────────────────────────────────────────────────────────
   The glow lives on its own layer rather than on the pill's background so the
   brightness pulse repaints one small element instead of the whole pill. */
.crazy-heat {
  position: absolute;
  inset: 0;
  z-index: 1;
  pointer-events: none;
  border-radius: inherit;
}

.pill[data-fire='1'] .crazy-heat {
  background: radial-gradient(130% 200% at 50% 135%, rgba(255, 140, 50, 0.15), rgba(255, 90, 20, 0) 58%);
}

.pill[data-fire='2'] .crazy-heat {
  background: radial-gradient(130% 200% at 50% 130%, rgba(255, 120, 30, 0.3), rgba(255, 90, 20, 0) 62%);
  animation: crazy-breathe 2.6s ease-in-out infinite;
}

.pill[data-fire='3'] .crazy-heat {
  background: radial-gradient(130% 190% at 50% 125%, rgba(255, 96, 16, 0.52), rgba(255, 90, 20, 0) 66%);
  animation: crazy-breathe 1.5s ease-in-out infinite;
}

@keyframes crazy-breathe {
  50% {
    filter: brightness(1.2);
  }
}

/* The border and the outer glow are the one thing that must be on the pill, and
   they are transitions rather than animations, so they do not collide with the
   flash. */
.pill[data-fire='1'] {
  border-color: rgba(255, 150, 70, 0.24);
}

.pill[data-fire='2'] {
  border-color: rgba(255, 140, 50, 0.44);
  box-shadow: 0 8px 20px rgba(0, 0, 0, 0.5), 0 0 20px rgba(255, 110, 30, 0.28),
    inset 0 1px 0 rgba(255, 190, 120, 0.14);
}

.pill[data-fire='3'] {
  border-color: rgba(255, 130, 40, 0.7);
  box-shadow: 0 8px 22px rgba(0, 0, 0, 0.55), 0 0 32px rgba(255, 90, 20, 0.46),
    inset 0 1px 0 rgba(255, 200, 140, 0.22);
}

/* Clipped to the pill by the overflow:hidden dotRow.css already sets so the
   collapsed and expanded variants reveal as the box grows. */
.crazy-flames {
  position: absolute;
  inset: auto 0 -2px 0;
  height: 24px;
  z-index: 2;
  pointer-events: none;
}

/* A fixed count at every level, with opacity and height carrying the ramp.
   Varying the count would remount DOM every time a session started or stopped,
   and the worst case is bounded at eight. */
.crazy-flames i {
  position: absolute;
  bottom: -6px;
  width: 12px;
  height: 18px;
  border-radius: 50% 50% 46% 46% / 62% 62% 38% 38%;
  background: linear-gradient(to top, #ffd07a, #ff8a1e 42%, rgba(255, 72, 10, 0));
  filter: blur(2.5px);
  mix-blend-mode: screen;
  transform-origin: 50% 100%;
  animation: crazy-lick 1.1s ease-in-out infinite;
  opacity: 0.5;
}

.pill[data-fire='2'] .crazy-flames i {
  height: 20px;
  opacity: 0.85;
}

.pill[data-fire='3'] .crazy-flames i {
  height: 23px;
  opacity: 1;
  animation-duration: 0.8s;
}

@keyframes crazy-lick {
  0%,
  100% {
    transform: scaleY(0.5) scaleX(0.9);
  }
  50% {
    transform: scaleY(1.2) scaleX(1.05);
  }
}

.crazy-spark {
  position: absolute;
  width: 2px;
  height: 2px;
  border-radius: 50%;
  background: #ffd9a0;
  box-shadow: 0 0 3px rgba(255, 180, 90, 0.9);
  z-index: 3;
  opacity: 0;
  pointer-events: none;
}

.pill[data-fire='2'] .crazy-spark {
  animation: crazy-rise 3s linear infinite;
}

.pill[data-fire='3'] .crazy-spark {
  animation: crazy-rise 1.6s linear infinite;
}

@keyframes crazy-rise {
  0% {
    transform: translateY(4px) scale(0.5);
    opacity: 0;
  }
  22% {
    opacity: 1;
  }
  100% {
    transform: translateY(-20px) scale(1);
    opacity: 0;
  }
}
```

- [ ] **Step 4: Wire it into DotRow**

In `src/views/dotRow/DotRow.tsx`:

Add to the imports:

```tsx
import { CALM, deriveHeat, isCalm } from './heat'
import './crazy.css'
```

Change the signature:

```tsx
export function DotRow({ sessions, usage = null, alerts = [], crazy = 'off' }: SessionViewProps) {
```

Add just above the `return`, after the last effect:

```tsx
  // Off is free: nothing is derived, no attribute is written, and no element is
  // mounted, so the widget is byte-for-byte what it was before crazy mode.
  const heat = crazy === 'ember' ? deriveHeat(sessions, usage, alerts) : CALM
  const lit = !isCalm(heat)

  // Fixed element counts, so changing level never remounts DOM. The staggered
  // delays are what make one keyframe track look like a fire rather than eight
  // things doing the same thing at once.
  const flames = lit && heat.fire > 0 ? FLAME_OFFSETS : []
  const sparks = lit && heat.fire >= 2 ? SPARK_OFFSETS : []
```

Add above the component:

```tsx
/** Where each flame sits along the pill, and how far into its cycle it starts. */
const FLAME_OFFSETS = [
  { left: '3%', delay: '0s' },
  { left: '15%', delay: '-0.3s' },
  { left: '27%', delay: '-0.7s' },
  { left: '39%', delay: '-0.15s' },
  { left: '51%', delay: '-0.55s' },
  { left: '63%', delay: '-0.9s' },
  { left: '75%', delay: '-0.25s' },
  { left: '88%', delay: '-0.65s' },
]

const SPARK_OFFSETS = [
  { left: '18%', bottom: '9px', delay: '0s' },
  { left: '38%', bottom: '6px', delay: '-0.5s' },
  { left: '58%', bottom: '8px', delay: '-0.9s' },
  { left: '80%', bottom: '5px', delay: '-1.3s' },
]
```

Wrap the pill. The existing `<div ref={pillRef} className="pill" …>` keeps every attribute it has; only `data-fire` is added and two wrappers go around it:

```tsx
      <div className={lit ? 'crazy-shake' : undefined}>
        <div className={lit ? 'crazy-shudder' : undefined}>
          <div
            ref={pillRef}
            className="pill"
            data-fire={lit && heat.fire > 0 ? String(heat.fire) : undefined}
            style={
              {
                '--morph': `${morphMs}ms`,
                ...(pillBox === null ? {} : { width: pillBox.width, height: pillBox.height }),
              } as CSSProperties
            }
          >
            {lit && heat.fire > 0 && <span className="crazy-heat" />}
            {flames.length > 0 && (
              <span className="crazy-flames">
                {flames.map((flame) => (
                  <i
                    key={flame.left}
                    style={{ left: flame.left, animationDelay: flame.delay }}
                  />
                ))}
              </span>
            )}
            {sparks.map((spark) => (
              <span
                key={spark.left}
                className="crazy-spark"
                style={{ left: spark.left, bottom: spark.bottom, animationDelay: spark.delay }}
              />
            ))}
            <div className="variant-slot" ref={collapsedSlot} data-show={showNamed ? 'false' : 'true'}>
              <CollapsedPill sessions={sessions} usage={usage} />
            </div>
            <div className="variant-slot" ref={expandedSlot} data-show={showNamed ? 'true' : 'false'}>
              <NamedDotRow
                usage={usage}
                sessions={sessions}
                hoveredSessionId={hoveredSessionId}
                onHoverSession={setHoveredSessionId}
              />
            </div>
          </div>
        </div>
      </div>
```

Note: the wrapper `div`s are always rendered so the pill is never remounted as heat comes and goes — a remount would restart the box morph mid-flight. Only their class is conditional, and a `div` with no class is inert.

- [ ] **Step 5: Run the tests**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: PASS. If the sizing tests in this file fail because they query `.dot-row > .pill` directly, relax that query to `.dot-row .pill` — the pill is now two levels deeper.

- [ ] **Step 6: Run everything**

```bash
npm run typecheck && npm test
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git status --short
git add src/views/dotRow/crazy.css src/views/dotRow/DotRow.tsx src/views/dotRow/DotRow.test.tsx
git commit -m "feat: set the pill on fire when sessions are working"
```

---

### Task 5: Jitter and shudder

The two transform animations, on the two wrappers.

**Files:**
- Modify: `src/views/dotRow/crazy.css`
- Modify: `src/views/dotRow/DotRow.tsx`
- Test: `src/views/dotRow/DotRow.test.tsx`

**Interfaces:**
- Consumes: the wrapper elements and `heat` from Task 4.
- Produces: `--crazy-amp` on `.crazy-shake`, `data-shake` and `data-shudder` attributes.

- [ ] **Step 1: Write the failing tests**

Add to `src/views/dotRow/DotRow.test.tsx`:

```tsx
  it('shakes harder the longer a session has waited', () => {
    const waited = makeSession({ sessionId: 'a', state: 'waiting', elapsedMs: 300_000 })
    const { container } = render(<DotRow sessions={[waited]} crazy="ember" />)
    const shake = container.querySelector<HTMLElement>('.crazy-shake')

    expect(shake?.getAttribute('data-shake')).toBe('true')
    expect(shake?.style.getPropertyValue('--crazy-amp')).toBe('1.4')
  })

  it('does not shake for a session that has only just asked', () => {
    const fresh = makeSession({ sessionId: 'a', state: 'waiting', elapsedMs: 1_000 })
    const { container } = render(<DotRow sessions={[fresh]} crazy="ember" />)
    expect(container.querySelector('.crazy-shake')?.getAttribute('data-shake')).toBeNull()
  })

  it('shudders only at critical usage', () => {
    const usage = { percent: 96, resetsAtMs: 0, severity: 'critical' as const }
    const { container } = render(
      <DotRow sessions={[makeSession({ sessionId: 'a' })]} usage={usage} crazy="ember" />,
    )
    expect(container.querySelector('.crazy-shudder')?.getAttribute('data-shudder')).toBe('true')
  })
```

- [ ] **Step 2: Run and watch it fail**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: FAIL, `data-shake` is null.

- [ ] **Step 3: Add the CSS**

Append to `src/views/dotRow/crazy.css`:

```css
/* ── Jitter ───────────────────────────────────────────────────────────
   Peak displacement is --crazy-amp px, which tops out at 1.4. --shadow-pad is
   30px, so the shake stays far inside the window: no resize, no clipping, and
   useWidgetSize needs no changes. */
.crazy-shake[data-shake='true'] {
  animation: crazy-shake 0.22s linear infinite;
}

@keyframes crazy-shake {
  0%,
  100% {
    transform: translate(0, 0);
  }
  20% {
    transform: translate(calc(var(--crazy-amp, 0) * -1px), calc(var(--crazy-amp, 0) * 0.4px));
  }
  40% {
    transform: translate(calc(var(--crazy-amp, 0) * 0.8px), calc(var(--crazy-amp, 0) * -0.5px));
  }
  60% {
    transform: translate(calc(var(--crazy-amp, 0) * -0.6px), calc(var(--crazy-amp, 0) * -0.3px));
  }
  80% {
    transform: translate(calc(var(--crazy-amp, 0) * 0.7px), calc(var(--crazy-amp, 0) * 0.5px));
  }
}

/* Slower and smaller than the shake, so strain reads as something giving way
   rather than as the fast agitation of a session waiting on you. */
.crazy-shudder[data-shudder='true'] {
  animation: crazy-shudder 3s ease-in-out infinite;
}

@keyframes crazy-shudder {
  48%,
  52% {
    transform: translateX(0.6px);
  }
  50% {
    transform: translateX(-0.6px);
  }
}
```

- [ ] **Step 4: Wire it up**

In `src/views/dotRow/DotRow.tsx`, after the `heat` / `lit` lines:

```tsx
  // The shake stops while the pointer is over the widget. Entries have hover
  // states and open popovers, and a pill shaking under the cursor makes hovering
  // a moving target — by the time you are pointing at it, it has done its job.
  //
  // `cursor.inside`, not CSS :hover: the widget is a non-activating NSPanel, so
  // WKWebView never delivers mouse events to the page and :hover never fires.
  const shaking = lit && heat.jitter > 0 && !cursor.inside
  const shuddering = lit && heat.strain === 2 && !cursor.inside
```

And on the wrappers:

```tsx
      <div
        className={lit ? 'crazy-shake' : undefined}
        data-shake={shaking ? 'true' : undefined}
        style={shaking ? ({ '--crazy-amp': heat.jitter * 1.4 } as CSSProperties) : undefined}
      >
        <div
          className={lit ? 'crazy-shudder' : undefined}
          data-shudder={shuddering ? 'true' : undefined}
        >
```

- [ ] **Step 5: Run and watch it pass**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: PASS. Note the test asserts `--crazy-amp` is `1.4` for a five-minute wait: `jitter` is 1 there, and `1 * 1.4` stringifies as `1.4`.

- [ ] **Step 6: Run everything and commit**

```bash
npm run typecheck && npm test
git status --short
git add src/views/dotRow/crazy.css src/views/dotRow/DotRow.tsx src/views/dotRow/DotRow.test.tsx
git commit -m "feat: shake the pill while a session waits, shudder as the limit runs out"
```

---

### Task 6: Strain

Cracks across the pill, and a molten meter when the usage bar is on screen.

**Files:**
- Modify: `src/views/dotRow/crazy.css`
- Modify: `src/views/dotRow/DotRow.tsx`
- Test: `src/views/dotRow/DotRow.test.tsx`

**Interfaces:**
- Consumes: `heat.strain`, `heat.fire`, the layer contract from Task 4.
- Produces: `.crazy-cracks` (an inline SVG), `data-strain` on the pill.

- [ ] **Step 1: Write the failing tests**

Add to `src/views/dotRow/DotRow.test.tsx`:

```tsx
  it('fractures the pill as the limit runs down', () => {
    const usage = { percent: 96, resetsAtMs: 0, severity: 'critical' as const }
    const { container } = render(
      <DotRow sessions={[makeSession({ sessionId: 'a' })]} usage={usage} crazy="ember" />,
    )
    expect(container.querySelector('.pill')?.getAttribute('data-strain')).toBe('2')
    // Each crack is drawn twice: a dark underlay, then a light hairline. One
    // stroke vanishes against the flames, the other against the pill.
    expect(container.querySelectorAll('.crazy-cracks path')).toHaveLength(10)
  })

  it('does not fracture while usage is normal', () => {
    const usage = { percent: 8, resetsAtMs: 0, severity: 'normal' as const }
    const { container } = render(
      <DotRow sessions={[makeSession({ sessionId: 'a' })]} usage={usage} crazy="ember" />,
    )
    expect(container.querySelector('.crazy-cracks')).toBeNull()
  })
```

- [ ] **Step 2: Run and watch it fail**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: FAIL, `.crazy-cracks` is null.

- [ ] **Step 3: Add the CSS**

Append to `src/views/dotRow/crazy.css`:

```css
/* ── Strain ───────────────────────────────────────────────────────────
   Above the fire layers but below the dots and the summary. The pill's width
   tracks its content, so a crack landing across a glyph run at a narrow width
   would trade legibility for decoration; under the content it still reads as
   damage to the pill's surface. */
.crazy-cracks {
  position: absolute;
  inset: 0;
  z-index: 4;
  pointer-events: none;
}

.crazy-cracks svg {
  display: block;
  width: 100%;
  height: 100%;
}

.crazy-cracks path {
  fill: none;
}

/* Two strokes per crack. A light hairline alone disappears against the flames
   and a dark one alone disappears against the pill, so each path is drawn as a
   dark underlay with a light line on top. */
.crazy-cracks .crack-dark {
  stroke: rgba(20, 6, 2, 0.75);
  stroke-width: 2.2;
}

.crazy-cracks .crack-lite {
  stroke: rgba(255, 236, 208, 0.75);
  stroke-width: 0.8;
}

.pill[data-strain='1'] .crazy-cracks {
  opacity: 0.35;
}

.pill[data-strain='2'] .crazy-cracks {
  opacity: 0.8;
}

/* Fire owns the pill from two sessions up. The cracks stay legible without
   competing with it for the same pixels. */
.pill[data-fire='2'] .crazy-cracks,
.pill[data-fire='3'] .crazy-cracks {
  opacity: 0.36;
}

/* And the flames give the cracks room to read. Same specificity as the
   per-level heights above and later in the file, so this wins wherever both
   apply. */
.pill[data-strain] .crazy-flames i {
  height: 19px;
}

/* The molten meter. Reached by descendant selector rather than by a prop, so
   UsageMeter is untouched and both variants get it for free. Only at critical,
   and only when the meter is on screen at all. */
.pill[data-strain='2'] .usage-fill {
  background: linear-gradient(90deg, #ff6a10, #ffc76a 60%, #fff0c9);
  box-shadow: 0 0 8px rgba(255, 120, 30, 0.85);
  animation: crazy-molten 1.2s ease-in-out infinite;
}

@keyframes crazy-molten {
  50% {
    box-shadow: 0 0 14px rgba(255, 150, 50, 1);
  }
}

.pill[data-strain='2'] .usage-track::after {
  content: '';
  position: absolute;
  top: 2px;
  right: 2px;
  width: 3px;
  height: 3px;
  border-radius: 0 0 50% 50%;
  background: #ffb257;
  box-shadow: 0 0 5px rgba(255, 140, 40, 0.9);
  animation: crazy-drip 2.2s ease-in infinite;
}

@keyframes crazy-drip {
  0% {
    transform: translateY(0) scaleY(0.6);
    opacity: 0;
  }
  25% {
    opacity: 1;
  }
  100% {
    transform: translateY(11px) scaleY(1.5);
    opacity: 0;
  }
}
```

`dotRow.css` sets `overflow: hidden` on `.usage-track` so the fill stays inside the track's rounded ends. The drip has to fall out of it, so lift the clip from `crazy.css` — do not edit `dotRow.css`, whose rule is correct for every other case:

```css
/* The track clips its fill so the bar's end stays inside the rounded ends. The
   drip has to fall out of it, so the clip is lifted only while it is dripping —
   the fill keeps its own border-radius, which is what the clip was for. */
.pill[data-strain='2'] .usage-track {
  overflow: visible;
}
```

- [ ] **Step 4: Wire it up**

In `src/views/dotRow/DotRow.tsx`, add above the component:

```tsx
/** Five fractures across the pill, each drawn twice so it reads over fire and
 *  over the dark background alike. Stretched to whatever width the pill is. */
const CRACKS = [
  'M52 0 L60 15 L48 24 L56 42',
  'M148 0 L140 13 L152 22 L144 42',
  'M96 5 L104 19 L90 27',
  'M228 0 L221 17 L234 25 L226 42',
  'M18 9 L27 21 L15 31',
]
```

Add to the derived values:

```tsx
  const cracked = lit && heat.strain > 0
```

Add `data-strain` to the pill and the layer inside it, immediately after the sparks:

```tsx
            data-strain={cracked ? String(heat.strain) : undefined}
```

```tsx
            {cracked && (
              <span className="crazy-cracks" aria-hidden="true">
                <svg viewBox="0 0 300 42" preserveAspectRatio="none">
                  {CRACKS.map((d) => (
                    <path key={d} className="crack-dark" d={d} />
                  ))}
                  {CRACKS.map((d) => (
                    <path key={d} className="crack-lite" d={d} />
                  ))}
                </svg>
              </span>
            )}
```

- [ ] **Step 5: Run and watch it pass**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: PASS.

- [ ] **Step 6: Run everything and commit**

```bash
npm run typecheck && npm test
git status --short
git add src/views/dotRow/crazy.css src/views/dotRow/DotRow.tsx src/views/dotRow/DotRow.test.tsx
git commit -m "feat: fracture the pill as the five-hour limit runs down"
```

---

### Task 7: Ash

A one-shot crumble when a session dies.

**Files:**
- Modify: `src/views/dotRow/crazy.css`
- Modify: `src/views/dotRow/DotRow.tsx`
- Modify: `src/views/dotRow/NamedDotRow.tsx`
- Modify: `src/views/dotRow/SessionEntry.tsx`
- Test: `src/views/dotRow/DotRow.test.tsx`

**Interfaces:**
- Consumes: `heat.ash` from Task 3.
- Produces: `NamedDotRow` gains `ashing?: readonly string[]`; `SessionEntry` gains `ashing?: boolean`; `.pill[data-ash='true']`.

- [ ] **Step 1: Write the failing test**

Add to `src/views/dotRow/DotRow.test.tsx`:

```tsx
  it('crumbles the dead dot when a session dies, then stops', async () => {
    vi.useFakeTimers()
    try {
      const dead = makeSession({ sessionId: 'a', state: 'dead' })
      const alerts = [{ sessionId: 'a', name: 'repo', kind: 'died' as const, detail: null }]
      const { container } = render(<DotRow sessions={[dead]} alerts={alerts} crazy="ember" />)

      expect(container.querySelector('.pill')?.getAttribute('data-ash')).toBe('true')

      // Held for the length of the animation and no longer: a dead session can
      // sit in the list for hours, and an effect that outlived the moment would
      // be permanent noise.
      await act(async () => {
        vi.advanceTimersByTime(1_500)
      })
      expect(container.querySelector('.pill')?.getAttribute('data-ash')).toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })
```

- [ ] **Step 2: Run and watch it fail**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: FAIL, `data-ash` is null.

- [ ] **Step 3: Add the CSS**

Append to `src/views/dotRow/crazy.css`:

```css
/* ── Ash ──────────────────────────────────────────────────────────────
   Dying is a moment, not a condition. The sequence runs once and the dot
   settles back to the ordinary dead cross it draws the rest of the time. */
.pill[data-ash='true'] .dot-dead::before {
  animation: crazy-fall-a 1.4s ease-in 1;
}

.pill[data-ash='true'] .dot-dead::after {
  animation: crazy-fall-b 1.4s ease-in 1;
}

@keyframes crazy-fall-a {
  0%,
  16% {
    transform: rotate(45deg) translate(0, 0);
    opacity: 1;
  }
  55% {
    transform: rotate(88deg) translate(2px, 7px);
    opacity: 0;
  }
  100% {
    transform: rotate(45deg) translate(0, 0);
    opacity: 1;
  }
}

@keyframes crazy-fall-b {
  0%,
  20% {
    transform: rotate(-45deg) translate(0, 0);
    opacity: 1;
  }
  62% {
    transform: rotate(-96deg) translate(-3px, 8px);
    opacity: 0;
  }
  100% {
    transform: rotate(-45deg) translate(0, 0);
    opacity: 1;
  }
}

.crazy-flake {
  position: absolute;
  width: 2px;
  height: 2px;
  border-radius: 50%;
  background: #8c7f78;
  opacity: 0;
  pointer-events: none;
  animation: crazy-drift 1.4s ease-in 1;
}

@keyframes crazy-drift {
  0%,
  20% {
    transform: translate(0, 0);
    opacity: 0;
  }
  34% {
    opacity: 0.9;
  }
  70%,
  100% {
    transform: translate(var(--crazy-drift, 2px), 13px);
    opacity: 0;
  }
}
```

- [ ] **Step 4: Hold the ids for the length of the animation**

In `src/views/dotRow/DotRow.tsx`, add state and an effect. `heat.ash` is non-empty for exactly one update, and React would strip the class on the next render and kill the animation halfway, so the ids are held:

```tsx
  // `died` alerts arrive on one update and are gone by the next. The crumble
  // takes 1.4s, so the ids are held for that long — without this the class is
  // removed on the following render and the animation stops halfway.
  const [ashing, setAshing] = useState<readonly string[]>([])
  const ashKey = heat.ash.join(',')
  useEffect(() => {
    if (ashKey === '') return
    setAshing(ashKey.split(','))
    const timer = setTimeout(() => setAshing([]), ASH_MS)
    return () => clearTimeout(timer)
  }, [ashKey])
```

Add beside `HOVER_GRACE_MS`:

```tsx
/** How long the crumble runs, matching `crazy-fall-a` in crazy.css. */
const ASH_MS = 1400
```

Note: `heat` is computed below the effects in Task 4's edit. Move the `const heat = …` / `const lit = …` lines above this effect so `heat.ash` is in scope — they are pure derivations with no dependency on anything below them.

Put `data-ash` on the pill:

```tsx
            data-ash={ashing.length > 0 ? 'true' : undefined}
```

And pass the ids to the expanded row:

```tsx
              <NamedDotRow
                usage={usage}
                sessions={sessions}
                hoveredSessionId={hoveredSessionId}
                onHoverSession={setHoveredSessionId}
                ashing={ashing}
              />
```

- [ ] **Step 5: Carry it to the entry**

In `src/views/dotRow/NamedDotRow.tsx`, add to `interface Props`:

```tsx
  /** Sessions crumbling right now, so only the one that died animates. */
  ashing?: readonly string[]
```

Destructure it with an empty default and pass it down:

```tsx
export function NamedDotRow({
  sessions,
  hoveredSessionId,
  onHoverSession,
  onHoverOffset,
  usage = null,
  ashing = [],
}: Props) {
```

```tsx
          ashing={ashing.includes(session.sessionId)}
```

In `src/views/dotRow/SessionEntry.tsx`, add to `interface Props`:

```tsx
  /** Whether this session just died and its dot should crumble. */
  ashing?: boolean
```

Destructure with `ashing = false` and put it on the dot:

```tsx
        <span className={`dot dot-${session.state}`} data-ash={ashing ? 'true' : undefined} />
```

Add the matching rule to `crazy.css`, which is what makes the per-session case precise while the collapsed chip's aggregate dot uses the pill-level attribute:

```css
/* The expanded row has a dot per session, so only the one that died crumbles.
   The collapsed row's "N died" chip is an aggregate with a single dot and no
   session to be precise about, so it follows the pill. */
.pill[data-ash='true'] .entry .dot-dead:not([data-ash='true'])::before,
.pill[data-ash='true'] .entry .dot-dead:not([data-ash='true'])::after {
  animation: none;
}
```

- [ ] **Step 6: Run and watch it pass**

```bash
npx vitest run src/views/dotRow/DotRow.test.tsx
```

Expected: PASS.

- [ ] **Step 7: Run everything and commit**

```bash
npm run typecheck && npm test
git status --short
git add src/views/dotRow/crazy.css src/views/dotRow/DotRow.tsx src/views/dotRow/NamedDotRow.tsx src/views/dotRow/SessionEntry.tsx src/views/dotRow/DotRow.test.tsx
git commit -m "feat: crumble the dot when a session dies"
```

---

### Task 8: Reduced motion

Caps crazy mode rather than killing it: the colour ramp survives, everything that moves goes.

**Files:**
- Modify: `src/views/dotRow/crazy.css`

**Interfaces:**
- Consumes: every class and keyframe from Tasks 4–7.
- Produces: nothing new.

- [ ] **Step 1: Add the block**

Append to `src/views/dotRow/crazy.css`:

```css
/* ── Reduced motion ───────────────────────────────────────────────────
   Capped, not killed. The glow, the border warmth and the cracks are what
   carry the 0→3 ramp, and none of them move — so the signal survives intact
   for someone who has asked the system not to animate things at them.

   Flames, sparks and the drip are removed rather than frozen: a static blurred
   smear reads as a rendering fault, not as a design. */
@media (prefers-reduced-motion: reduce) {
  .crazy-flames,
  .crazy-spark,
  .crazy-flake,
  .pill[data-strain='2'] .usage-track::after {
    display: none;
  }

  .crazy-heat,
  .crazy-shake[data-shake='true'],
  .crazy-shudder[data-shudder='true'],
  .pill[data-strain='2'] .usage-fill,
  .pill[data-ash='true'] .dot-dead::before,
  .pill[data-ash='true'] .dot-dead::after {
    animation: none;
  }

  /* The gradient stays; only its pulse goes. */
  .pill[data-strain='2'] .usage-fill {
    box-shadow: 0 0 8px rgba(255, 120, 30, 0.85);
  }
}
```

- [ ] **Step 2: Verify nothing else broke**

```bash
npm run typecheck && npm test
```

Expected: PASS. There is no unit test for a media query; this is verified by eye in Task 9's live run.

- [ ] **Step 3: Commit**

```bash
git status --short
git add src/views/dotRow/crazy.css
git commit -m "feat: cap crazy mode under reduced motion rather than killing it"
```

---

### Task 9: Documentation and a live look

The step that gets skipped because nothing fails when it does.

**Files:**
- Modify: `README.md`
- Modify: `CHANGELOG.md`

**Interfaces:**
- Consumes: everything above.
- Produces: nothing code depends on.

- [ ] **Step 1: Read CONTRIBUTING's table to find the right README section**

```bash
grep -n -A 20 "README" CONTRIBUTING.md | head -40
```

Use the section it names for a new setting. Do not invent a new heading if one fits.

- [ ] **Step 2: Add the README entry**

Insert into the section CONTRIBUTING's table names for a new setting. Adjust the heading level to match its neighbours; leave the prose as written:

```markdown
### Crazy mode

Off by default. Turned on, the widget stops being subtle about what it is
already telling you:

- **Fire** — the pill warms as one session goes busy and is properly alight at
  three, with flames along its bottom edge and sparks coming off it. Background
  jobs and subagents do not count towards it.
- **Shake** — a session that has been waiting on you for more than thirty
  seconds makes the pill tremble, harder the longer it waits. It stops while the
  pointer is over the widget, so it never turns into a moving target.
- **Fracture** — as the five-hour limit runs down, cracks spread across the
  pill. At the last of it the usage bar goes molten and drips.
- **Ash** — a session dying breaks its dot apart once, and it settles back to
  the ordinary cross.

If your Mac is set to reduce motion, the colours and the cracks still ramp but
nothing moves: no flames, no shake, no sparks.

Crazy mode applies to the floating widget. Notch placement is unaffected.
```

- [ ] **Step 3: Add the CHANGELOG entry**

Under an `Unreleased` heading, adding one if the file has no open one, matching the format of the `0.7.1` entry above it:

```markdown
### Added

- **Crazy mode.** An opt-in setting that lets the widget dramatise what it is
  already showing: the pill catches fire as sessions go busy, shakes while one
  waits on you, fractures as the five-hour limit runs down, and crumbles when a
  session dies. Off by default, and capped rather than disabled if your Mac is
  set to reduce motion.
```

- [ ] **Step 4: Run the widget against fixtures and look at it**

```bash
./scripts/dev-fixtures.sh
```

Turn crazy mode on in settings and check, in order: one busy session warms the pill; three set it alight; a waiting session starts shaking after thirty seconds and stops when the cursor enters the widget; critical usage fractures the pill and melts the meter; a death crumbles the dot once. `fixtures/` is the only acceptable source for any screenshot — the real registry holds private repository names and an account's real spend.

- [ ] **Step 5: Full suite, both languages**

```bash
npm run typecheck && npm test
cd src-tauri && cargo fmt && cargo test -- --test-threads=1
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git status --short
git add README.md CHANGELOG.md
git commit -m "docs: describe crazy mode"
```
