use super::bump::{preview_version_step, BumpDecision};
use super::jobs::{build_jobs, nothing_to_do_reason};
use super::pack::{pack_problem, plan_pack, uninstall_note};
use super::steps::{
    has_build_script, plan_audit_fix, plan_audit_report, plan_build, plan_install,
    plan_reinstall_after_audit_fix, plan_uninstall, plan_version_bump, save_exact_peer_note,
    PlannedCommand,
};
use crate::domain::{
    Job, JobPreview, PlanPreview, Reason, ReasonPreview, RunPlan, StepName, StepPreview,
    UpdateConfig,
};
use crate::workspace::Workspace;

fn command_lines(commands: Vec<PlannedCommand>) -> Vec<String> {
    commands.iter().map(PlannedCommand::command_line).collect()
}

fn version_note(decision: &BumpDecision) -> Option<ReasonPreview> {
    let reason = match decision {
        BumpDecision::Bump { reason } => reason.clone()?,
        BumpDecision::Skip { reason } => reason.clone(),
    };
    Some(ReasonPreview {
        reason,
        text: decision.log_line()?,
    })
}

fn preview_step(job: &Job, step: StepName, bump: Option<&BumpDecision>) -> StepPreview {
    let (commands, notes) = match step {
        StepName::Uninstall => (
            plan_uninstall(job).into_iter().collect(),
            uninstall_note(job)
                .map(ReasonPreview::from)
                .into_iter()
                .collect(),
        ),
        StepName::Install | StepName::ForceInstall => match pack_problem(job) {
            Some(reason) => (Vec::new(), vec![reason.into()]),
            None => (
                plan_install(job, step == StepName::ForceInstall),
                save_exact_peer_note(job)
                    .map(ReasonPreview::from)
                    .into_iter()
                    .collect(),
            ),
        },
        StepName::Version => match bump {
            Some(decision @ BumpDecision::Bump { .. }) => (
                vec![plan_version_bump(job.version.bump)],
                version_note(decision).into_iter().collect(),
            ),
            Some(decision) => (Vec::new(), version_note(decision).into_iter().collect()),
            None => (Vec::new(), Vec::new()),
        },
        StepName::Audit => (
            std::iter::once(plan_audit_fix(job))
                .chain(plan_reinstall_after_audit_fix(job))
                .collect(),
            Vec::new(),
        ),
        StepName::AuditReport => (vec![plan_audit_report()], Vec::new()),
        StepName::Build if has_build_script(&job.directory) => (vec![plan_build()], Vec::new()),
        StepName::Build => (Vec::new(), vec![Reason::NoBuildScript {}.into()]),
        StepName::Pack => match &job.packing {
            Some(packing) => {
                let pack = plan_pack(job, packing);
                let notes = pack
                    .notes
                    .iter()
                    .cloned()
                    .map(ReasonPreview::from)
                    .collect();
                (pack.commands(), notes)
            }
            None => (Vec::new(), Vec::new()),
        },
    };

    StepPreview {
        step,
        commands: command_lines(commands),
        notes,
    }
}

fn preview_job(job: &Job) -> JobPreview {
    let version = job
        .steps
        .contains(&StepName::Version)
        .then(|| preview_version_step(job));
    let decision = version.as_ref().map(|(_, decision)| decision);

    JobPreview {
        name: job.name.clone(),
        depends_on: job.depends_on.clone(),
        steps: job
            .steps
            .iter()
            .map(|step| preview_step(job, *step, decision))
            .collect(),
        version: version.map(|(preview, _)| preview),
    }
}

/// What starting `plan` would run, per job in run order, without touching any project.
pub fn preview_plan(workspace: &Workspace, config: &UpdateConfig, plan: &RunPlan) -> PlanPreview {
    let outcome = build_jobs(workspace, config, plan);
    let mut jobs: Vec<JobPreview> = outcome.jobs.iter().map(preview_job).collect();
    for job in jobs
        .iter_mut()
        .filter(|job| outcome.without_packages.contains(&job.name))
    {
        for step in job
            .steps
            .iter_mut()
            .filter(|step| step.step == StepName::Pack)
        {
            step.notes.push(Reason::OnlyPacked {}.into());
        }
    }

    PlanPreview {
        jobs,
        nothing_to_do: outcome
            .jobs
            .is_empty()
            .then(|| nothing_to_do_reason(plan).into()),
        missing: outcome.missing,
        without_packages: outcome.without_packages,
    }
}
