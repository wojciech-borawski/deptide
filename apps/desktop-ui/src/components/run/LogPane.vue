<script setup lang="ts">
import { nextTick, ref, watch } from "vue";
import { useI18n } from "vue-i18n";

import AppIcon from "@/components/ui/AppIcon.vue";
import { useCountedNoun } from "@/composables/useCountedNoun";

const props = defineProps<{ title: string; lines: string[] }>();
const { t } = useI18n();
const countOf = useCountedNoun();

const pane = ref<HTMLElement | null>(null);
const follow = ref(true);

function onScroll(): void {
  const element = pane.value;
  if (!element) return;
  const distance = element.scrollHeight - element.scrollTop - element.clientHeight;
  follow.value = distance < 24;
}

function scrollToBottom(): void {
  const element = pane.value;
  if (element) element.scrollTop = element.scrollHeight;
}

function lineClass(line: string): string {
  if (line.startsWith("$ ")) return "cmd";
  if (/\b(ERR!|error|failed)\b/i.test(line)) return "err";
  if (/\b(WARN|warning)\b/i.test(line)) return "warn";
  return "";
}

watch(
  () => props.lines.length,
  async () => {
    if (!follow.value) return;
    await nextTick();
    scrollToBottom();
  },
);

watch(
  () => props.title,
  async () => {
    follow.value = true;
    await nextTick();
    scrollToBottom();
  },
);
</script>

<template>
  <section class="log">
    <header class="log-header">
      <span class="row">
        <AppIcon name="terminal" :size="15" />
        <strong>{{ props.title }}</strong>
        <span class="muted">{{ countOf("line", props.lines.length) }}</span>
      </span>
      <label class="switch">
        <input v-model="follow" type="checkbox" @change="follow && scrollToBottom()" />
        <span class="muted">{{ t("common.followOutput") }}</span>
      </label>
    </header>
    <pre
      ref="pane"
      class="output selectable"
      @scroll="onScroll"
    ><template v-for="(line, index) in props.lines" :key="index"><span :class="lineClass(line)">{{ line }}</span>
</template><span v-if="!props.lines.length" class="faint">{{ t("common.noOutputYet") }}</span></pre>
  </section>
</template>

<style scoped>
.log {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-input);
  overflow: hidden;
}

.log-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  background: var(--bg-elevated);
  border-bottom: 1px solid var(--border);
  font-size: 12.5px;
}

.output {
  flex: 1;
  margin: 0;
  padding: 10px 14px;
  overflow: auto;
  font-family: var(--font-mono);
  font-size: 12px;
  line-height: 1.55;
  color: var(--text);
  white-space: pre-wrap;
  word-break: break-word;
  min-height: 0;
}

.cmd {
  color: var(--accent);
  font-weight: 600;
}

.err {
  color: var(--failed);
}

.warn {
  color: var(--warn);
}
</style>
