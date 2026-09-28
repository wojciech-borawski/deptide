import hljs from "highlight.js/lib/core";
import css from "highlight.js/lib/languages/css";
import javascript from "highlight.js/lib/languages/javascript";
import json from "highlight.js/lib/languages/json";
import markdown from "highlight.js/lib/languages/markdown";
import scss from "highlight.js/lib/languages/scss";
import typescript from "highlight.js/lib/languages/typescript";
import xml from "highlight.js/lib/languages/xml";
import yaml from "highlight.js/lib/languages/yaml";

import type { Language, WordRange } from "./diff-view";

export interface Token {
  text: string;
  scope: string | null;
}

const grammars = { css, javascript, json, markdown, scss, typescript, xml, yaml };
for (const [name, grammar] of Object.entries(grammars)) hljs.registerLanguage(name, grammar);

export const highlightLimit = 256 * 1024;

const entities: Record<string, string> = { "&amp;": "&", "&lt;": "<", "&gt;": ">", "&quot;": '"', "&#x27;": "'" };

const markup = /<span class="([^"]*)">|<\/span>|[^<]+/g;

const plainClassNames = /^[\w -]+$/;

function decode(text: string): string {
  return text.replace(/&(?:amp|lt|gt|quot|#x27);/g, (entity) => entities[entity] ?? entity);
}

function escapeHtml(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function tokensFromHtml(html: string, lineCount: number): Token[][] {
  const lines: Token[][] = [[]];
  const scopes: string[] = [];

  for (const match of html.matchAll(markup)) {
    if (match[1] !== undefined) {
      scopes.push(match[1]);
    } else if (match[0] === "</span>") {
      scopes.pop();
    } else {
      const scope = scopes[scopes.length - 1] ?? null;
      decode(match[0])
        .split("\n")
        .forEach((part, index) => {
          if (index > 0) lines.push([]);
          if (part) lines[lines.length - 1]?.push({ text: part, scope });
        });
    }
  }

  while (lines.length < lineCount) lines.push([]);
  return lines.slice(0, lineCount);
}

export function highlightLanguage(language: Language | null, sizes: readonly number[]): Language | null {
  return sizes.some((size) => size > highlightLimit) ? null : language;
}

export function highlightLines(lines: readonly string[], language: Language | null): Token[][] {
  if (!language) return lines.map((text) => (text ? [{ text, scope: null }] : []));
  const html = hljs.highlight(lines.join("\n"), { language, ignoreIllegals: true }).value;
  return tokensFromHtml(html, lines.length);
}

function wrap(text: string, classes: readonly string[]): string {
  const escaped = escapeHtml(text);
  return classes.length ? `<span class="${classes.join(" ")}">${escaped}</span>` : escaped;
}

export function lineHtml(tokens: readonly Token[], words: readonly WordRange[]): string {
  let html = "";
  let offset = 0;

  for (const token of tokens) {
    const scope = token.scope && plainClassNames.test(token.scope) ? [token.scope] : [];
    const end = offset + token.text.length;
    const cuts = new Set([offset, end]);
    for (const range of words) {
      if (range.start > offset && range.start < end) cuts.add(range.start);
      if (range.end > offset && range.end < end) cuts.add(range.end);
    }
    const edges = [...cuts].sort((left, right) => left - right);

    for (let index = 0; index < edges.length - 1; index += 1) {
      const from = edges[index] ?? offset;
      const to = edges[index + 1] ?? end;
      const changed = words.some((range) => range.start <= from && to <= range.end);
      html += wrap(token.text.slice(from - offset, to - offset), changed ? [...scope, "diff-word"] : scope);
    }
    offset = end;
  }

  return html;
}
