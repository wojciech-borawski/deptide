<script setup lang="ts">
import { computed, ref } from "vue";
import { useI18n } from "vue-i18n";

import type { JobPreview, ReasonPreview, StepPreview, VersionPreview } from "@/api/types";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { usePlanPreview } from "@/composables/usePlanPreview";
import { useWizardStore } from "@/stores/wizard";
import { useWorkspaceStore } from "@/stores/workspace";

const wizard = useWizardStore();
const workspace = useWorkspaceStore();
const { t } = useI18n();

const root = computed(() => workspace.root);
const plan = computed(() => wizard.plan);
const { preview, loading, error } = usePlanPreview(root, plan);
const stale = computed(() => loading.value && preview.value !== null);
const open = ref(new Set<string>());

function toggle(name: string): void {
  const next = new Set(open.value);
  if (next.has(name)) next.delete(name);
  else next.add(name);
  open.value = next;
}

function commandCount(job: JobPreview): number {
  return job.steps.reduce((sum, step) => sum + step.commands.length, 0);
}

function reasonText(reason: ReasonPreview): string {
  const noVersion = t("review.preview.noVersion");
  const params = Object.fromEntries(Object.entries(reason.params).map(([key, value]) => [key, value ?? noVersion]));
  if (reason.kind === "branchUnknown") params.cause = t(`review.lookupCause.${reason.params.cause}`);
  return t(`review.reason.${reason.kind}`, params);
}

function versionLine(version: VersionPreview): string {
  if (version.decision === "bump") {
    const current = version.current ?? t("review.preview.noVersion");
    return t("review.preview.bump", { current, next: version.next, level: version.level });
  }
  return t(`review.preview.${version.decision}`, { reason: reasonText(version.reason) });
}

function noteText(job: JobPreview, step: StepPreview, note: ReasonPreview): string {
  if (step.step !== "version") return reasonText(note);
  const outcome = job.version?.decision === "skip" ? "bumpSkipped" : "bumping";
  return t(`review.preview.${outcome}`, { reason: reasonText(note) });
}
</script>

<template>
  <section class="card">
    <div class="card-title">
      <h3>{{ t("review.preview.title") }}</h3>
      <span v-if="loading" class="spinner" />
      <span v-if="stale" class="muted updating">{{ t("review.preview.updating") }}</span>
    </div>
    <p class="hint muted">{{ t("review.preview.hint") }}</p>

    <p v-if="loading && !preview" class="muted">{{ t("review.preview.loading") }}</p>
    <NoticeBanner v-else-if="error" tone="error" selectable>
      {{ t("review.preview.failed", { error }) }}
    </NoticeBanner>

    <div v-if="preview && !error" class="result" :class="{ stale }" :aria-busy="stale ? 'true' : 'false'">
      <NoticeBanner v-if="plan.dryRun" tone="warn">{{ t("review.preview.dryRun") }}</NoticeBanner>
      <NoticeBanner v-if="preview.nothingToDo" tone="warn">{{ reasonText(preview.nothingToDo) }}</NoticeBanner>
      <NoticeBanner v-if="preview.missing.length" tone="warn">
        {{ t("review.preview.missing", { names: preview.missing.join(", ") }) }}
      </NoticeBanner>
      <NoticeBanner v-if="preview.withoutPackages.length" tone="info">
        {{ t("review.preview.withoutPackages", { names: preview.withoutPackages.join(", ") }) }}
      </NoticeBanner>

      <ul class="jobs">
        <li v-for="job in preview.jobs" :key="job.name" class="job">
          <button class="job-head" type="button" :aria-expanded="open.has(job.name)" @click="toggle(job.name)">
            <span class="caret">{{ open.has(job.name) ? "▾" : "▸" }}</span>
            <strong>{{ job.name }}</strong>
            <span v-if="job.dependsOn.length" class="muted">
              {{ t("review.preview.after", { names: job.dependsOn.join(", ") }) }}
            </span>
            <span class="muted count">
              {{
                commandCount(job)
                  ? t("review.preview.commands", { count: commandCount(job) }, commandCount(job))
                  : t("review.preview.noCommand")
              }}
            </span>
          </button>
          <p
            v-if="job.version"
            class="version mono"
            :class="{ skipped: job.version.decision === 'skip', failed: job.version.decision === 'fail' }"
          >
            {{ versionLine(job.version) }}
          </p>
          <ol v-if="open.has(job.name)" class="steps">
            <li v-for="step in job.steps" :key="step.step">
              <span class="badge badge-violet">{{ t(`steps.${step.step}`) }}</span>
              <code v-for="command in step.commands" :key="command" class="mono command">{{ command }}</code>
              <span v-for="note in step.notes" :key="note.kind" class="muted note">{{
                noteText(job, step, note)
              }}</span>
            </li>
          </ol>
        </li>
      </ul>
    </div>
  </section>
</template>

<style scoped>
.updating {
  font-size: 12px;
}

.result.stale {
  opacity: 0.55;
}

.hint {
  margin: 0 0 10px;
  font-size: 12px;
}

.jobs {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 10px 0 0;
  padding: 0;
  list-style: none;
}

.job-head {
  display: flex;
  align-items: baseline;
  gap: 8px;
  width: 100%;
  padding: 4px 0;
  border: 0;
  background: none;
  color: inherit;
  font: inherit;
  text-align: left;
  cursor: pointer;
}

.caret {
  width: 12px;
  color: var(--text-muted);
}

.count {
  margin-left: auto;
  font-size: 12px;
}

.version {
  margin: 0 0 0 20px;
  font-size: 12.5px;
}

.version.skipped {
  color: var(--text-muted);
}

.version.failed {
  color: var(--failed);
}

.steps {
  display: flex;
  flex-direction: column;
  gap: 6px;
  margin: 8px 0 4px 20px;
  padding: 0;
  list-style: none;
}

.steps li {
  display: flex;
  flex-wrap: wrap;
  align-items: baseline;
  gap: 6px 8px;
}

.command {
  font-size: 12px;
  word-break: break-all;
  user-select: text;
}

.note {
  font-size: 12px;
}
</style>
