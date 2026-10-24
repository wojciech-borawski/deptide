import type { JobPreview, JobSnapshot, RunEvent, RunPlan, RunSnapshot, StepName } from "@/api/types";
import { formatSpec } from "@/lib/versions";
import { emit } from "./events";
import { expectedVersion, mockAudit, mockJobs, previewJob, type MockJob } from "./preview";
import { delay, fakeRoot, projectViews, runFileName, workspaceState } from "./workspace";

let runCounter = 0;

const BACKED_UP_FILES = ["package.json", "package-lock.json"];

function commandJobs(projectNames: string[]): { name: string; directory: string }[] {
  return projectViews()
    .filter((project) => projectNames.includes(project.name))
    .map((project) => ({ name: project.name, directory: project.absolutePath }));
}

function onlyPacks(steps: StepName[]): boolean {
  return steps.length === 1 && steps[0] === "pack";
}

function simulatesTrouble(job: JobSnapshot): boolean {
  return /Admin|Billing/.test(job.name);
}

function commandLine(command: string, index: number): string {
  const word = command.split(/\s+/)[0] ?? "";
  const pool =
    word === "git"
      ? ["On branch main", "Your branch is up to date with 'origin/main'.", "nothing to commit, working tree clean"]
      : ["> demo@1.0.0 " + word, "checking 214 files", "✓ 212 passed, 2 skipped", "done in 1.42s"];
  return pool[index % pool.length] ?? "";
}

function sampleLine(step: StepName, index: number): string {
  const lines: Record<StepName, string[]> = {
    uninstall: ["removed 12 packages, and audited 640 packages in 2s", "found 0 vulnerabilities"],
    install: [
      "npm WARN deprecated inflight@1.0.6",
      "added 14 packages, changed 2 packages",
      "audited 654 packages in 4s",
      "found 0 vulnerabilities",
    ],
    "force-install": [
      "npm WARN using --force Recommended protections disabled.",
      "changed 2 packages, and audited 654 packages in 5s",
      "found 0 vulnerabilities",
    ],
    version: ["v1.4.1", "version 1.4.0 -> 1.4.1"],
    audit: ["up to date, audited 654 packages in 1s", "found 0 vulnerabilities"],
    "audit-report": ["npm warn audit reading the lockfile"],
    build: [
      "vite v7.1.0 building for production...",
      "✓ 212 modules transformed.",
      "dist/index.html  0.46 kB",
      "✓ built in 3.21s",
    ],
    pack: [
      '    "name": "@demo/core",',
      '    "version": "2.3.0-DEMO-7",',
      '    "filename": "demo-core-2.3.0-DEMO-7.tgz",',
    ],
  };
  const pool = lines[step];
  return pool[index % pool.length] ?? "";
}

function emptyJob(
  name: string,
  directory: string,
  packages: string[],
  dependsOn: string[] = [],
  backupAvailable = true,
): JobSnapshot {
  return {
    name,
    directory,
    packages,
    status: "pending",
    currentStep: "",
    startedAtMs: null,
    stepStartedAtMs: null,
    durationMs: 0,
    stepTimings: [],
    error: "",
    warning: "",
    dependsOn,
    installed: [],
    diagnosis: null,
    dependencyChanges: [],
    retries: 0,
    backupAvailable,
    audit: null,
    log: [],
  };
}

export class MockRun {
  readonly id: string;
  readonly state: RunSnapshot;
  private aborted = false;
  private readonly stoppedJobs = new Set<string>();
  private readonly npmJobs = new Map<string, { job: MockJob; preview: JobPreview }>();

  constructor(plan: RunPlan, command: string | null = null) {
    runCounter += 1;
    this.id = `mock-run-${runCounter}`;
    const npmJobs = command === null ? mockJobs(plan).jobs : [];
    for (const job of npmJobs) this.npmJobs.set(job.name, { job, preview: previewJob(job, plan) });
    const packages = plan.packages.map(formatSpec);
    const jobs =
      command === null
        ? npmJobs.map((job) =>
            emptyJob(job.name, job.directory, job.packages.map(formatSpec), job.dependsOn, !onlyPacks(job.steps)),
          )
        : commandJobs(plan.projectNames).map((job) => emptyJob(job.name, job.directory, packages));

    this.state = {
      id: this.id,
      label: plan.label,
      packages,
      steps: plan.steps,
      mode: plan.mode,
      concurrency: plan.concurrency,
      dryRun: plan.dryRun,
      startedAtMs: Date.now(),
      finishedAtMs: null,
      currentPhase: null,
      aborted: false,
      command,
      jobs,
      summary: null,
    };
  }

