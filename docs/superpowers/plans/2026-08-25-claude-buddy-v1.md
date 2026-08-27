# claude-buddy v1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A floating always-on-top macOS widget that shows live state for every Claude Code session the user drives, alerts when one needs input or dies, and raises the window running a session on click.

**Architecture:** Three layers with enforced boundaries. A Rust `watcher` owns `~/.claude/sessions/` and derives all state through one pure function. A Rust `bridge` performs the two operations the frontend cannot: raising a window and tailing a transcript. A React frontend renders precomputed snapshots and holds no derived state.

**Tech Stack:** Tauri v2, Rust (notify, serde, libc, plist), React 19 + TypeScript + Vite, Vitest + React Testing Library.

## Global Constraints

- Target platform: macOS only. Apple Silicon and Intel both.
- **Strictly read-only against `~/.claude`.** Never unlink, rewrite, or create files there. Claude Code owns that directory.
- No network calls. No telemetry.
- Session filter: only `entrypoint ∈ {"cli", "claude-desktop"}` reaches any layer above the watcher.
- Paused threshold default: `10 * 60 * 1000` ms.
- Transcript reads: last 64KB only (`65536` bytes). Never read a whole transcript.
- Liveness: `kill(pid, 0)` **and** a `procStart` match. Never pid alone.
- Alerts are edge-triggered on transitions, never on states. The first snapshot after launch fires nothing.
- Bundle identifier: `com.claude.buddy`. Config path: `~/Library/Application Support/com.claude.buddy/config.json`.
- All Rust external effects (process tree, app activation, pid liveness, clock) sit behind traits so tests never touch the real system.
- Serialize Rust types to the frontend as `camelCase`.

## File Structure

```
src-tauri/
  Cargo.toml
  tauri.conf.json                  LSUIElement, transparent frameless window
  src/
    main.rs                        entry point
    lib.rs                         app builder, plugin + command registration
    config.rs                      settings load/save, defaults
    watcher/
      mod.rs                       re-exports
      registry.rs                  RegistryFile, tolerant parsing, dir read
      liveness.rs                  PidLiveness trait, SysLiveness, FakeLiveness
      state.rs                     SessionState, SessionSnapshot, pure snapshot()
      alerts.rs                    Alert, AlertKind, diff_alerts()
      watch.rs                     FSEvents + 2s tick loop, event emission
    bridge/
      mod.rs                       re-exports
      proc_tree.rs                 ProcTree trait, ancestry walk, bundle id
      raise.rs                     Activator trait, raise_session command
      transcript.rs                tail parsing, session_detail command
    window.rs                      NSPanel setup, per-display position
    notify.rs                      alert delivery, mute handling
src/
  main.tsx                         React entry
  App.tsx                          view-mode switch
  types.ts                         SessionSnapshot / Alert / TranscriptDetail mirrors
  useSessions.ts                   snapshot subscription hook
  format.ts                        elapsed formatting, project name from cwd
  views/
    SessionView.ts                 shared renderer interface
    dotRow/
      DotRow.tsx                   mode container, hover state machine
      CollapsedPill.tsx            resting summary pill
      NamedDotRow.tsx              morphed named-dot row
      SessionPopover.tsx           stage-2 per-session detail
  settings/
    SettingsPanel.tsx              settings UI
```

Split by responsibility, not layer: each watcher submodule owns one decision and is testable alone. `state.rs` is the only place session state is decided; `alerts.rs` is the only place transitions are interpreted.

---

### Task 1: Project scaffold

**Files:**
- Create: `package.json`, `vite.config.ts`, `tsconfig.json`, `index.html`, `src/main.tsx`, `src/App.tsx`
- Create: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/src/main.rs`, `src-tauri/src/lib.rs`
- Create: `src-tauri/build.rs`
- Test: `src/format.ts`, `src/format.test.ts`, `src-tauri/src/config.rs` (placeholder test only)

**Interfaces:**
- Consumes: nothing.
- Produces: a repo where `cargo test` and `npm test` both run and pass. Later tasks assume `src-tauri/src/lib.rs` exists with a `run()` function and that `npm test` runs Vitest.

- [ ] **Step 1: Create the frontend package manifest**

`package.json`:

```json
{
  "name": "claude-buddy",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "tauri": "tauri",
    "test": "vitest run",
    "test:watch": "vitest"
  },
  "dependencies": {
    "@tauri-apps/api": "^2.1.1",
    "react": "^19.0.0",
    "react-dom": "^19.0.0"
  },
  "devDependencies": {
    "@tauri-apps/cli": "^2.1.0",
    "@testing-library/jest-dom": "^6.6.3",
    "@testing-library/react": "^16.1.0",
    "@testing-library/user-event": "^14.5.2",
    "@types/react": "^19.0.0",
    "@types/react-dom": "^19.0.0",
    "@vitejs/plugin-react": "^4.3.4",
    "jsdom": "^25.0.1",
    "typescript": "^5.7.2",
    "vite": "^6.0.3",
    "vitest": "^2.1.8"
  }
}
```

- [ ] **Step 2: Create Vite and TypeScript config**

`vite.config.ts`:

```ts
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  clearScreen: false,
  server: { port: 1420, strictPort: true },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test-setup.ts'],
  },
})
```

`tsconfig.json`:

```json
{
  "compilerOptions": {
    "target": "ES2022",
    "useDefineForClassFields": true,
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true,
    "types": ["vitest/globals"]
  },
  "include": ["src"]
}
```

`src/test-setup.ts`:

```ts
import '@testing-library/jest-dom/vitest'
```

- [ ] **Step 3: Create the HTML entry and React root**

`index.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>claude-buddy</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
```

`src/main.tsx`:

```tsx
import React from 'react'
import ReactDOM from 'react-dom/client'
import { App } from './App'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
```

`src/App.tsx`:

```tsx
export function App() {
  return <div data-testid="app-root" />
}
```

- [ ] **Step 4: Write a failing frontend test**

`src/format.test.ts`:

```ts
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
```

- [ ] **Step 5: Run the test to verify it fails**

```bash
npm install && npm test
```

Expected: FAIL — `Failed to resolve import "./format"`.

- [ ] **Step 6: Implement the formatter**

`src/format.ts`:

```ts
export function formatElapsed(ms: number): string {
  const clamped = Math.max(0, ms)
  const totalSeconds = Math.floor(clamped / 1000)
  if (totalSeconds < 60) return `${totalSeconds}s`
  const totalMinutes = Math.floor(totalSeconds / 60)
  if (totalMinutes < 60) return `${totalMinutes}m`
  const hours = Math.floor(totalMinutes / 60)
  return `${hours}h${totalMinutes % 60}m`
}
```

- [ ] **Step 7: Run the test to verify it passes**

```bash
npm test
```

Expected: PASS — 4 tests.

- [ ] **Step 8: Create the Rust crate manifest**

`src-tauri/Cargo.toml`:

```toml
[package]
name = "claude-buddy"
version = "0.1.0"
edition = "2021"
rust-version = "1.77"

[lib]
name = "claude_buddy_lib"
crate-type = ["staticlib", "cdylib", "rlib"]

[build-dependencies]
tauri-build = { version = "2", features = [] }

[dependencies]
tauri = { version = "2", features = ["tray-icon"] }
tauri-plugin-notification = "2"
tauri-plugin-autostart = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
notify = "8"
libc = "0.2"
plist = "1"
dirs = "5"
```

`src-tauri/build.rs`:

```rust
fn main() {
    tauri_build::build()
}
```

- [ ] **Step 9: Create the Tauri config**

`src-tauri/tauri.conf.json`:

```json
{
  "$schema": "https://schema.tauri.app/config/2",
  "productName": "claude-buddy",
  "version": "0.1.0",
  "identifier": "com.claude.buddy",
  "build": {
    "beforeDevCommand": "npm run dev",
    "beforeBuildCommand": "npm run build",
    "devUrl": "http://localhost:1420",
    "frontendDist": "../dist"
  },
  "app": {
    "windows": [
      {
        "label": "widget",
        "width": 200,
        "height": 40,
        "decorations": false,
        "transparent": true,
        "shadow": false,
        "resizable": false,
        "alwaysOnTop": true,
        "skipTaskbar": true,
        "visible": true
      }
    ],
    "security": { "csp": null }
  },
  "bundle": {
    "active": true,
    "targets": ["app", "dmg"],
    "macOS": {
      "minimumSystemVersion": "13.0"
    }
  }
}
```

- [ ] **Step 10: Add LSUIElement so there is no Dock icon**

Create `src-tauri/Info.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>LSUIElement</key>
  <true/>
</dict>
</plist>
```

Tauri merges a sibling `Info.plist` into the generated bundle plist automatically. No config key is needed.

- [ ] **Step 11: Create the Rust entry points**

`src-tauri/src/main.rs`:

```rust
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    claude_buddy_lib::run()
}
```

`src-tauri/src/lib.rs`:

```rust
pub mod config;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .run(tauri::generate_context!())
        .expect("error while running claude-buddy");
}
```

- [ ] **Step 12: Write a failing Rust test**

`src-tauri/src/config.rs`:

```rust
// Implementation arrives in Task 6. This module exists now so the crate has a
// test target from the first commit.

