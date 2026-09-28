<script setup lang="ts">
import { computed, ref, shallowRef, watch } from "vue";
import { useI18n } from "vue-i18n";

import type { FileSide, ReceiveFile, ReceiveFileContents } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { describeError } from "@/composables/useAsyncAction";
import {
  diffTexts,
  hasChanges,
  languageFor,
  rowCount,
  splitLines,
  splitRows,
  tooManyRows,
  unifiedRows,
  wholeFile,
  type DiffLine,
  type Gap,
  type LineEnding,
  type LineKind,
  type LineSide,
  type SplitCell,
} from "@/lib/diff-view";
import { formatBytes } from "@/lib/format";
import { statusTones } from "@/lib/receive-view";
import { highlightLanguage, highlightLines, lineHtml, type Token } from "@/lib/syntax";
import { useTransferStore } from "@/stores/transfer";
import { useUiStore, type DiffMode } from "@/stores/ui";
import { useWorkspaceStore } from "@/stores/workspace";

const props = defineProps<{ source: string; file: ReceiveFile }>();
const emit = defineEmits<{ close: [] }>();
const transfer = useTransferStore();
const ui = useUiStore();
const workspace = useWorkspaceStore();
const { t } = useI18n();

type TextSide = Extract<FileSide, { kind: "text" }>;

type Which = "old" | "new";

type UnifiedView = Gap | { kind: "line"; line: DiffLine; side: LineSide; which: Which };

const modes: { mode: DiffMode; label: string }[] = [
  { mode: "unified", label: "transfer.filePreview.unified" },
  { mode: "split", label: "transfer.filePreview.split" },
];
const signs: Record<LineKind, string> = { added: "+", removed: "-", context: "" };
const endingMarks: Record<LineEnding, string> = { crlf: "␍␊", lf: "␊", cr: "␍", none: "⊘" };

const contents = shallowRef<ReceiveFileContents | null>(null);
const error = ref<string | null>(null);
const hideWhitespace = ref(false);
const expanded = shallowRef<ReadonlySet<number>>(new Set());
const accepted = ref(0);
const section = ref<HTMLElement | null>(null);
let request = 0;

watch(
  () => [props.source, props.file.relative] as const,
  async ([source, relative]) => {
    const current = ++request;
    contents.value = null;
    error.value = null;
    hideWhitespace.value = props.file.status === "whitespace";
    expanded.value = new Set();
    accepted.value = 0;
    try {
      const loaded = await transfer.readFile(workspace.root, source, relative);
      if (current === request) contents.value = loaded;
    } catch (cause) {
      if (current === request) error.value = describeError(cause);
    }
  },
  { immediate: true },
);

watch(hideWhitespace, () => (expanded.value = new Set()));

function asText(side: FileSide | null | undefined): TextSide | null {
  return side?.kind === "text" ? side : null;
}

const local = computed(() => contents.value?.local ?? null);
const received = computed(() => contents.value?.received ?? null);
const oldText = computed(() => asText(local.value));
const newText = computed(() => asText(received.value));
const isDiff = computed(() => !!oldText.value && !!newText.value && props.file.status !== "identical");

const blocked = computed((): string | null => {
  const sides = [local.value, received.value].filter((side): side is FileSide => side !== null);
  if (!contents.value) return null;
  if (!sides.length) return t("transfer.filePreview.missing");
  if (sides.some((side) => side.kind === "binary")) {
    return local.value && received.value
      ? t("transfer.filePreview.binaryBoth", {
          local: formatBytes(local.value.size),
          received: formatBytes(received.value.size),
        })
      : t("transfer.filePreview.binary", { size: formatBytes(sides[0]?.size ?? 0) });
  }
  if (sides.some((side) => side.kind === "tooLarge")) {
    return t("transfer.filePreview.tooLarge", { size: formatBytes(Math.max(...sides.map((side) => side.size))) });
  }
  return null;
});

const lines = computed((): DiffLine[] => {
  if (blocked.value !== null) return [];
  if (isDiff.value && oldText.value && newText.value) {
    return diffTexts(oldText.value.text, newText.value.text, { ignoreWhitespace: hideWhitespace.value });
  }
  if (newText.value && !oldText.value) return wholeFile(newText.value.text, "added");
  if (oldText.value && !newText.value) return wholeFile(oldText.value.text, "removed");
  return wholeFile((newText.value ?? oldText.value)?.text ?? "", "context");
});

