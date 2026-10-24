<script setup lang="ts">
import { computed, ref } from "vue";
import { useElementSize } from "@vueuse/core";
import { useI18n } from "vue-i18n";

import type { DependencySection } from "@/api/types";
import AppIcon from "@/components/ui/AppIcon.vue";
import EmptyState from "@/components/ui/EmptyState.vue";
import NoticeBanner from "@/components/ui/NoticeBanner.vue";
import { useGraphHighlight } from "@/composables/useGraphHighlight";
import { ARROW_SIZE, layoutGraph, type PlacedNode } from "@/lib/dependency-graph-layout";
import { sectionField } from "@/lib/dependency-sections";
import { useDependenciesStore } from "@/stores/dependencies";

const MIN_GRAPH_WIDTH = 480;
const CHARACTER_WIDTH = 7;
const LABEL_PADDING = 20;

const dependencies = useDependenciesStore();
const { t } = useI18n();
const highlight = useGraphHighlight();

const frame = ref<HTMLElement | null>(null);
const { width: frameWidth } = useElementSize(frame);

const graph = computed(() => dependencies.graph);
const layout = computed(() =>
  graph.value ? layoutGraph(graph.value, Math.max(MIN_GRAPH_WIDTH, Math.floor(frameWidth.value))) : null,
);
const sections: DependencySection[] = ["dependencies", "dev", "peer"];

const linkedToActive = computed(() => {
  const active = highlight.active.value;
  const names = new Set<string>();
  if (!active || !graph.value) return names;
  names.add(active);
  for (const edge of graph.value.edges) {
    if (edge.consumer === active) names.add(edge.library);
    if (edge.library === active) names.add(edge.consumer);
  }
  return names;
});

function isEdgeOn(consumer: string, library: string): boolean {
  const active = highlight.active.value;
  return active === consumer || active === library;
}

function label(node: PlacedNode): string {
  const room = Math.max(4, Math.floor((node.width - LABEL_PADDING) / CHARACTER_WIDTH));
  return node.name.length > room ? `${node.name.slice(0, room - 1)}…` : node.name;
}

function describeNode(name: string): string {
  const edges = graph.value?.edges ?? [];
  const uses = edges.filter((edge) => edge.consumer === name).map((edge) => edge.library);
  const usedBy = edges.filter((edge) => edge.library === name).map((edge) => edge.consumer);
  const none = t("common.none");
  return t("dependencies.graph.nodeLabel", {
    name,
    uses: uses.join(", ") || none,
    usedBy: usedBy.join(", ") || none,
  });
}
</script>

