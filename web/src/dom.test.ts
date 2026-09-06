import { describe, expect, it } from "vitest";
import { appendText, replaceText } from "./dom";

describe("safe text rendering", () => {
  it("renders mission input as text and never creates executable elements", () => {
    const host = document.createElement("div");
    const malicious = "Mission <img src=x onerror=alert('unsafe')>";
    appendText(host, "strong", malicious);
    expect(host.textContent).toBe(malicious);
    expect(host.querySelector("img")).toBeNull();
  });

  it("replaces existing content with a text node", () => {
    const host = document.createElement("div");
    replaceText(host, "<script>unsafe</script>");
    expect(host.childNodes).toHaveLength(1);
    expect(host.firstChild?.nodeType).toBe(Node.TEXT_NODE);
  });
});
