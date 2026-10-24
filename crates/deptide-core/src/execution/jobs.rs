use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use super::pack::add_pack_jobs;
use crate::dependencies::projects::group_providers;
use crate::domain::{
    BumpWhen, ConfiguredProject, DependencySection, Job, PackageManifest, PackageSpec, Reason,
    RunPlan, StepName, UpdateConfig, VersionPolicy,
};
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

fn read_manifest(workspace: &Workspace, project: &ConfiguredProject) -> Option<PackageManifest> {
    read_package_manifest(&workspace.resolve_project(&project.path))
}

fn allowed_by_config(project: &ConfiguredProject, name: &str) -> bool {
    project
        .packages
        .as_ref()
        .map(|allowed| allowed.iter().any(|entry| entry == name))
        .unwrap_or(true)
}

fn is_own_packed_library(manifest: Option<&PackageManifest>, spec: &PackageSpec) -> bool {
    spec.pack.is_some()
        && manifest.and_then(|manifest| manifest.name.as_deref()) == Some(spec.name.as_str())
}

fn packages_for_project(
    project: &ConfiguredProject,
    manifest: Option<&PackageManifest>,
    plan: &RunPlan,
    declared_somewhere: &[bool],
) -> Vec<PackageSpec> {
    plan.packages
        .iter()
        .enumerate()
        .filter(|(_, spec)| allowed_by_config(project, &spec.name))
        .filter(|(_, spec)| !is_own_packed_library(manifest, spec))
        .filter(|(index, spec)| match manifest {
            Some(manifest) => {
                manifest.declared().find(&spec.name).is_some() || !declared_somewhere[*index]
            }
            None => true,
        })
        .map(|(_, spec)| {
            let (save_dev, save_peer) = match manifest {
                Some(manifest) => match manifest
                    .declared()
                    .find(&spec.name)
                    .map(|(section, _)| section)
                {
                    Some(DependencySection::Dev) => (true, false),
                    Some(DependencySection::Peer) => (false, true),
                    Some(DependencySection::Dependencies) | None => (false, false),
                },
                None => (spec.save_dev, spec.save_peer),
            };
            PackageSpec {
                name: spec.name.clone(),
                version: spec.version.clone(),
                save_dev,
                save_peer,
                pack: spec.pack,
            }
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
    manifest: Option<&PackageManifest>,
    providers: &BTreeMap<&str, &str>,
    own_name: &str,
) -> Vec<String> {
    let Some(manifest) = manifest else {
        return Vec::new();
    };

    let mut names: Vec<String> = manifest
        .dependencies
        .keys()
        .chain(manifest.dev_dependencies.keys())
        .filter_map(|package| providers.get(package.as_str()))
        .filter(|provider| **provider != own_name)
        .map(|provider| provider.to_string())
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
    let mut selected: Vec<(&ConfiguredProject, Option<PackageManifest>)> = Vec::new();

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
            selected.iter().any(|(_, manifest)| {
                manifest
                    .as_ref()
                    .is_some_and(|m| m.declared().find(&spec.name).is_some())
            })
        })
        .collect();

    let providers = group_providers(selected.iter().filter_map(|(project, manifest)| {
        Some((manifest.as_ref()?.name.as_deref()?, project.name.as_str()))
    }))
    .single;

    let without_any_package = plan.packages.is_empty();
    let steps: Vec<StepName> = plan
        .steps
        .iter()
        .copied()
        .filter(|step| !(without_any_package && step.changes_packages()))
        .collect();
    let mut jobs = Vec::new();
    let mut without_packages = Vec::new();

    for (project, manifest) in &selected {
        if steps.is_empty() {
            break;
        }
        let packages = packages_for_project(project, manifest.as_ref(), plan, &declared_somewhere);

        if packages.is_empty() && !without_any_package {
            without_packages.push(project.name.clone());
            continue;
        }

        jobs.push(Job {
            name: project.name.clone(),
            directory: workspace.resolve_project(&project.path),
            packages,
            steps: steps.clone(),
            install_args: install_args(project, config, plan),
            audit_fix_args: config.audit_fix_args.clone(),
            depends_on: local_dependencies(manifest.as_ref(), &providers, &project.name),
            command: None,
            version: VersionPolicy {
                bump: plan
                    .project_bumps
                    .get(&project.name)
                    .copied()
                    .unwrap_or(plan.version.bump),
                when: plan.version.when,
            },
            packing: None,
        });
    }

    add_pack_jobs(workspace, config, plan, &mut jobs);
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

pub fn nothing_to_do_reason(plan: &RunPlan) -> Reason {
    if plan.packages.is_empty() {
        Reason::NoLibraries {}
    } else {
        Reason::NoProjectUsesPackages {}
    }
}

pub fn nothing_to_do(plan: &RunPlan) -> String {
    nothing_to_do_reason(plan).text()
}

pub fn describe_version_policy(plan: &RunPlan) -> String {
    let condition = match plan.version.when {
        BumpWhen::Always => "",
        BumpWhen::SameAsMain => " (only when still equal to main)",
        BumpWhen::NotBumpedOnBranch => " (only when not yet bumped on this branch)",
    };
    let exceptions: Vec<String> = plan
        .project_bumps
        .iter()
        .filter(|(_, bump)| **bump != plan.version.bump)
        .map(|(project, bump)| format!("{project} {}", bump.label()))
        .collect();
    let exceptions = if exceptions.is_empty() {
        String::new()
    } else {
        format!(", except {}", exceptions.join(", "))
    };
    format!("{}{condition}{exceptions}", plan.version.bump.label())
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
                version: VersionPolicy::default(),
                packing: None,
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
    if jobs.iter().any(|job| job.steps.contains(&StepName::Pack)) {
        steps.push(StepName::Pack);
    }

    for job in jobs {
        for step in &job.steps {
            if !steps.contains(step) {
                steps.push(*step);
            }
        }
    }

    steps
}
