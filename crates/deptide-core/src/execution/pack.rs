use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::Deserialize;
use sha2::{Digest, Sha256};

use super::bump::preview_version_step;
use super::logger::stamped_run_name;
use super::steps::{has_build_script, plan_build, PlannedCommand};
use crate::dependencies::projects::WorkspaceProjects;
use crate::domain::{
    DependencyCandidate, ExecutionMode, Job, PackMode, PackageSpec, PackedLibrary, PackedTarball,
    Packing, Reason, RunPlan, StepName, UpdateConfig, VersionPolicy,
};
use crate::workspace::Workspace;

const LIBRARY_FOLDER_SUFFIX_BYTES: usize = 4;

pub struct LibraryProject {
    pub project: String,
    pub directory: PathBuf,
    pub version: Option<String>,
}

#[derive(Default)]
pub struct PackableLibraries {
    pub libraries: BTreeMap<String, LibraryProject>,
    pub ambiguous: BTreeMap<String, Vec<String>>,
}

impl PackableLibraries {
    pub fn read(workspace: &Workspace, config: &UpdateConfig) -> Self {
        let read = WorkspaceProjects::read(workspace, config);
        let providers = read.providers();
        Self {
            libraries: providers
                .single
                .iter()
                .map(|(package, project)| {
                    let library = LibraryProject {
                        project: project.name().to_string(),
                        directory: project.directory.clone(),
                        version: project.detected.version.clone(),
                    };
                    (package.to_string(), library)
                })
                .collect(),
            ambiguous: providers
                .ambiguous
                .iter()
                .map(|(package, found)| {
                    let names = found.iter().map(|project| project.name().to_string());
                    (package.to_string(), names.collect())
                })
                .collect(),
        }
    }

    pub fn refusal(&self, package: &str) -> Reason {
        refusal(package, self.ambiguous.get(package))
    }
}

fn refusal(package: &str, ambiguous: Option<&Vec<String>>) -> Reason {
    match ambiguous {
        Some(projects) => Reason::PackedLibraryAmbiguous {
            name: package.to_string(),
            projects: projects.join(", "),
        },
        None => Reason::PackedLibraryMissing {
            name: package.to_string(),
        },
    }
}

fn pack_job(library: &LibraryProject) -> Job {
    Job {
        name: library.project.clone(),
        directory: library.directory.clone(),
        packages: Vec::new(),
        steps: vec![StepName::Pack],
        install_args: Vec::new(),
        audit_fix_args: Vec::new(),
        depends_on: Vec::new(),
        command: None,
        version: VersionPolicy::default(),
        packing: None,
    }
}

fn packed_version(library: &LibraryProject, library_job: Option<&Job>) -> Option<String> {
    match library_job.filter(|job| runs_before_packing(job, StepName::Version)) {
        Some(job) => preview_version_step(job)
            .0
            .version_after()
            .map(str::to_string),
        None => library.version.clone(),
    }
}

