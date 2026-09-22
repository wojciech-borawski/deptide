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
   in-progress ones. Ticking a library opens the version picker:
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
   it can be limited to projects whose version still equals the one on the
   `main` (or `master`) branch, so a project already bumped on the current
   branch is left alone. Without a git checkout to compare with, the step logs
   why it skipped and the project still counts as ok. Presets: Full (uninstall, install, audit, build),
   Simple (uninstall, install) and Force (force install, audit, build).
4. **Review.** Everything in one place, a label for the log file, and the
   choice to save the run for later.

The footer validates cumulatively: the Next button only unlocks once the
current and previous steps are complete, and the first problem is spelled out
next to it. Completed steps in the header are clickable to go back.

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