#[cfg(test)]
mod tests {
    #[test]
    fn crate_test_harness_runs() {
        assert_eq!(2 + 2, 4);
    }
}
```

- [ ] **Step 13: Run the Rust tests**

```bash
cd src-tauri && cargo test
```

Expected: PASS — `test config::tests::crate_test_harness_runs ... ok`.

- [ ] **Step 14: Commit**

```bash
git add package.json vite.config.ts tsconfig.json index.html src src-tauri
git commit -m "chore: scaffold Tauri v2 + React app with test harnesses"
```

---

### Task 2: Registry file parsing

**Files:**
- Create: `src-tauri/src/watcher/mod.rs`
- Create: `src-tauri/src/watcher/registry.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline `#[cfg(test)]` module in `registry.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct RegistryFile` with public fields `pid: i32`, `session_id: String`, `cwd: String`, `started_at: i64`, `proc_start: Option<String>`, `entrypoint: Option<String>`, `name: Option<String>`, `status: Option<String>`, `status_updated_at: Option<i64>`, `waiting_for: Option<String>`
  - `pub fn parse_registry_file(bytes: &[u8]) -> Option<RegistryFile>`
  - `pub fn read_registry_dir(dir: &Path) -> Vec<RegistryFile>`
  - `pub fn registry_dir() -> PathBuf`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/watcher/registry.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const FULL: &str = r#"{
      "pid": 7952,
      "sessionId": "a1b2c3d4-0000-4000-8000-000000000001",
      "cwd": "/Users/n/Code/api-service",
      "startedAt": 1787637231465,
      "procStart": "Tue Aug 25 05:53:49 2026",
      "version": "2.1.234",
      "kind": "interactive",
      "entrypoint": "cli",
      "messagingSocketPath": "/tmp/cc-socks/7952.sock",
      "name": "api-service-55",
      "nameSource": "derived",
      "status": "waiting",
      "updatedAt": 1787662267409,
      "statusUpdatedAt": 1787662267409,
      "waitingFor": "input needed"
    }"#;

    // A session that has not reported status yet. Absence is normal, not an error.
    const NO_STATUS: &str = r#"{
      "pid": 99215,
      "sessionId": "a1b2c3d4-0000-4000-8000-000000000002",
      "cwd": "/Users/n/Code/claude-buddy",
      "startedAt": 1787662276356,
      "procStart": "Tue Aug 25 12:51:15 2026",
      "entrypoint": "claude-desktop",
      "name": "claude-buddy-1f"
    }"#;

    #[test]
    fn parses_a_full_record() {
        let f = parse_registry_file(FULL.as_bytes()).expect("should parse");
        assert_eq!(f.pid, 7952);
        assert_eq!(f.name.as_deref(), Some("api-service-55"));
        assert_eq!(f.status.as_deref(), Some("waiting"));
        assert_eq!(f.waiting_for.as_deref(), Some("input needed"));
        assert_eq!(f.status_updated_at, Some(1787662267409));
        assert_eq!(f.proc_start.as_deref(), Some("Tue Aug 25 05:53:49 2026"));
    }

    #[test]
    fn parses_a_record_with_no_status_fields() {
        let f = parse_registry_file(NO_STATUS.as_bytes()).expect("should parse");
        assert_eq!(f.pid, 99215);
        assert_eq!(f.status, None);
        assert_eq!(f.status_updated_at, None);
        assert_eq!(f.waiting_for, None);
        assert_eq!(f.entrypoint.as_deref(), Some("claude-desktop"));
    }

    #[test]
    fn returns_none_for_truncated_json() {
        // Registry writes are not atomic; a read can land mid-write.
        let truncated = &FULL.as_bytes()[..FULL.len() / 2];
        assert!(parse_registry_file(truncated).is_none());
    }

    #[test]
    fn returns_none_when_required_fields_are_missing() {
        assert!(parse_registry_file(br#"{"pid": 1}"#).is_none());
    }

    #[test]
    fn reads_only_numeric_json_filenames_from_a_directory() {
        let dir = std::env::temp_dir().join(format!("cb-registry-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("7952.json"), FULL).unwrap();
        std::fs::write(dir.join("99215.json"), NO_STATUS).unwrap();
        std::fs::write(dir.join("7952.abcdef.key"), "not json").unwrap();
        std::fs::write(dir.join("garbage.json"), "{ broken").unwrap();

        let mut files = read_registry_dir(&dir);
        files.sort_by_key(|f| f.pid);

        assert_eq!(files.len(), 2);
        assert_eq!(files[0].pid, 7952);
        assert_eq!(files[1].pid, 99215);

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_directory_yields_empty_vec() {
        let missing = std::path::Path::new("/nonexistent/claude-buddy/registry");
        assert!(read_registry_dir(missing).is_empty());
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test watcher::registry
```

Expected: FAIL — `cannot find function parse_registry_file in this scope`.

- [ ] **Step 3: Implement the parser**

Prepend to `src-tauri/src/watcher/registry.rs`:

```rust
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// One `~/.claude/sessions/<pid>.json` record.
///
/// Only fields claude-buddy consumes are modelled. Unknown fields are ignored
/// so a Claude Code upgrade that adds keys cannot break parsing.
#[derive(Debug, Clone, Deserialize)]
pub struct RegistryFile {
    pub pid: i32,
    #[serde(rename = "sessionId")]
    pub session_id: String,
    pub cwd: String,
    #[serde(rename = "startedAt")]
    pub started_at: i64,
    #[serde(rename = "procStart")]
    pub proc_start: Option<String>,
    pub entrypoint: Option<String>,
    pub name: Option<String>,
    pub status: Option<String>,
    #[serde(rename = "statusUpdatedAt")]
    pub status_updated_at: Option<i64>,
    #[serde(rename = "waitingFor")]
    pub waiting_for: Option<String>,
}

/// Parse one registry file. Returns `None` on any malformed input — including a
/// truncated read, which is expected because registry writes are not atomic.
pub fn parse_registry_file(bytes: &[u8]) -> Option<RegistryFile> {
    serde_json::from_slice(bytes).ok()
}

/// The live session registry directory.
pub fn registry_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".claude")
        .join("sessions")
}

/// Read every parseable `<pid>.json` in `dir`. Unparseable and non-matching
/// files are skipped silently; a missing directory yields an empty vec.
///
/// This function never writes to `dir`.
pub fn read_registry_dir(dir: &Path) -> Vec<RegistryFile> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let stem = path.file_stem()?.to_str()?;
            if path.extension()?.to_str()? != "json" || stem.parse::<i32>().is_err() {
                return None;
            }
            parse_registry_file(&std::fs::read(&path).ok()?)
        })
        .collect()
}
```

- [ ] **Step 4: Wire the module into the crate**

`src-tauri/src/watcher/mod.rs`:

```rust
pub mod registry;
```

In `src-tauri/src/lib.rs`, add below `pub mod config;`:

```rust
pub mod watcher;
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test watcher::registry
```

Expected: PASS — 6 tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/watcher src-tauri/src/lib.rs
git commit -m "feat(watcher): parse the Claude Code session registry"
```

---

### Task 3: Process liveness

**Files:**
- Create: `src-tauri/src/watcher/liveness.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Test: inline `#[cfg(test)]` module in `liveness.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub trait PidLiveness { fn is_alive(&self, pid: i32, proc_start: Option<&str>) -> bool; }`
  - `pub struct SysLiveness` — real implementation
  - `pub struct FakeLiveness` with `pub fn new() -> Self`, `pub fn with_alive(self, pid: i32, proc_start: &str) -> Self`, `pub fn with_alive_any_start(self, pid: i32) -> Self` — used by every later test that needs liveness

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/watcher/liveness.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_reports_registered_pids_alive() {
        let fake = FakeLiveness::new().with_alive(7952, "Tue Aug 25 05:53:49 2026");
        assert!(fake.is_alive(7952, Some("Tue Aug 25 05:53:49 2026")));
    }

    #[test]
    fn fake_reports_unregistered_pids_dead() {
        let fake = FakeLiveness::new().with_alive(7952, "Tue Aug 25 05:53:49 2026");
        assert!(!fake.is_alive(1234, Some("Tue Aug 25 05:53:49 2026")));
    }

    #[test]
    fn recycled_pid_reads_as_dead() {
        // Same pid number, different process start time: the original session is
        // gone and the number was reused.
        let fake = FakeLiveness::new().with_alive(7952, "Wed Aug 26 09:00:00 2026");
        assert!(!fake.is_alive(7952, Some("Tue Aug 25 05:53:49 2026")));
    }

    #[test]
    fn whitespace_differences_in_proc_start_still_match() {
        // `ps -o lstart=` pads single-digit days; the registry may not.
        let fake = FakeLiveness::new().with_alive(7952, "Tue Aug  5 05:53:49 2026");
        assert!(fake.is_alive(7952, Some("Tue Aug 5 05:53:49 2026")));
    }

    #[test]
    fn missing_proc_start_falls_back_to_pid_only() {
        let fake = FakeLiveness::new().with_alive(7952, "Tue Aug 25 05:53:49 2026");
        assert!(fake.is_alive(7952, None));
    }

    #[test]
    fn sys_liveness_sees_our_own_process() {
        let own = std::process::id() as i32;
        assert!(SysLiveness.is_alive(own, None));
    }

    #[test]
    fn sys_liveness_rejects_an_impossible_pid() {
        assert!(!SysLiveness.is_alive(-1, None));
    }

    #[test]
    fn sys_liveness_rejects_our_pid_with_a_wrong_start_time() {
        let own = std::process::id() as i32;
        assert!(!SysLiveness.is_alive(own, Some("Thu Jan  1 00:00:00 1970")));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test watcher::liveness
```

Expected: FAIL — `cannot find type FakeLiveness in this scope`.

- [ ] **Step 3: Implement the trait and both implementations**

Prepend to `src-tauri/src/watcher/liveness.rs`:

```rust
use std::collections::HashMap;

/// Whether a session's process is still running.
///
/// A pid alone is not sufficient evidence: pid numbers are recycled, and a
/// recycled number would otherwise present a dead session as live. Callers pass
/// the registry's `procStart` so implementations can confirm identity.
pub trait PidLiveness {
    fn is_alive(&self, pid: i32, proc_start: Option<&str>) -> bool;
}

/// Collapse runs of whitespace so `ps` padding differences do not defeat the
/// comparison: `"Tue Aug  5 ..."` and `"Tue Aug 5 ..."` are the same instant.
fn normalize_start(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Real liveness, via `kill(pid, 0)` plus `ps -o lstart=`.
pub struct SysLiveness;

impl SysLiveness {
    fn process_start(pid: i32) -> Option<String> {
        let out = std::process::Command::new("ps")
            .args(["-o", "lstart=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        if !out.status.success() {
            return None;
        }
        let text = String::from_utf8_lossy(&out.stdout);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(normalize_start(trimmed))
        }
    }
}

impl PidLiveness for SysLiveness {
    fn is_alive(&self, pid: i32, proc_start: Option<&str>) -> bool {
        if pid <= 0 {
            return false;
        }

        // EPERM means the process exists but belongs to another user, which
        // still counts as alive.
        let signalled = unsafe { libc::kill(pid, 0) };
        let exists = signalled == 0
            || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM);
        if !exists {
            return false;
        }

        match proc_start {
            None => true,
            Some(expected) => match Self::process_start(pid) {
                // If `ps` cannot answer, trust the signal probe rather than
                // declaring a live session dead.
                None => true,
                Some(actual) => actual == normalize_start(expected),
            },
        }
    }
}

/// Test double. `with_alive_any_start` registers a pid whose start time always
/// matches, for tests that do not care about pid reuse.
pub struct FakeLiveness {
    alive: HashMap<i32, Option<String>>,
}

impl FakeLiveness {
    pub fn new() -> Self {
        Self { alive: HashMap::new() }
    }

    pub fn with_alive(mut self, pid: i32, proc_start: &str) -> Self {
        self.alive.insert(pid, Some(normalize_start(proc_start)));
        self
    }

    pub fn with_alive_any_start(mut self, pid: i32) -> Self {
        self.alive.insert(pid, None);
        self
    }
}

impl Default for FakeLiveness {
    fn default() -> Self {
        Self::new()
    }
}

impl PidLiveness for FakeLiveness {
    fn is_alive(&self, pid: i32, proc_start: Option<&str>) -> bool {
        match self.alive.get(&pid) {
            None => false,
            Some(None) => true,
            Some(Some(registered)) => match proc_start {
                None => true,
                Some(expected) => *registered == normalize_start(expected),
            },
        }
    }
}
```

- [ ] **Step 4: Export the module**

`src-tauri/src/watcher/mod.rs`:

```rust
pub mod liveness;
pub mod registry;
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test watcher::liveness
```

Expected: PASS — 8 tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/watcher
git commit -m "feat(watcher): pid liveness with procStart identity check"
```

---

### Task 4: State derivation

This is the task that decides the app's correctness. Every state the UI can ever
show is produced here, by one pure function with injected time and injected
liveness.

**Files:**
- Create: `src-tauri/src/watcher/state.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Test: inline `#[cfg(test)]` module in `state.rs`

**Interfaces:**
- Consumes: `RegistryFile` (Task 2), `PidLiveness` + `FakeLiveness` (Task 3).
- Produces:
  - `pub enum SessionState { Waiting, Busy, Idle, Paused, Dead }` — serializes lowercase
  - `pub struct SessionSnapshot` with public fields `pid: i32`, `session_id: String`, `name: String`, `cwd: String`, `entrypoint: String`, `state: SessionState`, `detail: Option<String>`, `elapsed_ms: i64`, `uptime_ms: i64` — serializes camelCase
  - `pub const PAUSED_THRESHOLD_MS: i64`
  - `pub const DEAD_RETENTION_MS: i64`
  - `pub const ALLOWED_ENTRYPOINTS: [&str; 2]`
  - `pub fn snapshot(files: &[RegistryFile], liveness: &dyn PidLiveness, now_ms: i64, paused_threshold_ms: i64) -> Vec<SessionSnapshot>`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/watcher/state.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::liveness::FakeLiveness;
    use crate::watcher::registry::RegistryFile;

    const NOW: i64 = 1_787_662_300_000;
    const START: &str = "Tue Aug 25 05:53:49 2026";

    fn file(pid: i32, entrypoint: &str) -> RegistryFile {
        RegistryFile {
            pid,
            session_id: format!("session-{pid}"),
            cwd: format!("/Users/n/Code/project-{pid}"),
            started_at: NOW - 60_000,
            proc_start: Some(START.to_string()),
            entrypoint: Some(entrypoint.to_string()),
            name: Some(format!("project-{pid}")),
            status: None,
            status_updated_at: None,
            waiting_for: None,
        }
    }

    fn alive(pid: i32) -> FakeLiveness {
        FakeLiveness::new().with_alive(pid, START)
    }

    #[test]
    fn waiting_status_yields_waiting_with_detail() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        f.status_updated_at = Some(NOW - 6 * 60_000);

        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);

        assert_eq!(out.len(), 1);
        assert_eq!(out[0].state, SessionState::Waiting);
        assert_eq!(out[0].detail.as_deref(), Some("input needed"));
        assert_eq!(out[0].elapsed_ms, 6 * 60_000);
    }

    #[test]
    fn busy_status_yields_busy_with_no_detail() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        f.status_updated_at = Some(NOW - 3 * 60_000);

        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);

        assert_eq!(out[0].state, SessionState::Busy);
        assert_eq!(out[0].detail, None);
    }

    #[test]
    fn absent_status_within_threshold_yields_idle() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);

        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);

        assert_eq!(out[0].state, SessionState::Idle);
    }

    #[test]
    fn idle_status_word_is_treated_as_idle() {
        let mut f = file(1, "cli");
        f.status = Some("idle".into());
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS)[0].state, SessionState::Idle);
    }

    #[test]
    fn running_status_word_is_treated_as_idle() {
        let mut f = file(1, "cli");
        f.status = Some("running".into());
        f.status_updated_at = Some(NOW - 60_000);
        assert_eq!(snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS)[0].state, SessionState::Idle);
    }

    #[test]
    fn idle_past_threshold_yields_paused() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 11 * 60_000);

        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);

        assert_eq!(out[0].state, SessionState::Paused);
    }

    #[test]
    fn paused_boundary_is_inclusive() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - PAUSED_THRESHOLD_MS);
        assert_eq!(snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS)[0].state, SessionState::Paused);
    }

    #[test]
    fn busy_never_becomes_paused_however_stale() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());
        f.status_updated_at = Some(NOW - 60 * 60_000);
        assert_eq!(snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS)[0].state, SessionState::Busy);
    }

    #[test]
    fn waiting_never_becomes_paused_however_stale() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        f.status_updated_at = Some(NOW - 60 * 60_000);
        assert_eq!(snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS)[0].state, SessionState::Waiting);
    }

    #[test]
    fn dead_process_yields_dead_regardless_of_status() {
        let mut f = file(1, "cli");
        f.status = Some("busy".into());

        let out = snapshot(&[f], &FakeLiveness::new(), NOW, PAUSED_THRESHOLD_MS);

        assert_eq!(out[0].state, SessionState::Dead);
    }

    #[test]
    fn sdk_cli_sessions_are_filtered_out() {
        let files = vec![file(1, "cli"), file(2, "sdk-cli"), file(3, "claude-desktop")];
        let live = FakeLiveness::new()
            .with_alive_any_start(1)
            .with_alive_any_start(2)
            .with_alive_any_start(3);

        let out = snapshot(&files, &live, NOW, PAUSED_THRESHOLD_MS);

        let pids: Vec<i32> = out.iter().map(|s| s.pid).collect();
        assert_eq!(pids, vec![1, 3]);
    }

    #[test]
    fn sessions_with_no_entrypoint_are_filtered_out() {
        let mut f = file(1, "cli");
        f.entrypoint = None;
        assert!(snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS).is_empty());
    }

    #[test]
    fn elapsed_falls_back_to_started_at_when_status_time_is_absent() {
        let f = file(1, "cli");
        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);
        assert_eq!(out[0].elapsed_ms, 60_000);
        assert_eq!(out[0].uptime_ms, 60_000);
    }

    #[test]
    fn future_timestamps_clamp_elapsed_to_zero() {
        // Clock skew must not render as a negative age.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW + 3 * 60_000);
        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);
        assert_eq!(out[0].elapsed_ms, 0);
        assert_eq!(out[0].state, SessionState::Idle);
    }

    #[test]
    fn missing_name_falls_back_to_the_cwd_basename() {
        let mut f = file(1, "cli");
        f.name = None;
        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);
        assert_eq!(out[0].name, "project-1");
    }

    #[test]
    fn ordering_is_waiting_then_busy_then_idle_then_paused_then_dead() {
        let mut waiting = file(10, "cli");
        waiting.status = Some("waiting".into());
        let mut busy = file(20, "cli");
        busy.status = Some("busy".into());
        let idle = file(30, "cli");
        let mut paused = file(40, "cli");
        paused.status_updated_at = Some(NOW - 30 * 60_000);
        let dead = file(50, "cli");

        let live = FakeLiveness::new()
            .with_alive_any_start(10)
            .with_alive_any_start(20)
            .with_alive_any_start(30)
            .with_alive_any_start(40);

        let out = snapshot(&[dead, paused, idle, busy, waiting], &live, NOW, PAUSED_THRESHOLD_MS);
        let pids: Vec<i32> = out.iter().map(|s| s.pid).collect();

        assert_eq!(pids, vec![10, 20, 30, 40, 50]);
    }

    #[test]
    fn same_state_sessions_order_by_start_time_oldest_first() {
        let mut older = file(10, "cli");
        older.started_at = NOW - 600_000;
        let mut newer = file(20, "cli");
        newer.started_at = NOW - 60_000;

        let live = FakeLiveness::new().with_alive_any_start(10).with_alive_any_start(20);
        let out = snapshot(&[newer, older], &live, NOW, PAUSED_THRESHOLD_MS);

        assert_eq!(out.iter().map(|s| s.pid).collect::<Vec<_>>(), vec![10, 20]);
    }

    #[test]
    fn a_recently_dead_session_is_retained() {
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - 60_000);
        let out = snapshot(&[f], &FakeLiveness::new(), NOW, PAUSED_THRESHOLD_MS);
        assert_eq!(out[0].state, SessionState::Dead);
    }

    #[test]
    fn a_long_dead_session_drops_off_the_list() {
        // Claude Code prunes stale registry files itself; claude-buddy never
        // unlinks them, so it stops showing them instead.
        let mut f = file(1, "cli");
        f.status_updated_at = Some(NOW - DEAD_RETENTION_MS - 1);
        assert!(snapshot(&[f], &FakeLiveness::new(), NOW, PAUSED_THRESHOLD_MS).is_empty());
    }

    #[test]
    fn dead_retention_uses_started_at_when_no_status_time_exists() {
        let mut f = file(1, "cli");
        f.started_at = NOW - DEAD_RETENTION_MS - 1;
        f.status_updated_at = None;
        assert!(snapshot(&[f], &FakeLiveness::new(), NOW, PAUSED_THRESHOLD_MS).is_empty());
    }

    #[test]
    fn empty_input_yields_empty_output() {
        assert!(snapshot(&[], &FakeLiveness::new(), NOW, PAUSED_THRESHOLD_MS).is_empty());
    }

    #[test]
    fn snapshot_serializes_camel_case_with_lowercase_state() {
        let mut f = file(1, "cli");
        f.status = Some("waiting".into());
        f.waiting_for = Some("input needed".into());
        let out = snapshot(&[f], &alive(1), NOW, PAUSED_THRESHOLD_MS);

        let json = serde_json::to_value(&out[0]).unwrap();
        assert_eq!(json["state"], "waiting");
        assert_eq!(json["sessionId"], "session-1");
        assert_eq!(json["elapsedMs"], 60_000);
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test watcher::state
```

Expected: FAIL — `cannot find function snapshot in this scope`.

- [ ] **Step 3: Implement the derivation**

Prepend to `src-tauri/src/watcher/state.rs`:

```rust
use serde::Serialize;

use crate::watcher::liveness::PidLiveness;
use crate::watcher::registry::RegistryFile;

/// Idle sessions older than this read as `Paused`.
pub const PAUSED_THRESHOLD_MS: i64 = 10 * 60 * 1000;

/// How long a crashed session stays on the list. Its registry file lingers with
/// a dead pid, and claude-buddy never unlinks anything under `~/.claude`, so the
/// entry ages out of the display instead.
pub const DEAD_RETENTION_MS: i64 = 5 * 60 * 1000;

/// Entrypoints the user can actually answer. Everything else — notably
/// `sdk-cli`, which is plugin machinery — is dropped before any other layer
/// sees it, so no alert can ever fire for a session the user cannot reach.
pub const ALLOWED_ENTRYPOINTS: [&str; 2] = ["cli", "claude-desktop"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionState {
    Waiting,
    Busy,
    Idle,
    Paused,
    Dead,
}

impl SessionState {
    /// Display order: what needs the user comes first.
    fn rank(self) -> u8 {
        match self {
            SessionState::Waiting => 0,
            SessionState::Busy => 1,
            SessionState::Idle => 2,
            SessionState::Paused => 3,
            SessionState::Dead => 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub pid: i32,
    pub session_id: String,
    pub name: String,
    pub cwd: String,
    pub entrypoint: String,
    pub state: SessionState,
    /// The registry's `waitingFor`, present only while `Waiting`.
    pub detail: Option<String>,
    /// Age of the current state. Falls back to session age when the registry
    /// has not recorded a status time.
    pub elapsed_ms: i64,
    pub uptime_ms: i64,
}

/// Clock skew and stale files can both produce timestamps in the future.
/// Render those as zero rather than as a negative age.
fn age(now_ms: i64, then_ms: i64) -> i64 {
    (now_ms - then_ms).max(0)
}

fn display_name(file: &RegistryFile) -> String {
    if let Some(name) = file.name.as_deref().filter(|n| !n.is_empty()) {
        return name.to_string();
    }
    std::path::Path::new(&file.cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string()
}

/// Derive every session's state. Pure: all time and all liveness are injected,
/// so the whole state machine is testable without a filesystem or a clock.
pub fn snapshot(
    files: &[RegistryFile],
    liveness: &dyn PidLiveness,
    now_ms: i64,
    paused_threshold_ms: i64,
) -> Vec<SessionSnapshot> {
    let mut out: Vec<SessionSnapshot> = files
        .iter()
        .filter(|f| {
            f.entrypoint
                .as_deref()
                .is_some_and(|e| ALLOWED_ENTRYPOINTS.contains(&e))
        })
        .map(|f| {
            let status_time = f.status_updated_at.unwrap_or(f.started_at);
            let elapsed_ms = age(now_ms, status_time);
            let alive = liveness.is_alive(f.pid, f.proc_start.as_deref());

            let state = if !alive {
                SessionState::Dead
            } else {
                match f.status.as_deref() {
                    Some("waiting") => SessionState::Waiting,
                    Some("busy") => SessionState::Busy,
                    _ if elapsed_ms >= paused_threshold_ms => SessionState::Paused,
                    _ => SessionState::Idle,
                }
            };

            SessionSnapshot {
                pid: f.pid,
                session_id: f.session_id.clone(),
                name: display_name(f),
                cwd: f.cwd.clone(),
                entrypoint: f.entrypoint.clone().unwrap_or_default(),
                state,
                detail: match state {
                    SessionState::Waiting => f.waiting_for.clone(),
                    _ => None,
                },
                elapsed_ms,
                uptime_ms: age(now_ms, f.started_at),
            }
        })
        // A crash is worth showing once, not forever.
        .filter(|s| s.state != SessionState::Dead || s.elapsed_ms <= DEAD_RETENTION_MS)
        .collect();

    out.sort_by(|a, b| {
        a.state
            .rank()
            .cmp(&b.state.rank())
            .then(b.uptime_ms.cmp(&a.uptime_ms))
            .then(a.pid.cmp(&b.pid))
    });

    out
}
```

- [ ] **Step 4: Export the module**

`src-tauri/src/watcher/mod.rs`:

```rust
pub mod liveness;
pub mod registry;
pub mod state;
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test watcher::state
```

Expected: PASS — 22 tests.

- [ ] **Step 6: Run the whole suite**

```bash
cd src-tauri && cargo test
```

Expected: PASS — 37 tests across `config`, `watcher::registry`, `watcher::liveness`, `watcher::state`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/watcher
git commit -m "feat(watcher): derive session state as a pure function"
```

---

### Task 5: Alert diffing

**Files:**
- Create: `src-tauri/src/watcher/alerts.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Test: inline `#[cfg(test)]` module in `alerts.rs`

**Interfaces:**
- Consumes: `SessionSnapshot`, `SessionState` (Task 4).
- Produces:
  - `pub enum AlertKind { NeedsInput, Died }` — serializes camelCase
  - `pub struct Alert` with public fields `session_id: String`, `name: String`, `kind: AlertKind`, `detail: Option<String>`
  - `pub fn diff_alerts(prev: Option<&[SessionSnapshot]>, next: &[SessionSnapshot]) -> Vec<Alert>`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/watcher/alerts.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::state::{SessionSnapshot, SessionState};

    fn snap(id: &str, state: SessionState) -> SessionSnapshot {
        SessionSnapshot {
            pid: 1,
            session_id: id.to_string(),
            name: format!("name-{id}"),
            cwd: "/Users/n/Code/x".into(),
            entrypoint: "cli".into(),
            state,
            detail: match state {
                SessionState::Waiting => Some("input needed".into()),
                _ => None,
            },
            elapsed_ms: 0,
            uptime_ms: 0,
        }
    }

    #[test]
    fn cold_start_fires_nothing_even_when_a_session_is_already_waiting() {
        // The first snapshot after launch establishes a baseline. Without this,
        // every launch produces a burst of alerts for pre-existing state.
        let next = vec![snap("a", SessionState::Waiting), snap("b", SessionState::Dead)];
        assert!(diff_alerts(None, &next).is_empty());
    }

    #[test]
    fn transition_into_waiting_fires_needs_input() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Waiting)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::NeedsInput);
        assert_eq!(alerts[0].session_id, "a");
        assert_eq!(alerts[0].detail.as_deref(), Some("input needed"));
    }

    #[test]
    fn staying_in_waiting_does_not_fire_again() {
        let prev = vec![snap("a", SessionState::Waiting)];
        let next = vec![snap("a", SessionState::Waiting)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn transition_into_dead_fires_died() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Dead)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, AlertKind::Died);
    }

    #[test]
    fn staying_dead_does_not_fire_again() {
        let prev = vec![snap("a", SessionState::Dead)];
        let next = vec![snap("a", SessionState::Dead)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn a_session_appearing_already_waiting_fires() {
        // Not a cold start: the app was running, a new session showed up blocked.
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Busy), snap("b", SessionState::Waiting)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].session_id, "b");
    }

    #[test]
    fn a_session_appearing_busy_fires_nothing() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Busy), snap("b", SessionState::Busy)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn a_clean_exit_fires_nothing() {
        // The registry file was removed, so the session simply vanishes.
        let prev = vec![snap("a", SessionState::Busy)];
        assert!(diff_alerts(Some(&prev), &[]).is_empty());
    }

    #[test]
    fn turn_finishing_fires_nothing() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Idle)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn drifting_into_paused_fires_nothing() {
        let prev = vec![snap("a", SessionState::Idle)];
        let next = vec![snap("a", SessionState::Paused)];
        assert!(diff_alerts(Some(&prev), &next).is_empty());
    }

    #[test]
    fn answering_then_blocking_again_fires_a_second_time() {
        let waiting = vec![snap("a", SessionState::Waiting)];
        let busy = vec![snap("a", SessionState::Busy)];

        assert!(diff_alerts(Some(&waiting), &busy).is_empty());
        assert_eq!(diff_alerts(Some(&busy), &waiting).len(), 1);
    }

    #[test]
    fn multiple_transitions_in_one_tick_all_fire() {
        let prev = vec![snap("a", SessionState::Busy), snap("b", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Waiting), snap("b", SessionState::Dead)];

        let alerts = diff_alerts(Some(&prev), &next);

        assert_eq!(alerts.len(), 2);
        assert!(alerts.iter().any(|a| a.session_id == "a" && a.kind == AlertKind::NeedsInput));
        assert!(alerts.iter().any(|a| a.session_id == "b" && a.kind == AlertKind::Died));
    }

    #[test]
    fn alert_serializes_camel_case() {
        let prev = vec![snap("a", SessionState::Busy)];
        let next = vec![snap("a", SessionState::Waiting)];
        let json = serde_json::to_value(&diff_alerts(Some(&prev), &next)[0]).unwrap();

        assert_eq!(json["sessionId"], "a");
        assert_eq!(json["kind"], "needsInput");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test watcher::alerts
```

Expected: FAIL — `cannot find function diff_alerts in this scope`.

- [ ] **Step 3: Implement the diff**

Prepend to `src-tauri/src/watcher/alerts.rs`:

```rust
use std::collections::HashMap;

use serde::Serialize;

use crate::watcher::state::{SessionSnapshot, SessionState};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum AlertKind {
    NeedsInput,
    Died,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Alert {
    pub session_id: String,
    pub name: String,
    pub kind: AlertKind,
    pub detail: Option<String>,
}

/// Which states are worth interrupting the user for, once, on entry.
fn alert_kind(state: SessionState) -> Option<AlertKind> {
    match state {
        SessionState::Waiting => Some(AlertKind::NeedsInput),
        SessionState::Dead => Some(AlertKind::Died),
        _ => None,
    }
}

/// Alerts for transitions between two consecutive snapshots.
///
/// Edge-triggered: a session that was already in an alerting state stays quiet.
/// `prev == None` means this is the first snapshot after launch — it establishes
/// the baseline and fires nothing, so starting the app never floods the user
/// with alerts about state that predates it.
pub fn diff_alerts(prev: Option<&[SessionSnapshot]>, next: &[SessionSnapshot]) -> Vec<Alert> {
    let Some(prev) = prev else {
        return Vec::new();
    };

    let before: HashMap<&str, SessionState> = prev
        .iter()
        .map(|s| (s.session_id.as_str(), s.state))
        .collect();

    next.iter()
        .filter_map(|s| {
            let kind = alert_kind(s.state)?;
            let was = before.get(s.session_id.as_str()).copied();
            // Fire on entry only: unchanged alerting state is not an edge.
            if was == Some(s.state) {
                return None;
            }
            Some(Alert {
                session_id: s.session_id.clone(),
                name: s.name.clone(),
                kind,
                detail: s.detail.clone(),
            })
        })
        .collect()
}
```

- [ ] **Step 4: Export the module**

`src-tauri/src/watcher/mod.rs`:

```rust
pub mod alerts;
pub mod liveness;
pub mod registry;
pub mod state;
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test watcher::alerts
```

Expected: PASS — 13 tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/watcher
git commit -m "feat(watcher): edge-triggered alerts with cold-start suppression"
```

---

### Task 6: Config file

**Files:**
- Modify: `src-tauri/src/config.rs` (replaces the placeholder from Task 1)
- Test: inline `#[cfg(test)]` module in `config.rs`

**Interfaces:**
- Consumes: `PAUSED_THRESHOLD_MS` (Task 4).
- Produces:
  - `pub struct Config` with public fields `view_mode: String`, `paused_threshold_ms: i64`, `alert_needs_input: bool`, `alert_died: bool`, `sound: bool`, `mute_until_ms: i64`, `launch_at_login: bool`, `positions: HashMap<String, [f64; 2]>`
  - `pub fn config_path() -> PathBuf`
  - `pub fn load(path: &Path) -> Config`
  - `pub fn save(path: &Path, config: &Config) -> std::io::Result<()>`
  - `impl Config { pub fn alerts_muted(&self, now_ms: i64) -> bool }`

- [ ] **Step 1: Write the failing tests**

Replace the whole contents of `src-tauri/src/config.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("cb-config-{}-{tag}.json", std::process::id()))
    }

    #[test]
    fn defaults_are_sane() {
        let c = Config::default();
        assert_eq!(c.view_mode, "dotRow");
        assert_eq!(c.paused_threshold_ms, crate::watcher::state::PAUSED_THRESHOLD_MS);
        assert!(c.alert_needs_input);
        assert!(c.alert_died);
        assert!(!c.sound);
        assert_eq!(c.mute_until_ms, 0);
        assert!(!c.launch_at_login);
        assert!(c.positions.is_empty());
    }

    #[test]
    fn missing_file_yields_defaults() {
        let path = temp_path("missing");
        let _ = std::fs::remove_file(&path);
        assert_eq!(load(&path), Config::default());
    }

    #[test]
    fn corrupt_file_yields_defaults_rather_than_panicking() {
        let path = temp_path("corrupt");
        std::fs::write(&path, "{ not json").unwrap();
        assert_eq!(load(&path), Config::default());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn partial_file_fills_unspecified_keys_from_defaults() {
        let path = temp_path("partial");
        std::fs::write(&path, r#"{"sound": true}"#).unwrap();

        let c = load(&path);

        assert!(c.sound);
        assert_eq!(c.view_mode, "dotRow");
        assert_eq!(c.paused_threshold_ms, crate::watcher::state::PAUSED_THRESHOLD_MS);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("roundtrip");
        let mut c = Config::default();
        c.sound = true;
        c.view_mode = "cardStack".into();
        c.positions.insert("display-1".into(), [120.0, 44.5]);

        save(&path, &c).unwrap();

        assert_eq!(load(&path), c);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn save_creates_missing_parent_directories() {
        let dir = std::env::temp_dir().join(format!("cb-cfg-dir-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("nested").join("config.json");

        save(&path, &Config::default()).unwrap();

        assert!(path.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn mute_is_active_before_the_deadline_and_expired_after() {
        let mut c = Config::default();
        c.mute_until_ms = 1_000;
        assert!(c.alerts_muted(999));
        assert!(!c.alerts_muted(1_000));
        assert!(!c.alerts_muted(1_001));
    }

    #[test]
    fn default_config_is_never_muted() {
        assert!(!Config::default().alerts_muted(0));
    }

    #[test]
    fn config_path_lands_under_the_bundle_identifier() {
        let p = config_path();
        assert!(p.ends_with("com.claude.buddy/config.json"), "got {p:?}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test config
```

Expected: FAIL — `cannot find type Config in this scope`.

- [ ] **Step 3: Implement the config module**

Prepend to `src-tauri/src/config.rs`:

```rust
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::watcher::state::PAUSED_THRESHOLD_MS;

/// User settings. Hand-editable JSON: every field has a default so a
/// half-written file still loads.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    /// One of `dotRow`, `cardStack`, `characterBuddy`, `invisible`.
    pub view_mode: String,
    pub paused_threshold_ms: i64,
    pub alert_needs_input: bool,
    pub alert_died: bool,
    pub sound: bool,
    /// Epoch millis until which alerts stay suppressed. Backs "Mute alerts 1h".
    pub mute_until_ms: i64,
    pub launch_at_login: bool,
    /// Widget position keyed by display identifier, so docking and undocking a
    /// monitor does not leave the widget off-screen.
    pub positions: HashMap<String, [f64; 2]>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            view_mode: "dotRow".into(),
            paused_threshold_ms: PAUSED_THRESHOLD_MS,
            alert_needs_input: true,
            alert_died: true,
            sound: false,
            mute_until_ms: 0,
            launch_at_login: false,
            positions: HashMap::new(),
        }
    }
}

impl Config {
    pub fn alerts_muted(&self, now_ms: i64) -> bool {
        now_ms < self.mute_until_ms
    }
}

pub fn config_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join("Library")
        .join("Application Support")
        .join("com.claude.buddy")
        .join("config.json")
}

/// Load settings, falling back to defaults for a missing or corrupt file.
/// Never fails: a broken config must not prevent the widget from starting.
pub fn load(path: &Path) -> Config {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn save(path: &Path, config: &Config) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_vec_pretty(config)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    std::fs::write(path, json)
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test config
```

Expected: PASS — 9 tests.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "feat: hand-editable JSON config with defaults"
```

---

### Task 7: Watcher loop

FSEvents alone is not sufficient: it cannot report that a process died without
its registry file changing, and it coalesces or drops events under load. The 2s
reconcile tick is the backstop, not a redundancy.

**Files:**
- Create: `src-tauri/src/watcher/watch.rs`
- Modify: `src-tauri/src/watcher/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline `#[cfg(test)]` module in `watch.rs`

**Interfaces:**
- Consumes: `read_registry_dir` (Task 2), `SysLiveness` + `PidLiveness` (Task 3), `snapshot` (Task 4), `diff_alerts` (Task 5).
- Produces:
  - `pub struct Update { pub sessions: Vec<SessionSnapshot>, pub alerts: Vec<Alert> }`
  - `pub const TICK: Duration`
  - `pub fn now_ms() -> i64`
  - `pub fn spawn_watcher(dir: PathBuf, liveness: Arc<dyn PidLiveness + Send + Sync>, paused_threshold_ms: i64, on_update: impl Fn(Update) + Send + 'static) -> WatcherHandle`
  - `pub struct WatcherHandle` with `pub fn stop(self)`
  - Tauri event name `"sessions://update"` carrying `Update`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/watcher/watch.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::sync::Arc;

    use crate::watcher::liveness::FakeLiveness;
    use crate::watcher::state::{SessionState, PAUSED_THRESHOLD_MS};

    /// Long enough to cover one reconcile tick plus FSEvents latency.
    const WAIT: Duration = Duration::from_secs(6);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("cb-watch-{}-{tag}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Self(dir)
        }

        fn write_session(&self, pid: i32, status: Option<&str>) {
            let status_json = match status {
                Some(s) => format!(r#""status": "{s}", "waitingFor": "input needed","#),
                None => String::new(),
            };
            let body = format!(
                r#"{{
                  "pid": {pid},
                  "sessionId": "session-{pid}",
                  "cwd": "/Users/n/Code/proj",
                  "startedAt": {},
                  "entrypoint": "cli",
                  "name": "proj-{pid}",
                  {status_json}
                  "statusUpdatedAt": {}
                }}"#,
                now_ms(),
                now_ms()
            );
            std::fs::write(self.0.join(format!("{pid}.json")), body).unwrap();
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn recv_matching(
        rx: &mpsc::Receiver<Update>,
        pred: impl Fn(&Update) -> bool,
    ) -> Update {
        let deadline = std::time::Instant::now() + WAIT;
        while std::time::Instant::now() < deadline {
            if let Ok(update) = rx.recv_timeout(Duration::from_millis(500)) {
                if pred(&update) {
                    return update;
                }
            }
        }
        panic!("no matching update within {WAIT:?}");
    }

    #[test]
    fn emits_an_initial_snapshot_for_existing_sessions() {
        let dir = TempDir::new("initial");
        dir.write_session(4242, None);

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(dir.0.clone(), liveness, PAUSED_THRESHOLD_MS, move |u| {
            let _ = tx.send(u);
        });

        let update = recv_matching(&rx, |u| u.sessions.len() == 1);
        assert_eq!(update.sessions[0].pid, 4242);
        // First snapshot is the baseline, so it alerts about nothing.
        assert!(update.alerts.is_empty());

        handle.stop();
    }

    #[test]
    fn a_new_registry_file_produces_a_larger_snapshot() {
        let dir = TempDir::new("appear");
        dir.write_session(4242, None);

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(
            FakeLiveness::new()
                .with_alive_any_start(4242)
                .with_alive_any_start(4343),
        );
        let handle = spawn_watcher(dir.0.clone(), liveness, PAUSED_THRESHOLD_MS, move |u| {
            let _ = tx.send(u);
        });

        recv_matching(&rx, |u| u.sessions.len() == 1);
        dir.write_session(4343, None);

        let update = recv_matching(&rx, |u| u.sessions.len() == 2);
        assert!(update.sessions.iter().any(|s| s.pid == 4343));

        handle.stop();
    }

    #[test]
    fn a_session_turning_waiting_produces_an_alert() {
        let dir = TempDir::new("waiting");
        dir.write_session(4242, Some("busy"));

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(dir.0.clone(), liveness, PAUSED_THRESHOLD_MS, move |u| {
            let _ = tx.send(u);
        });

        recv_matching(&rx, |u| {
            u.sessions.iter().any(|s| s.state == SessionState::Busy)
        });
        dir.write_session(4242, Some("waiting"));

        let update = recv_matching(&rx, |u| !u.alerts.is_empty());
        assert_eq!(update.alerts[0].session_id, "session-4242");

        handle.stop();
    }

    #[test]
    fn a_removed_registry_file_drops_the_session_without_alerting() {
        let dir = TempDir::new("removed");
        dir.write_session(4242, Some("busy"));

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(dir.0.clone(), liveness, PAUSED_THRESHOLD_MS, move |u| {
            let _ = tx.send(u);
        });

        recv_matching(&rx, |u| u.sessions.len() == 1);
        std::fs::remove_file(dir.0.join("4242.json")).unwrap();

        let update = recv_matching(&rx, |u| u.sessions.is_empty());
        assert!(update.alerts.is_empty());

        handle.stop();
    }

    #[test]
    fn a_missing_directory_is_not_an_error() {
        let missing = std::env::temp_dir().join("cb-watch-does-not-exist");
        let _ = std::fs::remove_dir_all(&missing);

        let (tx, rx) = mpsc::channel();
        let handle = spawn_watcher(
            missing,
            Arc::new(FakeLiveness::new()),
            PAUSED_THRESHOLD_MS,
            move |u| {
                let _ = tx.send(u);
            },
        );

        let update = recv_matching(&rx, |u| u.sessions.is_empty());
        assert!(update.alerts.is_empty());

        handle.stop();
    }

    #[test]
    fn identical_state_does_not_re_emit() {
        let dir = TempDir::new("dedupe");
        dir.write_session(4242, Some("busy"));

        let (tx, rx) = mpsc::channel();
        let liveness = Arc::new(FakeLiveness::new().with_alive_any_start(4242));
        let handle = spawn_watcher(dir.0.clone(), liveness, PAUSED_THRESHOLD_MS, move |u| {
            let _ = tx.send(u);
        });

        recv_matching(&rx, |u| u.sessions.len() == 1);
        // Two reconcile ticks pass with nothing changing.
        std::thread::sleep(TICK * 2 + Duration::from_millis(500));

        // Elapsed time advances every tick, so re-emission would be constant
        // churn if the loop compared whole snapshots instead of states.
        assert!(rx.try_recv().is_err(), "unchanged state must not re-emit");

        handle.stop();
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test watcher::watch
```

Expected: FAIL — `cannot find function spawn_watcher in this scope`.

- [ ] **Step 3: Implement the loop**

Prepend to `src-tauri/src/watcher/watch.rs`:

```rust
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use serde::Serialize;

use crate::watcher::alerts::{diff_alerts, Alert};
use crate::watcher::liveness::PidLiveness;
use crate::watcher::registry::read_registry_dir;
use crate::watcher::state::{snapshot, SessionSnapshot, SessionState};

/// Reconcile interval. Catches process death and paused-threshold crossings,
/// neither of which changes a file and so neither of which FSEvents reports.
pub const TICK: Duration = Duration::from_secs(2);

/// Tauri event carrying every update to the frontend.
pub const UPDATE_EVENT: &str = "sessions://update";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Update {
    pub sessions: Vec<SessionSnapshot>,
    pub alerts: Vec<Alert>,
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Identity of a snapshot for change detection: everything the UI renders
/// except the clock-derived fields. Without this, elapsed time alone would make
/// every tick look like a change and the UI would re-render twice a second.
fn fingerprint(sessions: &[SessionSnapshot]) -> Vec<(String, SessionState, Option<String>)> {
    sessions
        .iter()
        .map(|s| (s.session_id.clone(), s.state, s.detail.clone()))
        .collect()
}

pub struct WatcherHandle {
    stop: Arc<AtomicBool>,
    join: Option<std::thread::JoinHandle<()>>,
}

impl WatcherHandle {
    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

/// Watch the registry directory and call `on_update` whenever session state
/// changes. Emits once immediately so the UI has data before the first tick.
///
/// This function only ever reads `dir`.
pub fn spawn_watcher(
    dir: PathBuf,
    liveness: Arc<dyn PidLiveness + Send + Sync>,
    paused_threshold_ms: i64,
    on_update: impl Fn(Update) + Send + 'static,
) -> WatcherHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let stop_thread = stop.clone();

    let join = std::thread::spawn(move || {
        let (tx, rx) = mpsc::channel::<()>();

        // A missing directory is normal (Claude Code never run). Watching fails,
        // the tick still runs, and the UI shows an empty state.
        let mut fs_watcher = notify::recommended_watcher(move |_res| {
            let _ = tx.send(());
        })
        .ok();
        if let Some(w) = fs_watcher.as_mut() {
            let _ = w.watch(&dir, RecursiveMode::NonRecursive);
        }

        let mut previous: Option<Vec<SessionSnapshot>> = None;

        while !stop_thread.load(Ordering::Relaxed) {
            let sessions = snapshot(
                &read_registry_dir(&dir),
                liveness.as_ref(),
                now_ms(),
                paused_threshold_ms,
            );

            let changed = previous
                .as_ref()
                .map(|prev| fingerprint(prev) != fingerprint(&sessions))
                .unwrap_or(true);

            if changed {
                let alerts = diff_alerts(previous.as_deref(), &sessions);
                on_update(Update { sessions: sessions.clone(), alerts });
                previous = Some(sessions);
            }

            // Wake on either an FSEvents notification or the reconcile tick,
            // whichever comes first.
            let _ = rx.recv_timeout(TICK);
        }
    });

    WatcherHandle { stop, join: Some(join) }
}
```

- [ ] **Step 4: Export the module**

`src-tauri/src/watcher/mod.rs`:

```rust
pub mod alerts;
pub mod liveness;
pub mod registry;
pub mod state;
pub mod watch;
```

- [ ] **Step 5: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test watcher::watch -- --test-threads=1
```

Expected: PASS — 6 tests. These touch the real filesystem and real time, so they
run serially and take roughly 20 seconds.

- [ ] **Step 6: Wire the watcher into the Tauri app**

Replace `src-tauri/src/lib.rs` with:

```rust
pub mod config;
pub mod watcher;

use std::sync::Arc;

use tauri::{Emitter, Manager};

use crate::watcher::liveness::SysLiveness;
use crate::watcher::registry::registry_dir;
use crate::watcher::watch::{spawn_watcher, UPDATE_EVENT};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let settings = config::load(&config::config_path());
            let handle = app.handle().clone();

            let watcher = spawn_watcher(
                registry_dir(),
                Arc::new(SysLiveness),
                settings.paused_threshold_ms,
                move |update| {
                    let _ = handle.emit(UPDATE_EVENT, &update);
                },
            );

            // Keep the handle alive for the process lifetime.
            app.manage(watcher);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running claude-buddy");
}
```

- [ ] **Step 7: Verify the app builds**

```bash
cd src-tauri && cargo build
```

Expected: compiles with no errors.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src
git commit -m "feat(watcher): FSEvents plus reconcile tick, emitting to the frontend"
```

---

### Task 8: Floating panel shell

A plain Tauri window with `alwaysOnTop` floats above normal windows but not
above fullscreen apps, and it steals focus on click. Both matter here, so the
window is converted to an `NSPanel`.

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/window.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: manual — panel behaviour cannot be asserted in a unit test

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub fn configure_panel(window: &tauri::WebviewWindow) -> Result<(), String>`
  - `pub fn build_tray_menu(app: &tauri::AppHandle) -> tauri::Result<()>`

- [ ] **Step 1: Confirm the current tauri-nspanel API**

The crate tracks Tauri's own release cadence and its method names have moved
before. Check the current surface before writing against it:

```bash
echo "resolve tauri-nspanel, then query: convert window to panel, set level, set collection behaviour, set style mask"
```

Use the context7 MCP tools for this: `resolve-library-id` with `tauri-nspanel`,
then `query-docs`. If a method name below differs, use the documented one — the
sequence of operations does not change.

- [ ] **Step 2: Add the dependency**

In `src-tauri/Cargo.toml`, under `[dependencies]`:

```toml
tauri-nspanel = { git = "https://github.com/ahkohd/tauri-nspanel", branch = "v2" }
```

- [ ] **Step 3: Implement panel configuration**

`src-tauri/src/window.rs`:

```rust
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_nspanel::cocoa::appkit::{NSWindowCollectionBehavior, NSWindowStyleMask};
use tauri_nspanel::WebviewWindowExt;

/// Above every ordinary window, including fullscreen ones. `NSStatusWindowLevel`
/// is 25; one above it keeps the widget clear of menu-bar extras.
const PANEL_LEVEL: i32 = 26;

/// Convert the widget window into a non-activating panel that follows the user
/// across Spaces and never takes focus.
pub fn configure_panel(window: &WebviewWindow) -> Result<(), String> {
    let panel = window.to_panel().map_err(|e| format!("to_panel failed: {e:?}"))?;

    panel.set_level(PANEL_LEVEL);

    // NonactivatingPanel: clicking the widget does not make claude-buddy the
    // active application, so the user's editor keeps focus and keyboard input.
    panel.set_style_mask(NSWindowStyleMask::NSNonactivatingPanelMask.bits() as i32);

    // CanJoinAllSpaces so the widget follows the user rather than living on one
    // Space; FullScreenAuxiliary so it draws over fullscreen apps.
    panel.set_collection_behaviour(
        NSWindowCollectionBehavior::NSWindowCollectionBehaviorCanJoinAllSpaces
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorFullScreenAuxiliary
            | NSWindowCollectionBehavior::NSWindowCollectionBehaviorStationary,
    );

    Ok(())
}

/// With `LSUIElement` there is no Dock icon and no app-switcher entry, so this
/// menu is the only route to quitting. It ships in v1, not later.
pub fn build_tray_menu(app: &AppHandle) -> tauri::Result<()> {
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, None::<&str>)?;
    let mute = MenuItem::with_id(app, "mute", "Mute alerts 1h", true, None::<&str>)?;

    // Only the dot row renderer exists in v1. The other three are listed but
    // disabled so the menu reflects the plan without offering a mode that would
    // render nothing; the separate view-modes plan enables them.
    let dot_row = MenuItem::with_id(app, "view:dotRow", "Dot row", true, None::<&str>)?;
    let card_stack = MenuItem::with_id(app, "view:cardStack", "Card stack", false, None::<&str>)?;
    let buddy = MenuItem::with_id(app, "view:characterBuddy", "Character buddy", false, None::<&str>)?;
    let invisible = MenuItem::with_id(app, "view:invisible", "Invisible until needed", false, None::<&str>)?;
    let views = Submenu::with_items(
        app,
        "View mode",
        true,
        &[&dot_row, &card_stack, &buddy, &invisible],
    )?;

    let quit = MenuItem::with_id(app, "quit", "Quit claude-buddy", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &settings,
            &mute,
            &PredefinedMenuItem::separator(app)?,
            &views,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("widget-menu")
        // A tray icon without an image fails to build on macOS.
        .icon(app.default_window_icon().cloned().ok_or_else(|| {
            tauri::Error::Anyhow(anyhow::anyhow!("no default window icon in the bundle"))
        })?)
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "quit" => app.exit(0),
            "mute" => {
                let path = crate::config::config_path();
                let mut settings = crate::config::load(&path);
                settings.mute_until_ms = crate::watcher::watch::now_ms() + 60 * 60 * 1000;
                let _ = crate::config::save(&path, &settings);
            }
            "settings" => {
                if let Some(w) = app.get_webview_window("widget") {
                    let _ = w.emit_to("widget", "ui://open-settings", ());
                }
            }
            id if id.starts_with("view:") => {
                let path = crate::config::config_path();
                let mut settings = crate::config::load(&path);
                settings.view_mode = id.trim_start_matches("view:").to_string();
                let _ = crate::config::save(&path, &settings);
                if let Some(w) = app.get_webview_window("widget") {
                    let _ = w.emit_to("widget", "ui://view-mode", &settings.view_mode);
                }
            }
            _ => {}
        })
        .build(app)?;

    Ok(())
}
```

Add `use tauri::Emitter;` to the import block — `emit_to` requires it.

- [ ] **Step 4: Wire it into setup**

In `src-tauri/src/lib.rs`, add to the imports:

```rust
pub mod window;
```

and inside `.setup(|app| { ... })`, before the watcher is spawned:

```rust
            let widget = app
                .get_webview_window("widget")
                .expect("widget window missing from tauri.conf.json");
            window::configure_panel(&widget).map_err(|e| tauri::Error::Anyhow(anyhow::anyhow!(e)))?;
            window::build_tray_menu(app.handle())?;
```

Add `anyhow = "1"` to `[dependencies]` in `src-tauri/Cargo.toml`.

Register the plugin on the builder, above `.setup`:

```rust
        .plugin(tauri_nspanel::init())
```

- [ ] **Step 5: Build and run**

```bash
npm run tauri dev
```

Expected: a small transparent window appears. No Dock icon, no entry in the
app switcher (Cmd-Tab).

- [ ] **Step 6: Verify panel behaviour manually**

Check each, in order:

1. Put another app fullscreen (Cmd-Ctrl-F in Safari). The widget stays visible on top.
2. Switch Spaces. The widget follows rather than staying behind.
3. Click the widget while an editor has focus. The editor keeps its cursor and keyboard focus — the widget must not activate.
4. Right-click the tray icon. The menu shows Settings, Mute alerts 1h, View mode, Quit.
5. Click Quit. The process exits.

If step 3 fails, the style mask did not apply — recheck the API from Step 1.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/src/window.rs src-tauri/src/lib.rs
git commit -m "feat: non-activating floating NSPanel with tray menu"
```

---

### Task 9: Per-display position persistence

Without this the widget lands off-screen on every dock and undock cycle.

**Files:**
- Modify: `src-tauri/src/window.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline `#[cfg(test)]` module in `window.rs`

**Interfaces:**
- Consumes: `Config` (Task 6).
- Produces:
  - `pub fn display_key(name: Option<&str>, width: u32, height: u32) -> String`
  - `pub fn resolve_position(saved: Option<[f64; 2]>, display: (f64, f64), widget: (f64, f64), margin: f64) -> (f64, f64)`
  - `pub const WIDGET_MARGIN: f64`

- [ ] **Step 1: Write the failing tests**

Append to `src-tauri/src/window.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const DISPLAY: (f64, f64) = (1920.0, 1080.0);
    const WIDGET: (f64, f64) = (200.0, 40.0);

    #[test]
    fn display_key_combines_name_and_resolution() {
        assert_eq!(display_key(Some("Built-in Retina Display"), 3024, 1964),
                   "Built-in Retina Display@3024x1964");
    }

    #[test]
    fn display_key_tolerates_an_unnamed_display() {
        assert_eq!(display_key(None, 1920, 1080), "unknown@1920x1080");
    }

    #[test]
    fn no_saved_position_defaults_to_the_top_right_corner() {
        let (x, y) = resolve_position(None, DISPLAY, WIDGET, WIDGET_MARGIN);
        assert_eq!(x, 1920.0 - 200.0 - WIDGET_MARGIN);
        assert_eq!(y, WIDGET_MARGIN);
    }

    #[test]
    fn a_saved_position_inside_the_display_is_honoured() {
        let (x, y) = resolve_position(Some([300.0, 120.0]), DISPLAY, WIDGET, WIDGET_MARGIN);
        assert_eq!((x, y), (300.0, 120.0));
    }

    #[test]
    fn a_position_saved_for_a_larger_display_is_clamped_back_on_screen() {
        // Saved while a 3440-wide monitor was attached, restored on the laptop.
        let (x, y) = resolve_position(Some([3200.0, 900.0]), DISPLAY, WIDGET, WIDGET_MARGIN);
        assert!(x + WIDGET.0 <= DISPLAY.0, "x={x} runs off the right edge");
        assert!(y + WIDGET.1 <= DISPLAY.1, "y={y} runs off the bottom edge");
    }

    #[test]
    fn a_negative_saved_position_is_clamped_to_the_origin() {
        let (x, y) = resolve_position(Some([-500.0, -80.0]), DISPLAY, WIDGET, WIDGET_MARGIN);
        assert_eq!((x, y), (0.0, 0.0));
    }

    #[test]
    fn a_widget_wider_than_the_display_pins_to_the_origin() {
        let (x, y) = resolve_position(Some([10.0, 10.0]), (150.0, 30.0), WIDGET, WIDGET_MARGIN);
        assert_eq!((x, y), (0.0, 0.0));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
cd src-tauri && cargo test window
```

Expected: FAIL — `cannot find function resolve_position in this scope`.

- [ ] **Step 3: Implement position resolution**

Add to `src-tauri/src/window.rs`, above the test module:

```rust
/// Gap from the screen edge for a first-run placement.
pub const WIDGET_MARGIN: f64 = 12.0;

/// Identify a display by name and resolution together. Name alone collides
/// across identical external monitors; resolution alone changes on scaling.
pub fn display_key(name: Option<&str>, width: u32, height: u32) -> String {
    format!("{}@{}x{}", name.unwrap_or("unknown"), width, height)
}

/// Where to put the widget on a given display.
///
/// A saved position is honoured only where it still fits: a position stored
/// against a wide external monitor would otherwise place the widget off-screen
/// on the laptop panel, where it cannot be dragged back.
pub fn resolve_position(
    saved: Option<[f64; 2]>,
    display: (f64, f64),
    widget: (f64, f64),
    margin: f64,
) -> (f64, f64) {
    let max_x = (display.0 - widget.0).max(0.0);
    let max_y = (display.1 - widget.1).max(0.0);

    match saved {
        Some([x, y]) => (x.clamp(0.0, max_x), y.clamp(0.0, max_y)),
        None => ((display.0 - widget.0 - margin).max(0.0), margin.min(max_y)),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

```bash
cd src-tauri && cargo test window
```

Expected: PASS — 7 tests.

- [ ] **Step 5: Restore position on startup and save on move**

Add to `src-tauri/src/window.rs`:

```rust
use tauri::{LogicalPosition, LogicalSize, WindowEvent};

/// Place the widget where it was last left on this display.
pub fn restore_position(window: &WebviewWindow) {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return;
    };
    let scale = monitor.scale_factor();
    let size = monitor.size().to_logical::<f64>(scale);
    let widget: LogicalSize<f64> = match window.outer_size() {
        Ok(s) => s.to_logical(scale),
        Err(_) => return,
    };

    let key = display_key(monitor.name().map(|s| s.as_str()), size.width as u32, size.height as u32);
    let settings = crate::config::load(&crate::config::config_path());

    let (x, y) = resolve_position(
        settings.positions.get(&key).copied(),
        (size.width, size.height),
        (widget.width, widget.height),
        WIDGET_MARGIN,
    );

    let _ = window.set_position(LogicalPosition::new(x, y));
}

/// Persist the widget's position against the display it now sits on.
pub fn attach_move_persistence(window: &WebviewWindow) {
    let handle = window.clone();
    window.on_window_event(move |event| {
        if !matches!(event, WindowEvent::Moved(_)) {
            return;
        }
        let Ok(Some(monitor)) = handle.current_monitor() else {
            return;
        };
        let scale = monitor.scale_factor();
        let size = monitor.size().to_logical::<f64>(scale);
        let Ok(pos) = handle.outer_position() else {
            return;
        };
        let logical = pos.to_logical::<f64>(scale);

        let key = display_key(monitor.name().map(|s| s.as_str()), size.width as u32, size.height as u32);
        let path = crate::config::config_path();
        let mut settings = crate::config::load(&path);
        settings.positions.insert(key, [logical.x, logical.y]);
        let _ = crate::config::save(&path, &settings);
    });
}
```

- [ ] **Step 6: Call both from setup**

In `src-tauri/src/lib.rs`, immediately after `window::configure_panel(&widget)`:

```rust
            window::restore_position(&widget);
            window::attach_move_persistence(&widget);
```

- [ ] **Step 7: Verify manually**

```bash
npm run tauri dev
```

Drag the widget somewhere distinctive, quit via the tray menu, relaunch.
Expected: it returns to where you left it. Then confirm the config file:

```bash
cat ~/Library/Application\ Support/com.claude.buddy/config.json
```

Expected: a `positions` entry keyed like `"Built-in Retina Display@3024x1964"`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/window.rs src-tauri/src/lib.rs
git commit -m "feat: remember widget position per display"
```

---

### Task 10: Frontend snapshot plumbing

**Files:**
- Create: `src/types.ts`
- Create: `src/useSessions.ts`
- Create: `src/useSessions.test.ts`
- Modify: `src/App.tsx`
- Create: `src/views/SessionView.ts`

**Interfaces:**
- Consumes: the `sessions://update` Tauri event and the `Update` payload (Task 7).
- Produces:
  - `src/types.ts`: `SessionState`, `SessionSnapshot`, `AlertKind`, `Alert`, `Update`, `TranscriptDetail`
  - `src/useSessions.ts`: `useSessions(): { sessions: SessionSnapshot[]; ready: boolean }`
  - `src/views/SessionView.ts`: `SessionViewProps` — the shared renderer interface every view mode implements

- [ ] **Step 1: Define the mirrored types**

`src/types.ts`:

```ts
// Mirrors src-tauri/src/watcher/state.rs and alerts.rs. Rust serializes
// camelCase; these names must match exactly.

export type SessionState = 'waiting' | 'busy' | 'idle' | 'paused' | 'dead'

export interface SessionSnapshot {
  pid: number
  sessionId: string
  name: string
  cwd: string
  entrypoint: string
  state: SessionState
  /** The registry's waitingFor, present only while waiting. */
  detail: string | null
  elapsedMs: number
  uptimeMs: number
}

export type AlertKind = 'needsInput' | 'died'

export interface Alert {
  sessionId: string
  name: string
  kind: AlertKind
  detail: string | null
}

export interface Update {
  sessions: SessionSnapshot[]
  alerts: Alert[]
}

/** Fields that live only in the session transcript, fetched lazily on hover. */
export interface TranscriptDetail {
  branch: string | null
  model: string | null
  effort: string | null
}

export const UPDATE_EVENT = 'sessions://update'
```

`src/views/SessionView.ts`:

```ts
import type { SessionSnapshot } from '../types'

/** Every view mode takes exactly this. Adding a mode touches no other layer. */
export interface SessionViewProps {
  sessions: SessionSnapshot[]
}
```

- [ ] **Step 2: Write the failing hook test**

`src/useSessions.test.ts`:

```ts
import { act, renderHook } from '@testing-library/react'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { Update } from './types'

const listeners = new Map<string, (event: { payload: unknown }) => void>()
const unlisten = vi.fn()

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
    cwd: '/Users/n/Code/x',
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 0,
    uptimeMs: 0,
  }
}

describe('useSessions', () => {
  beforeEach(() => {
    listeners.clear()
    unlisten.mockClear()
  })

  it('starts empty and not ready', () => {
    const { result } = renderHook(() => useSessions())
    expect(result.current.sessions).toEqual([])
    expect(result.current.ready).toBe(false)
  })

  it('exposes sessions from an update and becomes ready', async () => {
    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    emit({ sessions: [session('a', 'waiting')], alerts: [] })

    expect(result.current.sessions).toHaveLength(1)
    expect(result.current.sessions[0].sessionId).toBe('a')
    expect(result.current.ready).toBe(true)
  })

  it('replaces state wholesale rather than merging', async () => {
    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    emit({ sessions: [session('a', 'busy'), session('b', 'busy')], alerts: [] })
    emit({ sessions: [session('b', 'waiting')], alerts: [] })

    expect(result.current.sessions.map((s) => s.sessionId)).toEqual(['b'])
    expect(result.current.sessions[0].state).toBe('waiting')
  })

  it('becomes ready on an empty update', async () => {
    const { result } = renderHook(() => useSessions())
    await act(async () => {})

    emit({ sessions: [], alerts: [] })

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
```

- [ ] **Step 3: Run the test to verify it fails**

```bash
npm test -- useSessions
```

Expected: FAIL — cannot resolve `./useSessions`.

- [ ] **Step 4: Implement the hook**

`src/useSessions.ts`:

```ts
import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { UPDATE_EVENT, type SessionSnapshot, type Update } from './types'

/**
 * Subscribe to watcher updates.
 *
 * The backend sends complete snapshots, already sorted and with every derived
 * field computed. This hook therefore replaces state wholesale and derives
 * nothing — no merging, no local state machine.
 */
export function useSessions(): { sessions: SessionSnapshot[]; ready: boolean } {
  const [sessions, setSessions] = useState<SessionSnapshot[]>([])
  const [ready, setReady] = useState(false)

  useEffect(() => {
    let disposed = false
    let stop: (() => void) | undefined

    listen<Update>(UPDATE_EVENT, (event) => {
      setSessions(event.payload.sessions)
      setReady(true)
    }).then((unlisten) => {
      if (disposed) unlisten()
      else stop = unlisten
    })

    return () => {
      disposed = true
      stop?.()
    }
  }, [])

  return { sessions, ready }
}
```

- [ ] **Step 5: Run the test to verify it passes**

```bash
npm test -- useSessions
```

Expected: PASS — 5 tests.

- [ ] **Step 6: Commit**

```bash
git add src/types.ts src/useSessions.ts src/useSessions.test.ts src/views/SessionView.ts
git commit -m "feat(ui): subscribe to watcher snapshots"
```

---

### Task 11: Collapsed summary pill

The resting state. Constant width regardless of session count, and the amber
chip is absent entirely when nothing needs input.

**Files:**
- Modify: `src/format.ts`
- Modify: `src/format.test.ts`
- Create: `src/views/dotRow/CollapsedPill.tsx`
- Create: `src/views/dotRow/CollapsedPill.test.tsx`
- Create: `src/views/dotRow/dotRow.css`

**Interfaces:**
- Consumes: `SessionSnapshot` (Task 10), `formatElapsed` (Task 1).
- Produces:
  - `src/format.ts`: `shortName(name: string): string`, `countByState(sessions: SessionSnapshot[]): Record<SessionState, number>`
  - `src/views/dotRow/CollapsedPill.tsx`: `CollapsedPill({ sessions }: SessionViewProps)`

- [ ] **Step 1: Write the failing helper tests**

Append to `src/format.test.ts`:

```ts
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
  }
}

describe('shortName', () => {
  it('strips the two-character suffix Claude Code appends', () => {
    expect(shortName('api-service-55')).toBe('api-service')
    expect(shortName('claude-buddy-1f')).toBe('claude-buddy')
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
```

- [ ] **Step 2: Run to verify failure**

```bash
npm test -- format
```

Expected: FAIL — `shortName` is not exported.

- [ ] **Step 3: Implement the helpers**

Append to `src/format.ts`:

```ts
import type { SessionSnapshot, SessionState } from './types'

/**
 * Claude Code derives session names as `<project>-<2 chars>`, e.g.
 * `api-service-55`. The suffix disambiguates two sessions in one repo but
 * costs horizontal space the pill does not have, so the row drops it. The
 * popover shows the full name.
 */
export function shortName(name: string): string {
  const stripped = name.replace(/-[a-z0-9]{2}$/i, '')
  return stripped.length > 0 ? stripped : name
}

export function countByState(sessions: SessionSnapshot[]): Record<SessionState, number> {
  const counts: Record<SessionState, number> = {
    waiting: 0,
    busy: 0,
    idle: 0,
    paused: 0,
    dead: 0,
  }
  for (const session of sessions) counts[session.state] += 1
  return counts
}
```

- [ ] **Step 4: Run to verify the helpers pass**

```bash
npm test -- format
```

Expected: PASS — 10 tests.

- [ ] **Step 5: Write the failing component test**

`src/views/dotRow/CollapsedPill.test.tsx`:

```tsx
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
  }
}

describe('CollapsedPill', () => {
  it('shows the needs-you chip with a count when sessions are waiting', () => {
    render(<CollapsedPill sessions={[session('a', 'waiting'), session('b', 'busy')]} />)
    expect(screen.getByTestId('needs-you')).toHaveTextContent('1 needs you')
  })

  it('pluralises the working count', () => {
    render(<CollapsedPill sessions={[session('a', 'busy'), session('b', 'busy')]} />)
    expect(screen.getByTestId('summary')).toHaveTextContent('2 working')
  })

  it('omits the needs-you chip entirely when nothing is waiting', () => {
    render(<CollapsedPill sessions={[session('a', 'busy')]} />)
    expect(screen.queryByTestId('needs-you')).not.toBeInTheDocument()
  })

  it('counts waiting sessions across several at once', () => {
    render(<CollapsedPill sessions={[session('a', 'waiting'), session('b', 'waiting')]} />)
    expect(screen.getByTestId('needs-you')).toHaveTextContent('2 need you')
  })

  it('falls back to an idle count when nothing is working', () => {
    render(<CollapsedPill sessions={[session('a', 'idle'), session('b', 'paused')]} />)
    expect(screen.getByTestId('summary')).toHaveTextContent('2 idle')
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
```

- [ ] **Step 6: Run to verify failure**

```bash
npm test -- CollapsedPill
```

Expected: FAIL — cannot resolve `./CollapsedPill`.

- [ ] **Step 7: Implement the pill**

`src/views/dotRow/CollapsedPill.tsx`:

```tsx
import { countByState } from '../../format'
import type { SessionViewProps } from '../SessionView'
import './dotRow.css'

export function CollapsedPill({ sessions }: SessionViewProps) {
  const counts = countByState(sessions)
  const idle = counts.idle + counts.paused

  // Priority order: what needs the user, then what is running, then what is
  // merely present. Only one summary label shows, so width stays constant.
  const summary =
    sessions.length === 0
      ? 'no sessions'
      : counts.busy > 0
        ? `${counts.busy} working`
        : idle > 0
          ? `${idle} idle`
          : null

  return (
    <div className="pill" data-testid="collapsed-pill">
      {counts.waiting > 0 && (
        <span className="chip chip-waiting" data-testid="needs-you">
          <span className="dot dot-waiting" />
          {counts.waiting} {counts.waiting === 1 ? 'needs' : 'need'} you
        </span>
      )}
      {counts.dead > 0 && (
        <span className="chip chip-dead" data-testid="died">
          {counts.dead} died
        </span>
      )}
      {summary !== null && (
        <span className="summary" data-testid="summary">
          {summary}
        </span>
      )}
    </div>
  )
}
```

- [ ] **Step 8: Add the stylesheet**

`src/views/dotRow/dotRow.css`:

```css
:root {
  --bg: rgba(22, 26, 36, 0.86);
  --bg-lit: rgba(30, 35, 47, 0.92);
  --border: rgba(255, 255, 255, 0.1);
  --border-lit: rgba(255, 255, 255, 0.2);
  --text: #e8ecf5;
  --muted: #98a1b5;
  --waiting: #f5a524;
  --busy: #2f9be0;
  --idle: #5b6479;
  --paused: #3d4454;
  --dead: #e0533d;
}

html,
body {
  margin: 0;
  background: transparent;
  overflow: hidden;
  font: 500 12px/1 -apple-system, 'SF Pro Text', system-ui, sans-serif;
  color: var(--text);
  -webkit-user-select: none;
}

.pill {
  display: inline-flex;
  align-items: center;
  gap: 10px;
  padding: 8px 12px;
  white-space: nowrap;
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 999px;
  box-shadow: 0 8px 26px rgba(0, 0, 0, 0.45);
  backdrop-filter: blur(18px);
  transition: background 120ms ease, border-color 120ms ease;
}

.pill:hover {
  background: var(--bg-lit);
  border-color: var(--border-lit);
}

.dot {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  flex: none;
}

.dot-waiting { background: var(--waiting); box-shadow: 0 0 0 3px rgba(245, 165, 36, 0.2); }
.dot-busy    { background: var(--busy);    box-shadow: 0 0 0 3px rgba(47, 155, 224, 0.18); }
.dot-idle    { background: var(--idle); }
.dot-paused  { background: var(--paused); }
.dot-dead    { background: var(--dead);    box-shadow: 0 0 0 3px rgba(224, 83, 61, 0.18); }

.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 3px 8px;
  border-radius: 999px;
  font-size: 10.5px;
  font-weight: 600;
}

.chip-waiting {
  color: var(--waiting);
  background: rgba(245, 165, 36, 0.14);
  border: 1px solid rgba(245, 165, 36, 0.32);
}

.chip-dead {
  color: var(--dead);
  background: rgba(224, 83, 61, 0.14);
  border: 1px solid rgba(224, 83, 61, 0.32);
}

.summary {
  color: var(--muted);
  font-size: 11px;
}
```

- [ ] **Step 9: Run to verify it passes**

```bash
npm test -- CollapsedPill
```

Expected: PASS — 7 tests.

- [ ] **Step 10: Commit**

```bash
git add src/format.ts src/format.test.ts src/views/dotRow
git commit -m "feat(ui): collapsed summary pill"
```

---

### Task 12: Morph to named dots on hover

**Files:**
- Create: `src/views/dotRow/NamedDotRow.tsx`
- Create: `src/views/dotRow/NamedDotRow.test.tsx`
- Create: `src/views/dotRow/DotRow.tsx`
- Create: `src/views/dotRow/DotRow.test.tsx`
- Modify: `src/views/dotRow/dotRow.css`
- Modify: `src/App.tsx`

**Interfaces:**
- Consumes: `CollapsedPill` (Task 11), `shortName` (Task 11), `SessionViewProps` (Task 10).
- Produces:
  - `NamedDotRow({ sessions, onHoverSession, hoveredSessionId })` where `onHoverSession: (sessionId: string | null) => void`
  - `DotRow({ sessions }: SessionViewProps)` — owns the collapsed/expanded hover state
  - `pub const MAX_VISIBLE = 8` equivalent: `export const MAX_VISIBLE = 8` in `NamedDotRow.tsx`

- [ ] **Step 1: Write the failing NamedDotRow test**

`src/views/dotRow/NamedDotRow.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { NamedDotRow } from './NamedDotRow'
import type { SessionSnapshot, SessionState } from '../../types'

function session(name: string, state: SessionState): SessionSnapshot {
  return {
    pid: 1,
    sessionId: `id-${name}`,
    name,
    cwd: `/Users/n/Code/${name}`,
    entrypoint: 'cli',
    state,
    detail: state === 'waiting' ? 'input needed' : null,
    elapsedMs: 60_000,
    uptimeMs: 60_000,
  }
}

describe('NamedDotRow', () => {
  it('renders one entry per session with the suffix stripped', () => {
    render(
      <NamedDotRow
        sessions={[session('api-service-55', 'waiting'), session('web-app-e2', 'busy')]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByText('api-service')).toBeInTheDocument()
    expect(screen.getByText('web-app')).toBeInTheDocument()
  })

  it('marks each entry with its state for styling', () => {
    render(
      <NamedDotRow
        sessions={[session('a-11', 'waiting')]}
        hoveredSessionId={null}
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByTestId('session-id-a-11')).toHaveAttribute('data-state', 'waiting')
  })

  it('reports the hovered session', async () => {
    const onHover = vi.fn()
    render(
      <NamedDotRow
        sessions={[session('a-11', 'busy')]}
        hoveredSessionId={null}
        onHoverSession={onHover}
      />,
    )

    await userEvent.hover(screen.getByTestId('session-id-a-11'))

    expect(onHover).toHaveBeenCalledWith('id-a-11')
  })

  it('reports null when the cursor leaves an entry', async () => {
    const onHover = vi.fn()
    render(
      <NamedDotRow
        sessions={[session('a-11', 'busy')]}
        hoveredSessionId="id-a-11"
        onHoverSession={onHover}
      />,
    )

    await userEvent.unhover(screen.getByTestId('session-id-a-11'))

    expect(onHover).toHaveBeenLastCalledWith(null)
  })

  it('flags the hovered entry so it can be highlighted', () => {
    render(
      <NamedDotRow
        sessions={[session('a-11', 'busy')]}
        hoveredSessionId="id-a-11"
        onHoverSession={vi.fn()}
      />,
    )
    expect(screen.getByTestId('session-id-a-11')).toHaveAttribute('data-hovered', 'true')
  })

  it('caps the row and reports the overflow count', () => {
    const many = Array.from({ length: 11 }, (_, i) => session(`proj${i}-11`, 'busy'))
    render(<NamedDotRow sessions={many} hoveredSessionId={null} onHoverSession={vi.fn()} />)

    expect(screen.getAllByTestId(/^session-/)).toHaveLength(8)
    expect(screen.getByTestId('overflow')).toHaveTextContent('+3 more')
  })

  it('shows no overflow marker at exactly the cap', () => {
    const many = Array.from({ length: 8 }, (_, i) => session(`proj${i}-11`, 'busy'))
    render(<NamedDotRow sessions={many} hoveredSessionId={null} onHoverSession={vi.fn()} />)

    expect(screen.queryByTestId('overflow')).not.toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run to verify failure**

```bash
npm test -- NamedDotRow
```

Expected: FAIL — cannot resolve `./NamedDotRow`.

- [ ] **Step 3: Implement NamedDotRow**

`src/views/dotRow/NamedDotRow.tsx`:

```tsx
import { shortName } from '../../format'
import type { SessionSnapshot } from '../../types'
import './dotRow.css'

/**
 * Beyond this the row is wider than any sane corner of the screen, so the
 * remainder collapses into a count. Sessions are already sorted with whatever
 * needs the user first, so the hidden tail is the least urgent by construction.
 */
export const MAX_VISIBLE = 8

interface Props {
  sessions: SessionSnapshot[]
  hoveredSessionId: string | null
  onHoverSession: (sessionId: string | null) => void
}

export function NamedDotRow({ sessions, hoveredSessionId, onHoverSession }: Props) {
  const visible = sessions.slice(0, MAX_VISIBLE)
  const hidden = sessions.length - visible.length

  return (
    <div className="pill pill-expanded" data-testid="named-dot-row">
      {visible.map((session, index) => (
        <span key={session.sessionId} className="entry-group">
          {index > 0 && <span className="hairline" />}
          <span
            className="entry"
            data-testid={`session-${session.sessionId}`}
            data-state={session.state}
            data-hovered={hoveredSessionId === session.sessionId ? 'true' : 'false'}
            onMouseEnter={() => onHoverSession(session.sessionId)}
            onMouseLeave={() => onHoverSession(null)}
          >
            <span className={`dot dot-${session.state}`} />
            <span className="entry-name">{shortName(session.name)}</span>
          </span>
        </span>
      ))}
      {hidden > 0 && (
        <span className="summary" data-testid="overflow">
          +{hidden} more
        </span>
      )}
    </div>
  )
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
npm test -- NamedDotRow
```

Expected: PASS — 7 tests.

- [ ] **Step 5: Write the failing DotRow test**

`src/views/dotRow/DotRow.test.tsx`:

```tsx
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { DotRow } from './DotRow'
import type { SessionSnapshot } from '../../types'

const sessions: SessionSnapshot[] = [
  {
    pid: 7952,
    sessionId: 'id-a',
    name: 'api-service-55',
    cwd: '/Users/n/Code/api-service',
    entrypoint: 'cli',
    state: 'waiting',
    detail: 'input needed',
    elapsedMs: 360_000,
    uptimeMs: 3_600_000,
  },
]

describe('DotRow', () => {
  it('rests in the collapsed pill', () => {
    render(<DotRow sessions={sessions} />)
    expect(screen.getByTestId('collapsed-pill')).toBeInTheDocument()
    expect(screen.queryByTestId('named-dot-row')).not.toBeInTheDocument()
  })

  it('morphs to named dots when the pill is hovered', async () => {
    render(<DotRow sessions={sessions} />)

    await userEvent.hover(screen.getByTestId('dot-row'))

    expect(screen.getByTestId('named-dot-row')).toBeInTheDocument()
    expect(screen.queryByTestId('collapsed-pill')).not.toBeInTheDocument()
  })

  it('returns to collapsed when the cursor leaves', async () => {
    render(<DotRow sessions={sessions} />)
    const root = screen.getByTestId('dot-row')

    await userEvent.hover(root)
    await userEvent.unhover(root)

    expect(screen.getByTestId('collapsed-pill')).toBeInTheDocument()
  })

  it('stays collapsed with no sessions', async () => {
    render(<DotRow sessions={[]} />)

    await userEvent.hover(screen.getByTestId('dot-row'))

    expect(screen.getByTestId('collapsed-pill')).toBeInTheDocument()
    expect(screen.queryByTestId('named-dot-row')).not.toBeInTheDocument()
  })
})
```

- [ ] **Step 6: Run to verify failure**

```bash
npm test -- DotRow
```

Expected: FAIL — cannot resolve `./DotRow`.

- [ ] **Step 7: Implement DotRow**

`src/views/dotRow/DotRow.tsx`:

```tsx
import { useState } from 'react'
import { CollapsedPill } from './CollapsedPill'
import { NamedDotRow } from './NamedDotRow'
import type { SessionViewProps } from '../SessionView'
import './dotRow.css'

/**
 * Owns the two-stage hover state machine. Stage 1 (this component) swaps the
 * collapsed pill for the named row. Stage 2 — the per-session popover — arrives
 * in Task 16 and keys off `hoveredSessionId`.
 */
export function DotRow({ sessions }: SessionViewProps) {
  const [expanded, setExpanded] = useState(false)
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null)

  // With nothing to name, morphing would show an empty row.
  const showNamed = expanded && sessions.length > 0

  return (
    <div
      className="dot-row"
      data-testid="dot-row"
      onMouseEnter={() => setExpanded(true)}
      onMouseLeave={() => {
        setExpanded(false)
        setHoveredSessionId(null)
      }}
    >
      {showNamed ? (
        <NamedDotRow
          sessions={sessions}
          hoveredSessionId={hoveredSessionId}
          onHoverSession={setHoveredSessionId}
        />
      ) : (
        <CollapsedPill sessions={sessions} />
      )}
    </div>
  )
}
```

- [ ] **Step 8: Extend the stylesheet**

Append to `src/views/dotRow/dotRow.css`:

```css
.dot-row {
  display: inline-block;
  /* The panel is click-through except over the widget itself. */
  -webkit-app-region: drag;
}

.pill-expanded {
  gap: 8px;
}

.entry-group {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.entry {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 3px 7px;
  border-radius: 999px;
  cursor: pointer;
  -webkit-app-region: no-drag;
}

.entry[data-hovered='true'] {
  background: rgba(255, 255, 255, 0.07);
}

.entry[data-state='waiting'][data-hovered='true'] {
  background: rgba(245, 165, 36, 0.13);
}

.entry-name {
  font-size: 11px;
  color: var(--muted);
}

.entry[data-state='waiting'] .entry-name {
  color: var(--waiting);
  font-weight: 600;
}

.entry[data-state='dead'] .entry-name {
  color: var(--dead);
}

.hairline {
  width: 1px;
  height: 14px;
  background: rgba(255, 255, 255, 0.12);
}
```

- [ ] **Step 9: Run to verify it passes**

```bash
npm test -- DotRow
```

Expected: PASS — 4 tests.

- [ ] **Step 10: Render it from App**

`src/App.tsx`:

```tsx
import { useSessions } from './useSessions'
import { DotRow } from './views/dotRow/DotRow'

export function App() {
  const { sessions } = useSessions()
  return <DotRow sessions={sessions} />
}
```

- [ ] **Step 11: Verify against real sessions**

```bash
npm run tauri dev
```

Expected: the pill shows counts matching your live sessions. Hover: it morphs
into named dots. Compare against the registry:

```bash
ls ~/.claude/sessions/*.json | wc -l
```

The widget should show only `cli` and `claude-desktop` entrypoints, so its count
will be lower than that number whenever plugin sessions are running.

- [ ] **Step 12: Commit**

```bash
git add src/views/dotRow src/App.tsx
git commit -m "feat(ui): morph the pill into named dots on hover"
```

---

### Task 13: Transcript tail

Branch, model and effort exist only in the session transcript, never in the
registry. Transcripts reach megabytes — 3.4MB for a single session has been
observed — so only the tail is ever read, and only when the user hovers.

**Files:**
- Create: `src-tauri/src/bridge/mod.rs`
- Create: `src-tauri/src/bridge/transcript.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline `#[cfg(test)]` module in `transcript.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub struct TranscriptDetail { pub branch: Option<String>, pub model: Option<String>, pub effort: Option<String> }` — serializes camelCase
  - `pub const TAIL_BYTES: u64`
  - `pub fn detail_from_tail(bytes: &[u8]) -> TranscriptDetail`
  - `pub fn project_slug(cwd: &str) -> String`
  - `pub fn find_transcript(projects_dir: &Path, cwd: &str, session_id: &str) -> Option<PathBuf>`
  - `pub fn read_tail(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>>`
  - `#[tauri::command] pub fn session_detail(cwd: String, session_id: String) -> TranscriptDetail`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/bridge/transcript.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Shape taken from a real transcript: an assistant record carries model and
    /// effort, a user record carries neither, and an attachment record carries
    /// almost nothing. The newest record is last.
    const TAIL: &str = concat!(
        r#"{"type":"assistant","message":{"model":"claude-opus-5"},"effort":"xhigh","gitBranch":"feat/rate-limiting","cwd":"/Users/n/Code/api-service"}"#,
        "\n",
        r#"{"type":"user","message":{"role":"user"},"gitBranch":"feat/rate-limiting","cwd":"/Users/n/Code/api-service"}"#,
        "\n",
        r#"{"type":"attachment","attachment":{"type":"total_tokens_reminder"}}"#,
        "\n"
    );

    #[test]
    fn extracts_branch_model_and_effort_from_the_newest_records_that_have_them() {
        let d = detail_from_tail(TAIL.as_bytes());
        assert_eq!(d.branch.as_deref(), Some("feat/rate-limiting"));
        assert_eq!(d.model.as_deref(), Some("claude-opus-5"));
        assert_eq!(d.effort.as_deref(), Some("xhigh"));
    }

    #[test]
    fn prefers_the_newest_value_when_records_disagree() {
        let body = concat!(
            r#"{"gitBranch":"old-branch"}"#,
            "\n",
            r#"{"gitBranch":"new-branch"}"#,
            "\n"
        );
        assert_eq!(detail_from_tail(body.as_bytes()).branch.as_deref(), Some("new-branch"));
    }

    #[test]
    fn a_truncated_first_line_is_skipped_not_fatal() {
        // Reading a fixed tail almost always lands mid-record.
        let body = format!("{}{}", r#"del":"claude-opus-5"},"gitBranch":"junk"}"#, format!("\n{TAIL}"));
        let d = detail_from_tail(body.as_bytes());
        assert_eq!(d.branch.as_deref(), Some("feat/rate-limiting"));
    }

    #[test]
    fn a_transcript_with_no_assistant_message_yet_yields_no_model() {
        let body = concat!(r#"{"type":"user","gitBranch":"main"}"#, "\n");
        let d = detail_from_tail(body.as_bytes());
        assert_eq!(d.branch.as_deref(), Some("main"));
        assert_eq!(d.model, None);
        assert_eq!(d.effort, None);
    }

    #[test]
    fn empty_input_yields_all_none() {
        assert_eq!(detail_from_tail(b""), TranscriptDetail::default());
    }

    #[test]
    fn entirely_unparseable_input_yields_all_none() {
        assert_eq!(detail_from_tail(b"not json at all\nnor this\n"), TranscriptDetail::default());
    }

    #[test]
    fn slug_replaces_separators_with_dashes() {
        assert_eq!(
            project_slug("/Users/dev/Documents/Code/claude-buddy"),
            "-Users-dev-Documents-Code-claude-buddy"
        );
    }

    #[test]
    fn slug_also_replaces_dots() {
        assert_eq!(project_slug("/Users/n/.claude-mem/x"), "-Users-n--claude-mem-x");
    }

    #[test]
    fn find_transcript_locates_the_file_via_the_slug() {
        let root = std::env::temp_dir().join(format!("cb-tx-slug-{}", std::process::id()));
        let dir = root.join("-Users-n-Code-proj");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("abc-123.jsonl"), TAIL).unwrap();

        let found = find_transcript(&root, "/Users/n/Code/proj", "abc-123");

        assert_eq!(found, Some(dir.join("abc-123.jsonl")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_transcript_falls_back_to_scanning_when_the_slug_does_not_match() {
        // The session was started in a subdirectory, so the slug guess misses.
        let root = std::env::temp_dir().join(format!("cb-tx-scan-{}", std::process::id()));
        let dir = root.join("-Users-n-Code-somewhere-else");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("abc-123.jsonl"), TAIL).unwrap();

        let found = find_transcript(&root, "/Users/n/Code/proj", "abc-123");

        assert_eq!(found, Some(dir.join("abc-123.jsonl")));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn find_transcript_returns_none_when_absent() {
        let root = std::env::temp_dir().join(format!("cb-tx-none-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        assert_eq!(find_transcript(&root, "/Users/n/Code/proj", "nope"), None);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn read_tail_returns_only_the_last_bytes_of_a_large_file() {
        let path = std::env::temp_dir().join(format!("cb-tail-{}.jsonl", std::process::id()));
        let filler = "x".repeat(200_000);
        std::fs::write(&path, format!("{filler}\n{TAIL}")).unwrap();

        let bytes = read_tail(&path, TAIL_BYTES).unwrap();

        assert!(bytes.len() as u64 <= TAIL_BYTES);
        assert_eq!(
            detail_from_tail(&bytes).branch.as_deref(),
            Some("feat/rate-limiting")
        );
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn read_tail_handles_a_file_smaller_than_the_window() {
        let path = std::env::temp_dir().join(format!("cb-tail-small-{}.jsonl", std::process::id()));
        std::fs::write(&path, TAIL).unwrap();

        let bytes = read_tail(&path, TAIL_BYTES).unwrap();

        assert_eq!(bytes.len(), TAIL.len());
        std::fs::remove_file(&path).unwrap();
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd src-tauri && cargo test bridge::transcript
```

Expected: FAIL — `cannot find function detail_from_tail in this scope`.

- [ ] **Step 3: Implement the tail reader**

Prepend to `src-tauri/src/bridge/transcript.rs`:

```rust
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use serde::Serialize;

/// How much of a transcript to read. Transcripts reach megabytes; the fields
/// claude-buddy wants are always in the last few records.
pub const TAIL_BYTES: u64 = 65_536;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptDetail {
    pub branch: Option<String>,
    pub model: Option<String>,
    pub effort: Option<String>,
}

impl TranscriptDetail {
    fn complete(&self) -> bool {
        self.branch.is_some() && self.model.is_some() && self.effort.is_some()
    }
}

/// Extract the newest available value for each field.
///
/// Records are scanned newest-first because different record types carry
/// different fields: an assistant record has model and effort, a user record has
/// only branch, an attachment record has none. Unparseable lines — including the
/// truncated first line a fixed-size tail almost always produces — are skipped.
pub fn detail_from_tail(bytes: &[u8]) -> TranscriptDetail {
    let text = String::from_utf8_lossy(bytes);
    let mut detail = TranscriptDetail::default();

    for line in text.lines().rev() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };

        if detail.branch.is_none() {
            if let Some(branch) = record.get("gitBranch").and_then(|v| v.as_str()) {
                detail.branch = Some(branch.to_string());
            }
        }
        if detail.model.is_none() {
            if let Some(model) = record
                .get("message")
                .and_then(|m| m.get("model"))
                .and_then(|v| v.as_str())
            {
                detail.model = Some(model.to_string());
            }
        }
        if detail.effort.is_none() {
            if let Some(effort) = record.get("effort").and_then(|v| v.as_str()) {
                detail.effort = Some(effort.to_string());
            }
        }

        if detail.complete() {
            break;
        }
    }

    detail
}

/// Claude Code names project directories after the cwd with separators flattened
/// to dashes: `/Users/n/Code/proj` becomes `-Users-n-Code-proj`.
pub fn project_slug(cwd: &str) -> String {
    cwd.replace(['/', '.'], "-")
}

pub fn projects_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(".claude")
        .join("projects")
}

/// Locate a session's transcript.
///
/// The slug guess is tried first because it is a single stat. It misses when the
/// session was started in a subdirectory of the project, so a scan of the
/// project directories is the fallback — cheap, since there is one directory per
/// project the user has ever opened.
pub fn find_transcript(projects_dir: &Path, cwd: &str, session_id: &str) -> Option<PathBuf> {
    let filename = format!("{session_id}.jsonl");

    let guess = projects_dir.join(project_slug(cwd)).join(&filename);
    if guess.is_file() {
        return Some(guess);
    }

    std::fs::read_dir(projects_dir).ok()?.find_map(|entry| {
        let candidate = entry.ok()?.path().join(&filename);
        candidate.is_file().then_some(candidate)
    })
}

/// Read at most `max_bytes` from the end of a file.
pub fn read_tail(path: &Path, max_bytes: u64) -> std::io::Result<Vec<u8>> {
    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    let start = len.saturating_sub(max_bytes);
    file.seek(SeekFrom::Start(start))?;

    let mut buf = Vec::with_capacity(max_bytes.min(len) as usize);
    file.take(max_bytes).read_to_end(&mut buf)?;
    Ok(buf)
}

/// Transcript-only fields for one session.
///
/// Returns an all-`None` detail rather than an error when the transcript is
/// missing or unreadable: the popover must still open and show its
/// registry-sourced fields.
#[tauri::command]
pub fn session_detail(cwd: String, session_id: String) -> TranscriptDetail {
    find_transcript(&projects_dir(), &cwd, &session_id)
        .and_then(|path| read_tail(&path, TAIL_BYTES).ok())
        .map(|bytes| detail_from_tail(&bytes))
        .unwrap_or_default()
}
```

- [ ] **Step 4: Create the module and register the command**

`src-tauri/src/bridge/mod.rs`:

```rust
pub mod transcript;
```

In `src-tauri/src/lib.rs`, add `pub mod bridge;` beside the other modules, and add to the builder chain:

```rust
        .invoke_handler(tauri::generate_handler![bridge::transcript::session_detail])
```

- [ ] **Step 5: Run to verify it passes**

```bash
cd src-tauri && cargo test bridge::transcript
```

Expected: PASS — 13 tests.

- [ ] **Step 6: Sanity-check against a real transcript**

```bash
cd src-tauri && cargo test bridge::transcript -- --nocapture && ls ~/.claude/projects/*/ | head
```

Expected: tests pass, and the listing confirms the `-Users-...` slug layout the
implementation assumes.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/bridge src-tauri/src/lib.rs
git commit -m "feat(bridge): lazily tail transcripts for branch and model"
```

---

### Task 14: Process tree walk

**Files:**
- Create: `src-tauri/src/bridge/proc_tree.rs`
- Modify: `src-tauri/src/bridge/mod.rs`
- Test: inline `#[cfg(test)]` module in `proc_tree.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub trait ProcTree { fn parent(&self, pid: i32) -> Option<i32>; fn exe(&self, pid: i32) -> Option<String>; }`
  - `pub struct PsProcTree` with `pub fn snapshot() -> Self`
  - `pub struct FakeProcTree` with `pub fn new() -> Self`, `pub fn with(self, pid: i32, ppid: i32, exe: &str) -> Self`
  - `pub fn find_app_bundle(tree: &dyn ProcTree, pid: i32) -> Option<String>`
  - `pub fn bundle_identifier(bundle_path: &Path) -> Option<String>`
  - `pub const MAX_WALK_DEPTH: usize`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/bridge/proc_tree.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// The real ancestry of a cli session running in Cursor's integrated
    /// terminal, captured with `ps -o pid=,ppid=,comm=`.
    fn cursor_tree() -> FakeProcTree {
        FakeProcTree::new()
            .with(7952, 7951, "/Users/n/.volta/tools/image/packages/@anthropic-ai/claude-code/bin/claude")
            .with(7951, 7447, "claude")
            .with(7447, 6323, "/bin/zsh")
            .with(6323, 5524, "Cursor Helper: terminal pty-host")
            .with(5524, 1, "/Applications/Cursor.app/Contents/MacOS/Cursor")
    }

    /// The real ancestry of a claude-desktop session.
    fn desktop_tree() -> FakeProcTree {
        FakeProcTree::new()
            .with(99215, 99213, "/Users/n/Library/Application Support/Claude/claude-code/2.1.237/claude.app/Contents/MacOS/claude")
            .with(99213, 51954, "/Applications/Claude.app/Contents/Helpers/disclaimer")
            .with(51954, 1, "/Applications/Claude.app/Contents/MacOS/Claude")
    }

    #[test]
    fn finds_the_editor_bundle_for_a_cli_session() {
        assert_eq!(
            find_app_bundle(&cursor_tree(), 7952),
            Some("/Applications/Cursor.app".to_string())
        );
    }

    #[test]
    fn finds_the_desktop_bundle_for_a_desktop_session() {
        // The session's own executable lives inside a nested claude.app. The walk
        // must reach the outermost real application, not stop at that one.
        assert_eq!(
            find_app_bundle(&desktop_tree(), 99215),
            Some("/Applications/Claude.app".to_string())
        );
    }

    #[test]
    fn returns_none_for_an_orphan_whose_chain_reaches_no_bundle() {
        let tree = FakeProcTree::new()
            .with(300, 200, "/usr/local/bin/claude")
            .with(200, 1, "/bin/zsh");
        assert_eq!(find_app_bundle(&tree, 300), None);
    }

    #[test]
    fn returns_none_for_an_unknown_pid() {
        assert_eq!(find_app_bundle(&cursor_tree(), 424242), None);
    }

    #[test]
    fn a_parent_cycle_terminates_instead_of_looping() {
        let tree = FakeProcTree::new()
            .with(10, 11, "/bin/a")
            .with(11, 10, "/bin/b");
        assert_eq!(find_app_bundle(&tree, 10), None);
    }

    #[test]
    fn reaching_pid_one_terminates() {
        let tree = FakeProcTree::new().with(2, 1, "/bin/zsh").with(1, 1, "/sbin/launchd");
        assert_eq!(find_app_bundle(&tree, 2), None);
    }

    #[test]
    fn ps_snapshot_knows_our_own_process_and_its_parent() {
        let tree = PsProcTree::snapshot();
        let own = std::process::id() as i32;
        assert!(tree.exe(own).is_some(), "own executable should be known");
        assert!(tree.parent(own).is_some(), "own parent should be known");
    }

    #[test]
    fn ps_snapshot_parses_executables_containing_spaces() {
        // "Cursor Helper: terminal pty-host" would break naive whitespace splitting.
        let tree = PsProcTree::parse("  6323  5524 Cursor Helper: terminal pty-host\n");
        assert_eq!(tree.parent(6323), Some(5524));
        assert_eq!(tree.exe(6323).as_deref(), Some("Cursor Helper: terminal pty-host"));
    }

    #[test]
    fn ps_snapshot_skips_malformed_lines() {
        let tree = PsProcTree::parse("garbage\n  10  1 /bin/zsh\n\n");
        assert_eq!(tree.parent(10), Some(1));
    }

    #[test]
    fn bundle_identifier_reads_cfbundleidentifier() {
        let bundle = std::env::temp_dir().join(format!("cb-bundle-{}.app", std::process::id()));
        let contents = bundle.join("Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleIdentifier</key><string>com.example.thing</string>
</dict></plist>"#,
        )
        .unwrap();

        assert_eq!(bundle_identifier(&bundle).as_deref(), Some("com.example.thing"));
        std::fs::remove_dir_all(&bundle).unwrap();
    }

    #[test]
    fn bundle_identifier_returns_none_without_a_plist() {
        let bundle = std::env::temp_dir().join(format!("cb-nobundle-{}.app", std::process::id()));
        std::fs::create_dir_all(&bundle).unwrap();
        assert_eq!(bundle_identifier(&bundle), None);
        std::fs::remove_dir_all(&bundle).unwrap();
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd src-tauri && cargo test bridge::proc_tree
```

Expected: FAIL — `cannot find type FakeProcTree in this scope`.

- [ ] **Step 3: Implement the walk**

Prepend to `src-tauri/src/bridge/proc_tree.rs`:

```rust
use std::collections::HashMap;
use std::path::Path;

/// Guard against a malformed or cyclic parent chain. No real ancestry from a
/// session to its host application comes close to this depth.
pub const MAX_WALK_DEPTH: usize = 32;

pub trait ProcTree {
    fn parent(&self, pid: i32) -> Option<i32>;
    fn exe(&self, pid: i32) -> Option<String>;
}

/// Walk from a session's pid to the GUI application hosting it, and return that
/// application's bundle path.
///
/// A session's own executable can itself live inside a `.app` — Claude Desktop
/// ships `claude.app` inside its Application Support directory — so the walk
/// continues past the first match and returns the outermost bundle found.
pub fn find_app_bundle(tree: &dyn ProcTree, pid: i32) -> Option<String> {
    let mut current = pid;
    let mut seen = std::collections::HashSet::new();
    let mut outermost = None;

    for _ in 0..MAX_WALK_DEPTH {
        if !seen.insert(current) {
            break;
        }
        if let Some(exe) = tree.exe(current) {
            if let Some(index) = exe.find(".app/") {
                outermost = Some(exe[..index + 4].to_string());
            }
        }
        match tree.parent(current) {
            Some(parent) if parent != current && parent > 1 => current = parent,
            _ => break,
        }
    }

    outermost
}

pub fn bundle_identifier(bundle_path: &Path) -> Option<String> {
    let plist = plist::Value::from_file(bundle_path.join("Contents").join("Info.plist")).ok()?;
    plist
        .as_dictionary()?
        .get("CFBundleIdentifier")?
        .as_string()
        .map(str::to_string)
}

/// One `ps` invocation, indexed. Taken as a snapshot so a single raise cannot
/// see an inconsistent tree mid-walk.
pub struct PsProcTree {
    entries: HashMap<i32, (i32, String)>,
}

impl PsProcTree {
    pub fn snapshot() -> Self {
        let out = std::process::Command::new("ps")
            .args(["-Ao", "pid=,ppid=,comm="])
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        Self::parse(&out)
    }

    /// Parse `pid ppid comm` rows. Only the first two fields are whitespace
    /// delimited — the executable path may itself contain spaces, as
    /// `Cursor Helper: terminal pty-host` does.
    pub fn parse(output: &str) -> Self {
        let mut entries = HashMap::new();
        for line in output.lines() {
            let mut parts = line.trim_start().splitn(3, ' ');
            let Some(pid) = parts.next().and_then(|p| p.trim().parse::<i32>().ok()) else {
                continue;
            };
            let rest = parts.collect::<Vec<_>>().join(" ");
            let mut rest_parts = rest.trim_start().splitn(2, ' ');
            let Some(ppid) = rest_parts.next().and_then(|p| p.trim().parse::<i32>().ok()) else {
                continue;
            };
            let exe = rest_parts.next().unwrap_or("").trim().to_string();
            entries.insert(pid, (ppid, exe));
        }
        Self { entries }
    }
}

impl ProcTree for PsProcTree {
    fn parent(&self, pid: i32) -> Option<i32> {
        self.entries.get(&pid).map(|(ppid, _)| *ppid)
    }

    fn exe(&self, pid: i32) -> Option<String> {
        self.entries.get(&pid).map(|(_, exe)| exe.clone())
    }
}

pub struct FakeProcTree {
    entries: HashMap<i32, (i32, String)>,
}

impl FakeProcTree {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    pub fn with(mut self, pid: i32, ppid: i32, exe: &str) -> Self {
        self.entries.insert(pid, (ppid, exe.to_string()));
        self
    }
}

impl Default for FakeProcTree {
    fn default() -> Self {
        Self::new()
    }
}

impl ProcTree for FakeProcTree {
    fn parent(&self, pid: i32) -> Option<i32> {
        self.entries.get(&pid).map(|(ppid, _)| *ppid)
    }

    fn exe(&self, pid: i32) -> Option<String> {
        self.entries.get(&pid).map(|(_, exe)| exe.clone())
    }
}
```

Note the `PsProcTree::parse` splitting: `ps` right-aligns pid columns, so lines
begin with spaces. The two `splitn` passes handle that padding without breaking
on executables containing spaces.

- [ ] **Step 4: Export the module**

`src-tauri/src/bridge/mod.rs`:

```rust
pub mod proc_tree;
pub mod transcript;
```

- [ ] **Step 5: Run to verify it passes**

```bash
cd src-tauri && cargo test bridge::proc_tree
```

Expected: PASS — 11 tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/bridge
git commit -m "feat(bridge): walk the process tree to the host application"
```

---

### Task 15: Raise the session's window

**Files:**
- Create: `src-tauri/src/bridge/raise.rs`
- Modify: `src-tauri/src/bridge/mod.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline `#[cfg(test)]` module in `raise.rs`

**Interfaces:**
- Consumes: `ProcTree`, `FakeProcTree`, `PsProcTree`, `find_app_bundle`, `bundle_identifier` (Task 14).
- Produces:
  - `pub trait Activator { fn activate(&self, bundle_id: &str) -> Result<(), String>; }`
  - `pub struct OpenActivator`
  - `pub struct RecordingActivator` with `pub fn new() -> Self`, `pub fn calls(&self) -> Vec<String>`, `pub fn failing() -> Self`
  - `pub fn raise(tree: &dyn ProcTree, activator: &dyn Activator, resolve_id: &dyn Fn(&Path) -> Option<String>, pid: i32) -> Result<String, String>`
  - `#[tauri::command] pub fn raise_session(pid: i32) -> Result<String, String>`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/bridge/raise.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::bridge::proc_tree::FakeProcTree;

    fn cursor_tree() -> FakeProcTree {
        FakeProcTree::new()
            .with(7952, 7951, "/Users/n/.volta/bin/claude")
            .with(7951, 7447, "claude")
            .with(7447, 6323, "/bin/zsh")
            .with(6323, 5524, "Cursor Helper: terminal pty-host")
            .with(5524, 1, "/Applications/Cursor.app/Contents/MacOS/Cursor")
    }

    fn resolver(id: &'static str) -> impl Fn(&Path) -> Option<String> {
        move |_| Some(id.to_string())
    }

    #[test]
    fn activates_the_bundle_identifier_of_the_host_application() {
        let activator = RecordingActivator::new();

        let outcome = raise(&cursor_tree(), &activator, &resolver("com.todesktop.cursor"), 7952);

        assert_eq!(outcome.as_deref(), Ok("com.todesktop.cursor"));
        assert_eq!(activator.calls(), vec!["com.todesktop.cursor".to_string()]);
    }

    #[test]
    fn errors_when_the_chain_reaches_no_application() {
        let tree = FakeProcTree::new().with(300, 1, "/usr/local/bin/claude");
        let activator = RecordingActivator::new();

        let outcome = raise(&tree, &activator, &resolver("irrelevant"), 300);

        assert!(outcome.is_err());
        assert!(outcome.unwrap_err().contains("no host application"));
        assert!(activator.calls().is_empty(), "must not activate anything");
    }

    #[test]
    fn errors_when_the_bundle_has_no_identifier() {
        let activator = RecordingActivator::new();
        let no_id = |_: &Path| None;

        let outcome = raise(&cursor_tree(), &activator, &no_id, 7952);

        assert!(outcome.unwrap_err().contains("bundle identifier"));
        assert!(activator.calls().is_empty());
    }

    #[test]
    fn propagates_an_activation_failure() {
        let activator = RecordingActivator::failing();

        let outcome = raise(&cursor_tree(), &activator, &resolver("com.todesktop.cursor"), 7952);

        assert!(outcome.unwrap_err().contains("activation refused"));
    }

    #[test]
    fn errors_for_an_unknown_pid_without_activating() {
        let activator = RecordingActivator::new();

        let outcome = raise(&cursor_tree(), &activator, &resolver("x"), 999_999);

        assert!(outcome.is_err());
        assert!(activator.calls().is_empty());
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd src-tauri && cargo test bridge::raise
```

Expected: FAIL — `cannot find function raise in this scope`.

- [ ] **Step 3: Implement raising**

Prepend to `src-tauri/src/bridge/raise.rs`:

```rust
use std::path::Path;
use std::sync::Mutex;

use crate::bridge::proc_tree::{bundle_identifier, find_app_bundle, ProcTree, PsProcTree};

pub trait Activator {
    fn activate(&self, bundle_id: &str) -> Result<(), String>;
}

/// Activation via `open -b`.
///
/// Deliberately not AppleScript: `open` needs neither Accessibility nor
/// Automation permission, so a fresh install raises windows without ever
/// prompting the user. Tab-level targeting, which does need Automation, is a
/// later and strictly additive rung.
pub struct OpenActivator;

impl Activator for OpenActivator {
    fn activate(&self, bundle_id: &str) -> Result<(), String> {
        let status = std::process::Command::new("open")
            .args(["-b", bundle_id])
            .status()
            .map_err(|e| format!("could not run open: {e}"))?;

        if status.success() {
            Ok(())
        } else {
            Err(format!("activation refused by open for {bundle_id}"))
        }
    }
}

/// Raise the application hosting `pid`.
///
/// `resolve_id` is injected so tests never read a real bundle from disk.
pub fn raise(
    tree: &dyn ProcTree,
    activator: &dyn Activator,
    resolve_id: &dyn Fn(&Path) -> Option<String>,
    pid: i32,
) -> Result<String, String> {
    let bundle = find_app_bundle(tree, pid)
        .ok_or_else(|| format!("no host application found for pid {pid}"))?;

    let bundle_id = resolve_id(Path::new(&bundle))
        .ok_or_else(|| format!("no bundle identifier in {bundle}"))?;

    activator.activate(&bundle_id)?;
    Ok(bundle_id)
}

/// Bring the window running a session to the front. Returns the bundle
/// identifier that was activated, for display in the popover.
#[tauri::command]
pub fn raise_session(pid: i32) -> Result<String, String> {
    raise(
        &PsProcTree::snapshot(),
        &OpenActivator,
        &|path| bundle_identifier(path),
        pid,
    )
}

pub struct RecordingActivator {
    calls: Mutex<Vec<String>>,
    fail: bool,
}

impl RecordingActivator {
    pub fn new() -> Self {
        Self { calls: Mutex::new(Vec::new()), fail: false }
    }

    pub fn failing() -> Self {
        Self { calls: Mutex::new(Vec::new()), fail: true }
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
}

impl Default for RecordingActivator {
    fn default() -> Self {
        Self::new()
    }
}

impl Activator for RecordingActivator {
    fn activate(&self, bundle_id: &str) -> Result<(), String> {
        if self.fail {
            return Err(format!("activation refused for {bundle_id}"));
        }
        self.calls.lock().unwrap().push(bundle_id.to_string());
        Ok(())
    }
}
```

- [ ] **Step 4: Export and register**

`src-tauri/src/bridge/mod.rs`:

```rust
pub mod proc_tree;
pub mod raise;
pub mod transcript;
```

In `src-tauri/src/lib.rs`, extend the handler:

```rust
        .invoke_handler(tauri::generate_handler![
            bridge::transcript::session_detail,
            bridge::raise::raise_session
        ])
```

- [ ] **Step 5: Run to verify it passes**

```bash
cd src-tauri && cargo test bridge::raise
```

Expected: PASS — 5 tests.

- [ ] **Step 6: Verify against a real session**

Find a live pid and confirm the walk reaches a bundle:

```bash
cat ~/.claude/sessions/*.json | grep -o '"pid":[0-9]*' | head -3
```

Then, with `npm run tauri dev` running, the popover in Task 16 will exercise
this. For now confirm the whole suite is green:

```bash
cd src-tauri && cargo test
```

Expected: PASS — all tests across `config`, `watcher::*`, `window`, `bridge::*`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/bridge src-tauri/src/lib.rs
git commit -m "feat(bridge): raise a session's host application via open -b"
```

---

### Task 16: Per-session popover and click-to-raise

The window is 200×40 in `tauri.conf.json`, but the pill morphs and the popover
opens beneath it. Without dynamic resizing the panel clips its own content, so
auto-resize is part of this task rather than a later fix.

**Files:**
- Create: `src/useAutoResize.ts`
- Create: `src/useAutoResize.test.ts`
- Create: `src/views/dotRow/SessionPopover.tsx`
- Create: `src/views/dotRow/SessionPopover.test.tsx`
- Modify: `src/views/dotRow/DotRow.tsx`
- Modify: `src/views/dotRow/DotRow.test.tsx`
- Modify: `src/views/dotRow/dotRow.css`

**Interfaces:**
- Consumes: `TranscriptDetail` (Task 10), `session_detail` and `raise_session` commands (Tasks 13, 15), `formatElapsed` (Task 1), `NamedDotRow` (Task 12).
- Produces:
  - `useAutoResize(ref: RefObject<HTMLElement>): void`
  - `SessionPopover({ session })` — fetches its own detail
  - `export const HOVER_GRACE_MS = 180`

- [ ] **Step 1: Write the failing auto-resize test**

`src/useAutoResize.test.ts`:

```ts
import { renderHook } from '@testing-library/react'
import { describe, expect, it, vi } from 'vitest'

const setSize = vi.fn()

vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setSize }),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}))

const { useAutoResize } = await import('./useAutoResize')

function elementRef(width: number, height: number) {
  const el = document.createElement('div')
  el.getBoundingClientRect = () =>
    ({ width, height, top: 0, left: 0, right: width, bottom: height }) as DOMRect
  return { current: el }
}

describe('useAutoResize', () => {
  it('sizes the window to the measured content plus padding', () => {
    setSize.mockClear()
    renderHook(() => useAutoResize(elementRef(380, 96)))

    expect(setSize).toHaveBeenCalled()
    const size = setSize.mock.calls[0][0]
    expect(size.width).toBeGreaterThanOrEqual(380)
    expect(size.height).toBeGreaterThanOrEqual(96)
  })

  it('does nothing when the ref is empty', () => {
    setSize.mockClear()
    renderHook(() => useAutoResize({ current: null }))
    expect(setSize).not.toHaveBeenCalled()
  })

  it('never requests a zero-sized window', () => {
    setSize.mockClear()
    renderHook(() => useAutoResize(elementRef(0, 0)))

    const size = setSize.mock.calls[0]?.[0]
    if (size) {
      expect(size.width).toBeGreaterThan(0)
      expect(size.height).toBeGreaterThan(0)
    }
  })
})
```

- [ ] **Step 2: Run to verify failure**

```bash
npm test -- useAutoResize
```

Expected: FAIL — cannot resolve `./useAutoResize`.

- [ ] **Step 3: Implement auto-resize**

`src/useAutoResize.ts`:

```ts
import { useEffect, type RefObject } from 'react'
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window'

/** Room for the pill's shadow, which is drawn outside its box. */
const SHADOW_PAD = 24
const MIN_SIZE = 8

/**
 * Keep the panel exactly as large as its content.
 *
 * The widget changes size constantly — collapsed pill, morphed row, open
 * popover — and a fixed window would clip the larger states while leaving a
 * transparent dead zone around the smaller ones that swallows clicks.
 */
export function useAutoResize(ref: RefObject<HTMLElement | null>): void {
  useEffect(() => {
    const element = ref.current
    if (!element) return

    const apply = () => {
      const rect = element.getBoundingClientRect()
      const width = Math.max(MIN_SIZE, Math.ceil(rect.width) + SHADOW_PAD)
      const height = Math.max(MIN_SIZE, Math.ceil(rect.height) + SHADOW_PAD)
      void getCurrentWindow().setSize(new LogicalSize(width, height))
    }

    apply()

    // ResizeObserver is absent in some test environments; the initial measure
    // above is still correct without it.
    if (typeof ResizeObserver === 'undefined') return
    const observer = new ResizeObserver(apply)
    observer.observe(element)
    return () => observer.disconnect()
  })
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
npm test -- useAutoResize
```

Expected: PASS — 3 tests.

- [ ] **Step 5: Write the failing popover test**

`src/views/dotRow/SessionPopover.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { SessionSnapshot } from '../../types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const { SessionPopover } = await import('./SessionPopover')

const session: SessionSnapshot = {
  pid: 7952,
  sessionId: 'id-a',
  name: 'api-service-55',
  cwd: '/Users/dev/Documents/Code/api-service',
  entrypoint: 'cli',
  state: 'waiting',
  detail: 'input needed',
  elapsedMs: 360_000,
  uptimeMs: 24_900_000,
}

describe('SessionPopover', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockResolvedValue({ branch: 'feat/rate-limiting', model: 'claude-opus-5', effort: 'xhigh' })
  })

  it('shows the full session name, not the shortened one', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover-name')).toHaveTextContent('api-service-55')
  })

  it('shows the waiting detail with elapsed time', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover-state')).toHaveTextContent('input needed · 6m')
  })

  it('shows the state name when there is no detail', () => {
    render(<SessionPopover session={{ ...session, state: 'busy', detail: null }} />)
    expect(screen.getByTestId('popover-state')).toHaveTextContent('busy · 6m')
  })

  it('shows the cwd', () => {
    render(<SessionPopover session={session} />)
    expect(screen.getByTestId('popover-cwd')).toHaveTextContent('/Users/dev/Documents/Code/api-service')
  })

  it('fetches and shows transcript fields', async () => {
    render(<SessionPopover session={session} />)

    await waitFor(() => {
      expect(screen.getByTestId('popover-branch')).toHaveTextContent('feat/rate-limiting')
    })
    expect(screen.getByTestId('popover-model')).toHaveTextContent('claude-opus-5')
    expect(screen.getByTestId('popover-model')).toHaveTextContent('xhigh')
    expect(invoke).toHaveBeenCalledWith('session_detail', {
      cwd: session.cwd,
      sessionId: session.sessionId,
    })
  })

  it('renders an em dash for transcript fields that are absent', async () => {
    invoke.mockResolvedValue({ branch: null, model: null, effort: null })
    render(<SessionPopover session={session} />)

    await waitFor(() => expect(screen.getByTestId('popover-branch')).toHaveTextContent('—'))
    expect(screen.getByTestId('popover-model')).toHaveTextContent('—')
  })

  it('still opens when the transcript read fails', async () => {
    invoke.mockRejectedValue(new Error('unreadable'))
    render(<SessionPopover session={session} />)

    await waitFor(() => expect(screen.getByTestId('popover-branch')).toHaveTextContent('—'))
    expect(screen.getByTestId('popover-name')).toBeInTheDocument()
  })

  it('raises the session on click', async () => {
    render(<SessionPopover session={session} />)

    await userEvent.click(screen.getByTestId('popover'))

    expect(invoke).toHaveBeenCalledWith('raise_session', { pid: 7952 })
  })

  it('shows the failure in place when raising fails', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'raise_session'
        ? Promise.reject(new Error('no host application found for pid 7952'))
        : Promise.resolve({ branch: null, model: null, effort: null }),
    )
    render(<SessionPopover session={session} />)

    await userEvent.click(screen.getByTestId('popover'))

    await waitFor(() => {
      expect(screen.getByTestId('popover-error')).toHaveTextContent('no host application')
    })
  })
})
```

- [ ] **Step 6: Run to verify failure**

```bash
npm test -- SessionPopover
```

Expected: FAIL — cannot resolve `./SessionPopover`.

- [ ] **Step 7: Implement the popover**

`src/views/dotRow/SessionPopover.tsx`:

```tsx
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { formatElapsed } from '../../format'
import type { SessionSnapshot, TranscriptDetail } from '../../types'
import './dotRow.css'

const EMPTY: TranscriptDetail = { branch: null, model: null, effort: null }

function dash(value: string | null | undefined): string {
  return value && value.length > 0 ? value : '—'
}

export function SessionPopover({ session }: { session: SessionSnapshot }) {
  const [detail, setDetail] = useState<TranscriptDetail>(EMPTY)
  const [error, setError] = useState<string | null>(null)

  // Transcript fields are fetched per hover rather than for every session on
  // every tick: reading them eagerly would tail a file per session twice a
  // second for data the user is usually not looking at.
  useEffect(() => {
    let live = true
    invoke<TranscriptDetail>('session_detail', {
      cwd: session.cwd,
      sessionId: session.sessionId,
    })
      .then((result) => live && setDetail(result))
      .catch(() => live && setDetail(EMPTY))
    return () => {
      live = false
    }
  }, [session.cwd, session.sessionId])

  const raise = () => {
    setError(null)
    invoke<string>('raise_session', { pid: session.pid }).catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    )
  }

  const stateLine = `${session.detail ?? session.state} · ${formatElapsed(session.elapsedMs)}`
  const modelLine = detail.model
    ? `${detail.model}${detail.effort ? ` · ${detail.effort}` : ''}`
    : '—'

  return (
    <div className="popover" data-testid="popover" onClick={raise}>
      <div className="popover-head">
        <span className={`dot dot-${session.state}`} />
        <span className="popover-title" data-testid="popover-name">
          {session.name}
        </span>
      </div>
      <dl className="popover-fields">
        <dt>state</dt>
        <dd className={session.state === 'waiting' ? 'hot' : undefined} data-testid="popover-state">
          {stateLine}
        </dd>
        <dt>cwd</dt>
        <dd data-testid="popover-cwd">{session.cwd}</dd>
        <dt>branch</dt>
        <dd data-testid="popover-branch">{dash(detail.branch)}</dd>
        <dt>model</dt>
        <dd data-testid="popover-model">{modelLine}</dd>
        <dt>proc</dt>
        <dd data-testid="popover-proc">
          {session.entrypoint} · pid {session.pid} · {formatElapsed(session.uptimeMs)}
        </dd>
      </dl>
      {error === null ? (
        <div className="popover-foot">click → raise this window</div>
      ) : (
        <div className="popover-foot popover-foot-error" data-testid="popover-error">
          {error}
        </div>
      )}
    </div>
  )
}
```

- [ ] **Step 8: Run to verify it passes**

```bash
npm test -- SessionPopover
```

Expected: PASS — 9 tests.

- [ ] **Step 9: Add the grace delay and wire the popover into DotRow**

Replace `src/views/dotRow/DotRow.tsx` with:

```tsx
import { useEffect, useRef, useState } from 'react'
import { CollapsedPill } from './CollapsedPill'
import { NamedDotRow } from './NamedDotRow'
import { SessionPopover } from './SessionPopover'
import { useAutoResize } from '../../useAutoResize'
import type { SessionViewProps } from '../SessionView'
import './dotRow.css'

