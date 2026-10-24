# Workspace folder

A workspace is any folder the user picks on the start screen. The app never
stores project data anywhere else, so the folder can live next to the
repositories, inside a shared drive or in version control.

```
<workspace>/
  update-libs.json     projects and last used package versions
  settings.json        scanning root and wizard defaults
  runs/<name>.json     saved runs
  logs/<stamp>-<label>.log   transcript of one run
  logs/<stamp>-<label>.json  summary of the same run, including total time
  backups/<stamp>-<label>/   package.json and package-lock.json before a run
  packs/<stamp>-<label>/     tarballs npm pack wrote for a run's packed libraries
```

A folder that only contains `config/update-libs.json` (the layout of the
previous CLI) is picked up as well. Project paths are always resolved relative
to the folder that holds `update-libs.json`, so a workspace created by the CLI
keeps working unchanged.

The list of recently opened workspaces is the only thing stored outside, in the
per-user app config directory (`recent-workspaces.json`). The last opened one is
reopened automatically at start.

## update-libs.json

```json
{
  "installArgs": ["--no-fund"],
  "auditFixArgs": [],
  "packages": [{ "name": "@acme/core", "version": "3.1.0-ABC-123", "saveDev": false, "savePeer": false }],
  "projects": [
    { "name": "Shop-Frontend", "path": "../repos/shop/frontend" },
    {
      "name": "Legacy",
      "path": "../old/legacy",
      "skip": true,
      "packages": ["@acme/core"],
      "installArgs": ["--legacy-peer-deps"]
    }
  ],
  "groups": [{ "name": "shop", "projects": ["Shop-Frontend", "Shop-Admin"] }]
}
```

- `projects[].path` is a POSIX style relative path, written by the scan.
- `skip` unticks the project by default in the wizard.
- `packages` restricts which libraries may be applied to that project.
- `installArgs` on a project replaces the top-level `installArgs`.
- `saveDev` and `savePeer` on a package are the install flags used when a
  project's `package.json` cannot be read; otherwise the field the project
  declares the package in decides. `saveDev` wins when both are set. A missing
  `savePeer`, as in files written before it existed, reads as `false`; the same
  holds for packages in saved runs.
- Older shapes are accepted when reading: a package may be the string
  `name@version`, a project may be a bare path string.
- `groups` are named project selections, shared by the project pickers of the
  wizard, Terminal and Transfer and by `deptide-cli run --group NAME`. A member
  is a project `name`. The key is optional when reading and left out when
  there are no groups, so files from older versions load unchanged. An older
  version of the app that saves the file drops the key.
- Group members follow the project list. Saving the configuration, from the
  app or from `deptide-cli scan --apply`, drops members that are no longer a
  configured project and removes a group that ends up empty. Reading keeps
  members as written, so a hand edit survives until the next save. The app
  cannot rename a project; a project renamed by hand, or removed by a scan and
  added again under a new name, leaves its groups at the next save and has to
  be added again. `--group` passes members that are not configured on to the
  run, which skips them with a warning, as it does for unknown `-P` names.
- Saving keeps what the app does not know: unknown top-level keys such as
  `$schema`, and unknown fields inside a package or project entry. A package
  entry is matched by `name`. A project entry with a `name` is matched by that
  name, and one without a `name` by its full configured `path`, so
  `../team-a/api` and `../team-b/api` keep their own fields. When two entries
  share a name, only the one with the same path gets the fields; when that is
  still not one entry, the fields are not copied to any of them. Known
  optional fields the app clears, such as `skip`, are left out.

## settings.json

```json
{
  "projectsRoot": "D:/work/repositories",
  "scanDepth": 6,
  "concurrency": 3,
  "steps": ["uninstall", "install", "audit", "build"],
  "mode": "per-project",
  "extraIgnoredDirectories": [".vite-deps"]
}
```

Missing or malformed values fall back to defaults instead of failing. The
scan always skips `node_modules`, `.git`, `dist`, `build`, `coverage` and the
other folder names listed in `scan/detect.rs`. The workspace's own
`backups/`, `transfers/`, `runs/`, `logs/`, `packs/` and `locks/` folders are
skipped as well, so the `package.json` copies kept there never show up as
projects.

## Saved runs

