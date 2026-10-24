# Dependencies screen

The Dependencies screen answers three questions about the open workspace:
which version of each shared library every project really has installed, which
projects use which libraries and in what order a run builds them, and which
libraries sit in a project's `node_modules` more than once. It reads files and
asks npm; it never changes a project. The rail entry stays disabled until a
workspace is open.

A library here is either a workspace project that another workspace project
declares in `dependencies`, `devDependencies` or `peerDependencies`, or a
package listed in `update-libs.json`. The wizard's Libraries step offers these
too, along with the other dependencies the selected projects declare. Projects
whose `package.json` cannot be read are left out and named in a notice.

## Versions

One row per library, one column per project that declares it. Each cell shows
the version installed in `<project>/node_modules/<name>/package.json` (or
"none") and below it the declared range, with "dev" or "peer" when the
declaration is not in `dependencies`. A package listed in several fields
counts where `npm install` keeps it: `dependencies`, then `devDependencies`,
then `peerDependencies`. The run's job order reads declarations the same way
(`DeclaredDependencies::find` in `domain/project.rs`). A library
that is itself a workspace project shows its local `package.json` version as a
"local" badge. When two configured projects have that name in their
`package.json`, the badge reads "local version ambiguous" with both project
names instead, and the library cannot be packed or used for the run order.

Markers on a cell:

| marker       | meaning                                                                   |
| ------------ | ------------------------------------------------------------------------- |
| out of range | the installed version does not satisfy the declared range (npm semantics) |
| differs      | the projects have more than one distinct installed version of the library |
| behind       | the installed version is older than the registry's latest                 |

A library row with "out of range" or "differs" in any cell counts as a
mismatch. Ranges or versions that do not parse are never flagged. Versions and
ranges are read the way npm's `semver` package 7 (the copy npm 10 bundles)
reads them, so a version npm rejects, such as `1.2.3garbage` or one with a
number above 2^53 - 1, counts as unparsable.

"Check latest" runs `npm view <name> version --json` once per library, at most
four at the same time, each with a 60 second limit, in the folder of the first
project that declares the library so that its `.npmrc` (private registries,
tokens) applies. A failed lookup marks only that library ("latest unknown",
with the npm error in the tooltip); the others still get their latest version.
A lookup that passes its limit is stopped together with the processes npm
started before its slot goes to the next library. Leaving the screen or
opening another workspace cancels the lookups: running `npm view` processes
are stopped the same way and queued ones never start. Nothing contacts the
registry until the button is pressed.

The two search boxes filter libraries and projects by name, and "Only
mismatches" hides libraries without a mismatch. The "differs" marker is
computed over all projects, so it stays on when the filter hides the project
with the other version.

Clicking cells selects projects for one library; clicking the library name
selects the whole row. The selection bar then offers a version: let the wizard
propose one, the newest version in use, the registry's latest (when checked
and different) or the local version. "Update in wizard" starts a fresh update
in the wizard, the way Start over does: steps, mode and concurrency come from
the settings, and a loaded saved run or retry is replaced. The wizard opens on
the Libraries step with those projects selected and the library ticked, set to
the chosen version, or to the wizard's current proposal when none was picked.
Start over, loading a saved run or retry, or another handoff before the
libraries have loaded drops the pending one. Nothing runs until the wizard is
finished.

## Graph

The graph draws one box per workspace project that takes part in a library
link; projects without any link are named below it. Libraries sit on the
left, and each project sits one column to the right of the deepest library it
links to that the run builds before it, so a column equals the longest chain
of libraries below the project. Lines run from a library to each project that declares
it and end in an arrowhead at the project: solid for `dependencies`, dashed for
`devDependencies`, dotted for `peerDependencies`. Lines run only horizontally
and vertically. Every line leaves the middle of its library's right side, and
all lines of one library turn on one vertical lane in the gap to its right. A
line to the next column then runs straight into its port. A line to a project
more than one column away follows its own track in the space between rows and
its own lane in the gap before the project. A line that points back, as in a
cycle, uses the space below all boxes. Each line has its own arrowhead on the
project's left side, and lines arriving from higher up take the higher ports,
so incoming lines cross each other less. No line crosses a box; lines of
different libraries can still cross each other at right angles.

`lib/dependency-graph-layout.ts` keeps fixed pixel distances, whatever the
width of the screen:

- parallel lines of different libraries run at least 12 px apart, and so do
  the ports on one project;
- a line keeps at least 16 px from every box it does not start or end on, and
  its first and last bends sit at least 16 px from the side they leave or
  enter, so each lane, track and port has that room on both sides. Rows with
  no track between them sit 8 px apart, so a line entering a project's
  outermost port still keeps 16 px from the box above or below;
- an arrowhead is 8 px long and 8 px wide (`ARROW_SIZE`, which the graph tab
  uses for its marker), sits on the straight last run and inside the project's
  side, and never overlaps another one;
- ports are centred on the project's middle. When the library in the same row
  one column to the left feeds the project, its line takes the middle port and
  runs straight across; the other ports sit 12 px apart above and below it in
  arrival order. If that would make the box taller than keeping the middle
  free, or when that library does not feed the project, the middle stays free
  and the ports keep 12 px from it, because the library's line leaves at that
  height;
- each box is as tall as its own ports need, at least 30 px, and sits centred
  in its row. A row is as tall as its tallest box.

