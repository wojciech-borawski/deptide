use crate::domain::{JobStatus, RunProjectSummary, RunSummary};
use crate::execution::verify::short_integrity;
use crate::util::time::format_duration;

fn status_word(status: JobStatus) -> &'static str {
    match status {
        JobStatus::Pending => "pending",
        JobStatus::Running => "running",
        JobStatus::Ok => "ok",
        JobStatus::Warn => "warning",
        JobStatus::Failed => "failed",
        JobStatus::Skipped => "skipped",
    }
}

fn outcome_line(summary: &RunSummary) -> String {
    let verdict = if summary.aborted {
        "stopped before it finished"
    } else if summary.failed_count > 0 {
        "finished with failures"
    } else if summary.warn_count > 0 {
        "finished with warnings"
    } else {
        "finished successfully"
    };

    format!(
        "{verdict}: {} ok, {} warnings, {} failed, {} skipped",
        summary.ok_count, summary.warn_count, summary.failed_count, summary.skipped_count
    )
}

fn installed_line(project: &RunProjectSummary) -> String {
    project
        .installed
        .iter()
        .map(|entry| {
            let version = entry.installed.as_deref().unwrap_or("missing");
            let hash = entry
                .integrity
                .as_deref()
                .map(|value| format!(" {}", short_integrity(value)))
                .unwrap_or_default();
            let mark = if entry.matches {
                String::new()
            } else {
                format!(" (expected {})", entry.expected)
            };
            format!("{}@{version}{hash}{mark}", entry.name)
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn detail_line(project: &RunProjectSummary) -> String {
    let mut parts = Vec::new();
    if !project.error.is_empty() {
        parts.push(project.error.clone());
    }
    if !project.warning.is_empty() {
        parts.push(project.warning.clone());
    }
    if let Some(diagnosis) = &project.diagnosis {
        parts.push(format!("{}: {}", diagnosis.title, diagnosis.hint));
    }
    parts.join(" · ")
}

pub fn render_markdown(summary: &RunSummary) -> String {
    let mut out = String::new();

    out.push_str(&format!("# Deptide run: {}\n\n", summary.label));
    out.push_str(&format!("- Outcome: {}\n", outcome_line(summary)));
    out.push_str(&format!(
        "- Total time: **{}** ({} of npm work across {} parallel)\n",
        format_duration(summary.total_duration_ms),
        format_duration(summary.busy_duration_ms),
        summary.concurrency
    ));
    out.push_str(&format!(
        "- Started: {}\n- Finished: {}\n",
        summary.started_at, summary.finished_at
    ));
    out.push_str(&format!("- Packages: {}\n", summary.packages.join(", ")));
    out.push_str(&format!(
        "- Steps: {}{}\n\n",
        summary
            .steps
            .iter()
            .map(|step| step.label())
            .collect::<Vec<_>>()
            .join(", "),
        if summary.dry_run { " (dry run)" } else { "" }
    ));

    out.push_str("| Project | Status | Time | Installed | Notes |\n");
    out.push_str("| --- | --- | --- | --- | --- |\n");
    for project in &summary.projects {
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} |\n",
            project.name,
            status_word(project.status),
            format_duration(project.duration_ms),
            installed_line(project),
            detail_line(project)
        ));
    }

    out.push_str("\n## Step timings\n\n");
    for project in &summary.projects {
        let timings: Vec<String> = project
            .step_timings
            .iter()
            .map(|timing| {
                format!(
                    "{} {}",
                    timing.step.label(),
                    format_duration(timing.duration_ms)
                )
            })
            .collect();
        out.push_str(&format!("- {}: {}\n", project.name, timings.join(", ")));
    }

    if let Some(log_file) = &summary.log_file {
        out.push_str(&format!("\nTranscript: `{log_file}`\n"));
    }

    out
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

pub fn render_html(summary: &RunSummary) -> String {
    let rows: String = summary
        .projects
        .iter()
        .map(|project| {
            format!(
                "<tr><td>{}</td><td class=\"{}\">{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                escape_html(&project.name),
                status_word(project.status),
                status_word(project.status),
                format_duration(project.duration_ms),
                escape_html(&installed_line(project)),
                escape_html(&detail_line(project))
            )
        })
        .collect();

    format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>Deptide run: {label}</title>\
<style>body{{font-family:Segoe UI,system-ui,sans-serif;margin:32px;color:#1f2937}}table{{border-collapse:collapse;width:100%}}\
td,th{{border-bottom:1px solid #e5e7eb;padding:6px 8px;text-align:left;vertical-align:top}}th{{font-size:12px;text-transform:uppercase;color:#6b7280}}\
.ok{{color:#15803d}}.warning{{color:#b45309}}.failed{{color:#b91c1c}}.skipped{{color:#6b7280}}.total{{font-size:28px;font-weight:700}}</style></head><body>\
<h1>Deptide run: {label}</h1><p>{outcome}</p><p class=\"total\">{total}</p><p>{busy} of npm work across {concurrency} parallel · {started} → {finished}</p>\
<p>Packages: {packages}</p><table><thead><tr><th>Project</th><th>Status</th><th>Time</th><th>Installed</th><th>Notes</th></tr></thead><tbody>{rows}</tbody></table>\
{log}</body></html>",
        label = escape_html(&summary.label),
        outcome = escape_html(&outcome_line(summary)),
        total = format_duration(summary.total_duration_ms),
        busy = format_duration(summary.busy_duration_ms),
        concurrency = summary.concurrency,
        started = escape_html(&summary.started_at),
        finished = escape_html(&summary.finished_at),
        packages = escape_html(&summary.packages.join(", ")),
        rows = rows,
        log = summary
            .log_file
            .as_deref()
            .map(|file| format!("<p>Transcript: <code>{}</code></p>", escape_html(file)))
            .unwrap_or_default(),
    )
}