pub(super) fn add_pack_jobs(
    workspace: &Workspace,
    config: &UpdateConfig,
    plan: &RunPlan,
    jobs: &mut Vec<Job>,
) {
    let packed: Vec<&str> = plan
        .packages
        .iter()
        .filter(|spec| spec.pack.is_some())
        .map(|spec| spec.name.as_str())
        .collect();
    if packed.is_empty() {
        return;
    }

    let packable = PackableLibraries::read(workspace, config);
    let libraries: BTreeMap<&str, &LibraryProject> = packed
        .iter()
        .filter_map(|name| Some((*name, packable.libraries.get(*name)?)))
        .collect();

    for library in libraries.values() {
        match jobs.iter_mut().find(|job| job.name == library.project) {
            Some(job) => match plan.mode {
                ExecutionMode::PerProject => job.steps.push(StepName::Pack),
                ExecutionMode::PerStep => job.steps.insert(0, StepName::Pack),
            },
            None => jobs.push(pack_job(library)),
        }
    }

    let packing = Packing {
        folder: workspace
            .packs_directory()
            .join(stamped_run_name(Utc::now(), &plan.label)),
        libraries: libraries
            .iter()
            .map(|(name, library)| {
                let library_job = jobs.iter().find(|job| job.name == library.project);
                let packed = PackedLibrary {
                    directory: library.directory.clone(),
                    tarball: packed_version(library, library_job)
                        .map(|version| packed_tarball(name, version)),
                };
                (name.to_string(), packed)
            })
            .collect(),
        ambiguous: packable
            .ambiguous
            .iter()
            .filter(|(name, _)| packed.contains(&name.as_str()))
            .map(|(name, projects)| (name.clone(), projects.clone()))
            .collect(),
    };

    for job in jobs.iter_mut() {
        for spec in job.packages.iter().filter(|spec| spec.pack.is_some()) {
            if let Some(library) = libraries.get(spec.name.as_str()) {
                if !job.depends_on.contains(&library.project) {
                    job.depends_on.push(library.project.clone());
                }
            }
        }
        let installs_a_pack = job.packages.iter().any(|spec| spec.pack.is_some());
        if installs_a_pack || job.steps.contains(&StepName::Pack) {
            job.packing = Some(packing.clone());
        }
    }
}

pub struct PackPlan {
    pub notes: Vec<Reason>,
    pub build: Option<PlannedCommand>,
    pub pack: PlannedCommand,
    pub destination: PathBuf,
}

impl PackPlan {
    pub fn commands(self) -> Vec<PlannedCommand> {
        self.build.into_iter().chain([self.pack]).collect()
    }
}

fn plan_npm_pack(destination: &Path) -> PlannedCommand {
    PlannedCommand {
        label: "pack".to_string(),
        args: vec![
            "pack".to_string(),
            "--json".to_string(),
            "--pack-destination".to_string(),
            destination.to_string_lossy().to_string(),
        ],
    }
}

fn library_folder(packing: &Packing, package: &str) -> PathBuf {
    let name = match package.strip_prefix('@') {
        Some(scoped) => scoped.replacen('/', "+", 1),
        None => package.to_string(),
    };
    let suffix: String = Sha256::digest(package.as_bytes())
        .iter()
        .take(LIBRARY_FOLDER_SUFFIX_BYTES)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    packing.folder.join(format!("{name}-{suffix}"))
}

fn runs_before_packing(job: &Job, step: StepName) -> bool {
    let position = |wanted: StepName| job.steps.iter().position(|candidate| *candidate == wanted);
    matches!(
        (position(step), position(StepName::Pack)),
        (Some(before), Some(pack)) if before < pack
    )
}

pub fn only_packs(job: &Job) -> bool {
    job.steps == [StepName::Pack]
}

fn packs_before_its_own_steps(job: &Job) -> bool {
    job.steps.first() == Some(&StepName::Pack) && job.steps.len() > 1
}

pub fn plan_pack(job: &Job, packing: &Packing) -> PackPlan {
    let mut notes = Vec::new();
    if packs_before_its_own_steps(job) {
        notes.push(Reason::PackedBeforeOwnChanges {});
    }
    let mut build = None;
    if runs_before_packing(job, StepName::Build) {
        notes.push(Reason::BuiltBeforePacking {});
    } else if has_build_script(&job.directory) {
        build = Some(plan_build());
    }
    let destination = match own_package(job, packing) {
        Some(package) => library_folder(packing, package),
        None => packing.folder.clone(),
    };
    PackPlan {
        notes,
        build,
        pack: plan_npm_pack(&destination),
        destination,
    }
}

pub fn is_packed_without_saving(spec: &PackageSpec) -> bool {
    spec.pack == Some(PackMode::NoSave)
}

