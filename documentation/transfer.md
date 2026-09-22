# Transferring code through the clipboard

The Transfer screen moves whole projects between machines with the system
clipboard, which remote desktop sessions share. It has two tabs.

## Copy

1. Pick the projects (the list defaults to the wizard selection).
2. Review the ignore rules. Three layers apply, all in `.gitignore` syntax:
   - each project's own `.gitignore` (plus the global git excludes), which is
     what keeps `node_modules`, `dist` and similar out,
   - the patterns stored in `update-libs.json` under `transferIgnore`, edited
     in Settings and applied to every copy and receive,
   - the patterns typed on the Copy tab, which apply to this copy only and are
     not saved.
     The `.git` folder is never copied.
3. "Preview" lists per project how many files and bytes would go and how many
   files the patterns skipped. "Copy to clipboard" stages a filtered copy of
   each project under `%TEMP%\deptide-transfer\<stamp>\<project>` and puts
   those folders on the clipboard as files. From there they can be pasted in
   Explorer or received by Deptide on the other side. The five newest staging
   folders are kept.

The result panel shows what was copied per project, and the same record is
written to `transfers/<stamp>-copy.json` in the workspace.

## Receive

While the Receive tab is open, Deptide checks the clipboard every two seconds
for folders. Each folder that holds a `package.json` is matched to a configured
project by folder name, by the target folder name, or by the package name; the
match can be changed or set to "Do not receive". Run-only ignore patterns can
be added here as well.

"Analyze" compares the incoming files with the target project and classifies
each as new, replaced or identical (by size and SHA-256). New and replaced
files are ticked, identical ones are not; any file can be unticked. "Replace
selected files" copies exactly the ticked files into the target folders and
never deletes anything. Files that exist in the target project but not in the
received folder are listed separately under "only on this machine" (judged with
the same ignore rules, so build output and `node_modules` do not appear); they
may have been deleted on the other side and are candidates for manual removal
after the merge. The result panel lists what landed per project, and
`transfers/<stamp>-receive.json` records the same.

Paths containing `..` are rejected, so a crafted folder on the clipboard cannot
write outside the target project.

## Platform note

On Windows the file list goes through the native clipboard (`CF_HDROP`), so
Explorer can paste what Deptide copied and Deptide can receive what Explorer
copied. On Linux and macOS the `arboard` crate is used: folders are put on the
clipboard as `text/uri-list` (X11 and Wayland), which is what Deptide reads on
the other side and what most file managers offer when they copy files. Because
X11 and Wayland clipboards are served by the owning process, the copied
folders stay available while Deptide is running.
