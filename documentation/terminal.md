# Running commands in many projects

The Terminal screen is the third mode next to Run and Transfer. It takes one
command line and runs it in every ticked project at the same time, each in its
own folder, and shows the progress per project.

## Using it

1. Tick projects on the left. The list starts with the wizard selection, can
   be filtered by name or by kind (library / application) and stays disabled
   while a command is running.
2. Type the command in the prompt and press Enter or click Run. Arrow up and
   down recall earlier commands; the last thirty are kept in the browser
   storage of the app and offered as chips while no run is shown.
3. The "parallel" field limits how many projects execute at once. The rest
   wait for a free slot, which the table shows as "waiting".
4. The table lists status, project, the last line of output and the elapsed
   time. Clicking a row shows that project's full output below; a row can be
   stopped on its own, and "Stop everything" aborts the whole run.
5. When every project has finished, a notice reports the total wall-clock time
   and the summed shell time.

## How it runs

- The line is handed to the platform shell: `cmd.exe /D /C <line>` on Windows
  and `sh -c <line>` elsewhere, with the project folder as the working
  directory. Anything the shell accepts works, including pipes and `&&`.
- Exit code 0 is ok. Any other code fails the project with
  "command exited with code N", and a command that cannot be started fails
  with the spawn error. Output is streamed line by line.
- A command run is a normal run for the backend: it uses the same process
  registry, abort handling, event stream and transcript logger, so it appears
  in the logs folder and in History labelled `cmd: <line>`. It does not take
  backups, verify installs or diagnose failures, because it does not know what
  the command does.
- Only one run of either kind can be active at a time; starting a command
  while an update run is in progress is refused.
