# Development

## Prerequisites

- Node.js 20 or newer.
- Rust stable with the MSVC toolchain on Windows (`rustup` installs it), plus
  the Visual Studio Build Tools with the "Desktop development with C++"
  workload and a Windows 10 SDK.
- WebView2 runtime (ships with Windows 11 and recent Windows 10).
- On Linux: the usual Tauri 2 packages, on Debian/Ubuntu
  `libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev
libssl-dev libayatana-appindicator3-dev librsvg2-dev`. The tray icon needs
  the appindicator library at runtime as well.

## Commands

| command                  | what it does                                                                    |
| ------------------------ | ------------------------------------------------------------------------------- |
| `npm install`            | installs the frontend and the Tauri CLI                                         |
| `npm run dev`            | starts Vite and the Tauri window with hot reload                                |
| `npm run build`          | type-checks, bundles the frontend and builds the installer                      |
| `npm run web:mock`       | the UI alone in a browser at http://localhost:1420, backed by an in-memory fake |
| `npm run typecheck`      | `vue-tsc` over the frontend and the tests                                       |
| `npm test`               | Vitest suite in `apps/desktop-ui/tests/`                                        |
| `npm run lint`           | ESLint over the frontend package (`lint:fix` applies fixes)                     |
| `npm run format`         | Prettier over the frontend, scripts and docs (`format:check` only reports)      |
| `npm run check:rust`     | `cargo fmt --check`, Clippy with warnings as errors, `cargo test`               |
| `npm run check`          | everything above in one go; must be green before a change is done               |
| `cargo test --workspace` | every Rust test: unit tests in the sources and `crates/*/tests/`                |

If `npm install` stops with `Cannot read properties of null (reading
'edgesOut')`, that is an npm resolver bug triggered by a peer range; run
`npm install --legacy-peer-deps` instead.

## Mock mode

`npm run web:mock` starts Vite with `--mode mock`, which sets
`VITE_MOCK_IPC=1` from `.env.mock`. `apps/desktop-ui/src/main.ts` then installs
`apps/desktop-ui/src/dev/mock-backend.ts`, a replacement for `window.__TAURI_INTERNALS__`
that answers every command with in-memory data and simulates a run with
timed log lines. Nothing from that file is part of the production bundle;
the dynamic import is dead code when the variable is unset.