/**
 * Delay before the popover opens. Without it, dragging the cursor across the
 * row flashes a popover per name.
 */
export const HOVER_GRACE_MS = 180

export function DotRow({ sessions }: SessionViewProps) {
  const [expanded, setExpanded] = useState(false)
  const [pendingId, setPendingId] = useState<string | null>(null)
  const [hoveredSessionId, setHoveredSessionId] = useState<string | null>(null)
  const root = useRef<HTMLDivElement>(null)

  useAutoResize(root)

  useEffect(() => {
    if (pendingId === null) {
      setHoveredSessionId(null)
      return
    }
    const timer = setTimeout(() => setHoveredSessionId(pendingId), HOVER_GRACE_MS)
    return () => clearTimeout(timer)
  }, [pendingId])

  const showNamed = expanded && sessions.length > 0
  const hovered = sessions.find((s) => s.sessionId === hoveredSessionId) ?? null

  return (
    <div
      ref={root}
      className="dot-row"
      data-testid="dot-row"
      onMouseEnter={() => setExpanded(true)}
      onMouseLeave={() => {
        setExpanded(false)
        setPendingId(null)
      }}
    >
      {showNamed ? (
        <NamedDotRow
          sessions={sessions}
          hoveredSessionId={hoveredSessionId}
          onHoverSession={setPendingId}
        />
      ) : (
        <CollapsedPill sessions={sessions} />
      )}
      {showNamed && hovered !== null && <SessionPopover session={hovered} />}
    </div>
  )
}
```

- [ ] **Step 10: Add the popover test to DotRow's suite**

Append to `src/views/dotRow/DotRow.test.tsx`:

```tsx
import { waitFor } from '@testing-library/react'
import { HOVER_GRACE_MS } from './DotRow'

