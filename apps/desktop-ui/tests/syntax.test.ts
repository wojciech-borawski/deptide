import { describe, expect, it } from "vitest";

import { highlightLanguage, highlightLimit, highlightLines, lineHtml } from "@/lib/syntax";

describe("highlightLines", () => {
  it("returns one token list per line, even when a token spans lines", () => {
    const lines = ["/* first", "   second */", "const a = `x", "y`;"];

    const highlighted = highlightLines(lines, "typescript");

    expect(highlighted).toHaveLength(4);
    expect(highlighted.map((tokens) => tokens.map((token) => token.text).join(""))).toEqual(lines);
    expect(highlighted[0]).toEqual([{ text: "/* first", scope: "hljs-comment" }]);
    expect(highlighted[1]).toEqual([{ text: "   second */", scope: "hljs-comment" }]);
    expect(highlighted[3]?.[0]).toEqual({ text: "y`", scope: "hljs-string" });
  });

  it("decodes what highlight.js escaped so the text matches the file", () => {
    const lines = [`<a href="x">&amp; 'q'</a>`];

    const highlighted = highlightLines(lines, "xml");

    expect(highlighted[0]?.map((token) => token.text).join("")).toBe(lines[0]);
    expect(highlighted[0]?.some((token) => token.scope === "hljs-name" && token.text === "a")).toBe(true);
  });

  it("keeps plain text as one unscoped token per line", () => {
    expect(highlightLines(["a <b>", ""], null)).toEqual([[{ text: "a <b>", scope: null }], []]);
  });

  it("keeps empty lines in place between and inside highlighted tokens", () => {
    expect(highlightLines(["{", "", "}"], "json")).toEqual([
      [{ text: "{", scope: "hljs-punctuation" }],
      [],
      [{ text: "}", scope: "hljs-punctuation" }],
    ]);
    expect(highlightLines(["/* a", "", "b */", "x"], "typescript")).toEqual([
      [{ text: "/* a", scope: "hljs-comment" }],
      [],
      [{ text: "b */", scope: "hljs-comment" }],
      [{ text: "x", scope: null }],
    ]);
  });
});

describe("highlightLanguage", () => {
  it("keeps the language while both sides are at most 256 KB", () => {
    expect(highlightLimit).toBe(256 * 1024);
    expect(highlightLanguage("typescript", [highlightLimit, 10])).toBe("typescript");
    expect(highlightLanguage("typescript", [highlightLimit])).toBe("typescript");
  });

  it("turns highlighting off when either side is larger", () => {
    expect(highlightLanguage("typescript", [10, highlightLimit + 1])).toBeNull();
    expect(highlightLanguage("json", [highlightLimit + 1])).toBeNull();
    expect(highlightLanguage(null, [10])).toBeNull();
  });
});

describe("lineHtml", () => {
  it("escapes every character of the text", () => {
    expect(lineHtml([{ text: `<img src=x onerror="a('&')">`, scope: null }], [])).toBe(
      "&lt;img src=x onerror=&quot;a(&#39;&amp;&#39;)&quot;&gt;",
    );
  });

  it("escapes text inside a highlighted scope and inside a changed-word range", () => {
    expect(lineHtml([{ text: `"<a & 'b'>"`, scope: "hljs-string" }], [])).toBe(
      '<span class="hljs-string">&quot;&lt;a &amp; &#39;b&#39;&gt;&quot;</span>',
    );
    expect(lineHtml([{ text: `x <&"'>`, scope: null }], [{ start: 2, end: 7 }])).toBe(
      'x <span class="diff-word">&lt;&amp;&quot;&#39;&gt;</span>',
    );
  });

  it("wraps scoped tokens and splits them at word range edges", () => {
    const html = lineHtml(
      [
        { text: "const", scope: "hljs-keyword" },
        { text: " total = 1;", scope: null },
      ],
      [{ start: 3, end: 8 }],
    );

    expect(html).toBe(
      '<span class="hljs-keyword">con</span>' +
        '<span class="hljs-keyword diff-word">st</span>' +
        '<span class="diff-word"> to</span>' +
        "tal = 1;",
    );
  });

  it("drops scope names that are not plain class names", () => {
    expect(lineHtml([{ text: "x", scope: 'a" onclick="b' }], [])).toBe("x");
  });
});
