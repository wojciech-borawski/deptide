# Architecture

Deptide is a Tauri 2 desktop application. The Rust side does all the work
(reading workspaces, scanning for projects, running npm, keeping run state), the
Vue 3 side only asks questions and shows progress. The two talk through Tauri
commands (request/response) and one event stream (`run-event`).

```
apps/desktop-ui/     the Vue 3 + TypeScript + Pinia + vue-router app, an npm workspace package
    api/               typed wrappers around invoke() and listen(), the DTO types
    stores/            workspace (what is open), wizard (the draft plan), run and
                     terminal (live runs, both built on useRunTracker), transfer,
                     settings-draft (the form on the Settings screen), ui (preferences)
    composables/       reusable logic without a store: useRunTracker (event stream,
                     logs, clock, counts), useAsyncAction (busy + error around a
                     call), useProjectFilter and useProjectSelection (list search,
                     kind chips, toggle and select-all), useCountedNoun (plurals)
    lib/               pure functions: version helpers, plan builder, formatting
                     (luxon), timing baseline, run outcome, path grouping, reports
    views/             one thin component per screen; they compose the pieces below
    components/        wizard steps, run widgets (JobTable, LogPane, RunClock,
                     RunStatusLine, RunSummaryCard), terminal (CommandPrompt,
                     CommandTable, TerminalProjectPicker), settings cards,
                     projects (ProjectPickerList, ProjectKindChips), transfer
                     panels, ui primitives (ActionButton, NoticeBanner, SearchBox,
                     ProgressBar, ProjectPath, ModalDialog, icons), the rail
    dev/mock/          in-memory fake backend for the browser: workspace fixtures,
                     a MockRun that emits the same events, the command handlers

Cargo.toml           Rust workspace with three crates and one shared version
crates/deptide-core/ the engine, no Tauri dependency:
  domain/            serde types shared with the frontend, no logic
  workspace/         the folder the user picked: paths, settings, config, saved runs,
                     recent list, open_with_config
  scan/              package.json reader, project detection, dependency candidates,
                     duplicates, scan merge
  execution/         command runner (npm and shell lines), steps (what each npm
                     step runs and how a dry run describes it), pipeline (one job
                     through its steps with retry, verify and backup), scheduler
                     with dependency gate, run state, transcript logger, install
                     verification, failure diagnostics, report rendering
  transfer/          clipboard transfer: gitignore-aware file collection, staging
                     folders, the transfer journal, file clipboard (clipboard-win
                     on Windows, arboard elsewhere), receive analysis and apply
  util/              json files, text helpers, time formatting
  tests/             integration tests that drive the engine without a window
crates/deptide-desktop/ the Tauri application (binary `deptide`, product Deptide):
  src/app/           commands split by screen (workspace, scan, run, transfer,
                     misc) with one run launcher, event sink with notifications,
                     tray icon, error log, in-memory run registry
  tauri.conf.json, icons/, capabilities/
crates/deptide-cli/  the command line front end (binary `deptide-cli`): clap
                     arguments, a terminal ProgressSink, scan / run / exec / history
```

Layering inside Rust is a compiler rule: `deptide-core` has no Tauri
dependency, and both front ends depend on it by path. `domain` and `util` know
nothing, `workspace` knows the disk, `scan` and `execution` know the domain and
the workspace. The desktop crate adds the Tauri glue, the CLI crate adds
argument parsing and terminal output. Both implement the same `ProgressSink`
trait, which is how a run reports progress, so the tests in
`crates/deptide-core/tests` drive the whole engine with a recording sink.

## How a run flows

1. The wizard builds a `RunPlan` (projects, packages with resolved versions,
   steps, order, concurrency, flags, label, optional saved-run name). This is
   pure TypeScript in `apps/desktop-ui/src/lib/wizard-plan.ts`.
2. `start_run` (Rust) turns the plan into jobs. Every package is applied only to
   the selected projects whose `package.json` declares it. A package that no
   selected project declares is treated as "add everywhere". Whether a package
   goes in with `--save-dev` comes from the consuming project's manifest, not
   from a global flag.
3. A `RunContext` is created: shared state behind a mutex, a `ProcessRegistry`
   that knows every live npm process, an optional transcript logger and a
   `ProgressSink`. The Tauri sink forwards events to the window; tests use a
   recording sink.
4. The scheduler either sends each project through every step
   (`per-project`) or loops over the steps and sends every project through one
   step at a time (`per-step`). Both use a tokio semaphore so at most
   `concurrency` projects run npm at the same time.
5. Each npm invocation streams stdout and stderr line by line into the job's
   log, the transcript file and the event stream. Durations are recorded per
   step and per job.
6. When the last job finishes the run is closed: a `RunSummary` with the
   wall-clock total, the sum of per-project busy time and the per-project
   outcomes is written next to the transcript as JSON, and a `finished` event
   carries it to the UI.

Aborting sets a flag and kills every registered process tree (`taskkill /T` on
Windows, a process-group signal elsewhere). Jobs that were still waiting end
up as `skipped`, the job that was killed too, and the run is marked aborted.

## Why the frontend never touches the file system

Path handling differs between platforms and the CLI already had subtle bugs
around case-insensitive comparison and relative paths. Everything that needs a
path goes through `workspace::paths`, and the UI only ever sees strings that
came from there. The frontend sends back exactly what it received (for example
the scanned projects when applying a scan), so no path arithmetic is duplicated
in TypeScript.

## Events

There is exactly one event, `run-event`, with a `type` discriminator:

| type           | payload                        | sent when                            |
| -------------- | ------------------------------ | ------------------------------------ |
| `jobChanged`   | job snapshot without its log   | status or current step changes       |
| `logLine`      | job name and one line          | npm prints a line                    |
| `phaseChanged` | step name or null              | per-step mode moves to the next step |
| `finished`     | full run snapshot with summary | the run is over, aborted or not      |

`useRunTracker` keeps logs per job in memory (capped at 2000 lines, same as
the backend) so navigating away from the run view and back does not lose
output; the run store and the terminal store both use it and only add what is
specific to their screen.
`get_run_snapshot` returns the logs too, for the case where the view is opened
after a run started.
