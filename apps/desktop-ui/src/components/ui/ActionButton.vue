<script setup lang="ts">
import AppIcon, { type IconName } from "./AppIcon.vue";

export type ButtonVariant = "default" | "primary" | "danger" | "ghost";

const props = withDefaults(
  defineProps<{
    busy?: boolean;
    icon?: IconName;
    variant?: ButtonVariant;
    small?: boolean;
    disabled?: boolean;
    type?: "button" | "submit";
    title?: string;
  }>(),
  { busy: false, icon: undefined, variant: "default", small: false, disabled: false, type: "button", title: undefined },
);
</script>

<template>
  <button
    v-ripple
    class="btn"
    :class="{ [`btn-${props.variant}`]: props.variant !== 'default', 'btn-sm': props.small }"
    :type="props.type"
    :disabled="props.disabled || props.busy"
    :title="props.title"
  >
    <span v-if="props.busy" class="spinner" />
    <AppIcon v-else-if="props.icon" :name="props.icon" :size="props.small ? 14 : 15" />
    <slot />
  </button>
</template>
