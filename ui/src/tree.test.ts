import { describe, expect, it } from "vitest";
import type { FileView } from "./api";
import { badgeOf, buildTree } from "./tree";

const file = (path: string, status: FileView["status"] = null): FileView => ({ path, status, selected: false });

describe("buildTree", () => {
  it("nests files under their directories", () => {
    const tree = buildTree([file("a/b/x.json"), file("a/y.json"), file("z.json")]);
    expect(tree.files.map((f) => f.path)).toEqual(["z.json"]);
    const a = tree.directories[0];
    expect(a.path).toBe("a");
    expect(a.files.map((f) => f.path)).toEqual(["a/y.json"]);
    expect(a.directories[0].path).toBe("a/b");
  });

  it("marks every directory above a changed file", () => {
    const tree = buildTree([file("a/b/x.json", "modified"), file("a/y.json", "identical"), file("c/z.json", "identical")]);
    const [a, c] = tree.directories;
    expect(a.changed && a.directories[0].changed).toBe(true);
    expect(c.changed).toBe(false);
  });
});

describe("badgeOf", () => {
  it("names how a file compares", () => {
    expect([badgeOf("modified"), badgeOf("only_left"), badgeOf("only_right"), badgeOf("identical"), badgeOf(null)]).toEqual([
      "M",
      "D",
      "A",
      null,
      null,
    ]);
  });
});
