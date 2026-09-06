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
  "packages": [{ "name": "@acme/core", "version": "3.1.0-ABC-123", "saveDev": false }],
  "projects": [
    { "name": "Shop-Frontend", "path": "../repos/shop/frontend" },
    {
      "name": "Legacy",
      "path": "../old/legacy",
      "skip": true,
      "packages": ["@acme/core"],
      "installArgs": ["--legacy-peer-deps"]
    }
  ]
}
```

- `projects[].path` is a POSIX style relative path, written by the scan.
- `skip` unticks the project by default in the wizard.
- `packages` restricts which libraries may be applied to that project.
- `installArgs` on a project replaces the top-level `installArgs`.
- Older shapes are accepted when reading: a package may be the string
  `name@version`, a project may be a bare path string.

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
other folder names listed in `scan/detect.rs`.

## Saved runs

A saved run records the selected project names, the exact package versions,
steps, order, concurrency and extra install flags. Loading one in the History
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

The newest 50 transcripts are kept, older ones are removed automatically.

## Backups

`backups/<stamp>-<label>/<project>/` holds the `package.json` and
`package-lock.json` of a project as they were before a real run touched it.
The Run screen restores them per project; the twenty newest run folders are
kept.

`settings.json` also carries `branchSuffixPattern`, the regular expression
that turns a library branch name into a version suffix (default
`^([A-Za-z]+-\d+)`).