describe('DotRow popover', () => {
  it('opens the popover after the grace delay', async () => {
    render(<DotRow sessions={sessions} />)

    await userEvent.hover(screen.getByTestId('dot-row'))
    await userEvent.hover(screen.getByTestId('session-id-a'))

    expect(screen.queryByTestId('popover')).not.toBeInTheDocument()
    await waitFor(() => expect(screen.getByTestId('popover')).toBeInTheDocument(), {
      timeout: HOVER_GRACE_MS + 500,
    })
  })

  it('closes the popover when the cursor leaves the widget', async () => {
    render(<DotRow sessions={sessions} />)
    const root = screen.getByTestId('dot-row')

    await userEvent.hover(root)
    await userEvent.hover(screen.getByTestId('session-id-a'))
    await waitFor(() => expect(screen.getByTestId('popover')).toBeInTheDocument(), {
      timeout: HOVER_GRACE_MS + 500,
    })

    await userEvent.unhover(root)

    expect(screen.queryByTestId('popover')).not.toBeInTheDocument()
  })
})
```

The existing `DotRow.test.tsx` needs the Tauri mocks the popover depends on. Add
at the top of the file, above the other imports:

```tsx
import { vi } from 'vitest'

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn().mockResolvedValue({ branch: null, model: null, effort: null }),
}))
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({ setSize: vi.fn() }),
  LogicalSize: class {
    constructor(
      public width: number,
      public height: number,
    ) {}
  },
}))
```

- [ ] **Step 11: Style the popover**

Append to `src/views/dotRow/dotRow.css`:

```css
.popover {
  margin-top: 7px;
  width: 266px;
  padding-bottom: 2px;
  background: rgba(20, 24, 32, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.12);
  border-radius: 11px;
  box-shadow: 0 14px 40px rgba(0, 0, 0, 0.55);
  backdrop-filter: blur(18px);
  overflow: hidden;
  cursor: pointer;
  -webkit-app-region: no-drag;
}