A saved run records the selected project names, the exact package versions,
steps, order, concurrency, extra install flags and the version-bump policy
(`version: { bump, when }`, defaulting to a patch bump with `when: "always"`;
`when` is `always`, `same-as-main` or `not-bumped-on-branch`), plus
`projectBumps`, the projects whose bump level differs, such as
`{ "api": "major" }`; it is left out when every project shares the level. Runs saved
before `when` existed store `onlyIfSameAsMain` instead and still load: `true`
as `same-as-main`, `false` or missing as `always`. When both fields are
present, `when` wins. `steps` may contain `audit-report`; runs saved
before that step existed still load unchanged. A package installed from a
packed tarball carries `"pack": "no-save"` or `"pack": "save"`; packages
without the field, including every run saved before it existed, install
from the registry. Loading one in the History
screen pre-fills the wizard and jumps to the review step, so anything can still
be changed before starting.

## Run summaries

Each run writes a summary JSON next to its transcript. Besides the per-project
outcome it holds:

- `totalDurationMs`: wall-clock time from the first to the last npm command,
  which is the number shown as "Total time".
- `busyDurationMs`: the sum of the time every project spent in npm. With
  concurrency above one this is larger than the total, and the ratio is shown
  as the parallel speed-up.
- `stepTimings` per project: how long uninstall, install, audit fix and build
  took, which feeds the timeline bars in the run view.
- `audit` per project, only when it ran the audit report step:
  `{ "info", "low", "moderate", "high", "critical", "total" }`, the counts
  from `npm audit --json`. Summaries without it, including every summary
  written before the step existed, load as projects without audit counts.
