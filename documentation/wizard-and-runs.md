# The wizard and the run screen

## Steps

1. **Projects.** The configured projects with a checkbox each. Projects whose
   folder has no `package.json` are shown as "folder missing" and cannot be
   selected; two entries that resolve to the same folder are flagged as
   duplicates. The trash icon removes an entry from `update-libs.json`.
   "Scan for projects" opens the scanner: every `package.json` below the
   projects root is listed, already configured ones are ticked. Applying the
   selection adds ticked newcomers, removes unticked configured ones and leaves
   configured projects the scan did not reach untouched. New entries get a
   readable name derived from the folder path, made unique if needed.
2. **Libraries.** The dependencies of the selected projects, libraries that
   exist as a project under the projects root first, because those are the
   in-progress ones. The list is grouped under `dependencies`,
   `peerDependencies` and `devDependencies`, in that order. A package found in
   several of these fields, across all selected projects, is listed once, in
   the first of them, with a badge for each other field it is in: a peer that
   is also a dev dependency sits under `peerDependencies` with a `dev` badge.
   The filter and "Only libraries found locally" apply across the groups, and
   empty groups are hidden. Ticking a library opens the version picker:
   - the local version read from the library's own `package.json`,
   - the base version plus a ticket suffix (`3.1.0` + `ABC-123`),
   - keeping one of the ranges currently installed,
   - an exact version typed by hand.
     Packages that are not a dependency of any selected project can be added by
     name at the bottom; those go into every selected project.
3. **Options.** Which steps to run, in which order the projects and steps are
   combined, how many projects run in parallel, a dry-run switch and extra
   `npm install` flags. The steps are uninstall, install, force install
   (`npm install <versions> --force`, which refetches a version npm already
   has), bump version, audit fix and build. Bump version runs
   `npm version patch|minor|major --no-git-tag-version` on the project itself;
   when it bumps is one of three choices, see
   [Bump conditions](#bump-conditions). Presets: Full (uninstall, install, audit, build),
   Simple (uninstall, install) and Force (force install, audit, build).
4. **Review.** Everything in one place, a label for the log file, and the
   choice to save the run for later.

The footer validates cumulatively: the Next button only unlocks once the
current and previous steps are complete, and the first problem is spelled out
next to it. Completed steps in the header are clickable to go back.

## Bump conditions

The Options step asks when the version step bumps a project:

- **Always.** Every project that runs the step is bumped.
- **Only while equal to main.** The working-tree version is compared with the
  one committed on `main`, `master`, `origin/main` or `origin/master`,
  whichever is found first. Equal means bump, different means skip. On a
  branch created from a feature branch that already bumped, this skips even
  though the new branch changed nothing.
- **Only if this branch has not bumped it.** The working-tree version,
  uncommitted edits included, is compared with the version at the commit where
  the current branch left the branch it was created from, its fork point.
  Equal means bump, different means this branch already changed it and the
  step skips. This is the choice for stacked branches: a branch created from a
  feature branch at 1.1.0 is compared with 1.1.0, not with main.

The fork point is found from history. Every other local and remote branch is a
possible base, except the current branch's own copies on remotes, branches
built on top of the current one, local branches created after the current one,
and each configured remote's `HEAD` pointer such as `origin/HEAD` (a real
branch such as `origin/feature/HEAD` still counts). The branch's own commits
are those on its first-parent line that no possible base contains; the fork
point is the parent of the oldest of them, or the current commit when there are
none. The log names the base branch and the short commit.

A local branch's creation time is its oldest reflog entry. Another local branch
counts as a base only if it was created no later than the current branch, at
whatever commit it now points. A backup or child made from the current branch
after it bumped therefore never moves the fork point past that bump, whether it
sits at the same commit or has commits of its own. A local branch without a
reflog still counts, because nothing says it came later. The current branch
needs its own reflog: without it the step cannot tell which branches came later
and skips. Reflog entries expire after 90 days by default (`gc.reflogExpire`).

Remote branches have no creation time and are not filtered this way. A remote
branch created from the current one after it bumped, at the same commit or with
commits of its own, still counts as a base and can make the step bump again.

Once the branch is merged into main with a merge commit or a fast-forward, main
contains the bump. If work then continues on the branch, the next run bumps
again: that is intended, it starts a new release cycle. After a squash merge
main does not contain the branch's commits, so the step keeps skipping.

When the step cannot decide it skips, logs why, and the project still counts
as ok. For the main comparison that happens when no main branch can be read.
For the branch comparison it happens on `main` or `master` itself, when `HEAD`
is detached, outside a git repository, in a repository without commits, when
the current branch has no reflog, with no other branch to compare with, when
the branch shares no history with any possible base, or when `package.json`
has no version at the fork point. In a shallow clone the fork point can lie
beyond the fetched history; the log then says the clone is shallow and the full
history has to be fetched.

