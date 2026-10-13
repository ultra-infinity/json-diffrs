import { describe, expect, it } from "vitest";
import { matches, rank } from "./Combobox";

const options = ["azure.openai", "minimax/openai", "openai", "openrouter", "ollama"].map((id) => ({ id, label: id }));

describe("model search", () => {
  it("needs every word, in any case, in the label or id", () => {
    const gpt = { id: "gpt-5.4-mini", label: "GPT-5.4 Mini" };
    expect(matches(gpt, "gpt")).toBe(true);
    expect(matches(gpt, "GPT mini")).toBe(true);
    expect(matches(gpt, "gpt nano")).toBe(false);
    expect(matches(gpt, "")).toBe(true);
  });

  it("puts the exact id first, then prefixes, then the rest in order", () => {
    const shown = rank(options.filter((option) => matches(option, "open")), "open");
    expect(shown.map((option) => option.id)).toEqual(["openai", "openrouter", "azure.openai", "minimax/openai"]);
    expect(rank(options, "openai")[0].id).toBe("openai");
  });

  it("leaves the order alone without a query", () => {
    expect(rank(options, " ")).toEqual(options);
  });
});
