import type { Environment } from "vitest/environments";

export default {
  name: "client-node",
  viteEnvironment: "client",
  setup: () => ({ teardown: () => undefined }),
} satisfies Environment;