## Why uninstall then install

A prerelease such as `1.0.0-test-1` can be republished with the same version
and different content. `npm install` sees the version it already has and does
nothing, so the tool uninstalls the library first and installs it again, which
forces npm to fetch the new tarball. All selected libraries go into one
`npm uninstall` and one `npm install` per project, which is what lets peer
dependent libraries resolve against each other. `--force` is offered as an
extra install flag for cases where a reinstall is not enough.

npm writes the installed version into `package.json` with a caret by default
(`^1.0.0-test-1`). That is what the CLI did as well; tick the `--save-exact`
flag on the Options step to store the exact version instead.

## Which field a package is installed into

The run reads each project's own `package.json` and installs every library
into the field that project declares it in:

- listed in `dependencies`: a plain `npm install`;
- otherwise listed in `devDependencies`: `npm install --save-dev`, also when
  it is a peer dependency as well;
- only listed in `peerDependencies`: `npm install --save-peer`. npm rewrites
  the peer range to `^<version>` and adds no dev entry. `--save-exact`, `-E`
  and `--save-exact=true` are left out of this install and `--no-save-exact`
  is added, so `save-exact=true` in `.npmrc` cannot pin the range either,
  because an exact peer range would pin every consumer of the library to one
  version. The log says so when one of those flags was given.

When `package.json` cannot be read, the flags chosen in the wizard apply: dev
when every project that declares the package has it as a dev dependency, peer
when it was found only in `peerDependencies`. A package with both flags is
installed as dev.

`npm uninstall` also deletes a package from `peerDependencies`, and an
install with `--save-dev` or without a flag does not put it back. So when a
project's steps include install or force install, the run records every entry
the chosen packages have in `dependencies`, `devDependencies` and
`peerDependencies`, before the project's first uninstall, install or
force-install step. What is written back depends on how those steps end:

- the last of them succeeded: only the recorded `peerDependencies` ranges of
  packages not installed with `--save-peer`, so the new versions stay;
- one of them or a step between them failed, or the project was stopped:
  every recorded entry, so a rerun reads the same `package.json` as the first
  run and installs each package into the same field.

Only entries that are missing or changed are written, and each is logged as
`restored <field>.<name> = <range>`. Deptide writes them into `package.json`
itself rather than through `npm pkg set`, because npm forces a package listed
in both `dependencies` and `peerDependencies` to one range and would replace
the newly installed `dependencies` range with the old peer range. Like npm,
the write sorts the dependency sections it touches by name and keeps the
other keys in place, the indentation (a one-line file stays on one line), line
endings, byte order mark and final newline. It replaces the file in one step;
while another program holds `package.json` open it retries for about a
second and then writes the file in place. If that fails as well, it puts the
previous content back and the project fails. The run then reads
`package.json` again; if it cannot be written (for example it is not valid
JSON) or an entry still differs, the project fails with the entries named in
the error. A dry run prints `would restore <field>.<name> = <range>` instead.
A plan that uninstalls but does not install records and restores nothing,
since removing the package is what was asked for.

A stopped project is restored once its npm process has ended. The project
keeps its stopped status unless the restore fails. In per-step mode the
entries are recorded in the phase of the project's first package step and
restored in the phase of its last one; a project that does not reach that
phase, because it was stopped or its library failed, is restored once all
phases are done.

## The run screen

- The header shows the label, the packages, the elapsed clock and, in per-step
  mode, which step the whole group is in. "Stop everything" aborts.
- The table lists every project with its status, current step or error, a
  timeline bar with one segment per finished step and the live duration.
- Clicking a row shows that project's npm output in the pane below. Output
  follows the newest line unless scrolled up.
- When the run ends the summary card appears: total wall-clock time, the sum of
  npm time and the parallel factor, counts per outcome, a per-project table
  with step timings, and buttons to open the transcript.
- "Rerun projects…" (or the refresh icon on a row) opens a dialog to rerun any
  subset of the projects with any subset of the steps, using the same libraries
  and options as the finished run; the label gets a `-rerun` suffix and the
  result is a new run with its own transcript. "Retry in wizard" pre-fills the
  wizard with the failed and skipped projects instead, for changing versions or
  flags first; "New run" starts the wizard fresh.

Only one run can be active at a time. Closing the window kills every npm
process that is still running.

## History

Saved runs can be loaded into the wizard or deleted. Past runs list the
outcome, the total time and the npm time for every transcript in `logs/`, with
a button to open the log file.

## What happens around the npm steps