const textSizes = computed(() => [oldText.value, newText.value].flatMap((side) => (side ? [side.size] : [])));
const language = computed(() => highlightLanguage(languageFor(props.file.relative), textSizes.value));
const highlightOff = computed(() => language.value === null && languageFor(props.file.relative) !== null);

function tokensFor(side: TextSide | null): Token[][] {
  if (!side || blocked.value !== null) return [];
  return highlightLines(
    splitLines(side.text).map((line) => line.text),
    language.value,
  );
}

const oldTokens = computed(() => tokensFor(oldText.value));
const newTokens = computed(() => tokensFor(newText.value));

const mode = computed<DiffMode>(() => (isDiff.value ? ui.diffMode : "unified"));
const shown = computed(() => (isDiff.value ? expanded.value : "all"));
const large = computed(() => tooManyRows(lines.value, shown.value, mode.value, accepted.value));
const unified = computed((): UnifiedView[] => {
  if (mode.value !== "unified" || large.value) return [];
  return unifiedRows(lines.value, shown.value).flatMap((row): UnifiedView[] => {
    if (row.kind === "gap") return [row];
    const { line } = row;
    if (line.new) return [{ kind: "line", line, side: line.new, which: "new" }];
    return line.old ? [{ kind: "line", line, side: line.old, which: "old" }] : [];
  });
});
const split = computed(() => (mode.value === "split" && !large.value ? splitRows(lines.value, shown.value) : []));
const unchanged = computed(() => isDiff.value && !hasChanges(lines.value));

const bomNote = computed(() => {
  if (!oldText.value || !newText.value || oldText.value.bom === newText.value.bom) return null;
  return newText.value.bom ? t("transfer.filePreview.bomReceived") : t("transfer.filePreview.bomLocal");
});

const sizes = computed(() => {
  if (local.value && received.value) {
    return t("transfer.filePreview.sizes", {
      local: formatBytes(local.value.size),
      received: formatBytes(received.value.size),
    });
  }
  const only = local.value ?? received.value;
  return only ? formatBytes(only.size) : "";
});

function showAnyway(): void {
  accepted.value = rowCount(lines.value, shown.value, mode.value);
  section.value?.focus();
}

function expand(id: number): void {
  expanded.value = new Set([...expanded.value, id]);
}

function code(side: LineSide, which: Which): string {
  const tokens = (which === "old" ? oldTokens.value : newTokens.value)[side.number - 1] ?? [];
  return lineHtml(tokens, side.words);
}

function unifiedKey(line: DiffLine): string {
  return `${line.old?.number ?? ""}:${line.new?.number ?? ""}`;
}

function splitKey(left: SplitCell | null, right: SplitCell | null): string {
  return `${left?.side.number ?? ""}:${right?.side.number ?? ""}`;
}
</script>