The 12 px spacing is 2.5% of the narrowest graph (480 px) and a smaller share
of wider ones. The 16 px clearance is 3.3% of 480 px and 1.1% of 1,400 px. None of these
distances grow with the width.

Hovering, focusing or clicking a box highlights its lines and the boxes on the
other end and dims the rest. Click or Enter pins the highlight; Escape or a
click on the empty background clears it. Every box is reachable with Tab and
carries a label of the form "name. Uses: ... Used by: ...", which is also its
tooltip, and reports whether it is pinned. The graph scrolls sideways inside its card when the window is narrow.

Below the graph the build order is listed as numbered text, each project with
the libraries it waits for. It is computed by the same code a run uses
(`build_jobs` with every readable project selected), so it matches what the
Run screen will do. A run waits on `dependencies` and `devDependencies` only,
so peer links are drawn but do not create waiting.

Projects that depend on each other in a circle are named in a warning. The run
cannot order them, so it builds them after everything else and does not wait
between them; the build order shows the same.

## Duplicates

"Scan node_modules" walks every project's `node_modules`, including nested
`node_modules` folders inside packages, and lists the libraries found more
than once, each copy with its version and its path relative to the project.
"All packages" lists every package with more than one copy instead of only the
libraries. Projects without duplicates and projects without a `node_modules`
folder are named at the end.

The walk runs only when the button is pressed and is bounded:

- entries whose name starts with a dot (`.bin`, `.cache`,
  `.package-lock.json`) are skipped;
- a symlinked or junctioned package (from `npm link` or a `file:` dependency)
  is recorded but not entered, which keeps links back into the project from
  looping and keeps linked workspace folders from being counted twice;
- each `node_modules` folder is walked once per project, by its real path, so
  a link inside a package that leads back to an enclosing `node_modules` does
  not loop;
- nesting stops at 32 levels;
- projects are scanned in parallel on up to eight threads.

On a real workspace of 22 projects with about 77,000 installed packages, the
library scan took 0.1 to 0.2 seconds and the all-packages scan 0.2 to 0.6
seconds with a warm file cache. A cold cache was measured only before the
parallel walk, at 26 seconds for all packages. Leaving the screen or opening
another workspace stops a running scan: the walk checks the request before
each package and each scoped package, so after the cancel arrives each worker
thread finishes at most the one manifest it is reading, up to eight in all.

"Run npm dedupe…" opens the Terminal screen with `npm dedupe` in the prompt
and the affected projects ticked. It does not start the command; it is
disabled while a terminal command is running.

## Request lifetime

Only the newest request of each kind counts: versions (with or without "Check
latest") and the duplicate scan each have one current request, and the graph
has one too. Starting another of the same kind, leaving the screen or opening
another workspace drops the current one: the screen ignores whatever it
returns later, its error and its busy state, and, for versions and duplicates,
asks the backend to cancel it. The graph read is short and is not cancelled.

Before each versions or duplicate request the screen asks
`open_dependency_request` for an id and passes it to the command;
`cancel_dependency_request` cancels that id. The backend hands out the ids, so
they never repeat, not even after the webview reloads while old commands still
run. An id serves one command:

- a cancel that arrives before its command starts is kept, and the command
  then starts already cancelled and returns the cancelled error;
- an id that was cancelled, already used or never handed out also starts
  cancelled;
- when a command ends it removes only its own entry, so it cannot take
  another command's cancellation with it;
- a request dropped while its id is on the way is cancelled as soon as the id
  arrives, and its command is never called;
- closing the window cancels every open and running request.

The wizard handoff follows the same idea: a fresh draft or a newer inspection
makes any inspection reply still on the way stale, and a stale reply is
dropped together with its pending handoff.

## Command line

```bash
deptide-cli deps C:\example\workspace
deptide-cli deps C:\example\workspace --latest
deptide-cli deps C:\example\workspace --duplicates --all-packages
```

The command prints the versions table (one block per library, one line per
project with its declared range, installed version and markers), then the
numbered build order with any cycle, and with `--duplicates` the copies per
project. `--latest` asks the registry the same way the screen does, and
`--all-packages` widens the duplicate scan the same way. It exits with 0 even
when it finds mismatches, because it reports and does not judge.

## Where it lives

- `crates/deptide-core/src/dependencies/`: `projects.rs` (reading the
  workspace projects and the library list), `drift.rs` (versions, ranges,
  `npm view`), `graph.rs` (edges, layers, cycles, order), `copies.rs` (the
  `node_modules` walk), `request.rs` (a request that cancellation reaches, the
  lookup limits, and the table of open and running requests by id). It is a
  separate module from `scan/duplicates.rs`, which finds configuration entries
  that point at the same folder. Version parsing, comparison and range checks
  live in `util/semver.rs`, shared with the version bump step.
- `crates/deptide-desktop/src/app/commands/dependencies.rs`: the Tauri commands
  `read_dependency_drift`, `read_dependency_graph`, `find_installed_copies`,
  `open_dependency_request` and `cancel_dependency_request`; the first three
  run off the async runtime. The drift and duplicate commands take an id from
  `open_dependency_request` (see Request lifetime); closing the window
  cancels them all.
- `apps/desktop-ui/src/views/DependenciesView.vue`, the three tab components in
  `components/dependencies/`, `stores/dependencies.ts`,
  `lib/dependency-drift.ts`, `lib/dependency-graph-layout.ts` and
  `composables/useGraphHighlight.ts`. The mock backend answers all five
  commands; `?deps=cycle` in the URL adds a cycle to the mock graph.
