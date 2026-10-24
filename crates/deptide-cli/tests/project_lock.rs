use std::path::PathBuf;
use std::process::{Command, Output};

use deptide_core::domain::{ExecutionMode, Job, StepName, VersionPolicy};
use deptide_core::execution::{lock_project, RunOptions, RunState};

struct Workspace {
    root: PathBuf,
}

impl Workspace {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("deptide-cli-test-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("web")).unwrap();
        std::fs::write(
            root.join("update-libs.json"),
            r#"{ "projects": [{ "name": "web", "path": "./web" }] }"#,
        )
        .unwrap();
        Self { root }
    }

    fn join(&self, relative: &str) -> PathBuf {
        self.root.join(relative)
    }

    fn read(&self, relative: &str) -> String {
        std::fs::read_to_string(self.join(relative)).unwrap()
    }

    fn write(&self, relative: &str, content: &str) {
        let target = self.join(relative);
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, content).unwrap();
    }

    fn with_backed_up_run(&self, run: &str, backed_up: &str) {
        let web = Job {
            name: "web".to_string(),
            directory: self.join("web"),
            packages: vec![],
            steps: vec![StepName::Build],
            install_args: vec![],
            audit_fix_args: vec![],
            depends_on: vec![],
            command: None,
            version: VersionPolicy::default(),
            packing: None,
        };
        let options = RunOptions {
            concurrency: 1,
            mode: ExecutionMode::PerProject,
            dry_run: false,
        };
        let log_file = self.join(&format!("logs/{run}.log"));
        let mut summary = RunState::new(run.to_string(), run.to_string(), vec![web], &options)
            .finish(false, Some(log_file.to_string_lossy().to_string()));
        summary.backup_folder = Some(run.to_string());
        self.write(
            &format!("logs/{run}.json"),
            &serde_json::to_string(&summary).unwrap(),
        );
        self.write(&format!("backups/{run}/web/package.json"), backed_up);
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn cli(arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_deptide-cli"))
        .args(arguments)
        .output()
        .expect("deptide-cli runs")
}

fn text(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

fn root_of(workspace: &Workspace) -> &str {
    workspace.root.to_str().unwrap()
}

fn locks_of(workspace: &Workspace) -> PathBuf {
    workspace.join("locks")
}

#[test]
fn restore_refuses_a_project_another_process_holds_and_restores_it_once_released() {
    let workspace = Workspace::new("restore-held");
    workspace.with_backed_up_run("past", "{\"version\":\"1.0.0\"}");
    workspace.write("web/package.json", "{\"version\":\"2.0.0\"}");
    let held = lock_project(&locks_of(&workspace), &workspace.join("web"), "desktop run")
        .expect("this test process holds the lock");

    let refused = cli(&["restore", root_of(&workspace), "past"]);
    let written_while_held = workspace.read("web/package.json");
    drop(held);
    let restored = cli(&["restore", root_of(&workspace), "past"]);

    assert_eq!(written_while_held, "{\"version\":\"2.0.0\"}");
    assert_eq!(refused.status.code(), Some(1), "{}", text(&refused));
    assert!(
        text(&refused)
            .contains("failed   web: Not restored: desktop run is working in this project"),
        "{}",
        text(&refused)
    );
    assert_eq!(restored.status.code(), Some(0), "{}", text(&restored));
    assert_eq!(
        workspace.read("web/package.json"),
        "{\"version\":\"1.0.0\"}"
    );
}

const HOLD_SCRIPT: &str = "const fs = require('node:fs'); const start = Date.now(); \
fs.writeFileSync('started', 'yes'); \
setInterval(() => { if (fs.existsSync('stop') || Date.now() - start > 60000) process.exit(0); }, 50);";

fn wait_for(path: &std::path::Path) {
    for _ in 0..600 {
        if path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("{} never appeared", path.display());
}

#[test]
fn restore_refuses_while_a_cli_run_works_in_the_project_and_takes_over_its_lock_once_that_process_died(
) {
    let workspace = Workspace::new("restore-vs-run");
    workspace.with_backed_up_run("past", "{\"version\":\"1.0.0\"}");
    workspace.write("web/package.json", "{\"version\":\"2.0.0\"}");
    workspace.write("web/hold.cjs", HOLD_SCRIPT);
    let mut running = Command::new(env!("CARGO_BIN_EXE_deptide-cli"))
        .args([
            "exec",
            root_of(&workspace),
            "-P",
            "web",
            "--quiet",
            "--",
            "node",
            "hold.cjs",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("deptide-cli exec starts");
    wait_for(&workspace.join("web/started"));

    let refused = cli(&["restore", root_of(&workspace), "past"]);
    let written_while_running = workspace.read("web/package.json");
    running.kill().expect("the run's process is ended abruptly");
    running.wait().unwrap();
    workspace.write("web/stop", "stop");
    let after_crash = cli(&["restore", root_of(&workspace), "past"]);

    assert_eq!(written_while_running, "{\"version\":\"2.0.0\"}");
    assert!(
        text(&refused).contains("failed   web: Not restored: the run \"cmd: node hold.cjs\" in deptide-cli is working in this project"),
        "{}",
        text(&refused)
    );
    assert_eq!(after_crash.status.code(), Some(0), "{}", text(&after_crash));
    assert_eq!(
        workspace.read("web/package.json"),
        "{\"version\":\"1.0.0\"}"
    );
}
