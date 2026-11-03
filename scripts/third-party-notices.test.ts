import { afterEach, beforeEach, describe, expect, test } from "bun:test";
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  bundledJs,
  catalogue,
  licenceFiles,
  render,
  shippedCrates,
  type CargoMetadata,
  type Component,
} from "./third-party-notices";

let dir: string;
beforeEach(() => {
  dir = mkdtempSync(join(tmpdir(), "takyon-notices-"));
});
afterEach(() => rmSync(dir, { recursive: true, force: true }));

function write(path: string, text: string) {
  mkdirSync(join(dir, path, ".."), { recursive: true });
  writeFileSync(join(dir, path), text);
}

describe("licenceFiles", () => {
  test("finds every licence, copying and notice file, case-insensitive, sorted", () => {
    write("LICENSE-MIT", "mit\r\n");
    write("license-apache-2.0", "apache");
    write("LICENCE.md", "uk spelling");
    write("COPYING", "copying");
    write("NOTICE", "notice");
    write("README.md", "not a licence");
    write("src/LICENSE", "nested, not top level");
    expect(licenceFiles(dir).map((f) => f.name)).toEqual([
      "COPYING",
      "LICENCE.md",
      "license-apache-2.0",
      "LICENSE-MIT",
      "NOTICE",
    ]);
  });

  /** CRLF on a Windows checkout, LF on CI: same file must render the same bytes. */
  test("normalises line endings and trailing blank lines", () => {
    write("LICENSE", "line one\r\nline two\r\n\r\n");
    expect(licenceFiles(dir)).toEqual([{ name: "LICENSE", text: "line one\nline two" }]);
  });

  test("adds a declared licence file outside the usual names", () => {
    write("legal/terms.txt", "custom terms");
    expect(licenceFiles(dir, "legal/terms.txt")).toEqual([{ name: "legal/terms.txt", text: "custom terms" }]);
  });

  test("a directory with none returns empty", () => {
    expect(licenceFiles(dir)).toEqual([]);
  });
});

describe("shippedCrates", () => {
  const pkg = (name: string, source: string | null = "registry+https://github.com/rust-lang/crates.io-index") => ({
    id: `${name}-id`,
    name,
    version: "1.0.0",
    license: "MIT",
    license_file: null,
    repository: `https://example.com/${name}`,
    source,
    manifest_path: `/reg/${name}/Cargo.toml`,
  });
  const edge = (to: string, ...kinds: (string | null)[]) => ({
    pkg: `${to}-id`,
    dep_kinds: kinds.map((kind) => ({ kind, target: null })),
  });
  const meta: CargoMetadata = {
    packages: [
      pkg("app", null),
      pkg("helper", null),
      pkg("serde"),
      pkg("serde_derive"),
      pkg("cc"),
      pkg("tempfile"),
      pkg("shared-dev-and-normal"),
      pkg("helper-only"),
    ],
    resolve: {
      root: "app-id",
      nodes: [
        {
          id: "app-id",
          deps: [
            edge("serde", null),
            edge("tempfile", "dev"),
            edge("cc", "build"),
            edge("shared-dev-and-normal", "dev", null),
          ],
        },
        { id: "serde-id", deps: [edge("serde_derive", null), edge("cc", "build")] },
        { id: "serde_derive-id", deps: [] },
        { id: "helper-id", deps: [edge("helper-only", null)] },
        { id: "cc-id", deps: [] },
        { id: "tempfile-id", deps: [] },
        { id: "shared-dev-and-normal-id", deps: [] },
        { id: "helper-only-id", deps: [] },
      ],
    },
  };

  test("walks normal edges only, skips path crates, sorted by name", () => {
    expect(shippedCrates(meta, ["app"]).map((p) => p.name)).toEqual([
      "serde",
      "serde_derive",
      "shared-dev-and-normal",
    ]);
  });

  test("a second shipped root adds its own normal dependencies", () => {
    expect(shippedCrates(meta, ["app", "helper"]).map((p) => p.name)).toContain("helper-only");
  });

  test("an unknown root is an error, not an empty list", () => {
    expect(() => shippedCrates(meta, ["nope"])).toThrow("nope");
  });
});

