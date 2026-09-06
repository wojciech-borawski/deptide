use crate::domain::{Job, PackageSpec, StepName};
use crate::scan::BUILD_SCRIPT_NAME;
use crate::util::text::describe_count;

pub struct PlannedCommand {
    pub label: String,
    pub args: Vec<String>,
}

pub fn group_by_save_dev(packages: &[PackageSpec]) -> Vec<(bool, Vec<String>)> {
    [false, true]
        .into_iter()
        .map(|save_dev| {
            let specs: Vec<String> = packages
                .iter()
                .filter(|spec| spec.save_dev == save_dev)
                .map(PackageSpec::to_spec)
                .collect();
            (save_dev, specs)
        })
        .filter(|(_, specs)| !specs.is_empty())
        .collect()
}

pub fn plan_uninstall(job: &Job) -> PlannedCommand {
    let names: Vec<String> = job.packages.iter().map(|spec| spec.name.clone()).collect();
    let label = format!(
        "uninstall {}",
        describe_count(names.len(), names.first().map(String::as_str), "packages")
    );

    let mut args = vec!["uninstall".to_string()];
    args.extend(names);
    PlannedCommand { label, args }
}

pub fn plan_install(job: &Job, force: bool) -> Vec<PlannedCommand> {
    group_by_save_dev(&job.packages)
        .into_iter()
        .map(|(save_dev, specs)| {
            let label = format!(
                "{} {}",
                if force { "force install" } else { "install" },
                describe_count(specs.len(), specs.first().map(String::as_str), "packages")
            );

            let mut args = vec!["install".to_string()];
            args.extend(specs);
            if save_dev {
                args.push("--save-dev".to_string());
            }
            if force && !job.install_args.iter().any(|arg| arg == "--force") {
                args.push("--force".to_string());
            }
            args.extend(job.install_args.iter().cloned());
            PlannedCommand { label, args }
        })
        .collect()
}

pub fn plan_audit_fix(job: &Job) -> PlannedCommand {
    let mut args = vec!["audit".to_string(), "fix".to_string()];
    args.extend(job.audit_fix_args.iter().cloned());
    PlannedCommand {
        label: "audit fix".to_string(),
        args,
    }
}

pub fn plan_build() -> PlannedCommand {
    PlannedCommand {
        label: "build".to_string(),
        args: vec!["run".to_string(), BUILD_SCRIPT_NAME.to_string()],
    }
}

pub fn dry_run_lines(job: &Job, step: StepName) -> Vec<String> {
    match step {
        StepName::Uninstall => {
            let names: Vec<&str> = job.packages.iter().map(|spec| spec.name.as_str()).collect();
            vec![format!("would uninstall {}", names.join(" "))]
        }
        StepName::Install | StepName::ForceInstall => {
            let force = if step == StepName::ForceInstall {
                " --force"
            } else {
                ""
            };
            job.packages
                .iter()
                .map(|spec| {
                    let flag = if spec.save_dev { " --save-dev" } else { "" };
                    format!("would install {}{flag}{force}", spec.to_spec())
                })
                .collect()
        }
        StepName::Audit => vec!["would run npm audit fix".to_string()],
        StepName::Build => vec!["would run npm run build".to_string()],
    }
}