- `backupFolder`: the run's folder under `backups/`, written by real runs only.
  See [Backups](#backups) for summaries without it.

A run's name is its transcript's file name without `.log`: `<stamp>-<label>`,
where `<stamp>` is the local start time to the second. Creating the transcript
claims the name, so a second run started in the same second with the same
label, in the desktop app or the CLI, gets `<stamp>-<label>-2`, then `-3`. The
backup folder of a real run has the same name, so no two runs share a backup
folder.

The newest 50 transcripts are kept, older ones are removed automatically.

## Backups

`backups/<run>/<project>/` holds the `package.json` and
`package-lock.json` of a project as they were before a real run touched it.
`<project>` is the project name turned into a folder name: lower case, any
run of characters other than ASCII letters, digits, `.`, `_` and `-` becomes
one `-`, and dots become `_`. The twenty newest run folders are
kept; starting a real run removes the older ones.

A run's backups are found from its summary's `backupFolder`. Only a single
folder name directly under `backups/` is accepted, so an edited summary cannot
point a restore elsewhere. A dry run never has backups, whatever its summary
says.

Summaries written before `backupFolder` existed fall back to the folder named
after their exact start second and label, `<stamp>-<label>`. That folder is
used only when it exists and no other summary, old or new, claims the same
name; otherwise the run shows no restore action. Neighbouring seconds are not
tried, because they can belong to another run.

A project is offered only when its folder holds at least one of the two files
and no other project of the same run maps to the same folder name. Restoring
copies those files back into the directory recorded in the summary and
touches nothing else. It refuses a project that a run is working in; see
[Project locks](#project-locks).

## Project locks

`locks/<hash>.lock` marks a project that a Deptide process is writing to.
`<hash>` is derived from the project's normalized folder, so the desktop app
and the CLI use the same file for the same project. Locks belong to one
workspace: two workspaces that list the same folder keep separate locks and do
not exclude each other. The file holds the owner's process number (on Windows
also its start time), a description such as `the run "nightly" in deptide-cli`,
the time it was taken and the folder.

- A real run takes the lock of every project before its first step and gives
  each one back when that project's job ends: when it finishes, fails or is
  stopped, and at the latest when the run ends. In release builds an internal
  error (a Rust panic) closes Deptide, but only after it has killed the
  process tree of every running npm or shell command, in every run, so on no
  operating system does an npm process keep writing after its lock is free.
  From the moment the panic starts, no new command is started. Deptide waits
  up to 2 seconds for commands that are being started, then kills every tree
  that has not finished. A command that finishes while the crash is under way
  still counts as running: its tree is killed with the rest, its Job Object
  handle stays open and on Linux and macOS its process stays unreaped until
  Deptide has closed, so the kill does not reach a reused handle or process
  number, and its project lock is not given back. When several threads panic
  at once, none of them lets Deptide close while another is still killing
  trees, for up to 5 seconds. A command that finished before the panic is
  treated as after any normal finish: processes it left running in the
  background are not killed. A command still being started
  when the 2 seconds are up is never let run unprotected. On Windows it is
  still suspended and is never resumed; if it was already in its Job Object
  it ends when Deptide closes, otherwise it stays suspended. On Linux and
  macOS it has recorded its process number before it runs the command, so it
  is killed with the rest, or it sees the crash and does not start. One
  exception remains there: if starting the program fails after it recorded
  its number, the number stays recorded until Deptide's own thread clears it,
  and a crash in that time sends `SIGKILL` to the process group with that
  number, which by then may belong to an unrelated program. The Linux and
  macOS behavior is checked by compilation only. One Deptide process runs at
  most 1024 commands at once, across all its runs. A command beyond that is
  not started and fails with this message:

  ```
  not started because 1024 commands are already running
  ```

  The lock files stay behind and the next run takes them over as stale, as
  described below. In development builds only the job whose task panicked
  fails: Deptide kills the process tree of its running command, then gives
  the lock back, projects that need it are skipped and the run continues. A
  job whose task is aborted is handled the same way. On Windows each command
  runs in its own Job Object, so the tree includes processes the command left
  running after its shell exited. Deptide waits up to 5 seconds for the Job
  Object to empty before it gives the lock back, and Windows ends all of them
  when Deptide itself is killed. On Linux and macOS each command runs in its
  own process group, which gets `SIGKILL` without that wait; a process that
  moved to another group or session is not reached. If Deptide is killed from
  outside there, the group keeps running while the operating system frees
  the lock. Stopping a project ends its tree too
  (`SIGTERM` on Linux and macOS) but does not wait for it to empty: the run
  waits for the command's own process to exit and its output to close, then
  gives the lock back. In per-step mode a project's job lasts until
  the run ends, because saved entries are put back at the end, so its lock is
  held until then. Several jobs in one folder share one lock. A project whose
  lock another process holds is skipped with the message below, and projects
  that need it are skipped as their library failed. A lock is never waited
  for, so runs cannot deadlock. Dry runs take no locks.

  ```
  not run because <owner> is working in this project (process <pid>, since <time>)
  ```

- A restore never waits for a lock. It takes the lock for the time it copies a
  project's files and reports a locked project as failed with
  `Not restored: <owner> is working in this project (…)`.
- A locks folder that cannot be written fails the run's or restore's claim
  with the system's own error, not with the message about another process.
- On Windows the lock file is created atomically and removed when the lock is
  given back. A lock whose process has ended, or whose process number now
  belongs to a process started at another time, is stale and taken over. The
  owner keeps the file open without delete sharing, so a live lock cannot be
  removed by another process. A lock file without a readable owner is taken
  over once it is ten seconds old.
- On Linux, macOS and other Unix systems the owner holds an operating-system
  lock (`flock`) on the lock file for as long as it owns it. A lock file that
  no process holds that way is free, whatever process number it names, so a
  crashed owner or a reused process number never keeps a project locked.

## Packs

`packs/<stamp>-<label>/` holds the tarballs `npm pack` wrote for one run's
packed libraries, one folder per library, such as
`acme+core-91484758/acme-core-3.1.0.tgz` for `@acme/core`. The folder name is
the package name with the scope's `@` dropped and `/` written as `+`, then a
dash and the first eight hex digits of the SHA-256 of the exact package name.
The digits keep names that differ only in case, such as `Acme-Core` and
`acme-core`, in separate folders on a case-insensitive file system. A run
that installs a packed library without saving leaves `package.json` alone, so
nothing points at these
files after the run; a run that saves points `package.json` and the lockfile
at one of them until it is restored. A folder's stamp is the time the run
started, not the time its pack job ran, and the file names are the ones
`npm pack` reported. The twenty newest run folders are kept, counting the
running one; a pack job removes the older ones after creating its run's
folder.

`settings.json` also carries `branchSuffixPattern`, the regular expression
that turns a library branch name into a version suffix (default
`^([A-Za-z]+-\d+)`).
