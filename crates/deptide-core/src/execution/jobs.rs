use std::collections::{BTreeMap, HashMap, HashSet};

use serde::Serialize;

use crate::domain::{ConfiguredProject, Job, PackageSpec, RunPlan, StepName, UpdateConfig};
use crate::scan::read_package_manifest;
use crate::workspace::Workspace;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JobBuildOutcome {
    #[serde(skip)]
    pub jobs: Vec<Job>,
    pub missing: Vec<String>,
    pub without_packages: Vec<String>,
}

struct ProjectManifest {
    package_name: Option<String>,
    dependencies: BTreeMap<String, String>,
    dev_dependencies: BTreeMap<String, String>,
}

impl ProjectManifest {
    fn declares(&self, name: &str) -> bool {
        self.dependencies.contains_key(name) || self.dev_dependencies.contains_key(name)
    }

    fn is_dev(&self, name: &str) -> bool {
        self.dev_dependencies.contains_key(name) && !self.dependencies.contains_key(name)
    }
}

fn read_manifest(workspace: &Workspace, project: &ConfiguredProject) -> Option<ProjectManifest> {
    let directory = workspace.resolve_project(&project.path);
    read_package_manifest(&directory).map(|manifest| ProjectManifest {
        package_name: manifest.name,
        dependencies: manifest.dependencies,
        dev_dependencies: manifest.dev_dependencies,
    })
}

fn allowed_by_config(project: &ConfiguredProject, name: &str) -> bool {
    project
        .packages
        .as_ref()
        .map(|allowed| allowed.iter().any(|entry| entry == name))
        .unwrap_or(true)
}

fn packages_for_project(
    project: &ConfiguredProject,
    manifest: Option<&ProjectManifest>,
    plan: &RunPlan,
    declared_somewhere: &[bool],
) -> Vec<PackageSpec> {
    plan.packages
        .iter()
        .enumerate()
        .filter(|(_, spec)| allowed_by_config(project, &spec.name))
        .filter(|(index, spec)| match manifest {
            Some(manifest) => manifest.declares(&spec.name) || !declared_somewhere[*index],
            None => true,
        })
        .map(|(_, spec)| PackageSpec {
            name: spec.name.clone(),
            version: spec.version.clone(),
            save_dev: manifest
                .map(|manifest| manifest.is_dev(&spec.name))
                .unwrap_or(spec.save_dev),
        })
        .collect()
}

fn install_args(project: &ConfiguredProject, config: &UpdateConfig, plan: &RunPlan) -> Vec<String> {
    let base = project
        .install_args
        .clone()
        .unwrap_or_else(|| config.install_args.clone());

    let mut args = base;
    for extra in &plan.extra_install_args {
        if !args.contains(extra) {
            args.push(extra.clone());
        }
    }
    args
}

fn local_dependencies(
    manifest: Option<&ProjectManifest>,
    providers: &HashMap<String, String>,
    own_name: &str,
) -> Vec<String> {
    let Some(manifest) = manifest else {
        return Vec::new();
    };

    let mut names: Vec<String> = manifest
        .dependencies
        .keys()
        .chain(manifest.dev_dependencies.keys())
        .filter_map(|package| providers.get(package))
        .filter(|provider| provider.as_str() != own_name)
        .cloned()
        .collect();
    names.sort();
    names.dedup();
    names
}

pub fn order_by_dependencies(jobs: Vec<Job>) -> Vec<Job> {
    let mut remaining: Vec<Job> = jobs;
    let mut ordered: Vec<Job> = Vec::new();
    let mut placed: HashSet<String> = HashSet::new();

    loop {
        let (ready, rest): (Vec<Job>, Vec<Job>) = remaining
            .into_iter()
            .partition(|job| job.depends_on.iter().all(|name| placed.contains(name)));
        remaining = rest;

        if ready.is_empty() {
            break;
        }

        for job in ready {
            placed.insert(job.name.clone());
            ordered.push(job);
        }
    }

    for mut job in remaining {
        job.depends_on.retain(|name| placed.contains(name));
        ordered.push(job);
    }

    ordered
}

pub fn build_jobs(workspace: &Workspace, config: &UpdateConfig, plan: &RunPlan) -> JobBuildOutcome {
    let mut missing = Vec::new();
    let mut selected: Vec<(&ConfiguredProject, Option<ProjectManifest>)> = Vec::new();

    for name in &plan.project_names {
        match config.projects.iter().find(|project| &project.name == name) {
            Some(project) => selected.push((project, read_manifest(workspace, project))),
            None => missing.push(name.clone()),
        }
    }

    let declared_somewhere: Vec<bool> = plan
        .packages
        .iter()
        .map(|spec| {
            selected
                .iter()
                .any(|(_, manifest)| manifest.as_ref().is_some_and(|m| m.declares(&spec.name)))
        })
        .collect();

    let providers: HashMap<String, String> = selected
        .iter()
        .filter_map(|(project, manifest)| {
            let package_name = manifest.as_ref()?.package_name.clone()?;
            Some((package_name, project.name.clone()))
        })
        .collect();

    let mut jobs = Vec::new();
    let mut without_packages = Vec::new();

    for (project, manifest) in &selected {
        let packages = packages_for_project(project, manifest.as_ref(), plan, &declared_somewhere);

        if packages.is_empty() {
            without_packages.push(project.name.clone());
            continue;
        }

        jobs.push(Job {
            name: project.name.clone(),
            directory: workspace.resolve_project(&project.path),
            packages,
            steps: plan.steps.clone(),
            install_args: install_args(project, config, plan),
            audit_fix_args: config.audit_fix_args.clone(),
            depends_on: local_dependencies(manifest.as_ref(), &providers, &project.name),
            command: None,
        });
    }

    let job_names: HashSet<String> = jobs.iter().map(|job| job.name.clone()).collect();
    for job in &mut jobs {
        job.depends_on.retain(|name| job_names.contains(name));
    }

    JobBuildOutcome {
        jobs: order_by_dependencies(jobs),
        missing,
        without_packages,
    }
}

pub fn build_command_jobs(
    workspace: &Workspace,
    config: &UpdateConfig,
    project_names: &[String],
    command: &str,
) -> JobBuildOutcome {
    let mut missing = Vec::new();
    let mut jobs = Vec::new();

    for name in project_names {
        match config.projects.iter().find(|project| &project.name == name) {
            Some(project) => jobs.push(Job {
                name: project.name.clone(),
                directory: workspace.resolve_project(&project.path),
                packages: Vec::new(),
                steps: Vec::new(),
                install_args: Vec::new(),
                audit_fix_args: Vec::new(),
                depends_on: Vec::new(),
                command: Some(command.to_string()),
            }),
            None => missing.push(name.clone()),
        }
    }

    JobBuildOutcome {
        jobs,
        missing,
        without_packages: Vec::new(),
    }
}

pub fn collect_package_specs(jobs: &[Job]) -> Vec<String> {
    let mut specs: Vec<String> = Vec::new();

    for job in jobs {
        for package in &job.packages {
            let spec = package.to_spec();
            if !specs.contains(&spec) {
                specs.push(spec);
            }
        }
    }

    specs
}

pub fn collect_steps(jobs: &[Job]) -> Vec<StepName> {
    let mut steps: Vec<StepName> = Vec::new();

    for job in jobs {
        for step in &job.steps {
            if !steps.contains(step) {
                steps.push(*step);
            }
        }
    }

    steps
}
