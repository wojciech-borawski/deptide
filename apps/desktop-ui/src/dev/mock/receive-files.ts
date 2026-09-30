import type { FileSide, FileStatus, ReceiveFileContents } from "@/api/types";

interface MockPair {
  local: string | FileSide | null;
  received: string | FileSide | null;
}

const ordersLocal = `import { Router } from "express";

import { db } from "../db";
import { requireUser } from "../auth";
import type { Order, OrderLine } from "../types";

export const orders = Router();

orders.use(requireUser);

orders.get("/", async (request, response) => {
  const page = Number(request.query.page ?? 1);
  const rows = await db.orders.list({ page, size: 20 });
  response.json(rows);
});

orders.get("/:id", async (request, response) => {
  const order = await db.orders.find(request.params.id);
  if (!order) {
    response.status(404).end();
    return;
  }
  response.json(order);
});

function total(lines: OrderLine[]): number {
  return lines.reduce((sum, line) => sum + line.price * line.count, 0);
}

function describe(order: Order): string {
  return \`Order \${order.id} for \${order.customer}\`;
}

orders.post("/", async (request, response) => {
  const lines = request.body.lines as OrderLine[];
  const order = await db.orders.create({
    customer: request.user.id,
    lines,
    total: total(lines),
  });
  response.status(201).json(order);
});

orders.delete("/:id", async (request, response) => {
  await db.orders.remove(request.params.id);
  response.status(204).end();
});

/* Keeps the old export name working for the admin panel. */
export default orders;
`;

const ordersReceived = `import { Router } from "express";

import { db } from "../db";
import { requireUser, requireRole } from "../auth";
import type { Order, OrderLine } from "../types";

export const orders = Router();

orders.use(requireUser);

orders.get("/", async (request, response) => {
  const page = Number(request.query.page ?? 1);
  const size = Math.min(Number(request.query.size ?? 20), 100);
  const rows = await db.orders.list({ page, size });
  response.json(rows);
});

orders.get("/:id", async (request, response) => {
  const order = await db.orders.find(request.params.id);
  if (!order) {
    response.status(404).end();
    return;
  }
  response.json(order);
});

function total(lines: OrderLine[]): number {
  return lines.reduce((sum, line) => sum + line.price * line.amount, 0);
}

function describe(order: Order): string {
  return \`Order \${order.id} for \${order.customer}\`;
}

orders.post("/", async (request, response) => {
  const lines = request.body.lines as OrderLine[];
  const order = await db.orders.create({
    customer: request.user.id,
    lines,
    total: total(lines),
  });
  response.status(201).json(order);
});

orders.delete("/:id", requireRole("admin"), async (request, response) => {
  await db.orders.remove(request.params.id);
  response.status(204).end();
});

/* Keeps the old export name working for the admin panel. */
export default orders;
`;

const appLocal = `<template>
  <div class="app">
    <AppHeader :user="user" />
    <main>
      <RouterView />
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

import AppHeader from "./components/AppHeader.vue";
import { useSession } from "./session";

const session = useSession();
const user = computed(() => session.user);
</script>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  min-height: 100vh;
}
</style>
`;

const appReceived = `<template>
  <div class="app" :class="{ dark: session.dark }">
    <AppHeader :user="user" @logout="session.logout()" />
    <main>
      <RouterView />
    </main>
    <ToastHost />
  </div>
</template>

<script setup lang="ts">
import { computed } from "vue";

import AppHeader from "./components/AppHeader.vue";
import ToastHost from "./components/ToastHost.vue";
import { useSession } from "./session";

const session = useSession();
const user = computed(() => session.user);
</script>

<style scoped>
.app {
  display: flex;
  flex-direction: column;
  min-height: 100dvh;
}
</style>
`;

const mainLocal = [
  `import { createApp } from "vue";`,
  `import { createPinia } from "pinia";`,
  ``,
  `import App from "./App.vue";`,
  `import { router } from "./router";`,
  ``,
  `createApp(App).use(createPinia()).use(router).mount("#app");`,
  ``,
].join("\r\n");

const mainReceived = [
  `import { createApp } from "vue";`,
  `import { createPinia } from "pinia";   `,
  ``,
  `import App from "./App.vue";`,
  `import { router } from "./router";`,
  ``,
  ``,
  `createApp(App).use(createPinia()).use(router).mount("#app");`,
  ``,
].join("\n");

const themeLocal = `:root {\r\n  --brand: #0f766e;\r\n  --radius: 8px;\r\n}\r\n\r\nbody {\r\n  margin: 0;\r\n  font-family: system-ui, sans-serif;\r\n}\r\n`;