- **Dependency order.** When a selected project is itself a library that
  another selected project depends on (its `package.json` name appears in the
  other's dependencies), the library runs first. The run table lists projects
  in that order and shows "waits for …" on the dependents. If a library fails
  or is stopped, its dependents are skipped with a note instead of building
  against a stale version. Cycles are broken by ignoring the edge that closes
  them.
- **Install verification.** After the install or force-install step the app
  reads `node_modules/<library>/package.json` and the hidden
  `node_modules/.package-lock.json`. The run table and the summary show the
  version that actually landed and the first characters of its integrity hash,
  so a stale tarball is visible immediately. A version that differs from the
  requested one turns the project into a warning.
- **Failure diagnostics.** When a step fails, the last 200 output lines are
  matched against known npm and build failures (peer conflicts, registry 401/
  403/404, no matching version, integrity mismatch, network errors, locked
  files, lockfile drift, TypeScript errors, missing modules, out of memory).
  The match is shown as a one-line title in the table and as a hint with a
  suggested action in the summary and the report.
- **Slower than usual.** Step durations of the last twelve successful runs in
  `logs/` form a per-project, per-step median. A finished step that took at
  least 10 seconds and 1.5 times its median is flagged in the table and the
  summary with the ratio and the median.
- **Notifications and tray.** Deptide keeps an icon in the system tray; while a
  run is active its tooltip names the run, and a left click or the menu brings
  the window back. When a run finishes a system notification reports the
  counts and the total time.
- **Reports.** The summary card offers "Copy Markdown" and "Save .md" or
  "Save .html"; the History screen has a "Report" button for every past run.
  The report contains the outcome, total and busy time, installed versions
  with hashes, step timings and any diagnosis, ready for a ticket or merge
  request.

## Versions from branch names

When a library is checked out under the projects root, Deptide reads its git
branch (`.git/HEAD`, worktrees included) and applies the ticket pattern from
Settings, by default `^([A-Za-z]+-\d+)`. The first capture group becomes the
version suffix, so a library at `3.1.0-ABC-100` on branch `ABC-123-widgets`
is proposed as `3.1.0-ABC-123`, which is what a CI pipeline publishes for that
branch. The Libraries step shows the branch as a badge and the version picker
offers this as the first, preselected option. Change the pattern in Settings
when the branch naming differs; an invalid or empty pattern falls back to the
default.

## Safety around a run

- **Backups.** Before the first step touches a project, its `package.json`
  and `package-lock.json` are copied to `backups/<run>/<project>/` in the
  workspace. After the run, failed, skipped and warned projects offer
  "Restore files" in the table and the summary, which copies both files back.
  The newest twenty run backups are kept. Dry runs take no backups.
- **Stop one project.** While a run is active, hovering a running or waiting
  project shows a stop button. Its npm process tree is killed and the project
  ends as skipped while every other project continues.
- **Transient retries.** When a step fails and the output matches a network
  or integrity problem, the step is retried once automatically. The retry is
  logged and the project shows a "retried" badge.
- **Transitive changes.** The top-level packages in `node_modules` are read
  from the hidden lockfile before and after the install step; the difference
  is listed under the project in the summary and in the report as
  added, removed and changed versions.

## Trends, appearance, language, keyboard, updates

- The History screen charts the total time of the last twenty real runs and
  the slowest projects by average duration, once two runs exist.
- Settings has an Appearance section: theme (follow system, dark, light),
  density (comfortable, compact) and language (English, Polish, German).
  These are per user, stored in the browser storage of the app, not in the
  workspace.
- In the wizard, Enter goes to the next step or starts the run, Alt+Left goes
  back and Alt+Right forward; Space toggles a focused checkbox.
- An update manifest URL can be set in Settings. Deptide fetches it at start
  and on demand; a newer version shows a badge in the rail and a link to the
  download.

## Libraries, applications and paths

Every project is classified from its `package.json`: a manifest with `main`,
`module`, `exports`, `types`, `files`, `publishConfig`, `bin` or peer
dependencies is a library (violet package icon), everything else an
application (teal layers icon). The Projects step, the scan dialog and the
Transfer screen show the icon, and the Projects step can filter the list to
libraries or applications only. Paths are shown absolute by default; Settings
has an Appearance option to show them relative to the config file instead.

Project lists (Projects step, Terminal picker, Transfer copy tab) collapse the
part of the path that every listed project shares into a `[…]` button; clicking
it shows the full prefix for that row. What remains is used to group the
projects by folder: each folder gets a header row, and when a folder holds more
than one project both the header and the folder name in the paths are drawn in
the accent colour. Projects directly under the shared prefix are listed without
a header. Grouping follows the current filter, so it reflects what is visible.