<template>
  <section ref="section" class="preview" tabindex="-1" :aria-label="props.file.relative" @keydown.esc="emit('close')">
    <header class="head">
      <div class="title">
        <span class="mono truncate selectable path" :title="props.file.relative">{{ props.file.relative }}</span>
        <span class="badge" :class="statusTones[props.file.status]">{{
          t(`transfer.statuses.${props.file.status}`)
        }}</span>
        <span class="mono muted sizes">{{ sizes }}</span>
        <span class="spacer" />
        <button
          class="close"
          type="button"
          :title="t('transfer.filePreview.close')"
          :aria-label="t('transfer.filePreview.close')"
          @click="emit('close')"
        >
          <AppIcon name="close" :size="14" />
        </button>
      </div>
      <div v-if="isDiff && !blocked" class="toolbar options">
        <span class="segmented">
          <button
            v-for="entry in modes"
            :key="entry.mode"
            class="chip"
            :class="{ active: ui.diffMode === entry.mode }"
            type="button"
            :aria-pressed="ui.diffMode === entry.mode"
            @click="ui.diffMode = entry.mode"
          >
            {{ t(entry.label) }}
          </button>
        </span>
        <label class="switch small">
          <input v-model="hideWhitespace" type="checkbox" />
          {{ t("transfer.filePreview.hideWhitespace") }}
        </label>
      </div>
      <p v-if="bomNote" class="muted small note">{{ bomNote }}</p>
      <p v-if="highlightOff && !blocked" class="muted small note">{{ t("transfer.filePreview.highlightOff") }}</p>
    </header>

    <div class="body">
      <NoticeBanner v-if="error" tone="error" selectable>{{ error }}</NoticeBanner>
      <p v-else-if="!contents" class="muted small message">{{ t("transfer.filePreview.loading") }}</p>
      <p v-else-if="blocked" class="muted message">{{ blocked }}</p>
      <div v-else-if="large" class="message large">
        <p class="muted">{{ t("transfer.filePreview.largeDiff") }}</p>
        <button class="btn btn-sm" type="button" @click="showAnyway">
          {{ t("transfer.filePreview.showAnyway") }}
        </button>
      </div>
      <template v-else>
        <p v-if="unchanged" class="muted small message">
          {{ hideWhitespace ? t("transfer.filePreview.onlyWhitespace") : t("transfer.filePreview.noDifferences") }}
        </p>
        <p v-else-if="!lines.length" class="muted small message">{{ t("transfer.filePreview.empty") }}</p>

        <table v-if="mode === 'unified' && unified.length" class="diff unified mono">
          <colgroup>
            <col class="num" />
            <col class="num" />
            <col class="sign" />
            <col />
          </colgroup>
          <tbody>
            <template v-for="row in unified" :key="row.kind === 'gap' ? `gap${row.id}` : unifiedKey(row.line)">
              <tr v-if="row.kind === 'gap'" class="gap">
                <td colspan="4">
                  <button type="button" class="expand" @click="expand(row.id)">
                    <AppIcon name="chevronRight" :size="12" class="expand-icon" />
                    {{ t("transfer.filePreview.unchangedLines", { count: row.count }, row.count) }}
                  </button>
                </td>
              </tr>
              <tr v-else :class="row.line.kind">
                <td class="num">{{ row.line.old?.number }}</td>
                <td class="num">{{ row.line.new?.number }}</td>
                <td class="sign">{{ signs[row.line.kind] }}</td>
                <td class="code">
                  <!-- eslint-disable-next-line vue/no-v-html -- lineHtml escapes all file text -->
                  <span v-html="code(row.side, row.which)" /><span
                    v-if="row.side.endingChanged"
                    class="eol"
                    :title="t(`transfer.filePreview.endings.${row.side.ending}`)"
                    >{{ endingMarks[row.side.ending] }}</span
                  >
                </td>
              </tr>
            </template>
          </tbody>
        </table>

        <table v-else-if="mode === 'split' && split.length" class="diff split mono">
          <colgroup>
            <col class="num" />
            <col class="half" />
            <col class="num" />
            <col class="half" />
          </colgroup>
          <thead>
            <tr>
              <th colspan="2">{{ t("transfer.filePreview.oldSide") }}</th>
              <th colspan="2">{{ t("transfer.filePreview.newSide") }}</th>
            </tr>
          </thead>
          <tbody>
            <template v-for="row in split" :key="row.kind === 'gap' ? `gap${row.id}` : splitKey(row.left, row.right)">
              <tr v-if="row.kind === 'gap'" class="gap">
                <td colspan="4">
                  <button type="button" class="expand" @click="expand(row.id)">
                    <AppIcon name="chevronRight" :size="12" class="expand-icon" />
                    {{ t("transfer.filePreview.unchangedLines", { count: row.count }, row.count) }}
                  </button>
                </td>
              </tr>
              <tr v-else>
                <template v-for="(cell, index) in [row.left, row.right]" :key="index">
                  <td class="num" :class="cell ? cell.kind : 'empty'">{{ cell?.side.number }}</td>
                  <td class="code" :class="cell ? cell.kind : 'empty'">
                    <template v-if="cell">
                      <!-- eslint-disable-next-line vue/no-v-html -- lineHtml escapes all file text -->
                      <span v-html="code(cell.side, index === 0 ? 'old' : 'new')" /><span
                        v-if="cell.side.endingChanged"
                        class="eol"
                        :title="t(`transfer.filePreview.endings.${cell.side.ending}`)"
                        >{{ endingMarks[cell.side.ending] }}</span
                      >
                    </template>
                  </td>
                </template>
              </tr>
            </template>
          </tbody>
        </table>
      </template>
    </div>
  </section>
</template>

<style scoped>
.preview {
  display: flex;
  flex-direction: column;
  min-width: 0;
  min-height: 0;
  border: 1px solid var(--border);
  border-radius: var(--radius);
  background: var(--bg-panel);
  overflow: hidden;
}

.head {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border);
  background: var(--bg-elevated);
}