describe("bundledJs", () => {
  const pkgJson = (path: string, body: object) => write(join(path, "package.json"), JSON.stringify(body));

  test("walks production dependencies transitively, never dev ones", () => {
    pkgJson("app", {
      name: "@takyon/desktop",
      dependencies: { a: "1", "@takyon/shared": "workspace:*", plugin: "1" },
      devDependencies: { d: "1" },
    });
    pkgJson("app/node_modules/a", { name: "a", version: "1.0.0", license: "MIT", dependencies: { b: "1" } });
    // `b` resolves by walking up from `a`, as Node does.
    pkgJson("node_modules/b", { name: "b", version: "2.0.0", license: { type: "ISC" }, repository: "git+https://x/b.git" });
    pkgJson("app/node_modules/d", { name: "d", version: "1.0.0", license: "MIT" });
    pkgJson("app/node_modules/plugin", { name: "plugin", version: "1.0.0", dependencies: { heavy: "1" } });
    pkgJson("node_modules/heavy", { name: "heavy", version: "1.0.0" });
    pkgJson("app/node_modules/@takyon/shared", { name: "@takyon/shared", private: true, dependencies: { c: "1" } });
    pkgJson("node_modules/c", { name: "c", version: "3.0.0", license: "MIT" });

    const found = bundledJs(join(dir, "app"), new Set(["plugin"]));
    expect(found.map((p) => `${p.name}@${p.version}`)).toEqual(["a@1.0.0", "b@2.0.0", "c@3.0.0"]);
    expect(found[1]).toMatchObject({ licence: "ISC", repository: "https://x/b.git" });
  });

  test("a production dependency that is not installed is an error", () => {
    pkgJson("app", { name: "@takyon/desktop", dependencies: { missing: "1" } });
    expect(() => bundledJs(join(dir, "app"), new Set())).toThrow("missing");
  });
});

describe("render", () => {
  const apache = "Apache text";
  const canonical = { "Apache-2.0": "Canonical Apache", MIT: "Canonical MIT" };
  const components: Component[] = [
    {
      kind: "npm",
      name: "react",
      version: "19.0.0",
      licence: "MIT",
      repository: "https://github.com/facebook/react",
      files: [{ name: "LICENSE", text: "Copyright Meta" }],
    },
    {
      kind: "crate",
      name: "serde",
      version: "1.0.0",
      licence: "MIT OR Apache-2.0",
      repository: null,
      files: [
        { name: "LICENSE-APACHE", text: apache },
        { name: "LICENSE-MIT", text: "Copyright Tolnay" },
      ],
    },
    { kind: "crate", name: "bare", version: "0.1.0", licence: "Apache-2.0 OR MIT", repository: null, files: [] },
    {
      kind: "crate",
      name: "anyhow",
      version: "1.0.0",
      licence: "MIT OR Apache-2.0",
      repository: null,
      files: [{ name: "LICENSE-APACHE", text: apache }],
    },
  ];

  test("same input in any order renders the same bytes", () => {
    expect(render([...components].reverse(), canonical)).toBe(render(components, canonical));
  });

  test("names Takyon's own licence as separate, lists crates before npm, sorted", () => {
    const out = render(components, canonical);
    expect(out).toContain("CC-BY-NC-SA-4.0");
    const order = ["crate anyhow 1.0.0", "crate bare 0.1.0", "crate serde 1.0.0", "npm react 19.0.0"].map((s) =>
      out.indexOf(s),
    );
    expect(order.every((at) => at >= 0)).toBe(true);
    expect([...order].sort((a, b) => a - b)).toEqual(order);
  });

  test("an identical text is printed once and referenced by both packages", () => {
    const out = render(components, canonical);
    expect(out.split(apache).length - 1).toBe(1);
    expect(out).toContain("Copyright Tolnay");
    expect(out).toContain("Copyright Meta");
  });

  test("a package shipping no text points at SPDX and the canonical appendix", () => {
    const out = render(components, canonical);
    expect(out).toContain("licence text not shipped by package; see https://spdx.org/licenses/Apache-2.0.html");
    expect(out).toContain("Canonical Apache");
    expect(out).toContain("Canonical MIT");
  });

  test("no canonical appendix when every package ships its texts", () => {
    const out = render([components[0]!], canonical);
    expect(out).not.toContain("Canonical");
  });
});

