# Manual check: receiving from a Citrix session

Run this on the machine that connects to the Citrix (DDoD) desktop, with the
portable build (`npm run build:portable`, then `release\Deptide.exe`). It
checks that Receive can take folders copied inside the remote session, which
reach the local clipboard without a path. How it works is described in
[transfer.md](../transfer.md#where-the-folders-come-from).

Before starting, open a workspace in Deptide that has at least two projects
you can overwrite safely (every replaced file goes to the Recycle Bin, but a
scratch copy is better).

## Steps

1. In the remote session, select two project folders in Explorer and press
   Ctrl+C.
2. Locally, open Transfer, then the Receive tab. Expect within a few seconds:
   - the line "Source: files from a remote desktop, not downloaded yet",
   - both folder names, each with a file count and a size,
   - a suggested target for each folder whose name or package name matches a
     project.
3. Pick a target for each folder and press Analyze. Expect a progress bar with
   "Files: x of y" and the bytes, and a Cancel button. Analyze, the target
   lists and the ignore chips are disabled while it runs.
4. When the download ends, expect the line "Source: downloaded to
   C:\Users\<you>\AppData\Local\Temp\deptide-received\<stamp>", the downloaded
   path under each folder name, and the plan with one tab per project that has
   changes.
5. Press Analyze again. Expect the plan back without a second download (no
   progress bar).
6. Tick a few files and press Replace in one tab. Expect the result panel and
   the files changed in the target project.
7. Copy a normal local folder in Explorer on this machine. Expect "Source: file
   list on the clipboard" and the folder's real path under its name.
8. In the remote session, copy a folder large enough that its download takes
   several seconds (a few hundred MB), press Analyze, and while the progress
   bar runs copy something else in the remote session. Expect the error "The
   clipboard changed, analyze again" and no plan.
9. Copy the two remote folders again, press Analyze and then Cancel while it
   runs. Expect "The download was cancelled" and no plan. Analyze again works.
10. Open `%TEMP%\deptide-received` in Explorer and note how many folders it
    holds. Then copy the remote folders again and press Analyze, letting the
    download finish, and do this six times. Note the folder count again.
    Expect at most five folders, the newest ones.

## If a step fails

Send back:

- the step number and what you saw instead,
- the exact error text from the banner (copy it, do not retype it),
- the source line shown on the Receive tab at that moment,
- the clipboard formats your probe listed for the same copy, in particular
  whether `FileGroupDescriptorW` and `FileContents` were there,
- the newest lines of the error log (Settings shows where it is).
