import { describe, expect, it } from "vitest";

import { useGraphHighlight } from "@/composables/useGraphHighlight";

describe("graph highlight", () => {
  it("follows the pointer and ends when the pointer leaves", () => {
    const highlight = useGraphHighlight();

    highlight.enter("core");
    expect(highlight.active.value).toBe("core");
    highlight.leave("core");
    expect(highlight.active.value).toBeNull();
  });

  it("follows keyboard focus and ends on blur", () => {
    const highlight = useGraphHighlight();

    highlight.focus("ui");
    expect(highlight.active.value).toBe("ui");
    highlight.focus("web");
    highlight.blur("ui");
    expect(highlight.active.value).toBe("web");
    highlight.blur("web");
    expect(highlight.active.value).toBeNull();
  });

  it("keeps a tapped node until it is tapped again, Escape is pressed or the background is tapped", () => {
    const highlight = useGraphHighlight();

    highlight.toggle("core");
    highlight.leave("core");
    expect(highlight.active.value).toBe("core");
    highlight.toggle("core");
    expect(highlight.active.value).toBeNull();

    highlight.toggle("ui");
    highlight.clear();
    expect(highlight.active.value).toBeNull();
  });

  it("lets the pointer and focus win over a pinned node while they last", () => {
    const highlight = useGraphHighlight();
    highlight.toggle("core");

    highlight.enter("web");
    expect(highlight.active.value).toBe("web");
    highlight.leave("web");
    highlight.focus("ui");
    expect(highlight.active.value).toBe("ui");
    highlight.blur("ui");
    expect(highlight.active.value).toBe("core");
  });
});