.title {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.path {
  min-width: 0;
  font-weight: 600;
}

.sizes {
  font-size: 11.5px;
  white-space: nowrap;
}

.spacer {
  flex: 1;
}

.close {
  display: grid;
  place-items: center;
  width: 24px;
  height: 24px;
  padding: 0;
  border: none;
  border-radius: var(--radius-sm);
  background: none;
  color: var(--text-muted);
  cursor: pointer;
  flex-shrink: 0;
}

.close:hover {
  background: var(--bg-hover);
  color: var(--text);
}

.options {
  gap: 12px;
}

.segmented {
  display: inline-flex;
  gap: 4px;
}

.switch.small,
.small {
  font-size: 12px;
}

.body {
  flex: 1 1 auto;
  min-height: 0;
  overflow: auto;
  user-select: text;
}

.message {
  padding: 12px;
}

.large {
  display: flex;
  flex-direction: column;
  align-items: flex-start;
  gap: 8px;
}

.diff {
  width: 100%;
  border-collapse: collapse;
  table-layout: fixed;
  font-size: 12px;
  line-height: 1.5;
}

.diff col.num {
  width: 48px;
}

.diff col.sign {
  width: 18px;
}

.diff th {
  padding: 4px 10px;
  text-align: left;
  font-family: var(--font);
  font-size: 11.5px;
  font-weight: 600;
  color: var(--text-muted);
  border-bottom: 1px solid var(--border);
}

.diff td {
  padding: 0 8px;
  vertical-align: top;
}

.num {
  text-align: right;
  color: var(--text-faint);
  user-select: none;
}

.sign {
  color: var(--text-muted);
  user-select: none;
}

.code {
  white-space: pre-wrap;
  overflow-wrap: anywhere;
  tab-size: 4;
  color: var(--text);
}

.split .code + .num {
  border-left: 1px solid var(--border);
}

.added {
  background: var(--diff-add-bg);
}

.removed {
  background: var(--diff-del-bg);
}

.empty {
  background: var(--bg-elevated);
}

.added :deep(.diff-word) {
  background: var(--diff-add-word);
}

.removed :deep(.diff-word) {
  background: var(--diff-del-word);
}

.eol {
  margin-left: 2px;
  padding: 0 2px;
  border-radius: 3px;
  font-size: 10.5px;
  color: var(--text-muted);
  background: var(--bg-hover);
  user-select: none;
}

.gap td {
  padding: 0;
  background: var(--bg-elevated);
  border-top: 1px solid var(--border);
  border-bottom: 1px solid var(--border);
}

.expand {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  width: 100%;
  padding: 3px 12px;
  border: none;
  background: none;
  color: var(--text-muted);
  font: inherit;
  font-family: var(--font);
  font-size: 11.5px;
  text-align: left;
  cursor: pointer;
}

.expand:hover {
  background: var(--bg-hover);
  color: var(--accent);
}

.expand-icon {
  transform: rotate(90deg);
}

.diff :deep(.hljs-keyword),
.diff :deep(.hljs-built_in),
.diff :deep(.hljs-literal),
.diff :deep(.hljs-selector-tag) {
  color: var(--syntax-keyword);
}

.diff :deep(.hljs-string),
.diff :deep(.hljs-regexp),
.diff :deep(.hljs-addition),
.diff :deep(.hljs-code) {
  color: var(--syntax-string);
}

.diff :deep(.hljs-number),
.diff :deep(.hljs-symbol),
.diff :deep(.hljs-bullet) {
  color: var(--syntax-number);
}

.diff :deep(.hljs-comment),
.diff :deep(.hljs-quote),
.diff :deep(.hljs-meta) {
  color: var(--syntax-comment);
}

.diff :deep(.hljs-title),
.diff :deep(.hljs-section),
.diff :deep(.hljs-selector-id),
.diff :deep(.hljs-selector-class) {
  color: var(--syntax-title);
}

.diff :deep(.hljs-attr),
.diff :deep(.hljs-attribute),
.diff :deep(.hljs-property),
.diff :deep(.hljs-variable),
.diff :deep(.hljs-params),
.diff :deep(.hljs-template-variable) {
  color: var(--syntax-attr);
}

.diff :deep(.hljs-name),
.diff :deep(.hljs-tag),
.diff :deep(.hljs-type) {
  color: var(--syntax-tag);
}

.diff :deep(.hljs-emphasis) {
  font-style: italic;
}

.diff :deep(.hljs-strong) {
  font-weight: 700;
}
</style>