The mock clipboard holds a file list. Add `?clipboard=virtual` before the `#`
of the address (http://localhost:1420/?clipboard=virtual) for two folders from
a remote desktop that download in about two seconds, or
`?clipboard=virtual-fail` for a download that fails with "The clipboard
changed, analyze again" partway through.

On the Dependencies screen, `?deps=cycle` adds two projects that depend on each
other to the mock graph, so the cycle warning can be checked.

## Tests

- `apps/desktop-ui/tests/` holds Vitest tests for the pure TypeScript modules in `src/lib`
  (version arithmetic, plan building, validation, formatting).
- Vitest runs in `tests/helpers/client-environment.ts`: plain Node with no
  DOM, but `.vue` files are compiled the way the app compiles them rather than
  for server rendering. That lets `tests/helpers/test-renderer.ts` mount a
  component into an in-memory node tree with Vue's `createRenderer` and fire
  its handlers (`click`, `dispatch`); `renderToString` from
  `vue/server-renderer` still works for tests that only read the markup.
- `crates/deptide-core/tests/` holds Cargo integration tests. They create temporary
  folders with fake `package.json` files and exercise the real modules:
  path normalisation, scanning, the scan merge, config parsing, saved runs,
  job building and a complete dry run through the scheduler with a recording
  event sink, in both execution orders and with an abort.

- `crates/deptide-core/tests/npm_real.rs` spawns the real npm against a temporary
  project. The tests that restore `package.json` after a failed or stopped
  install write an `.npmrc` with `offline=true` and an unreachable registry
  into the project, so the uninstall works, every install fails and nothing
  reaches the network; they run with the rest of the suite, as does a test
  that stops a long-running `node` command and expects it killed. The tests that
  install a real version need network access and are marked `#[ignore]`; run
  them on demand with `cargo test -p deptide-core --test npm_real -- --ignored`.

- `crates/deptide-core/tests/semver.rs` takes its expected values from
  node-semver 7.7.4, the copy bundled with npm 10.9.8.

- `crates/deptide-core/tests/latest_lookup.rs` puts a fake `npm` first on
  PATH, in a temporary folder shared by the running tests of that file and
  removed when the last of them ends. It answers
  `npm view <name> version --json` with "1.2.3" after 300 ms, never answers
  for names starting with `hang-`, and logs each start to `<name>.events`
  beside itself.

- `crates/deptide-core/tests/clipboard_real.rs` and `clipboard_virtual.rs` use
  the real system clipboard and are marked `#[ignore]`. `clipboard_virtual.rs`
  puts a fake file source on it, the way a remote desktop client does. Run
  them on Windows with
  `cargo test -p deptide-core --test clipboard_real --test clipboard_virtual -- --ignored --test-threads=1`.

Keep new tests in those folders. The exception is a test that needs a private
seam no public interface reaches, such as the manifest reader of the
duplicate walk or the late process registration in `execution/command.rs`:
it goes in a `#[cfg(test)] mod tests` at the end of that source file, as in
`dependencies/copies.rs`, `execution/manifest.rs` and `transfer/merge.rs`.
Source files carry no comments by design; explanations belong in this folder.
The one exception is doc comments: a file that already documents a kind of
declaration with doc comments may keep them and add more on declarations of
the same kind.

## Adding a Tauri command

1. Add the function to the matching file under `crates/deptide-desktop/src/app/commands/` and register it in the
   `generate_handler!` list in `lib.rs`.
2. Add a wrapper in `apps/desktop-ui/src/api/commands.ts` and, if the payload is new, its type
   in `src/api/types.ts`. Field names are camelCase on both sides: the Rust
   DTOs use `#[serde(rename_all = "camelCase")]`.
3. Teach `apps/desktop-ui/src/dev/mock/handlers.ts` about the command so mock mode keeps working.

## Design references

The look follows GitKraken: a dark slate surface, an icon rail on the left,
one accent gradient (teal to violet) for primary actions and the active step,
chip and badge styling for metadata, monospace only where paths, versions and
npm output appear. All colours are CSS custom properties in
`apps/desktop-ui/src/styles/theme.css`.

## Error log

Panics on the Rust side, failed commands and uncaught errors in the frontend
(window errors, unhandled promise rejections, Vue component errors) are
appended to `deptide-errors.log` in the per-user app log directory
(`%LOCALAPPDATA%\dev.deptide.app\logs` on Windows). The Settings screen has an
"Error log" button that reveals the file. Frontend errors reach the file
through the `log_client_error` command; the panic hook is installed in
`app/error_log.rs` during setup.

Release builds use `panic = "abort"` to keep the binaries small, so a panic
ends the process. Both the CLI and the desktop app call
`deptide_core::execution::install_crash_hook`, which kills the process tree of
every running command before the process aborts. Development and test builds
unwind, the hook does nothing there, and the scheduler fails only the
panicking job.

The hook works on a fixed table of 1024 slots in `execution/process_tree.rs`,
without locks or heap allocation. A command takes a slot before it is spawned
and fails with `not started because 1024 commands are already running` when
none is free. When the hook runs, it closes admission first, waits up to 2
seconds for commands that are being started, and then claims every slot with
an atomic swap. It kills only the Job Objects or process groups it claimed. No
thread ever waits on the crash: an owner that retires its tree while the crash
is under way, or finds its slot claimed, leaves the token in the slot, keeps
its Job Object handle open (`ManuallyDrop`) or leaves its child unreaped
(`mem::forget`), and returns. A `ProjectLock` dropped during the crash keeps
its file, which the next run takes over as stale. An owner that retires before
the crash flag is set clears its slot with a compare-and-swap of its own
token, so a later scan does not see it. Each hook counts itself in a cleanup
counter before it claims anything and leaves the count after its kills; before
it returns to the panic runtime it waits, up to 5 seconds, until the counter is
0, so a second panicking thread cannot abort the process while the first still
holds claimed trees it has not killed.

On Windows the slot is filled after the child joins its kill-on-close Job
Object and before its suspended thread is resumed. When the crash refuses the
start, the Job Object is closed, which kills the suspended child. On Linux and
macOS `std` waits inside `spawn` until the child has called `exec`, so a
`pre_exec` step cannot wait for the parent. Instead, a shared anonymous memory
page holds one process number per slot and a crash flag. In `pre_exec` the
child writes its process number, then reads the flag and fails the start if it
is set. The hook sets the flag and then sends `SIGKILL` to the process group of
every number it finds (`kill(-pid)`, never the single process), before it
claims the slots. Either the child sees the flag or the hook sees the
child. After `spawn`, the slot gets the process group id and the page entry is
cleared. The child's exit is observed with `waitid(WNOWAIT)`, the slot is
cleared, and only then is the child reaped.

Remaining limits:

- A Windows child that was not yet in its Job Object when the 2 seconds ran
  out stays suspended after Deptide closes. It never runs its command.
- On Linux and macOS, when `exec` fails after the child wrote its process
  number, `std` reaps the child inside `spawn` and only then returns the error;
  the page entry is cleared when the admission is dropped after that. `std`
  offers no step between the failed `exec` and the reap, so this order cannot
  be changed without replacing its `spawn`. The window lasts as long as the
  parent thread takes to get from the reap to the drop, which has no upper
  bound if the thread is descheduled. A crash inside it sends `SIGKILL` to the
  process group with that number. That reaches another program only if the
  number was reused and the new process leads a group of the same number.
- The hook waits at most 2 seconds for starts and at most 5 seconds for other
  panicking threads to finish their kills.
- A previous panic hook that blocks still blocks the abort.

The slot protocol and the Windows paths have unit tests in that file. The Unix
paths have Linux-only tests there too. By decision, Linux and macOS ship with
compilation as their only check: the Linux clippy check compiles those tests,
and their runtime behavior is unverified.

## Portable build

```bash
npm run build:portable
```

builds the release executable without installers and copies it to
`release/Deptide.exe`. That single file runs from any folder; it only needs
the WebView2 runtime that ships with Windows 10 and 11. Passing a folder as
the first argument (`Deptide.exe C:\example\workspace`) opens that workspace
directly, which makes a per-workspace shortcut possible. `npm run build`
additionally produces the MSI and NSIS installers under
`target/release/bundle`.

On Linux `npm run build:portable` copies a bare `release/Deptide` binary that
needs the system webkit2gtk, and `npm run build:linux` produces an AppImage and
a `.deb` under `target/release/bundle`. The code paths that differ
between platforms are the process runner (`cmd.exe` versus `sh -c`, a Job
Object per command versus process-group signals) and the clipboard
(`clipboard-win` versus `arboard`); everything else is shared.

## Translations

All UI strings live in `apps/desktop-ui/src/i18n/en.ts`, `pl.ts` and `de.ts`, keyed by
screen. `tests/i18n.test.ts` fails when a language misses a key or uses
different placeholders than English, so adding a string means adding it to
all three files. Nouns that are counted (`common.project` and friends) use
vue-i18n plural forms; Polish has three forms, wired through the plural rule
in `apps/desktop-ui/src/i18n/index.ts`. Backend texts (diagnosis titles and hints) are
translated in the frontend by their code, with the English text from the
backend as fallback.

## Update manifest

The update check fetches a JSON document of the shape

```json
{ "version": "3.1.0", "url": "https://example.com/deptide/Deptide.exe", "notes": "What changed" }
```

and compares `version` with the running version numerically, ignoring
prerelease tags. Host the file anywhere the machine can reach, for example a
raw file in a repository or a release asset, and paste its URL into Settings.

## The command line front end

`npm run build:cli` (or `cargo build --release -p deptide-cli`) produces
`target/release/deptide-cli`. It works on the same workspace folders as the
desktop app and writes the same transcripts, so its runs appear in the History
screen.

```bash
deptide-cli scan C:\example\workspace --apply
deptide-cli run C:\example\workspace -p @acme/core@3.1.0-ABC-123 -P web -P api --steps install,build
deptide-cli run C:\example\workspace -p @acme/core@3.1.0-ABC-123 --steps install,version,build --bump minor --bump-when not-bumped-on-branch
deptide-cli run C:\example\workspace --steps version,audit --bump patch --project-bump api=major --bump-when not-bumped-on-branch
deptide-cli run C:\example\workspace --steps audit-report --preview
deptide-cli exec C:\example\workspace --concurrency 4 -- git status
deptide-cli deps C:\example\workspace --latest --duplicates
deptide-cli groups C:\example\workspace
deptide-cli run C:\example\workspace -p @acme/core@3.1.0-ABC-123 --group "frontend apps" -P api --steps install,build
deptide-cli history C:\example\workspace
deptide-cli restore C:\example\workspace 2026-10-22T09-30-00-acme-core-3.1.0 --project web
```

Exit codes: 0 when every project is ok, 1 when any failed, 2 when the run was
stopped with Ctrl+C, 3 for a usage or configuration error. `--quiet` hides the
npm output and keeps the status lines; `--json` prints the run summary as JSON
at the end. `--preview` prints the npm commands and version bumps each project
would get and exits without running. Step names for `--steps` are
`uninstall`, `install`, `force-install`, `version`, `audit`, `audit-report`
and `build`. `deps` prints the library versions, the build order and, with
`--duplicates`, the duplicate copies; see [Dependencies](dependencies.md). `groups` lists the project groups;
`--group NAME` on `run` adds a group's projects to the `-P` ones, and an
unknown group is a usage error naming the known ones. `history` ends each line
with the run's name, which `restore` takes to put back the files that run
backed up; see [Undo a run](wizard-and-runs.md#undo-a-run).