pub fn uninstall_note(job: &Job) -> Option<Reason> {
    let names: Vec<&str> = job
        .packages
        .iter()
        .filter(|spec| is_packed_without_saving(spec))
        .map(|spec| spec.name.as_str())
        .collect();
    (!names.is_empty()).then(|| Reason::PackedNotUninstalled {
        names: names.join(", "),
    })
}

fn tarball_prefix(package: &str) -> String {
    format!("{}-", package.replacen('@', "", 1).replacen('/', "-", 1))
}

fn packed_tarball(package: &str, version: String) -> PackedTarball {
    PackedTarball {
        file: format!("{}{version}.tgz", tarball_prefix(package)),
        version,
    }
}

pub fn tarball_path(job: &Job, package: &str) -> Option<PathBuf> {
    let packing = job.packing.as_ref()?;
    let tarball = packing.libraries.get(package)?.tarball.as_ref()?;
    Some(library_folder(packing, package).join(&tarball.file))
}

pub fn own_package<'a>(job: &Job, packing: &'a Packing) -> Option<&'a str> {
    packing
        .libraries
        .iter()
        .find(|(_, library)| library.directory == job.directory)
        .map(|(package, _)| package.as_str())
}

#[derive(Deserialize)]
struct PackReport {
    name: String,
    version: String,
    filename: String,
}

fn pack_report(stdout: &[String]) -> Option<PackReport> {
    (0..stdout.len())
        .rev()
        .filter(|start| stdout[*start].trim() == "[")
        .find_map(|start| {
            let text = stdout[start..].join("\n");
            let mut reports =
                serde_json::Deserializer::from_str(&text).into_iter::<Vec<PackReport>>();
            reports.next()?.ok()?.into_iter().next()
        })
}

pub fn reported_tarball(package: &str, stdout: &[String]) -> Result<PackedTarball, String> {
    let report =
        pack_report(stdout).ok_or_else(|| format!("npm pack reported no tarball of {package}"))?;
    if report.name != package {
        return Err(format!(
            "npm pack packed {} instead of {package}",
            report.name
        ));
    }
    Ok(PackedTarball {
        file: report.filename,
        version: report.version,
    })
}

pub fn record_tarball(job: &mut Job, package: &str, tarball: &PackedTarball) {
    let library = job
        .packing
        .as_mut()
        .and_then(|packing| packing.libraries.get_mut(package));
    if let Some(library) = library {
        library.tarball = Some(tarball.clone());
    }
}

pub fn expected_packages(job: &Job) -> Vec<PackageSpec> {
    job.packages
        .iter()
        .map(|spec| {
            let packed_version = spec
                .pack
                .and_then(|_| job.packing.as_ref()?.libraries.get(&spec.name))
                .and_then(|library| library.tarball.as_ref())
                .map(|tarball| tarball.version.clone());
            match packed_version {
                Some(version) => PackageSpec {
                    version,
                    ..spec.clone()
                },
                None => spec.clone(),
            }
        })
        .collect()
}

pub fn pack_problem(job: &Job) -> Option<Reason> {
    let packing = job.packing.as_ref();
    job.packages
        .iter()
        .filter(|spec| spec.pack.is_some())
        .find(|spec| packing.map_or(true, |packing| !packing.libraries.contains_key(&spec.name)))
        .map(|spec| {
            let ambiguous = packing.and_then(|packing| packing.ambiguous.get(&spec.name));
            refusal(&spec.name, ambiguous)
        })
}

pub fn mark_packable_candidates(
    candidates: &mut [DependencyCandidate],
    packable: &PackableLibraries,
) {
    for candidate in candidates {
        if let Some(projects) = packable.ambiguous.get(&candidate.name) {
            candidate.ambiguous_projects = projects.clone();
        }
        let Some(library) = packable.libraries.get(&candidate.name) else {
            continue;
        };
        candidate.library_project = Some(library.project.clone());
        if candidate.local_version.is_none() {
            candidate.local_version = library.version.clone();
        }
    }
}
