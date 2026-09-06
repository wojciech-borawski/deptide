<script setup lang="ts">
import { computed } from "vue";
import { useI18n } from "vue-i18n";

import { baseVersion, choiceProblem, resolveVersion, type PackageChoice, type VersionMode } from "@/lib/wizard-plan";

const props = defineProps<{ choice: PackageChoice }>();
const emit = defineEmits<{ patch: [patch: Partial<PackageChoice>] }>();
const { t } = useI18n();

const base = computed(() => baseVersion(props.choice));
const resolved = computed(() => resolveVersion(props.choice));
const problem = computed(() => choiceProblem(props.choice));

function setMode(mode: VersionMode, keepRange?: string): void {
  emit("patch", keepRange === undefined ? { mode } : { mode, keepRange });
}
</script>

<template>
  <div class="picker">
    <label v-if="props.choice.branchSuffix && base" class="check option">
      <input type="radio" :checked="props.choice.mode === 'branch'" @change="setMode('branch')" />
      <i18n-t keypath="version.branch" tag="span">
        <template #base
          ><code>{{ base }}</code></template
        >
        <template #suffix
          ><code>{{ props.choice.branchSuffix }}</code></template
        >
      </i18n-t>
      <span class="hint">{{ t("version.branchHint", { branch: props.choice.localBranch ?? "" }) }}</span>
    </label>

    <label v-if="props.choice.localVersion" class="check option">
      <input type="radio" :checked="props.choice.mode === 'local'" @change="setMode('local')" />
      <i18n-t keypath="version.local" tag="span">
        <template #version
          ><code>{{ props.choice.localVersion }}</code></template
        >
      </i18n-t>
      <span class="hint">{{ t("version.localHint") }}</span>
    </label>

    <label v-if="base" class="check option">
      <input type="radio" :checked="props.choice.mode === 'suffix'" @change="setMode('suffix')" />
      <i18n-t keypath="version.suffix" tag="span">
        <template #base
          ><code>{{ base }}</code></template
        >
      </i18n-t>
      <input
        class="input input-sm mono suffix"
        placeholder="ABC-123"
        :value="props.choice.suffix"
        @focus="setMode('suffix')"
        @input="emit('patch', { suffix: ($event.target as HTMLInputElement).value })"
      />
    </label>

    <label v-for="range in props.choice.currentRanges" :key="range" class="check option">
      <input
        type="radio"
        :checked="props.choice.mode === 'keep' && props.choice.keepRange === range"
        @change="setMode('keep', range)"
      />
      <i18n-t keypath="version.keep" tag="span">
        <template #range
          ><code>{{ range }}</code></template
        >
      </i18n-t>
      <span class="hint">{{ t("version.keepHint") }}</span>
    </label>

    <label class="check option">
      <input type="radio" :checked="props.choice.mode === 'manual'" @change="setMode('manual')" />
      <span>{{ t("version.exact") }}</span>
      <input
        class="input input-sm mono manual"
        placeholder="1.2.3-ABC-001"
        :value="props.choice.manualVersion"
        @focus="setMode('manual')"
        @input="emit('patch', { manualVersion: ($event.target as HTMLInputElement).value })"
      />
    </label>

    <div class="result">
      <span v-if="problem" class="badge badge-warn">{{ t(problem) }}</span>
      <span v-else class="badge badge-accent mono">{{ props.choice.name }}@{{ resolved }}</span>
      <span v-if="props.choice.saveDev" class="badge">--save-dev</span>
    </div>
  </div>
</template>

<style scoped>
.picker {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px 12px 30px;
  background: var(--bg-input);
  border-top: 1px solid var(--border);
}

.option {
  gap: 10px;
  flex-wrap: wrap;
}

.hint {
  font-size: 12px;
  color: var(--text-faint);
}

.suffix {
  width: 140px;
}

.manual {
  width: 200px;
}

.result {
  display: flex;
  gap: 8px;
  margin-top: 4px;
}
</style>