describe("catalogue", () => {
  const canonical = { "Apache-2.0": "Canonical Apache", MIT: "Canonical MIT" };
  const components: Component[] = [
    {
      kind: "npm",
      name: "react",
      version: "19.0.0",
      licence: "MIT",
      repository: "https://github.com/facebook/react",
      files: [{ name: "LICENSE", text: "Copyright Meta" }],
    },
    {
      kind: "crate",
      name: "serde",
      version: "1.0.0",
      licence: "MIT OR Apache-2.0",
      repository: null,
      files: [
        { name: "LICENSE-APACHE", text: "Apache text" },
        { name: "LICENSE-MIT", text: "Copyright Tolnay" },
      ],
    },
    {
      kind: "crate",
      name: "anyhow",
      version: "1.0.0",
      licence: "MIT OR Apache-2.0",
      repository: null,
      files: [{ name: "LICENSE-APACHE", text: "Apache text" }],
    },
    { kind: "crate", name: "bare", version: "0.1.0", licence: "Apache-2.0", repository: null, files: [] },
    { kind: "crate", name: "undeclared", version: "0.1.0", licence: null, repository: null, files: [] },
  ];

  test("lists crates before npm, sorted, whatever the input order", () => {
    const out = catalogue([...components].reverse(), canonical);
    expect(out.components.map((c) => `${c.kind} ${c.name}`)).toEqual([
      "crate anyhow",
      "crate bare",
      "crate serde",
      "crate undeclared",
      "npm react",
    ]);
    expect(out).toEqual(catalogue(components, canonical));
  });

  test("an identical text is stored once and both components point at it", () => {
    const out = catalogue(components, canonical);
    const [anyhow, , serde] = out.components;
    expect(out.texts.filter((t) => t.text === "Apache text")).toHaveLength(1);
    expect(anyhow!.texts[0]).toBe(serde!.texts[0]!);
    expect(serde!.texts.map((i) => out.texts[i]!.name)).toEqual(["LICENSE-APACHE", "LICENSE-MIT"]);
  });

  test("identical text keeps each component's own filename", () => {
    const out = catalogue(
      [
        components[0]!,
        { ...components[0]!, name: "another", files: [{ name: "COPYING", text: "Copyright Meta" }] },
      ],
      canonical,
    );
    expect(out.components.map((c) => out.texts[c.texts[0]!]!.name)).toEqual(["COPYING", "LICENSE"]);
  });

  test("a component shipping no text gets the canonical one, marked as such", () => {
    const out = catalogue(components, canonical);
    const bare = out.components.find((c) => c.name === "bare")!;
    expect(bare.canonical).toBe(true);
    expect(bare.texts.map((i) => out.texts[i])).toEqual([{ name: "Apache-2.0", text: "Canonical Apache" }]);
  });

  test("names the SPDX ids and keeps a missing licence as null", () => {
    const out = catalogue(components, canonical);
    expect(out.components.find((c) => c.name === "serde")!.licenceName).toBe(
      "MIT License (MIT) OR Apache License 2.0 (Apache-2.0)",
    );
    expect(out.components.find((c) => c.name === "undeclared")).toMatchObject({
      licence: null,
      licenceName: null,
      texts: [],
      canonical: false,
    });
  });
});
