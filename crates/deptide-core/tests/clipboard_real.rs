mod common;

use common::{manifest, TempDir};
use deptide_core::domain::{ConfiguredProject, UpdateConfig};
use deptide_core::transfer::{copy_to_clipboard, inspect_clipboard};
use deptide_core::workspace::{save_config, Workspace};

#[test]
#[ignore = "touches the real system clipboard"]
fn staged_folders_round_trip_through_the_clipboard() {
    let root = TempDir::new("clipboard");
    root.write(
        "repos/web/package.json",
        &manifest("@acme/web", "1.0.0", &[]),
    );
    root.write("repos/web/src/index.ts", "export const a = 1;\n");
    root.write("repos/web/node_modules/dep/index.js", "dep\n");
    root.write("repos/web/.gitignore", "node_modules/\n");
    let tool = root.mkdir("tool");

    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../repos/web")],
        ..UpdateConfig::default()
    };
    save_config(&workspace, &config).unwrap();

    let result = copy_to_clipboard(&workspace, &config, &["web".to_string()], &[], &[]).unwrap();
    assert_eq!(result.projects[0].files, 3);
    let staged = result.projects[0].staged_path.clone().unwrap();
    assert!(std::path::Path::new(&staged).join("src/index.ts").exists());
    assert!(!std::path::Path::new(&staged).join("node_modules").exists());

    let contents = inspect_clipboard(&workspace, &config);
    let entry = contents
        .entries
        .iter()
        .find(|entry| entry.path.eq_ignore_ascii_case(&staged))
        .expect("the staged folder is on the clipboard");
    assert!(entry.is_project);
    assert_eq!(entry.package_name.as_deref(), Some("@acme/web"));
    assert_eq!(entry.suggested_project.as_deref(), Some("web"));
}

#[cfg(windows)]
fn clipboard_owner_process() -> u32 {
    use windows::Win32::System::DataExchange::GetClipboardOwner;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

    let Ok(window) = (unsafe { GetClipboardOwner() }) else {
        return 0;
    };
    let mut process = 0;
    unsafe { GetWindowThreadProcessId(window, Some(&mut process)) };
    process
}

#[cfg(windows)]
fn powershell(script: &str) -> std::process::Command {
    let mut command = std::process::Command::new("powershell.exe");
    command.args(["-NoProfile", "-NonInteractive", "-STA", "-Command", script]);
    command
}

#[cfg(windows)]
#[test]
#[ignore = "touches the real system clipboard"]
fn a_copy_replaces_what_another_program_left_on_the_clipboard() {
    use std::io::{BufRead, BufReader};
    use std::process::Stdio;

    let root = TempDir::new("clipboard-owner");
    root.write(
        "repos/web/package.json",
        &manifest("@acme/web", "1.0.0", &[]),
    );
    let tool = root.mkdir("tool");
    let workspace = Workspace::open(&tool).unwrap();
    let config = UpdateConfig {
        projects: vec![ConfiguredProject::new("web", "../repos/web")],
        ..UpdateConfig::default()
    };

    let mut owner = powershell(
        "Add-Type -AssemblyName System.Windows.Forms; \
         [Windows.Forms.Clipboard]::SetDataObject('left by another program', $false); \
         [Console]::Out.WriteLine('ready'); [Console]::Out.Flush(); \
         $end = (Get-Date).AddSeconds(30); \
         while ((Get-Date) -lt $end) { [Windows.Forms.Application]::DoEvents(); Start-Sleep -Milliseconds 20 }",
    )
    .stdout(Stdio::piped())
    .spawn()
    .unwrap();
    let mut ready = String::new();
    BufReader::new(owner.stdout.take().unwrap())
        .read_line(&mut ready)
        .unwrap();
    assert_eq!(ready.trim(), "ready");
    assert_eq!(clipboard_owner_process(), owner.id());

    let result = copy_to_clipboard(&workspace, &config, &["web".to_string()], &[], &[]);
    let owner_after_copy = clipboard_owner_process();
    let pasted = powershell(
        "Add-Type -AssemblyName System.Windows.Forms; \
         [Windows.Forms.Clipboard]::GetFileDropList() | ForEach-Object { [Console]::Out.WriteLine($_) }",
    )
    .output()
    .unwrap();
    let _ = owner.kill();
    let _ = owner.wait();

    let staged = result.unwrap().projects[0].staged_path.clone().unwrap();
    assert_ne!(
        owner_after_copy,
        owner.id(),
        "the other program still owns the clipboard, so clipboard sync tools ignore the copy"
    );
    let pasted = String::from_utf8_lossy(&pasted.stdout);
    assert!(
        pasted
            .lines()
            .any(|line| line.trim().eq_ignore_ascii_case(&staged)),
        "a paste sees {pasted:?} instead of {staged}"
    );
}
