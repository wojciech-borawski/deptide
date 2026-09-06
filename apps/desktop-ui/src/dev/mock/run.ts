import type { JobSnapshot, RunEvent, RunPlan, RunSnapshot, StepName } from "@/api/types";
import { emit } from "./events";
import { delay, fakeRoot, projectViews, workspaceState } from "./workspace";

let runCounter = 0;

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
    audit: ["up to date, audited 654 packages in 1s", "found 0 vulnerabilities"],
    build: [
      "vite v7.1.0 building for production...",
      "✓ 212 modules transformed.",
      "dist/index.html  0.46 kB",
      "✓ built in 3.21s",
    ],
  };
  const pool = lines[step];
  return pool[index % pool.length] ?? "";
}

function emptyJob(name: string, directory: string, packages: string[]): JobSnapshot {
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
    dependsOn: [],
    installed: [],
    diagnosis: null,
    dependencyChanges: [],
    retries: 0,
    backupAvailable: true,
    log: [],
  };
}

export class MockRun {
  readonly id: string;
  readonly state: RunSnapshot;
  private aborted = false;
  private readonly stoppedJobs = new Set<string>();

  constructor(plan: RunPlan, command: string | null = null) {
    runCounter += 1;
    this.id = `mock-run-${runCounter}`;
    const views = projectViews().filter((project) => plan.projectNames.includes(project.name));
    const packages = plan.packages.map((spec) => `${spec.name}@${spec.version}`);

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
      jobs: views.map((project) => emptyJob(project.name, project.absolutePath, packages)),
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
    const queue = [...this.state.jobs.keys()];
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

  private async runJob(index: number): Promise<void> {
    const job = this.state.jobs[index];
    if (!job) return;

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
    for (const step of this.state.steps) {
      if (this.isStopped(job)) break;
      const stepStart = Date.now();
      job.currentStep = step === "audit" ? "audit fix" : `${step} ${job.packages[0] ?? ""}`;
      this.jobChanged(job);
      this.log(
        job,
        `$ npm ${step === "audit" ? "audit fix" : step === "build" ? "run build" : `${step} ${job.packages.join(" ")}`}`,
      );

      const lines = 3 + Math.floor(Math.random() * 5);
      for (let i = 0; i < lines; i += 1) {
        await delay(120 + Math.random() * 380);
        if (this.aborted) break;
        this.log(job, sampleLine(step, i));
      }

      job.stepTimings.push({ step, durationMs: Date.now() - stepStart });
      if (step === "install" || step === "force-install") this.recordInstall(job);
      if (step === "audit" && simulatesTrouble(job)) {
        job.warning = "audit fix left unresolved vulnerabilities";
        this.log(job, "found 2 moderate severity vulnerabilities");
      }
    }

    const stopped = this.isStopped(job);
    if (stopped) job.error = "stopped before it finished";
    job.status = stopped ? "skipped" : job.warning ? "warn" : "ok";
  }

  private recordInstall(job: JobSnapshot): void {
    job.dependencyChanges = [
      { name: "left-pad", before: "1.2.0", after: "1.3.0" },
      { name: "tslib", before: null, after: "2.8.1" },
    ];
    job.installed = job.packages.map((spec) => {
      const [name, version] = [spec.slice(0, spec.lastIndexOf("@")), spec.slice(spec.lastIndexOf("@") + 1)];
      return {
        name,
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
      logFile: `${fakeRoot}\\logs\\mock.log`,
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
      })),
      okCount: count("ok"),
      warnCount: count("warn"),
      failedCount: count("failed"),
      skippedCount: count("skipped"),
      aborted: this.aborted,
    };

    workspaceState.history.unshift(this.state.summary);
    const event: RunEvent = { type: "finished", runId: this.id, snapshot: this.state };
    emit("run-event", event);
  }
}