const packageLocal = `{
  "name": "shop-admin",
  "version": "2.3.0",
  "private": true,
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build"
  },
  "dependencies": {
    "pinia": "^2.1.7",
    "vue": "^3.4.21",
    "vue-router": "^4.3.0"
  }
}
`;

const packageReceived = `{
  "name": "shop-admin",
  "version": "2.4.0",
  "private": true,
  "scripts": {
    "dev": "vite",
    "build": "vue-tsc --noEmit && vite build",
    "test": "vitest run"
  },
  "dependencies": {
    "pinia": "^3.0.1",
    "vue": "^3.5.13",
    "vue-router": "^4.5.0"
  }
}
`;

const configLocal = `server:
  port: 8080
  host: 0.0.0.0
logging:
  level: info
features:
  - orders
  - refunds
`;

const configReceived = `server:
  port: 8080
  host: 0.0.0.0
  timeout: 30s
logging:
  level: debug
features:
  - orders
  - refunds
  - invoices
`;

const readme = `# Shop Admin

Admin panel for the shop.

## Development

1. \`npm install\`
2. \`npm run dev\`, then open **http://localhost:5173**.

> Needs the orders API running on port 8080.
`;

const newPanel = `<template>
  <section class="panel">
    <h2>{{ title }}</h2>
    <slot />
  </section>
</template>

<script setup lang="ts">
defineProps<{ title: string }>();
</script>
`;

const byteOrderMark = String.fromCharCode(0xfeff);

function generatedModule(prefix: string, lines: number, width: number): string {
  const rows = Array.from({ length: lines }, (_, index) => {
    const row = `export const ${prefix}${index} = "${String(index * 7919).padStart(8, "0")}"; //`;
    return row.padEnd(width, "-");
  });
  return rows.map((row) => `${row}\n`).join("");
}

const iconsLocal = generatedModule("icon", 4000, 80);

const routesLocal = generatedModule("route", 14000, 44);

const pairs: Record<string, MockPair> = {
  "src/handlers/orders.ts": { local: ordersLocal, received: ordersReceived },
  "src/App.vue": { local: appLocal, received: appReceived },
  "src/main.ts": { local: mainLocal, received: mainReceived },
  "src/styles/theme.css": { local: themeLocal, received: themeLocal.replace(/\r\n/g, "\n") },
  "package.json": { local: packageLocal, received: `${byteOrderMark}${packageReceived}` },
  "config/app.yml": { local: configLocal, received: configReceived },
  "README.md": { local: readme, received: readme },
  "src/components/NewPanel.vue": { local: null, received: newPanel },
  "public/favicon.ico": { local: { kind: "binary", size: 4286 }, received: { kind: "binary", size: 4286 } },
  "public/logo.png": { local: { kind: "binary", size: 18_204 }, received: { kind: "binary", size: 21_877 } },
  "src/generated/icons.ts": { local: iconsLocal, received: iconsLocal.replace("icon2000 =", "iconHome =") },
  "src/generated/routes.ts": { local: routesLocal, received: generatedModule("path", 14000, 44) },
  "src/generated/schema.json": {
    local: { kind: "tooLarge", size: 2_310_000 },
    received: { kind: "tooLarge", size: 2_480_000 },
  },
};

/** A stand-in for SHA-256: stable per content, 64 hex digits. */
function fakeSha256(value: string): string {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash = Math.imul(hash ^ value.charCodeAt(index), 0x01000193) >>> 0;
  }
  let digits = "";
  for (let round = 0; round < 8; round += 1) {
    hash = Math.imul(hash ^ round, 0x01000193) >>> 0;
    digits += hash.toString(16).padStart(8, "0");
  }
  return digits;
}

function toSide(value: string | FileSide | null): FileSide | null {
  if (value === null || typeof value !== "string") return value;
  const bom = value.startsWith(byteOrderMark);
  const text = bom ? value.slice(1) : value;
  const size = new TextEncoder().encode(value).length;
  return { kind: "text", text, bom, size, sha256: fakeSha256(value), utf8: true };
}

function generated(relative: string, status: FileStatus): MockPair {
  const body = [`// ${relative}`, `export const name = "${relative}";`, `export const version = 1;`, ``].join("\n");
  const changed = body.replace("version = 1", "version = 2");
  if (status === "added") return { local: null, received: body };
  if (status === "removed") return { local: body, received: null };
  if (status === "identical") return { local: body, received: body };
  if (status === "whitespace") return { local: body.replace(/\n/g, "\r\n"), received: body };
  return { local: body, received: changed };
}

export function mockReceiveFile(relative: string, status: FileStatus): ReceiveFileContents {
  const pair = pairs[relative] ?? generated(relative, status);
  return { local: toSide(pair.local), received: toSide(pair.received) };
}
