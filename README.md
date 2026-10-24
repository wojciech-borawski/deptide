# Deptide

<sub>Vibe-coded with Claude</sub>

[![CI](https://github.com/wojciech-borawski/deptide/actions/workflows/ci.yml/badge.svg)](https://github.com/wojciech-borawski/deptide/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/wojciech-borawski/deptide)](https://github.com/wojciech-borawski/deptide/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Desktop app and command line tool for teams that develop shared npm libraries
and the many projects that consume them. Pick the projects, pick the library
versions, and Deptide updates every project in parallel while you watch each
step live. Built with Tauri 2, Rust and Vue 3; ships as a single portable
executable.

It also helps when the remote desktop you are supposed to develop on is too
slow to work with. Keep the projects on your own machine, where builds and
editors are fast, and use Deptide to move code between the two sides: the
Transfer screen copies whole projects through the clipboard without
`node_modules` or git-ignored files, and receives them back with a
file-by-file review. Combined with symlinked project folders on the fast side,
the day-to-day work happens locally and the remote machine only sees the
result.

## What it does

- **Updates a library everywhere at once.** Runs `npm uninstall`,
  `npm install <exact version>`, `npm install --force`, `npm audit fix` and
  `npm run build` per project, several projects in parallel, in the order
  their dependencies require. The version can come from the library's git
  branch (`3.1.0` on branch `ABC-123-widgets` becomes `3.1.0-ABC-123`), from
  its local `package.json`, from a typed suffix, or by hand.
- **Tries a library before it is published.** A library that is one of your
  projects can be packed from its folder with `npm pack` and installed into
  the applications as a tarball, by default with `--no-save` so
  `package.json` and the lockfile stay untouched.
- **Keeps you safe.** `package.json` and the lockfile are backed up before a
  run and can be restored per project, or for the whole run at once from the
  Run screen or later from History, with a handoff to `npm install`. A single
  project can be stopped while the rest continue, network hiccups are
  retried, and every run leaves a transcript, a summary and its total time.
- **Remembers your project sets.** Project groups such as "frontend apps" are
  saved in the workspace and offered as chips in the wizard, the Terminal and
  Transfer, and as `--group` on the command line.
- **Shows what will happen first.** The Review step lists the exact npm
  command lines per project and the version each one will get
  (`1.4.2 -> 1.5.0 minor`) or why its bump is skipped, before anything runs.
  `deptide-cli run --preview` prints the same.
- **Counts vulnerabilities.** An audit report step runs `npm audit --json`
  without changing any file and shows the findings per severity in the run
  table, the summary and the reports.
- **Shows what really happened.** The version and hash that landed in
  `node_modules`, the transitive packages that changed, a diagnosis with a
  suggested fix for every failure, per-step timings, "slower than usual"
  flags against your history, trend charts, and Markdown or HTML reports.
- **Shows how the workspace hangs together.** The Dependencies screen lists
  every shared library with the range each project declares and the version
  its `node_modules` really holds, flags versions outside the range, versions
  that differ between projects and, after "Check latest", versions behind the
  registry. It draws which projects use which libraries next to the build
  order a run uses, and finds libraries installed more than once in nested
  `node_modules` with a handoff to `npm dedupe`.
- **Runs any command everywhere.** The Terminal screen takes one command line
  (`git status`, `npm test`, `npx eslint .`) and runs it in the chosen projects
  in parallel, with a progress table and the live output of the project you
  click.
- **Moves code between machines.** The Transfer screen copies whole projects
  to the clipboard without `node_modules` or git-ignored files and receives
  them on the other side with a file-by-file review.
- **Fits into the day.** Tray icon and finish notifications, dark and light
  themes, English, Polish and German, keyboard navigation, an update check,
  and an error log for anything that goes wrong.

## Install

Download the latest build from the
[releases page](https://github.com/wojciech-borawski/deptide/releases/latest):

| file                          | what it is                                             |
| ----------------------------- | ------------------------------------------------------ |
| `Deptide-windows-x64.exe`     | the desktop app, a single file, no installation needed |
| `deptide-cli-windows-x64.exe` | the command line tool                                  |
| `Deptide-linux-x64.AppImage`  | the desktop app for Linux                              |
| `deptide-linux-x64.deb`       | the same as a Debian package                           |
| `deptide-cli-linux-x64`       | the command line tool for Linux                        |

Windows 10 or 11 needs nothing else. Linux needs `libwebkit2gtk-4.1` and, for
the tray icon, `libayatana-appindicator3`.

## Getting started

1. Start Deptide and choose a workspace folder. The workspace holds
   `update-libs.json` (the projects and packages), `settings.json`, and the
   `runs/`, `logs/`, `backups/` and `transfers/` folders Deptide writes.
2. Set the projects root in Settings and use "Scan for projects" in the wizard
   to fill the configuration.
3. Walk through the wizard: projects, libraries and versions, options, review.
   Start the run and follow it on the Run screen.

Passing a workspace folder as the first argument (`Deptide.exe C:\example\workspace`)
opens it directly.

## Command line

`deptide-cli` uses the same engine and the same workspace folders, and its
runs show up in the desktop History screen.

```bash
deptide-cli scan C:\example\workspace --apply
deptide-cli run C:\example\workspace -p @acme/core@3.1.0-ABC-123 -P web -P api --steps install,build
deptide-cli run C:\example\workspace -p @acme/core@3.1.0-ABC-123 --steps install,version,build --bump minor --bump-when not-bumped-on-branch
deptide-cli run C:\example\workspace --steps version,audit --bump patch --project-bump api=major --bump-when not-bumped-on-branch
deptide-cli run C:\example\workspace --steps audit-report --preview
deptide-cli run C:\example\workspace --pack @acme/core -P web --steps install,build --preview
deptide-cli exec C:\example\workspace --concurrency 4 -- git status
deptide-cli deps C:\example\workspace --latest --duplicates
deptide-cli groups C:\example\workspace
deptide-cli run C:\example\workspace -p @acme/core@3.1.0-ABC-123 --group "frontend apps" -P api --steps install,build
deptide-cli history C:\example\workspace
deptide-cli restore C:\example\workspace 2026-10-22T09-30-00-acme-core-3.1.0 --project web
```

Exit codes: 0 when every project is ok, 1 when any failed, 2 when the run was
stopped, 3 for a usage or configuration error.

## Building from source

Requirements: Node.js 20 or newer, a stable Rust toolchain, and on Windows the
Visual Studio Build Tools with the C++ workload. Linux needs the packages
listed in `documentation/development.md`.

```bash
npm install --legacy-peer-deps
npm run dev               # desktop app with hot reload
npm run web:mock          # the UI alone in a browser, backed by an in-memory fake
npm run check             # type check, lint, format, tests, Clippy
npm run build:portable    # release/Deptide.exe and release/deptide-cli.exe
```

## Releasing

Releases are built by GitHub Actions. To publish a version:

```bash
node scripts/set-version.mjs 1.1.0
git commit -am "Release 1.1.0"
git tag v1.1.0
git push && git push --tags
```

The `Release` workflow builds the Windows and Linux binaries, checks that the
tag matches the version in `package.json`, and publishes a GitHub release with
the assets listed above and generated release notes.

## Layout

| folder                    | content                                                                       |
| ------------------------- | ----------------------------------------------------------------------------- |
| `apps/desktop-ui/`        | the Vue frontend, an npm workspace package with its own tests                 |
| `crates/deptide-core/`    | the engine: workspaces, scanning, runs, transfers, and its tests              |
| `crates/deptide-desktop/` | the Tauri application and its configuration                                   |
| `crates/deptide-cli/`     | the command line front end                                                    |
| `documentation/`          | architecture, workspace format, wizard, transfer, terminal, dependencies, dev |
| `scripts/`                | build and version helpers                                                     |

## Documentation

- [Architecture](documentation/architecture.md)
- [Workspace format](documentation/workspace-format.md)
- [Wizard and runs](documentation/wizard-and-runs.md)
- [Terminal](documentation/terminal.md)
- [Dependencies](documentation/dependencies.md)
- [Transfer](documentation/transfer.md)
- [Development](documentation/development.md)
- [Manual checks before a release](documentation/walkthrough.md)

## License

[MIT](LICENSE)