.popover-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 11px 8px;
}

.popover-title {
  font-size: 11.5px;
  font-weight: 600;
}

.popover-fields {
  display: grid;
  grid-template-columns: 58px 1fr;
  gap: 4px 8px;
  margin: 0;
  padding: 2px 11px 9px;
}

.popover-fields dt {
  font: 10px ui-monospace, SFMono-Regular, Menlo, monospace;
  color: #69738a;
  text-transform: uppercase;
  letter-spacing: 0.4px;
}

.popover-fields dd {
  margin: 0;
  font: 10.5px ui-monospace, SFMono-Regular, Menlo, monospace;
  color: #b6bfd0;
  overflow-wrap: anywhere;
}

.popover-fields dd.hot {
  color: var(--waiting);
  font-weight: 600;
}

.popover-foot {
  padding: 6px 11px;
  border-top: 1px solid rgba(255, 255, 255, 0.07);
  font: 10px ui-monospace, Menlo, monospace;
  color: #69738a;
}

.popover-foot-error {
  color: var(--dead);
}
```

- [ ] **Step 12: Anchor the popover under the hovered name and keep it on screen**

Two separate problems. The popover must sit beneath the name it describes rather
than beneath the pill's left edge, and the grown window must not run off the
display — near the right edge of the screen it otherwise clips.

Anchoring, in `src/views/dotRow/NamedDotRow.tsx`, add to the `Props` interface:

```tsx
  onHoverOffset?: (offsetPx: number) => void