<template>
  <div class="stack">
    <div class="toolbar">
      <p class="muted intro">{{ t("dependencies.graph.intro") }}</p>
      <span class="spacer" />
      <button
        class="btn btn-ghost btn-sm"
        type="button"
        :disabled="dependencies.graphBusy"
        @click="dependencies.loadGraph"
      >
        <AppIcon name="refresh" :size="14" />
        {{ t("common.refresh") }}
      </button>
    </div>

    <NoticeBanner v-if="dependencies.graphError" tone="error" selectable>{{ dependencies.graphError }}</NoticeBanner>
    <NoticeBanner v-for="cycle in graph?.cycles ?? []" :key="cycle.join('|')" tone="warn">
      {{ t("dependencies.graph.cycle", { names: cycle.join(", ") }) }}
    </NoticeBanner>

    <p v-if="dependencies.graphBusy && !graph" class="muted" role="status">
      <span class="spinner" aria-hidden="true" /> {{ t("dependencies.graph.loading") }}
    </p>

    <template v-else-if="graph && layout">
      <EmptyState
        v-if="!layout.nodes.length"
        icon="graph"
        :title="t('dependencies.graph.emptyTitle')"
        :text="t('dependencies.graph.emptyText')"
      />

      <div v-else class="card graph-card">
        <div ref="frame" class="frame" @click.self="highlight.clear()">
          <svg
            class="graph"
            :width="layout.width"
            :height="layout.height"
            :viewBox="`0 0 ${layout.width} ${layout.height}`"
            role="group"
            :aria-label="t('dependencies.graph.title')"
            @click.self="highlight.clear()"
            @keydown.escape="highlight.clear()"
          >
            <defs>
              <marker
                v-for="marker in ['dependency-arrow', 'dependency-arrow-on']"
                :id="marker"
                :key="marker"
                class="arrowhead"
                :class="{ on: marker === 'dependency-arrow-on' }"
                viewBox="0 0 8 8"
                refX="8"
                refY="4"
                :markerWidth="ARROW_SIZE"
                :markerHeight="ARROW_SIZE"
                markerUnits="userSpaceOnUse"
                orient="auto"
              >
                <path d="M 0 0 L 8 4 L 0 8 z" />
              </marker>
            </defs>
            <path
              v-for="edge in layout.edges"
              :key="`${edge.consumer}->${edge.library}`"
              class="edge"
              :class="[
                edge.section,
                {
                  on: isEdgeOn(edge.consumer, edge.library),
                  dim: highlight.active.value && !isEdgeOn(edge.consumer, edge.library),
                },
              ]"
              :d="edge.path"
              :marker-end="`url(#${isEdgeOn(edge.consumer, edge.library) ? 'dependency-arrow-on' : 'dependency-arrow'})`"
              aria-hidden="true"
            />
            <g
              v-for="node in layout.nodes"
              :key="node.name"
              class="node"
              :class="{
                on: linkedToActive.has(node.name),
                dim: highlight.active.value && !linkedToActive.has(node.name),
                current: highlight.active.value === node.name,
              }"
              tabindex="0"
              role="button"
              :aria-pressed="highlight.pinned.value === node.name"
              :aria-label="describeNode(node.name)"
              @pointerenter="highlight.enter(node.name)"
              @pointerleave="highlight.leave(node.name)"
              @focus="highlight.focus(node.name)"
              @blur="highlight.blur(node.name)"
              @click="highlight.toggle(node.name)"
              @keydown.enter.prevent="highlight.toggle(node.name)"
              @keydown.space.prevent="highlight.toggle(node.name)"
            >
              <title>{{ describeNode(node.name) }}</title>
              <rect :x="node.x" :y="node.y" :width="node.width" :height="node.height" rx="6" />
              <text :x="node.x + 10" :y="node.y + node.height / 2" dominant-baseline="central">{{ label(node) }}</text>
            </g>
          </svg>
        </div>

        <div class="legend" aria-hidden="true">
          <span v-for="section in sections" :key="section" class="legend-entry">
            <svg width="28" height="8"><path class="edge" :class="section" d="M 0 4 L 28 4" /></svg>
            <span class="mono">{{ sectionField[section] }}</span>
          </span>
        </div>
      </div>

      <p v-if="layout.unlinked.length" class="muted">
        {{ t("dependencies.graph.unlinked", { names: layout.unlinked.join(", ") }) }}
      </p>

      <section>
        <h3 class="section-title">{{ t("dependencies.graph.buildOrder") }}</h3>
        <p class="muted hint">{{ t("dependencies.graph.buildOrderHint") }}</p>
        <ol class="order">
          <li v-for="node in graph.nodes" :key="node.name">
            <span class="order-name">{{ node.name }}</span>
            <span v-if="node.dependsOn.length" class="muted">
              {{ t("dependencies.graph.waitsFor", { names: node.dependsOn.join(", ") }) }}
            </span>
          </li>
        </ol>
      </section>
    </template>
  </div>
</template>

<style scoped>
.intro {
  max-width: 760px;
}

.spacer {
  flex: 1;
}

.graph-card {
  padding: 12px;
}

.frame {
  overflow-x: auto;
}

.graph {
  display: block;
}

.edge {
  fill: none;
  stroke: var(--border-strong);
  stroke-width: 1.6;
  transition:
    stroke 0.12s ease,
    opacity 0.12s ease;
}

.edge {
  stroke-linejoin: round;
}

.arrowhead path {
  fill: var(--border-strong);
}

.arrowhead.on path {
  fill: var(--accent);
}

.edge.dev {
  stroke-dasharray: 6 4;
}

.edge.peer {
  stroke-dasharray: 2 4;
}

.edge.on {
  stroke: var(--accent);
  stroke-width: 2.4;
}

.edge.dim {
  opacity: 0.25;
}

.node {
  cursor: pointer;
  outline: none;
}

.node rect {
  fill: var(--bg-elevated);
  stroke: var(--border-strong);
  stroke-width: 1.2;
}

.node text {
  fill: var(--text);
  font-size: 12.5px;
  font-weight: 600;
}

.node.on rect {
  stroke: var(--accent);
}

.node.current rect {
  fill: var(--accent-soft);
  stroke-width: 2;
}

.node.dim {
  opacity: 0.4;
}

.node:focus-visible rect {
  stroke: var(--accent);
  stroke-width: 3;
}

.legend {
  display: flex;
  flex-wrap: wrap;
  gap: 16px;
  margin-top: 10px;
  color: var(--text-muted);
  font-size: 12px;
}

.legend-entry {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.section-title {
  margin-bottom: 4px;
}

.hint {
  margin-bottom: 8px;
}

.order {
  display: flex;
  flex-direction: column;
  gap: 4px;
  padding-left: 24px;
}

.order-name {
  font-weight: 600;
  margin-right: 8px;
}
</style>
