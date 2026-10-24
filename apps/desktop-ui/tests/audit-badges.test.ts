import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { type App } from "vue";
import { createPinia, setActivePinia, type Pinia } from "pinia";

import type { AuditCounts, RunProjectSummary } from "@/api/types";

vi.mock("@/api/commands", () => ({ reportError: () => undefined }));
vi.mock("@tauri-apps/plugin-opener", () => ({ openPath: vi.fn(), revealItemInDir: vi.fn() }));

import JobTable from "@/components/run/JobTable.vue";
import RunSummaryCard from "@/components/run/RunSummaryCard.vue";
import { i18n } from "@/i18n";
import { makeJob, makeSnapshot, makeSummary } from "./helpers/run-fixtures";
import { mount, testDocument, textOf, type Mounted } from "./helpers/test-renderer";

const billing = { info: 0, low: 0, moderate: 1, high: 2, critical: 1, total: 4 };
const clean = { info: 0, low: 0, moderate: 0, high: 0, critical: 0, total: 0 };

let pinia: Pinia;
let mounted: Mounted | undefined;

function install(app: App): void {
  app.use(pinia);
  app.use(i18n);
  app.directive("ripple", {});
}

beforeEach(() => {
  pinia = createPinia();
  setActivePinia(pinia);
  i18n.global.locale.value = "en";
  vi.stubGlobal("document", testDocument);
});

afterEach(() => {
  mounted?.unmount();
  mounted = undefined;
  vi.unstubAllGlobals();
});

describe("audit counts on the run screen", () => {
  it("shows a severity badge per finding in the job table and marks a clean audit", () => {
    const jobs = [
      makeJob("Billing-Api", { status: "ok", audit: billing }),
      makeJob("Shop-Frontend", { status: "ok", audit: clean }),
      makeJob("Orders-Api", { status: "ok" }),
    ];
    mounted = mount(JobTable as never, install as never, {
      jobs,
      steps: ["audit-report"],
      selected: null,
      nowMs: 0,
      baseline: {},
      rerunnable: false,
      active: false,
    });

    const text = textOf(mounted.root);
    expect(text).toContain("1 critical");
    expect(text).toContain("2 high");
    expect(text).toContain("1 moderate");
    expect(text).not.toContain("0 low");
    expect(text.match(/no vulnerabilities/g)).toHaveLength(1);
  });

  it("shows the totals over all audited projects in the run summary", () => {
    const project = (name: string, audit: AuditCounts): RunProjectSummary => ({
      name,
      directory: `C:\\repos\\${name}`,
      status: "ok",
      durationMs: 1_000,
      stepTimings: [],
      error: "",
      warning: "",
      installed: [],
      diagnosis: null,
      dependencyChanges: [],
      retries: 0,
      audit,
    });
    const summary = makeSummary(makeSnapshot("run-1", []), {
      projects: [project("Billing-Api", billing), project("Shop-Admin", { ...clean, low: 3, total: 3 })],
    });
    mounted = mount(RunSummaryCard as never, install as never, { summary, baseline: {}, restorable: [] });

    const text = textOf(mounted.root);
    expect(text).toContain("Vulnerabilities");
    expect(text).toContain("7 vulnerabilities");
    expect(text).toContain("3 low");
  });
});
