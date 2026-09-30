# Transferring code through the clipboard

The Transfer screen moves whole projects between machines with the system
clipboard, which remote desktop sessions share. It has two tabs.

## Copy

1. Pick the projects (the list defaults to the wizard selection).
2. Review the ignore rules. Three layers apply, all in `.gitignore` syntax:
   - each project's own `.gitignore` (plus the global git excludes), which is
     what keeps `node_modules`, `dist` and similar out,
   - the patterns stored in `update-libs.json` under `transferIgnore`, edited
     in Settings and applied to every copy and receive. They show as chips;
     clicking one leaves that pattern out of this run only (the chip is
     struck through), clicking it again brings it back. On the Copy tab a
     click clears the preview, and the chips are disabled while a preview or
     copy runs. Copy and Receive keep
     their own set, it lives in memory until the app restarts and is never
     written to the settings,
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

Receive is a two-step wizard with a stepper header like the Update wizard.
Step 1, "Clipboard", is where the incoming folders are chosen. Step 2, "Review
& replace", is where the files are picked and replaced. A successful Analyze
moves to step 2. "Back", Alt+Left and a click on step 1 go back to step 1.
Alt+Right and a click on step 2 go forward, but only while a receive session
exists (see [Session](#session)); without one, step 2 cannot be reached.
Alt+Left and Alt+Right are ignored while the cursor is in a text area. A step
that can be clicked is also a button, so Tab reaches it and Enter or Space
opens it. Enter does not start Analyze; the button in the footer of step 1
does.

Step 1 holds everything that decides what is analyzed. While the Receive tab is
open, Deptide checks the clipboard every two seconds for folders. Each folder
that holds a `package.json` is matched to a configured project by folder name,
by the target folder name, or by the package name; the match can be changed or
set to "Do not receive". The configured ignore patterns show as chips that can
be switched off for this run, as on the Copy tab. Run-only ignore patterns can
be added here as well. The chips and the Analyze button are disabled while
Analyze runs, and only the answer to the latest Analyze is shown. The chips
and the run-only patterns exist on step 1 only and are read when Analyze
starts; switching a chip does not run Analyze again.

### Where the folders come from

The clipboard can hold folders in two ways, and Receive handles both without a
switch. A line above the folder list says which one it found:

- **A file list** (`CF_HDROP`), which Explorer and Deptide's own Copy put
  there: "Source: file list on the clipboard". Each folder shows its path and
  Analyze reads it where it is.
- **Files a remote desktop offers without a path** (Citrix, RDP): "Source:
  files from a remote desktop, not downloaded yet". Names and sizes are known,
  the contents are not, so each folder shows its file count and size instead
  of a path (the size is left out when a file came without one). Analyze first
  downloads the whole clipboard into a new folder under
  `%TEMP%\deptide-received\<stamp>`, one subfolder per folder on the
  clipboard; the five newest download folders are kept. The line then reads
  "Source: downloaded to <folder>", each folder shows where it landed, and the
  plan, the file preview and Replace work on the downloaded folders.

While the download runs, a progress bar shows files and bytes done out of the
total (a moving stripe while the total size is not known yet), with a Cancel
button. The bar appears with the first progress report, or after 300 ms if
none has come. Analyze and the ignore chips are
disabled from the moment Analyze is pressed, and the target lists until the
download ends. A
cancelled or failed download shows its error in the error banner and leaves
no plan; "The remote desktop stopped sending files" means nothing arrived for
60 seconds. Every Analyze on a remote-desktop clipboard asks the backend for the
download; on the same clipboard the backend returns
the earlier download at once, without a progress bar, as long as all its
folders still exist, and downloads again otherwise. If a folder on the
clipboard is missing from the download, the error banner says so and asks to
copy the folders again on the remote desktop; the session keeps its earlier plan, if it has one.

Other lines that can appear:

- "N files skipped because their names are not safe on Windows": these files
  are never downloaded (the rules are under
  [Files without a path on the clipboard](#files-without-a-path-on-the-clipboard)).
- "The clipboard is busy, trying again": another program held the clipboard.
  The folders and targets on screen, and the session, stay as they were until
  the next check reads it.
- The reason the clipboard could not be read. The folder list is emptied.

The clipboard check only updates step 1. When the clipboard changes (other
folders, or the same folders copied again in the remote session), the folder
list and the targets are rebuilt from the new clipboard, and the line above
the list reads "not downloaded yet" again for a remote-desktop clipboard. The
session on step 2 is left alone, see [Session](#session). A check that finds
the same clipboard, including one during a download, changes nothing.

Changing a target on step 1 drops the plan of that project from the session
while the clipboard is unchanged; Analyze brings it back.

Ignore files above a received folder, such as a stray `%TEMP%\.gitignore`, are
not applied to it. What applies is the ignore patterns, the `.gitignore` and
`.ignore` files inside the folder and its subfolders, `.git\info\exclude` of a
Git repository inside the folder, and this machine's global Git excludes file
(`core.excludesFile` from the user's Git config, otherwise
`.config\git\ignore` in the home folder).

### Session

A successful Analyze starts a receive session. It holds the plan, the ticks,
the chunk choices, the results of replaced projects and the kept download, and
it does not depend on what the clipboard holds afterwards:

- If the clipboard changes while a session exists, step 2 shows the notice
  "The clipboard has changed." with a "Back to Clipboard" button. The plan,
  ticks, choices and results stay as they were, and Replace still works on
  them.
- Analyze on the same clipboard refreshes the plan and keeps the work: the
  results of projects already replaced, the ticks of files whose status did
  not change, and the chunk choices of files whose status and both sides did
  not change.
- Analyze on a different clipboard replaces the session. When at least one
  project is not replaced yet, a dialog "Discard the current receive?" says how
  many projects are not replaced and asks for "Discard and analyze" or
  "Cancel". When every project is replaced or closed, the session is replaced
  without asking.
- A failed Analyze (an error, a cancelled or failed download) shows its error
  and keeps the old session.
- "Receive more", shown in the footer of step 2 once every project is done,
  ends the session and returns to step 1.
- Leaving the Receive tab or the page keeps the session. Restarting the app
  ends it.

### Analyze

"Analyze" compares the incoming files with the target project and gives each
file one status:

| status          | meaning                                                          | pill |
| --------------- | ---------------------------------------------------------------- | ---- |
| new             | not in the target project                                        | `+N` |
| replaced        | content differs                                                  | `~N` |
| whitespace only | differs only in whitespace                                       | `≈N` |
| identical       | same size and SHA-256                                            |      |
| removed         | in the target project but not in the received folder (see below) | `−N` |

Removed files are judged with the same ignore rules as the received files, so
build output and `node_modules` never show up as removed.

Each project with at least one file that is not identical gets a tab. Its
label shows the project name and one pill per status with a count above zero,
counted over the whole project. Projects where every file is identical get no
tab; they are named in one collapsed "No changes" row and are never part of a
replace. The cross on a tab takes the project out of this receive: its target
becomes "Do not receive" and its plan is dropped. Picking a target again and
running Analyze brings it back. The Left and Right arrow keys move between
tabs; the cross is left out of the keyboard order, and "Do not receive" in the
folder list does the same from the keyboard.

New, replaced and removed files start ticked; whitespace-only and identical
files do not. The chips "Only changes", "All" and "None" reset the ticks of
one tab; "All" ticks the files the tab currently shows, so identical files
only while "Only affected" is off. The tab toolbar also holds three view options, shared by all tabs and
remembered between launches:

- List or Tree. The tree folds folders with a chevron; a folder checkbox ticks
  or unticks every visible file below it and shows a dash when only some are
  ticked. Folders start expanded when they hold a file that is not identical.
  Folder rows show the pills of their subtree.
- Only affected (on by default) hides identical files. In the tree, folders
  with nothing left to show disappear. Turning it on also unticks identical
  files in every tab, so a replace never includes a file that is not shown.
- Sort, in the list only: by path, or by change, which groups the files as
  replaced, whitespace only, new, removed, identical, each group in path
  order.

"Replace selected in this project" applies one tab, "Replace selected in all
projects" applies every tab that has not been applied yet. Ticked files are
copied into the target folder; ticked removed files are deleted from it. When
the replace includes at least one removed file, a dialog first lists those
files per project; "Delete and replace" goes ahead, "Cancel" changes nothing.
Every file that is overwritten or deleted is moved to the Recycle Bin first,
so a replace can be undone from there. If moving to the Recycle Bin fails,
nothing is copied into that project.

Projects are applied in order and a replace stops at the first project that
fails, so the ones before it are already changed. After a failed replace
Deptide runs Analyze again for the projects on screen, with the folders and
ignore patterns of the session, and keeps the error visible; the tabs then
show what is left to do. This re-analysis never downloads again, and it keeps
the ticks, chunk choices and results as Analyze on the same clipboard does.

An applied tab turns into its result: counts of new, replaced, deleted and
recycled files, the paths that failed validation and were left alone, and the
copied and deleted files. A file received in part is listed under "Copied"
with "N of M chunks" and counts as replaced. A file received in part that
changed on either side since its preview was read is left alone and listed
under "Changed since Analyze, not replaced. Analyze again." Other tabs keep their plan. "Close" on the result
takes the project out of this receive, like the cross on its tab. `transfers/<stamp>-receive.json` records every replace.

Paths containing `..` are rejected, so a crafted folder on the clipboard cannot
write outside the target project.

### File preview

Step 2 fills the height of the window: the file list and the preview scroll
inside it. Clicking a file row opens a preview to the right of the list, inside
the tab. A divider between the two can be dragged; with the divider focused,
Left and Right move it by 2%, Shift with either by 10%, Home and End move it
to the smallest and largest list, and a double-click puts it back to the
default (32% for the list). With Alt held, Left and Right switch wizard steps
and leave the divider alone. The list keeps at least 220px and the preview at
least 360px. The position is remembered between launches. When the file area
is narrower than 1100px the preview sits under the list instead, and there is
no divider. Without an open preview the list takes the whole width. The
previewed row is highlighted. Clicking a checkbox only ticks the file. The
cross in the preview header closes it.

The list is announced as a listbox (a tree in the tree view) whose active row
follows the keyboard. The list itself is a Tab stop, and so is each checkbox in
its rows; in the tree each folder chevron and folder checkbox is one too. Each
row is announced once by its path (its name in the tree) and status, and the
previewed row is described as shown in the preview. With the list focused,
the up and down arrow keys move through the visible rows, and the preview
follows while it is open. In the list the rows are the files. In the tree
folders are rows too: the preview stays on its file while the cursor is on a
folder, Right expands a collapsed folder or moves into an expanded one, Left
collapses an expanded folder or moves to the parent folder, and Enter expands
or collapses the folder. Enter on a file opens the preview for it and Escape
closes it. Escape inside the preview closes it too and puts focus back on the
list.

What the preview shows depends on the status:

- replaced: a diff, the file on this machine against the received one, with
  three lines of context around each change. Longer unchanged runs fold into
  an "N unchanged lines" row that expands on click. Changed words inside a
  changed line are highlighted.
- whitespace only: the same diff with "Hide whitespace changes" switched on.
  Hiding ignores all whitespace inside lines, line endings and blank lines
  that were only added or removed, like `git diff -w`. With the switch off,
  changed lines whose line endings differ show the ending (␍␊, ␊, ␍, or ⊘ for
  no final line break). The switch is also offered for replaced files, off
  by default.
- new: the received file, every line green.
- removed: the file on this machine, every line red.
- identical: the file once, without colours.

Diffs can be shown Unified or Side by side; the choice is shared by all tabs
and remembered between launches. Line numbers of both sides are shown, and a
note appears when only one side starts with a byte order mark. TypeScript,
JavaScript, JSON, CSS, SCSS, HTML and Vue, Markdown and YAML files are syntax
highlighted, picked by extension. Highlighting is switched off, with a note,
when either side is over 256 KiB.

A side with a NUL byte in its first 8 KiB is shown as "Binary file" with its
size, and a side over 1 MiB as "File too large to preview". Other text is
decoded as UTF-8, with invalid bytes replaced.

The line diff gives up when more than 1000 lines would be inserted or removed,
and then shows the changed middle of the file as one removed block followed
by one added block, so a rewritten file does not stall the window. A preview
that would show more than 5000 rows shows a notice with "Show diff anyway"
instead, and renders only after that click. Rows are counted as the current
view shows them: collapsed gaps do not count, and in Side by side a removed and
an added line that share a row count once. Opening a gap or switching the view
so that more rows would show than the 5000, or than the count already shown
anyway, brings the notice back.

The preview reads both files through the `read_receive_file` command, which
rejects paths that `safe_relative` would reject during a replace. The last 20
files read are cached per received folder and path until the next Analyze.

#### Taking some changes

A replaced or whitespace-only file can be received in part. Each chunk of its
diff, a run of removed lines followed by the added lines that replace them, has
a pill above it: "Take" or "Keep mine". A replaced file starts with every chunk
taken, a whitespace-only file with none. "Take all" and "Keep all" in the
preview header set every chunk. With focus anywhere in the preview, `n` and `p`
move to the next and previous pill (the first or last one when no pill is
focused), and Space toggles the focused pill. A screen reader hears the pill as
"Chunk 2 of 5: Take", so the state is part of its name.

The file's checkbox follows the pills: ticked when every chunk is taken,
unticked when none is, and a dash with "2/5 chunks" in the row otherwise.
Ticking or unticking the file, a folder above it, or a selection chip sets
every chunk again. A file received in part is rebuilt from this machine's
lines with the taken chunks replaced by the received lines, and keeps this
machine's byte order mark.

With "Hide whitespace changes" on, a pill sits on each visible change and
controls every chunk of the exact diff it overlaps, showing "Mixed" when
those differ. Chunks with only whitespace changes are hidden, and turning the
switch on sets them to "Keep mine". A whitespace-only file opens with the
switch on, so it shows no pills until the switch is turned off.

There are no pills when either side is binary, too large to preview, not
valid UTF-8, or when the line diff gave up (see above); a note says the file
is received whole. Analyze again keeps the chunks picked for a file only when
its status is the same and both of its sides are byte for byte unchanged;
otherwise the file goes back to
its default tick. The preview also ignores the chunks picked for a file when
the file it reads now has other hashes than when they were picked (the file
changed on disk and the preview read it again): the pills start from the
default tick, and a click saves a new pick on the chunks shown. The file row
keeps showing the old pick until then, and Replace reports such a file as
stale instead of writing it.

## Platform note

On Windows the file list goes through the native clipboard (`CF_HDROP`), so
Explorer can paste what Deptide copied and Deptide can receive what Explorer
copied. On Linux and macOS the `arboard` crate is used: folders are put on the
clipboard as `text/uri-list` (X11 and Wayland), which is what Deptide reads on
the other side and what most file managers offer when they copy files. Because
X11 and Wayland clipboards are served by the owning process, the copied
folders stay available while Deptide is running.

## Files without a path on the clipboard

A remote desktop client such as Citrix Workspace can offer copied files
without `CF_HDROP`: `FileGroupDescriptorW` lists relative names, sizes and
folder flags, and `FileContents` hands out the bytes one file at a time. The
core library reads these on Windows, and Receive downloads them as described
under [Where the folders come from](#where-the-folders-come-from).

- `read_clipboard_files` returns `Paths` when `CF_HDROP` is present, and
  `Virtual` with the file list otherwise. It never reads file contents.
- `extract_virtual` writes the files into an empty folder, normally the one
  `create_receive_directory` makes under `%TEMP%\deptide-received`. On any
  error it removes that folder.
- The listing keeps the clipboard sequence number read before the file list.
  Reads check it before and after every file, and fail with "The clipboard
  changed, analyze again" rather than mix in files from a newer clipboard.
- Names that are empty or absolute, carry a `:`, or have a segment made only
  of dots and spaces are skipped and counted in `rejected`.
- Other programs open the clipboard for up to about 200 ms after each change
  (seen here: VBoxTray, Explorer, the Citrix client `wfica32.exe`, svchost).
  ole32 then answers `CLIPBRD_E_CANT_OPEN` or `DV_E_FORMATETC`, and keeps the
  failure for as long as the data object from `OleGetClipboard` lives. Each
  read therefore drops that object and fetches a new one, `BUSY_ATTEMPTS`
  times, `BUSY_PAUSE` apart (about one second in all).