```

and in the `onMouseEnter` handler, report where the entry sits:

```tsx
            onMouseEnter={(e) => {
              onHoverSession(session.sessionId)
              const entry = e.currentTarget.getBoundingClientRect()
              const row = e.currentTarget.closest('.pill')?.getBoundingClientRect()
              onHoverOffset?.(row ? entry.left - row.left : 0)
            }}
```

In `src/views/dotRow/DotRow.tsx`, track the offset and pass it to the popover:

```tsx
  const [anchorOffset, setAnchorOffset] = useState(0)
```

Pass `onHoverOffset={setAnchorOffset}` to `NamedDotRow`, and
`anchorOffset={anchorOffset}` to `SessionPopover`.

In `src/views/dotRow/SessionPopover.tsx`, accept and apply it:

```tsx
export function SessionPopover({
  session,
  anchorOffset = 0,
}: {
  session: SessionSnapshot
  anchorOffset?: number
}) {
```

and on the root element:

```tsx
    <div
      className="popover"
      data-testid="popover"
      style={{ marginLeft: Math.max(0, anchorOffset) }}
      onClick={raise}
    >
```

Screen clamping, added to `src-tauri/src/window.rs`:

```rust
/// Nudge the widget back onto its display.
///
/// The panel resizes to its own content, so opening the popover can push its
/// right or bottom edge past the screen. Clamping after a resize is what the
/// design calls edge-flipping: the widget stays wholly visible wherever it is
/// parked.
#[tauri::command]
pub fn clamp_to_screen(window: tauri::WebviewWindow) -> Result<(), String> {
    let Ok(Some(monitor)) = window.current_monitor() else {
        return Ok(());
    };
    let scale = monitor.scale_factor();
    let screen = monitor.size().to_logical::<f64>(scale);
    let size = window.outer_size().map_err(|e| e.to_string())?.to_logical::<f64>(scale);
    let pos = window.outer_position().map_err(|e| e.to_string())?.to_logical::<f64>(scale);

    let (x, y) = resolve_position(
        Some([pos.x, pos.y]),
        (screen.width, screen.height),
        (size.width, size.height),
        WIDGET_MARGIN,
    );

    if (x - pos.x).abs() > 0.5 || (y - pos.y).abs() > 0.5 {
        window
            .set_position(LogicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
```

Register it in `src-tauri/src/lib.rs`:

```rust
            window::clamp_to_screen,
```

added to the `generate_handler!` list. Then call it after every resize, in
`src/useAutoResize.ts` — extend `apply`:

```ts
      void getCurrentWindow()
        .setSize(new LogicalSize(width, height))
        .then(() => invoke('clamp_to_screen'))
```

with `import { invoke } from '@tauri-apps/api/core'` at the top. Extend the
`@tauri-apps/api/core` mock in `src/useAutoResize.test.ts`:

```ts
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn().mockResolvedValue(undefined) }))
```

and make the window mock's `setSize` resolve: `setSize: vi.fn().mockResolvedValue(undefined)`.

Add the anchoring test to `src/views/dotRow/SessionPopover.test.tsx`:

```tsx
  it('offsets itself to sit under the hovered name', () => {
    render(<SessionPopover session={session} anchorOffset={140} />)
    expect(screen.getByTestId('popover')).toHaveStyle({ marginLeft: '140px' })
  })

  it('never offsets negatively', () => {
    render(<SessionPopover session={session} anchorOffset={-40} />)
    expect(screen.getByTestId('popover')).toHaveStyle({ marginLeft: '0px' })
  })
```

- [ ] **Step 13: Run the frontend suite**

```bash
npm test && cd src-tauri && cargo test
```

Expected: PASS — all suites.

- [ ] **Step 14: Verify the whole interaction against real sessions**

```bash
npm run tauri dev
```

Check in order:

1. The pill shows counts matching your live `cli` and `claude-desktop` sessions.
2. Hovering morphs it into named dots; the window grows to fit rather than clipping.
3. Hovering one name opens the popover after a beat, showing cwd, branch and model.
4. Clicking the popover brings that session's editor to the front.
5. Moving the cursor off the widget collapses everything and the window shrinks back.

- [ ] **Step 15: Commit**

```bash
git add src/useAutoResize.ts src/useAutoResize.test.ts src/views/dotRow src-tauri/src/window.rs src-tauri/src/lib.rs
git commit -m "feat(ui): per-session popover with click-to-raise"
```

---

### Task 17: Alert delivery

**Files:**
- Create: `src-tauri/src/notify.rs`
- Modify: `src-tauri/src/lib.rs`
- Test: inline `#[cfg(test)]` module in `notify.rs`

**Interfaces:**
- Consumes: `Alert`, `AlertKind` (Task 5), `Config` (Task 6), `now_ms` (Task 7).
- Produces:
  - `pub fn should_deliver(alert: &Alert, config: &Config, now_ms: i64) -> bool`
  - `pub fn alert_text(alert: &Alert) -> (String, String)`
  - `pub fn deliver(app: &tauri::AppHandle, alerts: &[Alert])`

- [ ] **Step 1: Write the failing tests**

`src-tauri/src/notify.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::watcher::alerts::{Alert, AlertKind};

    fn alert(kind: AlertKind) -> Alert {
        Alert {
            session_id: "id-a".into(),
            name: "api-service-55".into(),
            kind,
            detail: match kind {
                AlertKind::NeedsInput => Some("input needed".into()),
                AlertKind::Died => None,
            },
        }
    }

    #[test]
    fn defaults_deliver_both_kinds() {
        let config = Config::default();
        assert!(should_deliver(&alert(AlertKind::NeedsInput), &config, 0));
        assert!(should_deliver(&alert(AlertKind::Died), &config, 0));
    }

    #[test]
    fn disabling_needs_input_suppresses_only_that_kind() {
        let mut config = Config::default();
        config.alert_needs_input = false;

        assert!(!should_deliver(&alert(AlertKind::NeedsInput), &config, 0));
        assert!(should_deliver(&alert(AlertKind::Died), &config, 0));
    }

    #[test]
    fn disabling_died_suppresses_only_that_kind() {
        let mut config = Config::default();
        config.alert_died = false;

        assert!(should_deliver(&alert(AlertKind::NeedsInput), &config, 0));
        assert!(!should_deliver(&alert(AlertKind::Died), &config, 0));
    }

    #[test]
    fn an_active_mute_suppresses_everything() {
        let mut config = Config::default();
        config.mute_until_ms = 10_000;

        assert!(!should_deliver(&alert(AlertKind::NeedsInput), &config, 9_999));
        assert!(!should_deliver(&alert(AlertKind::Died), &config, 9_999));
    }

    #[test]
    fn an_expired_mute_delivers_again() {
        let mut config = Config::default();
        config.mute_until_ms = 10_000;
        assert!(should_deliver(&alert(AlertKind::NeedsInput), &config, 10_000));
    }

    #[test]
    fn needs_input_text_names_the_session_and_its_reason() {
        let (title, body) = alert_text(&alert(AlertKind::NeedsInput));
        assert_eq!(title, "api-service-55 needs you");
        assert_eq!(body, "input needed");
    }

    #[test]
    fn needs_input_text_survives_a_missing_reason() {
        let mut a = alert(AlertKind::NeedsInput);
        a.detail = None;
        let (_, body) = alert_text(&a);
        assert_eq!(body, "waiting for input");
    }

    #[test]
    fn died_text_says_so_plainly() {
        let (title, body) = alert_text(&alert(AlertKind::Died));
        assert_eq!(title, "api-service-55 died");
        assert_eq!(body, "the session's process is gone");
    }
}
```

- [ ] **Step 2: Run to verify failure**

```bash
cd src-tauri && cargo test notify
```

Expected: FAIL — `cannot find function should_deliver in this scope`.

- [ ] **Step 3: Implement delivery**

Prepend to `src-tauri/src/notify.rs`:

```rust
use tauri::Emitter;
use tauri_plugin_notification::NotificationExt;

use crate::config::{self, Config};
use crate::watcher::alerts::{Alert, AlertKind};
use crate::watcher::watch::now_ms;

/// Emitted when a notification could not be shown, so the widget can flash
/// instead of the alert being lost.
pub const FLASH_EVENT: &str = "ui://flash";

/// Whether this alert reaches the user, given their settings.
pub fn should_deliver(alert: &Alert, config: &Config, now_ms: i64) -> bool {
    if config.alerts_muted(now_ms) {
        return false;
    }
    match alert.kind {
        AlertKind::NeedsInput => config.alert_needs_input,
        AlertKind::Died => config.alert_died,
    }
}

pub fn alert_text(alert: &Alert) -> (String, String) {
    match alert.kind {
        AlertKind::NeedsInput => (
            format!("{} needs you", alert.name),
            alert
                .detail
                .clone()
                .unwrap_or_else(|| "waiting for input".to_string()),
        ),
        AlertKind::Died => (
            format!("{} died", alert.name),
            "the session's process is gone".to_string(),
        ),
    }
}

/// Deliver alerts as native notifications.
///
/// Settings are re-read per batch rather than cached, so toggling an alert or
/// muting takes effect immediately without restarting the watcher.
pub fn deliver(app: &tauri::AppHandle, alerts: &[Alert]) {
    if alerts.is_empty() {
        return;
    }

    let config = config::load(&config::config_path());
    let now = now_ms();

    for alert in alerts {
        if !should_deliver(alert, &config, now) {
            continue;
        }
        let (title, body) = alert_text(alert);
        let mut builder = app.notification().builder().title(title).body(body);
        if config.sound {
            builder = builder.sound("default");
        }

        // A failed notification must not stop the remaining alerts. The usual
        // cause is denied permission, so fall back to flashing the widget —
        // otherwise a user who declined the prompt gets no signal at all.
        if builder.show().is_err() {
            let _ = app.emit(FLASH_EVENT, alert);
        }
    }
}
```

- [ ] **Step 4: Run to verify it passes**

```bash
cd src-tauri && cargo test notify
```

Expected: PASS — 8 tests.

- [ ] **Step 5: Flash the widget when a notification cannot be shown**

Add to `src/views/dotRow/dotRow.css`:

```css
@keyframes flash-attention {
  0%,
  100% {
    box-shadow: 0 8px 26px rgba(0, 0, 0, 0.45);
  }
  50% {
    box-shadow: 0 0 0 3px rgba(245, 165, 36, 0.75), 0 8px 26px rgba(0, 0, 0, 0.45);
  }
}

.dot-row[data-flashing='true'] .pill {
  animation: flash-attention 900ms ease-in-out infinite;
}
```

In `src/views/dotRow/DotRow.tsx`, add the subscription and the attribute. Imports
first:

```tsx
import { listen } from '@tauri-apps/api/event'
```

Then inside the component, above the existing `useEffect`:

```tsx
  const [flashing, setFlashing] = useState(false)

  // Notifications may be denied. Flashing is the fallback signal, and it
  // persists until the user looks at the widget — that is the acknowledgement.
  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://flash', () => setFlashing(true)).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [])
```

Add `data-flashing={flashing ? 'true' : 'false'}` to the root `div`, and clear it
in the existing `onMouseEnter`:

```tsx
      onMouseEnter={() => {
        setExpanded(true)
        setFlashing(false)
      }}
```

Add the corresponding test to `src/views/dotRow/DotRow.test.tsx`. The file
already mocks `@tauri-apps/api/core` and `@tauri-apps/api/window`; extend the
mocks with an event mock that captures handlers:

```tsx
const eventHandlers = new Map<string, () => void>()
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(async (name: string, handler: () => void) => {
    eventHandlers.set(name, handler)
    return vi.fn()
  }),
}))
```

and the test:

```tsx
describe('DotRow flash fallback', () => {
  it('flashes on ui://flash and stops once the user hovers', async () => {
    render(<DotRow sessions={sessions} />)
    await waitFor(() => expect(eventHandlers.has('ui://flash')).toBe(true))

    act(() => eventHandlers.get('ui://flash')!())
    expect(screen.getByTestId('dot-row')).toHaveAttribute('data-flashing', 'true')

    await userEvent.hover(screen.getByTestId('dot-row'))
    expect(screen.getByTestId('dot-row')).toHaveAttribute('data-flashing', 'false')
  })
})
```

Import `act` from `@testing-library/react` at the top of the test file.

- [ ] **Step 6: Run the frontend suite**

```bash
npm test -- DotRow
```

Expected: PASS — all DotRow tests including the flash fallback.

- [ ] **Step 7: Wire delivery into the watcher callback**

In `src-tauri/src/lib.rs`, add `pub mod notify;` beside the other modules and
replace the watcher closure body with:

```rust
                move |update| {
                    crate::notify::deliver(&handle, &update.alerts);
                    let _ = handle.emit(UPDATE_EVENT, &update);
                },
```

- [ ] **Step 8: Verify cold-start suppression for real**

Start a session and leave it waiting for input, then launch the widget:

```bash
npm run tauri dev
```

Expected: the pill shows `1 needs you`, and **no** notification appears — the
first snapshot is a baseline. Now answer that session and let it block again.
Expected: a notification arrives.

- [ ] **Step 9: Verify muting**

Right-click the tray icon, choose Mute alerts 1h, then let a session block.
Expected: the pill updates, no notification. Confirm the file:

```bash
grep muteUntilMs ~/Library/Application\ Support/com.claude.buddy/config.json
```

Expected: a timestamp roughly one hour ahead.

- [ ] **Step 10: Commit**

```bash
git add src-tauri/src/notify.rs src-tauri/src/lib.rs
git commit -m "feat: deliver alerts as native notifications with mute support"
```

---

### Task 18: Settings

**Files:**
- Create: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/Cargo.toml`
- Create: `src/settings/SettingsPanel.tsx`
- Create: `src/settings/SettingsPanel.test.tsx`
- Modify: `src/App.tsx`
- Modify: `src/types.ts`

**Interfaces:**
- Consumes: `Config` (Task 6), `ui://open-settings` event (Task 8).
- Produces:
  - `#[tauri::command] pub fn get_config() -> Config`
  - `#[tauri::command] pub fn set_config(config: Config) -> Result<(), String>`
  - `src/types.ts`: `AppConfig`, `VIEW_MODES`
  - `SettingsPanel({ onClose }: { onClose: () => void })`

- [ ] **Step 1: Add the autostart plugin**

In `src-tauri/Cargo.toml` the `tauri-plugin-autostart` dependency is already
present from Task 1. Register it in `src-tauri/src/lib.rs`:

```rust
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
```

- [ ] **Step 2: Write the failing command tests**

`src-tauri/src/commands.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_config_through_the_persist_helper() {
        let path = std::env::temp_dir().join(format!("cb-cmd-{}.json", std::process::id()));
        let mut config = Config::default();
        config.sound = true;
        config.paused_threshold_ms = 5 * 60 * 1000;

        persist(&path, &config).unwrap();

        assert_eq!(crate::config::load(&path), config);
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn rejects_a_nonsensical_paused_threshold() {
        let mut config = Config::default();
        config.paused_threshold_ms = 0;
        assert!(validate(&config).is_err());

        config.paused_threshold_ms = -1;
        assert!(validate(&config).is_err());
    }

    #[test]
    fn rejects_an_unknown_view_mode() {
        let mut config = Config::default();
        config.view_mode = "hologram".into();
        assert!(validate(&config).is_err());
    }

    #[test]
    fn accepts_every_shipped_view_mode() {
        for mode in ["dotRow", "cardStack", "characterBuddy", "invisible"] {
            let mut config = Config::default();
            config.view_mode = mode.into();
            assert!(validate(&config).is_ok(), "{mode} should be valid");
        }
    }
}
```

- [ ] **Step 3: Run to verify failure**

```bash
cd src-tauri && cargo test commands
```

Expected: FAIL — `cannot find function validate in this scope`.

- [ ] **Step 4: Implement the commands**

Prepend to `src-tauri/src/commands.rs`:

```rust
use std::path::Path;

use crate::config::{self, Config};

pub const VIEW_MODES: [&str; 4] = ["dotRow", "cardStack", "characterBuddy", "invisible"];

/// Reject settings that would break the widget rather than writing them.
/// A zero paused threshold would mark every session paused instantly.
pub fn validate(config: &Config) -> Result<(), String> {
    if config.paused_threshold_ms <= 0 {
        return Err("paused threshold must be greater than zero".into());
    }
    if !VIEW_MODES.contains(&config.view_mode.as_str()) {
        return Err(format!("unknown view mode: {}", config.view_mode));
    }
    Ok(())
}

pub fn persist(path: &Path, config: &Config) -> Result<(), String> {
    validate(config)?;
    config::save(path, config).map_err(|e| format!("could not write config: {e}"))
}

#[tauri::command]
pub fn get_config() -> Config {
    config::load(&config::config_path())
}

#[tauri::command]
pub fn set_config(config: Config) -> Result<(), String> {
    persist(&config::config_path(), &config)
}
```

Register both in `src-tauri/src/lib.rs`, adding `pub mod commands;` and extending
the handler:

```rust
        .invoke_handler(tauri::generate_handler![
            bridge::transcript::session_detail,
            bridge::raise::raise_session,
            commands::get_config,
            commands::set_config
        ])
```

- [ ] **Step 5: Run to verify it passes**

```bash
cd src-tauri && cargo test commands
```

Expected: PASS — 4 tests.

- [ ] **Step 6: Mirror the config type in TypeScript**

Append to `src/types.ts`:

```ts
// Mirrors src-tauri/src/config.rs.
export interface AppConfig {
  viewMode: string
  pausedThresholdMs: number
  alertNeedsInput: boolean
  alertDied: boolean
  sound: boolean
  muteUntilMs: number
  launchAtLogin: boolean
  positions: Record<string, [number, number]>
}

/** `shipped: false` modes are listed but not selectable until their own plan lands. */
export const VIEW_MODES = [
  { id: 'dotRow', label: 'Dot row', shipped: true },
  { id: 'cardStack', label: 'Card stack', shipped: false },
  { id: 'characterBuddy', label: 'Character buddy', shipped: false },
  { id: 'invisible', label: 'Invisible until needed', shipped: false },
] as const
```

- [ ] **Step 7: Write the failing settings test**

`src/settings/SettingsPanel.test.tsx`:

```tsx
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { AppConfig } from '../types'

const invoke = vi.fn()
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...args: unknown[]) => invoke(...args) }))

const { SettingsPanel } = await import('./SettingsPanel')

const config: AppConfig = {
  viewMode: 'dotRow',
  pausedThresholdMs: 600_000,
  alertNeedsInput: true,
  alertDied: true,
  sound: false,
  muteUntilMs: 0,
  launchAtLogin: false,
  positions: {},
}

describe('SettingsPanel', () => {
  beforeEach(() => {
    invoke.mockReset()
    invoke.mockImplementation((cmd: string) =>
      cmd === 'get_config' ? Promise.resolve({ ...config }) : Promise.resolve(),
    )
  })

  it('loads current settings into the form', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    await waitFor(() => expect(screen.getByLabelText('Alert when a session needs input')).toBeChecked())
    expect(screen.getByLabelText('Paused after (minutes)')).toHaveValue(10)
  })

  it('saves a toggled alert setting', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    await waitFor(() => expect(screen.getByLabelText('Play a sound')).toBeInTheDocument())

    await userEvent.click(screen.getByLabelText('Play a sound'))

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ sound: true }),
      }),
    )
  })

  it('converts the paused threshold from minutes to milliseconds', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)
    const field = await screen.findByLabelText('Paused after (minutes)')

    await userEvent.clear(field)
    await userEvent.type(field, '25')

    await waitFor(() =>
      expect(invoke).toHaveBeenCalledWith('set_config', {
        config: expect.objectContaining({ pausedThresholdMs: 1_500_000 }),
      }),
    )
  })

  it('offers every view mode', async () => {
    render(<SettingsPanel onClose={vi.fn()} />)

    await waitFor(() => expect(screen.getByLabelText('View mode')).toBeInTheDocument())
    expect(screen.getByRole('option', { name: 'Dot row' })).toBeInTheDocument()
    expect(screen.getByRole('option', { name: 'Character buddy' })).toBeInTheDocument()
  })

  it('surfaces a rejected save instead of silently dropping it', async () => {
    invoke.mockImplementation((cmd: string) =>
      cmd === 'get_config'
        ? Promise.resolve({ ...config })
        : Promise.reject(new Error('paused threshold must be greater than zero')),
    )
    render(<SettingsPanel onClose={vi.fn()} />)
    const field = await screen.findByLabelText('Paused after (minutes)')

    await userEvent.clear(field)
    await userEvent.type(field, '0')

    await waitFor(() =>
      expect(screen.getByTestId('settings-error')).toHaveTextContent('greater than zero'),
    )
  })

  it('closes on request', async () => {
    const onClose = vi.fn()
    render(<SettingsPanel onClose={onClose} />)

    await userEvent.click(await screen.findByRole('button', { name: 'Done' }))

    expect(onClose).toHaveBeenCalled()
  })
})
```

- [ ] **Step 8: Run to verify failure**

```bash
npm test -- SettingsPanel
```

Expected: FAIL — cannot resolve `./SettingsPanel`.

- [ ] **Step 9: Implement the settings panel**

`src/settings/SettingsPanel.tsx`:

```tsx
import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { VIEW_MODES, type AppConfig } from '../types'

export function SettingsPanel({ onClose }: { onClose: () => void }) {
  const [config, setConfig] = useState<AppConfig | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    invoke<AppConfig>('get_config')
      .then(setConfig)
      .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
  }, [])

  // Save on every change: there is no Apply button, so a rejected value must be
  // reported rather than left looking accepted.
  const update = (patch: Partial<AppConfig>) => {
    if (config === null) return
    const next = { ...config, ...patch }
    setConfig(next)
    setError(null)
    invoke('set_config', { config: next }).catch((e: unknown) =>
      setError(e instanceof Error ? e.message : String(e)),
    )
  }

  if (config === null) {
    return <div className="settings">loading…</div>
  }

  return (
    <div className="settings" data-testid="settings">
      <label htmlFor="view-mode">View mode</label>
      <select
        id="view-mode"
        value={config.viewMode}
        onChange={(e) => update({ viewMode: e.target.value })}
      >
        {VIEW_MODES.map((mode) => (
          <option key={mode.id} value={mode.id} disabled={!mode.shipped}>
            {mode.label}
          </option>
        ))}
      </select>

      <label htmlFor="paused-after">Paused after (minutes)</label>
      <input
        id="paused-after"
        type="number"
        min={1}
        value={Math.round(config.pausedThresholdMs / 60_000)}
        onChange={(e) => update({ pausedThresholdMs: Number(e.target.value) * 60_000 })}
      />

      <label>
        <input
          type="checkbox"
          checked={config.alertNeedsInput}
          onChange={(e) => update({ alertNeedsInput: e.target.checked })}
        />
        Alert when a session needs input
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.alertDied}
          onChange={(e) => update({ alertDied: e.target.checked })}
        />
        Alert when a session dies
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.sound}
          onChange={(e) => update({ sound: e.target.checked })}
        />
        Play a sound
      </label>

      <label>
        <input
          type="checkbox"
          checked={config.launchAtLogin}
          onChange={(e) => update({ launchAtLogin: e.target.checked })}
        />
        Launch at login
      </label>

      {error !== null && (
        <p className="settings-error" data-testid="settings-error">
          {error}
        </p>
      )}

      <button type="button" onClick={onClose}>
        Done
      </button>
    </div>
  )
}
```

Note: the `Alert when…`, `Play a sound` and `Launch at login` checkboxes are
labelled by wrapping, which is why the tests reach them with `getByLabelText`.

- [ ] **Step 10: Run to verify it passes**

```bash
npm test -- SettingsPanel
```

Expected: PASS — 6 tests.

- [ ] **Step 11: Open settings from the tray menu**

`src/App.tsx`:

```tsx
import { useEffect, useState } from 'react'
import { listen } from '@tauri-apps/api/event'
import { useSessions } from './useSessions'
import { DotRow } from './views/dotRow/DotRow'
import { SettingsPanel } from './settings/SettingsPanel'

export function App() {
  const { sessions } = useSessions()
  const [settingsOpen, setSettingsOpen] = useState(false)

  useEffect(() => {
    let stop: (() => void) | undefined
    listen('ui://open-settings', () => setSettingsOpen(true)).then((unlisten) => {
      stop = unlisten
    })
    return () => stop?.()
  }, [])

  if (settingsOpen) {
    return <SettingsPanel onClose={() => setSettingsOpen(false)} />
  }
  return <DotRow sessions={sessions} />
}
```

- [ ] **Step 12: Apply launch-at-login when it changes**

In `src-tauri/src/commands.rs`, extend `set_config`:

```rust
#[tauri::command]
pub fn set_config(app: tauri::AppHandle, config: Config) -> Result<(), String> {
    use tauri_plugin_autostart::ManagerExt;

    persist(&config::config_path(), &config)?;

    let manager = app.autolaunch();
    let result = if config.launch_at_login {
        manager.enable()
    } else {
        manager.disable()
    };
    result.map_err(|e| format!("could not update launch at login: {e}"))
}
```

- [ ] **Step 13: Run the whole suite**

```bash
npm test && cd src-tauri && cargo test
```

Expected: PASS on both.

- [ ] **Step 14: Verify settings end to end**

```bash
npm run tauri dev
```

Open settings from the tray menu, set the paused threshold to 1 minute, close.
Expected: an idle session flips to the paused colour within two ticks. Set it to
0. Expected: an error appears in the panel and the file keeps the old value.

- [ ] **Step 15: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs src/settings src/App.tsx src/types.ts
git commit -m "feat: settings panel backed by the config file"
```
