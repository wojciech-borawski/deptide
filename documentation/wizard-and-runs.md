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
   empty groups are hidden. "Update in wizard" on the Dependencies screen
   lands here with its projects selected and its library already ticked.
   Ticking a library opens the version picker:
   - the local version read from the library's own `package.json`,
   - the base version plus a ticket suffix (`3.1.0` + `ABC-123`),
   - keeping one of the ranges currently installed,
   - an exact version typed by hand,
   - for a library that is a configured project, a tarball packed from its
     folder, see [Pack from a local folder](#pack-from-a-local-folder).
     Packages that are not a dependency of any selected project can be added by
     name at the bottom; those go into every selected project.
     Choosing no library is allowed. The run then only bumps versions, runs
     audit fix, reports vulnerabilities or builds: leaving this step without a library unticks
     uninstall, install and force install (bump version is ticked if nothing
     else is left), the Options step marks them "Needs a library", and every
     selected project takes part.
3. **Options.** Which steps to run, in which order the projects and steps are
   combined, how many projects run in parallel, a dry-run switch and extra
   `npm install` flags. The steps are uninstall, install, force install
   (`npm install <versions> --force`, which refetches a version npm already
   has), bump version, audit fix, audit report and build. Bump version runs
   `npm version patch|minor|major --no-git-tag-version` on the project itself;
   when it bumps is one of three choices, see
   [Bump conditions](#bump-conditions). "Not every project gets the same
   bump" lists the selected projects with their own patch, minor or major
   choice; projects left alone use the level chosen above. Audit report runs
   `npm audit --json` and counts the vulnerabilities per severity, see
   [Audit report](#audit-report). Presets: Full (uninstall, install, audit, build),
   Simple (uninstall, install) and Force (force install, audit, build).
4. **Review.** Everything in one place, a label for the log file, the choice
   to save the run for later, and "What will run", see
   [What will run](#what-will-run).

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
- **Only if this branch has not bumped it.** Every commit the current branch
  made since it left the branch it was created from, its fork point, is
  checked. If any of them changed the `package.json` version, even one that a
  later commit changed back, the step skips and the log names that commit.
  A merge commit whose version came in from the merged branch, such as a
  release bump merged from main, is not a change by this branch. An
  uncommitted version edit also counts as changed. Otherwise the step bumps.
  This is the choice for stacked branches: a branch created from a feature
  branch that bumped to 1.1.0 only looks at its own commits after that.

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

Merging main into the branch does not count as a bump by the branch, even
when main's version changed in the meantime.

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

## Pack from a local folder

When a library is itself a configured project (its `package.json` name
matches), the version picker on the Libraries step also offers "Pack from
local folder". The run then installs a tarball of the library's working copy
instead of a published version, so a change can be tried in the applications
before it is released.

The run adds one pack job per packed library, named after the library's
project. It runs `npm run build` whenever the library has a build script,
whatever `prepack` or `prepare` scripts it also has, then
`npm pack --json --pack-destination <workspace>/packs/<stamp>-<label>/<library folder>`.
Each library packs into its own folder, named after its package name with the
scope's `@` dropped and `/` written as `+`, followed by a dash and eight hex
digits of the SHA-256 of the exact package name (`@acme/core` packs into
`acme+core-91484758`, `acme-core` into `acme-core-b1eb1f2c`, `Acme-Core` into
`Acme-Core-44cac569`). npm gives `@acme/core` and `acme-core` the same file
name, `acme-core-1.2.3.tgz`, so one shared folder would let the second pack
overwrite the first. `Acme-Core` and `acme-core` get file names that Windows
treats as the same, so the digits keep their folders apart. The pack job takes the library's project lock like
every other job, so a library another run is working in is skipped together
with the projects that install it. A job whose only step is the pack takes no
backup and is not part of "Restore all": the run edits no file in that project
itself. Its build and npm's lifecycle scripts can still change files there,
and a restore would not undo those changes.

Every project that installs the library waits for the pack job, listed once
however many of the library project's packages it installs; when the pack
fails, those projects are skipped with "not run because <library> did not
succeed". `npm pack --json` reports the name, version and file name of the
package it packed. The pack job fails when that name is not the library's
package name, for example when a build renames `@acme/core` to
`@acme/core-extra` or to `acme-core`, so its consumers are skipped instead of
installing another package. Otherwise it records the file name and version, and
the install uses the full path of that file in the library's folder, so the
run installs exactly the tarball that was written. The check after
the install, and the one after `npm audit fix`, compare the installed version
with that recorded version. A later change to the library's `package.json` in
the same run, such as its own version step in step-by-step mode, does not count
as a difference. A project never installs its
own tarball: when the library's project is selected, the packed package is
not applied to it.

The picker carries one of two modes:

- Install without saving (the default):
  `npm install <tarball> <install arguments> --no-save`. `--no-save` comes
  last, so no extra install argument can turn saving back on. npm puts the
  library into `node_modules` and leaves `package.json` and
  `package-lock.json` byte for byte as they were. The uninstall step skips
  the library, since uninstalling would write both files, and logs
  "<library> not uninstalled: installed from a packed tarball without
  saving". `npm audit fix` rebuilds `node_modules` from the lockfile and drops
  the tarball, so in a job with an install step the audit step installs the
  same tarball again the same way right after `npm audit fix` and checks the
  installed version again. The build after it therefore sees the packed
  library. The next plain `npm install` or `npm ci` brings back the published
  version.
- Save to package.json: the tarball goes through the normal install, so
  `package.json` and the lockfile point at a `file:` path inside the
  workspace. The wizard and the review step warn about this: restore the run
  (see [Undo a run](#undo-a-run)) or install a published version before
  committing. The backups are taken as for any other run.

When the library's own project is also selected, its job gets the pack step
instead of a separate pack job. Project by project, the pack step comes after
its other steps, so a library that is built by its build step is packed
without building again ("already built by its build step, packed without
building again"), and a library that bumps its own version in the same run
packs the new version. Step by step, the pack phase runs first, before any
project installs, and the library is built in it and again in the build
phase. Its tarball then holds the library as it was before its own install
phase, and the preview and the run say so: "packed before its own install
phase, so the tarball does not hold this run's changes to it". A selected
library project that declares none of the chosen packages stays in the list
of projects none of the chosen libraries applies to, and the preview notes on
its pack step "only packed: none of the chosen libraries applies to this project, so its
other steps do not run".

The preview names each tarball the way npm does: the scope's `@` is dropped
and the `/` becomes `-`, and the version is kept as written, including a
leading `v` or build metadata (`@acme/core` at `3.1.0` becomes
`acme-core-3.1.0.tgz`). For a library whose version step runs before the
pack and would bump, the preview uses the version the bump produces, so the
preview and the run name the same file.

A packed library that no configured project names cannot be packed: the
install step of each project that would receive it fails with "<library>
cannot be packed: no configured project has that name in its package.json",
and the preview says the same. The same holds when two configured projects
have that name in their `package.json`: "<library> cannot be packed: more
than one configured project has that name in its package.json (<projects>)".
When more than one selected project has such a name, it gives no dependency
order either; when only one of them is selected, the projects that use the
name wait for that one. The version picker and the Dependencies screen show
the name as ambiguous. On the command line `--pack <name>`
refuses both cases before the run starts.

The pack folders are kept under `packs/` in the workspace. A folder is named
with the time the run starts, not the time a pack job runs. The twenty newest,
counting the new run's own folder, are kept; a pack job removes the older
ones once it has created the run's folder. The project scan does not look inside `packs/`. A
dry run prints the commands the run would execute, with the same quoting and
flags, for example `would run npm install "<tarball>" --no-fund --no-save`,
and the reinstall after `npm audit fix`. Saved runs keep the pack choice, and
a retry from the run screen packs again.

On the command line, `--pack <name>` packs a library at the version in its
own `package.json`; it can be repeated, and `--pack-save` switches every
packed library to the saving mode. `--preview` lists the pack jobs and the
tarball installs like the Review step does.

## What will run

The Review step asks the backend what the run would do and lists every
project that gets a job, in run order, with the projects it waits for. Each
project shows how many npm commands it gets and, when the version step is
ticked, one version line:

- `1.4.2 -> 1.5.0 minor` when it bumps, with the version npm will write. npm
  starts from `0.0.0` when `package.json` has no version, so a patch shows
  `none -> 0.0.1` for a missing or `null` version and ` -> 0.0.1`, with
  nothing before the arrow, for an empty one, as the run logs them;
- `skipped: <reason>` when the bump condition skips it, naming the commit
  that already bumped or the main version it differs from;
- `fails: <reason>` when npm cannot bump the version, such as `01.2.3`,
  `1.2.3-!` or a number above 9007199254740991. The preview checks a version
  the way npm does (strict semver after dropping leading `v` and `=`), so
  `v1.2.3` and `1.2.3+build` bump and the others fail as the run would.

The reasons and notes come from the backend as a kind with its values, so the
Review step shows them in the chosen language. A reason's version is null
when `package.json` has none, and the wording shows it as "none". The CLI
and the run transcript keep the English wording. When git cannot tell whether
the branch bumped the version, the cause, such as a folder that is not a git repository or a
detached HEAD, is one of Deptide's own fixed explanations and is translated
too.

Clicking a project unfolds its steps with the exact command lines, install
flags included, and the notes the run will log, such as "no build script,
step skipped". Projects not in the configuration and projects none of the
chosen libraries applies to are listed with that reason.

The preview is worked out from the same code the run uses, so the commands
and the bump decisions match what the run then does, as long as nothing
changes in between: a commit or a version edit made after the preview can
change the decision. It reloads whenever the plan changes, except for the
saved name, and for the run label when no library is packed. A packed run's
tarball folder carries the label, so editing it reloads the preview. While a new preview loads, the previous one stays
visible, dimmed and marked "Updating…". While it loads or after it fails the
run can still be started. On a dry run the preview shows the commands the run
would otherwise execute.

The command lines wrap arguments that contain spaces or characters such as
`^`, `&` or `%` in double quotes, so each argument reads as one. They are for
reading only: the quoting is not shell escaping, and a line pasted into `cmd`
or PowerShell can expand `%VAR%` or `$name` inside the quotes and give npm
different arguments. The run itself does not use this text and passes each
argument to npm separately. The run's transcript logs each command in the
same form.

`deptide-cli run ... --preview` prints the same preview and exits without
running anything.

## Audit report

The audit report step runs `npm audit --json` after audit fix and before
build. It needs no library, so it can run alone on any set of projects. It
reads `metadata.vulnerabilities` from the JSON and stores the counts for
critical, high, moderate, low and info findings plus the total. npm exits
with code 1 when it finds vulnerabilities; the step still counts as ok as long
as the JSON report can be read. When npm prints an error instead of a report,
for example without a lockfile or without access to the registry, the step
fails with npm's error code and summary. Network failures are retried like the
other npm steps. The step changes neither `package.json` nor the lockfile.

The counts appear as severity badges in the run table and the summary's
project table, as totals over all audited projects in the summary card, as an
"Audit" table in the Markdown and HTML reports, and in the command line
summary and transcript.

## The run screen

- The header shows the label, the packages, the elapsed clock and, in per-step
  mode, which step the whole group is in. "Stop everything" aborts.
- The table lists every project with its status, current step or error, a
  timeline bar with one segment per finished step and the live duration.
- Clicking a row shows that project's npm output in the pane below. Output
  follows the newest line unless scrolled up.
- When the run ends the summary card appears: total wall-clock time, the sum of
  npm time and the parallel factor, counts per outcome, a per-project table
  with step timings, and buttons to open the transcript. After an audit
  report the card adds the vulnerability totals and each project row its
  severity badges.
- "Rerun projects…" (or the refresh icon on a row) opens a dialog to rerun any
  subset of the projects with any subset of the steps, using the same libraries
  and options as the finished run; the label gets a `-rerun` suffix and the
  result is a new run with its own transcript. "Retry in wizard" pre-fills the
  wizard with the failed and skipped projects instead, for changing versions or
  flags first; "New run" starts the wizard fresh.
- "Restore all" undoes the whole run: it puts back the `package.json` and
  `package-lock.json` of every project the run backed up. See
  [Undo a run](#undo-a-run).

Only one run can be active at a time. Closing the window kills every npm
process that is still running.

## History

Saved runs can be loaded into the wizard or deleted. Past runs list the
outcome, the total time and the npm time for every transcript in `logs/`, with
a button to open the log file. A past run whose backup folder still exists
offers "Restore files", the same undo as "Restore all" on the Run screen. It
works after the app is restarted, because the backups are found from the
summary on disk. Dry runs, and runs whose backups were pruned, show no restore
button.

## Undo a run

"Restore all" on a finished run and "Restore files" in History open one
confirmation that lists every backed-up project with the files that will be
written back. Confirming copies `package.json` and `package-lock.json` from
`backups/<run>/<project>/` into each project. No other file is changed or
deleted, and a lockfile created after the run stays where it is. The result
lists each project as restored, with its files, or failed, with the reason.
A project that a run or a Terminal command is still working in, in this app,
another window or the CLI, fails with "Not restored: … is working in this
project"; the others are restored. See
[Project locks](workspace-format.md#project-locks).

Closing the confirmation while a restore is writing does not stop the write.
The dialog drops that answer, and opening it again starts a new confirmation.

`node_modules` still holds what the run installed, so the result asks for
`npm install` and offers "Open Terminal with npm install". It opens the
Terminal with that command and the restored projects selected, without
starting it.

On the command line, `deptide-cli history <workspace>` prints each run's name
in its last column, and `deptide-cli restore <workspace> <run>` restores it,
optionally limited with `--project NAME` (repeatable). The command exits with
1 when any project failed. It refuses projects that a desktop or CLI run is
working in, through the same project locks.

## Project groups

Every project picker, in the wizard's Projects step, the Terminal and the
Transfer copy panel, shows the workspace's groups as chips above the list.
Clicking a chip selects exactly that group's projects that this picker can
select; Shift+click or Ctrl+click adds them to the current selection. "Save
selection as group" asks for a name and, when a group of that name exists,
asks before replacing its projects. "Edit groups" shows a rename and a delete
button next to each chip. The chips are buttons, so Tab and Enter or Space
work, and Shift+Enter adds. Groups are stored in `update-libs.json` (see
[the workspace format](workspace-format.md#update-libsjson)) and every picker
shows a change at once.

`deptide-cli groups <workspace>` lists the groups, and `deptide-cli run ...
--group NAME` (repeatable) adds a group's projects to those named with `-P`.

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
  with hashes, step timings, the audit counts per project and in total, and
  any diagnosis, ready for a ticket or merge request.

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
  "Restore files" in the table and the summary, which copies both files back;
  "Restore all" and History undo every project of a run at once
  ([Undo a run](#undo-a-run)). The newest twenty run backups are kept. Dry runs
  take no backups.
- **One writer per project.** A run locks each project for the time of its
  job, so within one workspace two runs, in the app and the CLI, never work in
  the same project at once. A project another run holds is skipped with the
  holder named, and a restore of it is refused with `Not restored: …` instead
  of waiting. See [Project locks](workspace-format.md#project-locks).
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
  back and Alt+Right forward; Space toggles a focused checkbox. While a
  dialog is open these shortcuts do nothing. Opening a dialog moves focus into
  it, Tab and Shift+Tab stay inside it, Escape closes the topmost one, and
  closing it returns focus to where it was.
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
the accent colour. Projects directly under the shared prefix are listed first,
without a header, so they never look like part of another folder's group.
Grouping follows the current filter, so it reflects what is visible.