  abort(): void {
    this.aborted = true;
  }

  abortJob(name: string): void {
    this.stoppedJobs.add(name);
  }

  async execute(): Promise<void> {
    const jobs = this.state.jobs;
    const queue = [...jobs.keys()].sort((a, b) => (jobs[a]?.dependsOn.length ?? 0) - (jobs[b]?.dependsOn.length ?? 0));
    const workers = Array.from({ length: Math.max(1, this.state.concurrency) }, async () => {
      for (let index = queue.shift(); index !== undefined; index = queue.shift()) {
        await this.runJob(index);
      }
    });

    await Promise.all(workers);
    this.finish();
  }

  private isStopped(job: JobSnapshot): boolean {
    return this.aborted || this.stoppedJobs.has(job.name);
  }

  private jobChanged(job: JobSnapshot): void {
    const event: RunEvent = { type: "jobChanged", runId: this.id, job: { ...job, log: [] } };
    emit("run-event", event);
  }

  private log(job: JobSnapshot, line: string): void {
    job.log.push(line);
    const event: RunEvent = { type: "logLine", runId: this.id, job: job.name, line };
    emit("run-event", event);
  }

  private async waitForDependencies(job: JobSnapshot): Promise<string | null> {
    for (const name of job.dependsOn) {
      const dependency = this.state.jobs.find((entry) => entry.name === name);
      while (dependency && (dependency.status === "pending" || dependency.status === "running")) await delay(100);
      if (dependency && (dependency.status === "failed" || dependency.status === "skipped")) return name;
    }
    return null;
  }

  private async runJob(index: number): Promise<void> {
    const job = this.state.jobs[index];
    if (!job) return;

    const failedDependency = await this.waitForDependencies(job);
    if (failedDependency) {
      job.status = "skipped";
      job.error = `not run because ${failedDependency} did not succeed`;
      this.jobChanged(job);
      return;
    }

    if (this.aborted || !job.directory || job.name.includes("Legacy")) {
      job.status = this.aborted ? "skipped" : "failed";
      job.error = this.aborted ? "" : "npm install exited with code 1";
      job.diagnosis = this.aborted
        ? null
        : {
            code: "ERESOLVE",
            title: "Peer dependency conflict",
            hint: "Add --legacy-peer-deps or --force to the install flags, or align the peer versions.",
          };
      this.jobChanged(job);
      return;
    }

    job.status = "running";
    job.startedAtMs = Date.now();
    job.stepStartedAtMs = Date.now();
    this.jobChanged(job);

    const started = Date.now();

    if (this.state.command !== null) {
      await this.runCommand(job, this.state.command);
    } else {
      await this.runNpmSteps(job);
    }

    job.durationMs = Date.now() - started;
    job.stepStartedAtMs = null;
    job.currentStep = "";
    this.jobChanged(job);
  }

  private async runNpmSteps(job: JobSnapshot): Promise<void> {
    const npmJob = this.npmJobs.get(job.name);
    for (const { step, commands, notes } of npmJob?.preview.steps ?? []) {
      if (this.isStopped(job)) break;
      const stepStart = Date.now();
      job.currentStep = step === "audit" ? "audit fix" : `${step} ${job.packages[0] ?? ""}`;
      this.jobChanged(job);
      for (const note of notes) this.log(job, note.text);

      for (const command of commands) {
        this.log(job, `$ ${command}`);
        const lines = 3 + Math.floor(Math.random() * 5);
        for (let i = 0; i < lines; i += 1) {
          await delay(120 + Math.random() * 380);
          if (this.aborted) break;
          this.log(job, sampleLine(step, i));
        }
      }

      job.stepTimings.push({ step, durationMs: Date.now() - stepStart });
      if (step === "audit-report") this.recordAudit(job);
      if (npmJob && (step === "install" || step === "force-install")) this.recordInstall(job, npmJob.job);
      if (step === "audit" && simulatesTrouble(job)) {
        job.warning = "audit fix left unresolved vulnerabilities";
        this.log(job, "found 2 moderate severity vulnerabilities");
      }
    }

    const stopped = this.isStopped(job);
    if (stopped) job.error = "stopped before it finished";
    job.status = stopped ? "skipped" : job.warning ? "warn" : "ok";
  }

