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

| command              | what it does                                                                    |
| -------------------- | ------------------------------------------------------------------------------- |
| `npm install`        | installs the frontend and the Tauri CLI                                         |
| `npm run dev`        | starts Vite and the Tauri window with hot reload                                |
| `npm run build`      | type-checks, bundles the frontend and builds the installer                      |
| `npm run web:mock`   | the UI alone in a browser at http://localhost:1420, backed by an in-memory fake |
| `npm run typecheck`  | `vue-tsc` over the frontend and the tests                                       |
| `npm test`           | Vitest suite in `apps/desktop-ui/tests/`                                        |
| `npm run lint`       | ESLint over the frontend package (`lint:fix` applies fixes)                     |
| `npm run format`     | Prettier over the frontend, scripts and docs (`format:check` only reports)      |
| `npm run check:rust` | `cargo fmt --check`, Clippy with warnings as errors, `cargo test`               |
| `npm run check`      | everything above in one go; must be green before a change is done               |
| `npm run test:rust`  | Cargo integration tests in `crates/*/tests/`                                    |

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

## Tests

- `apps/desktop-ui/tests/` holds Vitest tests for the pure TypeScript modules in `src/lib`
  (version arithmetic, plan building, validation, formatting).
- `crates/deptide-core/tests/` holds Cargo integration tests. They create temporary
  folders with fake `package.json` files and exercise the real modules:
  path normalisation, scanning, the scan merge, config parsing, saved runs,
  job building and a complete dry run through the scheduler with a recording
  event sink, in both execution orders and with an abort.

- `crates/deptide-core/tests/npm_real.rs` spawns the real npm against a temporary
  project (uninstall, install, and an abort while npm runs). Those two tests
  need network access and are marked `#[ignore]`; run them on demand with
  `cargo test -p deptide-core --test npm_real -- --ignored`.

Keep new tests in those folders. Source files carry no comments by design;
explanations belong in this folder.

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
between platforms are the process runner (`cmd.exe` versus `sh -c`, `taskkill`
versus process-group signals) and the clipboard (`clipboard-win` versus
`arboard`); everything else is shared.

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
deptide-cli run C:\example\workspace -p @acme/core@3.1.0-ABC-123 --steps install,version,build --bump minor --bump-only-if-same-as-main
deptide-cli exec C:\example\workspace --concurrency 4 -- git status
deptide-cli history C:\example\workspace
```

Exit codes: 0 when every project is ok, 1 when any failed, 2 when the run was
stopped with Ctrl+C, 3 for a usage or configuration error. `--quiet` hides the
npm output and keeps the status lines; `--json` prints the run summary as JSON
at the end.
