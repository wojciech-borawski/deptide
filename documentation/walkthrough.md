# Manual checks before a release

Run this list before tagging a release, in the mock preview
(`npm run web:mock`) and, for the rows marked native, in `release/Deptide.exe`
against a workspace with real projects. Tick each row only when it looks and
behaves as described in the documentation.

| screen            | check                                                                                                                                                       |
| ----------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Workspace         | recent list, open folder, feature list, version shown, error on a bad folder                                                                                |
| Wizard, Projects  | grouped list with `[…]` prefix button, filter, kind chips, select all, remove project dialog, scan dialog                                                   |
| Wizard, Libraries | candidates load, branch-derived version, manual package add with an invalid spec, filter                                                                    |
| Wizard, Options   | steps toggle in order, mode switch, concurrency, dry run, extra install arguments                                                                           |
| Wizard, Review    | plan summary, label suggestion, save as run, aligned inputs, Enter starts the run                                                                           |
| Run               | live rows, clock, progress bar, phase badge in step-by-step mode, log pane follows output, stop one project, stop everything                                |
| Run, finished     | summary card with total time, warnings and failures counted, restore files, rerun dialog with step choice, retry in wizard, report export                   |
| Terminal          | picker mirrors the wizard selection, command history with arrow keys, parallel field, waiting rows, click row shows its output, stop one row, finish notice |
| Transfer, Copy    | preview counts, copy result, per-run ignore patterns, global patterns link to settings (native: paste in Explorer)                                          |
| Transfer, Receive | clipboard folders appear only on this tab, target mapping, analyze classification, untick a file, apply result (native)                                     |
| History           | run list, trend charts with two or more real runs, load saved run into the wizard, delete saved run, open log                                               |
| Settings          | save without toolbar movement, saved notice, root folder dialog, steps and mode defaults, appearance switches, language switch, update check, about section |
| Rail              | active-run pulse on Run and Terminal, disabled items without a workspace, update badge                                                                      |
| Appearance        | light theme, compact density, Polish and German strings without `@` errors in the console                                                                   |
| Narrow window     | terminal and run screens wrap instead of overflowing at 760 px                                                                                              |
| Startup (native)  | `Deptide.exe <workspace>` opens that workspace, tray icon present, notification on finish                                                                   |
