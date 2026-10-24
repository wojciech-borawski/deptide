use std::collections::{BTreeMap, BTreeSet, HashMap};

use super::projects::WorkspaceProjects;
use crate::domain::{
    DependencyGraph, DependencySection, ExecutionMode, GraphEdge, GraphNode, RunPlan, StepName,
    UpdateConfig, VersionPolicy,
};
use crate::execution::build_jobs;
use crate::workspace::Workspace;

fn order_plan(project_names: Vec<String>) -> RunPlan {
    RunPlan {
        project_names,
        packages: Vec::new(),
        steps: vec![StepName::Build],
        mode: ExecutionMode::PerProject,
        concurrency: 1,
        dry_run: true,
        extra_install_args: Vec::new(),
        label: String::new(),
        save_as: None,
        version: VersionPolicy::default(),
        project_bumps: BTreeMap::new(),
    }
}

fn workspace_edges(read: &WorkspaceProjects) -> Vec<GraphEdge> {
    let providers = read.providers();
    let mut edges: Vec<GraphEdge> = Vec::new();

    for project in &read.projects {
        let sections = [
            (
                &project.detected.dependencies,
                DependencySection::Dependencies,
            ),
            (&project.detected.dev_dependencies, DependencySection::Dev),
            (&project.detected.peer_dependencies, DependencySection::Peer),
        ];
        for (entries, section) in sections {
            for package in entries.keys() {
                let Some(provider) = providers.single.get(package.as_str()) else {
                    continue;
                };
                let library = provider.name();
                let known = edges
                    .iter()
                    .any(|edge| edge.consumer == project.name() && edge.library == library);
                if library != project.name() && !known {
                    edges.push(GraphEdge {
                        consumer: project.name().to_string(),
                        library: library.to_string(),
                        section,
                    });
                }
            }
        }
    }

    edges
}

fn reachable_from(start: &str, links: &HashMap<&str, Vec<&str>>) -> BTreeSet<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut pending: Vec<&str> = links.get(start).cloned().unwrap_or_default();

    while let Some(next) = pending.pop() {
        if seen.insert(next.to_string()) {
            pending.extend(links.get(next).cloned().unwrap_or_default());
        }
    }

    seen
}

fn find_cycles(nodes: &[GraphNode], edges: &[GraphEdge]) -> Vec<Vec<String>> {
    let mut links: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in edges
        .iter()
        .filter(|edge| edge.section != DependencySection::Peer)
    {
        links
            .entry(edge.consumer.as_str())
            .or_default()
            .push(edge.library.as_str());
    }

    let reach: HashMap<&str, BTreeSet<String>> = nodes
        .iter()
        .map(|node| (node.name.as_str(), reachable_from(&node.name, &links)))
        .collect();

    let mut cycles: Vec<Vec<String>> = Vec::new();
    for node in nodes {
        let in_known_cycle = cycles.iter().any(|cycle| cycle.contains(&node.name));
        if in_known_cycle || !reach[node.name.as_str()].contains(&node.name) {
            continue;
        }
        let members: Vec<String> = nodes
            .iter()
            .filter(|other| {
                reach[node.name.as_str()].contains(&other.name)
                    && reach[other.name.as_str()].contains(&node.name)
            })
            .map(|other| other.name.clone())
            .collect();
        cycles.push(members);
    }

    cycles
}

fn assign_layers(nodes: &mut [GraphNode], edges: &[GraphEdge]) {
    let position: HashMap<String, usize> = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node.name.clone(), index))
        .collect();

    for index in 0..nodes.len() {
        let layer = edges
            .iter()
            .filter(|edge| edge.consumer == nodes[index].name)
            .filter_map(|edge| position.get(&edge.library))
            .filter(|library| **library < index)
            .map(|library| nodes[*library].layer + 1)
            .max()
            .unwrap_or(0);
        nodes[index].layer = layer;
    }
}

pub fn read_dependency_graph(workspace: &Workspace, config: &UpdateConfig) -> DependencyGraph {
    let read = WorkspaceProjects::read(workspace, config);
    let jobs = build_jobs(workspace, config, &order_plan(read.names())).jobs;

    let mut nodes: Vec<GraphNode> = jobs
        .into_iter()
        .map(|job| GraphNode {
            package_name: read
                .projects
                .iter()
                .find(|project| project.name() == job.name)
                .and_then(|project| project.detected.package_name.clone()),
            name: job.name,
            layer: 0,
            depends_on: job.depends_on,
        })
        .collect();

    let edges = workspace_edges(&read);
    assign_layers(&mut nodes, &edges);
    let cycles = find_cycles(&nodes, &edges);

    DependencyGraph {
        nodes,
        edges,
        cycles,
    }
}