  private recordAudit(job: JobSnapshot): void {
    const audit = mockAudit(job.name);
    job.audit = audit;
    this.log(
      job,
      `audit: ${audit.total} vulnerabilities (${audit.critical} critical, ${audit.high} high, ${audit.moderate} moderate, ${audit.low} low, ${audit.info} info)`,
    );
  }

  private recordInstall(job: JobSnapshot, npmJob: MockJob): void {
    job.dependencyChanges = [
      { name: "left-pad", before: "1.2.0", after: "1.3.0" },
      { name: "tslib", before: null, after: "2.8.1" },
    ];
    job.installed = npmJob.packages.map((spec) => {
      const version = expectedVersion(npmJob, spec);
      return {
        name: spec.name,
        expected: version,
        installed: version,
        integrity: "sha512-Qm9ndLkq8Ykp3Mv2xL5hRZyT9pXd",
        resolved: null,
        matches: true,
      };
    });
  }

  private async runCommand(job: JobSnapshot, command: string): Promise<void> {
    job.currentStep = command;
    this.jobChanged(job);
    this.log(job, `$ ${command}`);

    const lines = 4 + Math.floor(Math.random() * 6);
    for (let i = 0; i < lines; i += 1) {
      await delay(150 + Math.random() * 400);
      if (this.isStopped(job)) break;
      this.log(job, commandLine(command, i));
    }

    const stopped = this.isStopped(job);
    const failed = !stopped && simulatesTrouble(job);
    const code = stopped ? "none" : failed ? 1 : 0;
    this.log(job, `finished in ${Date.now() - (job.startedAtMs ?? Date.now())} ms with exit code ${code}`);

    if (stopped) {
      job.error = "stopped before it finished";
      job.status = "skipped";
    } else if (failed) {
      job.error = "command exited with code 1";
      job.status = "failed";
    } else {
      job.status = "ok";
    }
  }

  private finish(): void {
    const finishedAt = Date.now();
    const count = (status: JobSnapshot["status"]) => this.state.jobs.filter((job) => job.status === status).length;

    this.state.finishedAtMs = finishedAt;
    this.state.aborted = this.aborted;
    const run = runFileName(this.state.startedAtMs, this.state.label);
    const logFile = `${fakeRoot}\\logs\\${run}.log`;
    const backedUp = !this.state.dryRun && this.state.command === null;
    this.state.summary = {
      label: this.state.label,
      packages: this.state.packages,
      projectCount: this.state.jobs.length,
      concurrency: this.state.concurrency,
      steps: this.state.steps,
      mode: this.state.mode,
      dryRun: this.state.dryRun,
      startedAt: new Date(this.state.startedAtMs).toISOString(),
      finishedAt: new Date(finishedAt).toISOString(),
      totalDurationMs: finishedAt - this.state.startedAtMs,
      busyDurationMs: this.state.jobs.reduce((sum, job) => sum + job.durationMs, 0),
      logFile,
      projects: this.state.jobs.map((job) => ({
        name: job.name,
        directory: job.directory,
        status: job.status,
        durationMs: job.durationMs,
        stepTimings: job.stepTimings,
        error: job.error,
        warning: job.warning,
        installed: job.installed,
        diagnosis: job.diagnosis,
        dependencyChanges: job.dependencyChanges,
        retries: job.retries,
        ...(job.audit ? { audit: job.audit } : {}),
      })),
      okCount: count("ok"),
      warnCount: count("warn"),
      failedCount: count("failed"),
      skippedCount: count("skipped"),
      aborted: this.aborted,
      ...(backedUp ? { backupFolder: run } : {}),
    };

    workspaceState.history.unshift(this.state.summary);
    const backedUpProjects = this.state.jobs
      .filter((job) => job.backupAvailable)
      .map((job) => ({ name: job.name, directory: job.directory, files: BACKED_UP_FILES }));
    if (backedUp && backedUpProjects.length > 0) {
      workspaceState.backups.unshift({ run, logFile, folder: run, projects: backedUpProjects });
    }
    const event: RunEvent = { type: "finished", runId: this.id, snapshot: this.state };
    emit("run-event", event);
  }
}
