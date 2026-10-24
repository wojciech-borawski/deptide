use deptide_core::domain::UpdateConfig;
use deptide_core::error::AppResult;
use deptide_core::workspace::open_with_config;

use crate::arguments::GroupsArgs;
use crate::exit_code::ExitCode;

fn group_lines(config: &UpdateConfig) -> Vec<String> {
    if config.groups.is_empty() {
        return vec!["no groups defined in update-libs.json".to_string()];
    }

    let width = config
        .groups
        .iter()
        .map(|group| group.name.chars().count())
        .max()
        .unwrap_or(0);

    config
        .groups
        .iter()
        .map(|group| {
            let noun = if group.projects.len() == 1 {
                "project"
            } else {
                "projects"
            };
            format!(
                "{:<width$}  {} {noun}: {}",
                group.name,
                group.projects.len(),
                group.projects.join(", ")
            )
        })
        .collect()
}

pub fn execute(args: GroupsArgs) -> AppResult<ExitCode> {
    let (_, config) = open_with_config(&args.workspace)?;

    for line in group_lines(&config) {
        println!("{line}");
    }

    Ok(ExitCode::Success)
}

#[cfg(test)]
mod tests {
    use deptide_core::domain::{ConfiguredProject, ProjectGroup, UpdateConfig};

    use super::group_lines;

    #[test]
    fn each_group_is_listed_with_its_projects() {
        let config = UpdateConfig {
            projects: ["web", "admin", "api"]
                .map(|name| ConfiguredProject::new(name, format!("../{name}")))
                .to_vec(),
            groups: vec![
                ProjectGroup {
                    name: "frontend apps".to_string(),
                    projects: vec!["web".to_string(), "admin".to_string()],
                },
                ProjectGroup {
                    name: "api".to_string(),
                    projects: vec!["api".to_string()],
                },
            ],
            ..UpdateConfig::default()
        };

        assert_eq!(
            group_lines(&config),
            vec![
                "frontend apps  2 projects: web, admin",
                "api            1 project: api",
            ]
        );
        assert_eq!(
            group_lines(&UpdateConfig::default()),
            vec!["no groups defined in update-libs.json"]
        );
    }
}
