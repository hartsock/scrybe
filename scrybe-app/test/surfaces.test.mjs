// SPDX-License-Identifier: Apache-2.0
// LazySurface and the view-mode state machine. Both modules import only
// types from their siblings, so Node can load the .ts sources directly.
import assert from "node:assert/strict";
import test from "node:test";

const { LazySurface, WYSIWYG_CAPABILITIES } = await import("../src/surfaces.ts");
const { AppState, VIEW_MODES, isViewMode } = await import("../src/state.ts");

/// A recording stand-in for a real surface.
function fakeFactory(log, { ready } = {}) {
  return (parent, initial, onChange) => {
    let text = initial;
    log.push(["create", initial]);
    return {
      capabilities: WYSIWYG_CAPABILITIES,
      ready,
      onChange,
      getContent: () => text,
      setContent: t => { text = t; log.push(["set", t]); },
      replaceContent: t => { text = t; log.push(["replace", t]); },
      setTheme: t => log.push(["theme", t]),
      setVim: () => {},
      setWrap: () => {},
      setLanguage: async () => "markdown",
      classify: () => "markdown",
      focus: () => log.push(["focus"]),
      destroy: () => log.push(["destroy"]),
      reformatPending: () => text.startsWith("*"),
    };
  };
}
const lazy = (load, initial) =>
  new LazySurface({}, load, () => {}, WYSIWYG_CAPABILITIES, p => (p?.endsWith(".rs") ? "code" : "markdown"), initial);

test("before loading, the wrapper answers from what it was given and loads nothing", async () => {
  let loads = 0;
  const s = lazy(async () => { loads++; return fakeFactory([]); }, "first");
  assert.equal(s.getContent(), "first");
  s.setContent("second");
  s.replaceContent("third");
  s.setTheme("dark");
  s.focus();
  assert.equal(s.getContent(), "third");
  assert.equal(s.reformatPending(), false);
  assert.equal(s.classify("a.rs"), "code");
  assert.equal(await s.setLanguage("a.rs"), "code");
  assert.deepEqual(s.capabilities, WYSIWYG_CAPABILITIES);
  assert.equal(loads, 0);
});

test("ensure mounts once with the latest content and theme, then forwards", async () => {
  const log = [];
  let loads = 0;
  const s = lazy(async () => { loads++; return fakeFactory(log); }, "old");
  s.setContent("* latest");
  s.setTheme("dark");
  await Promise.all([s.ensure(), s.ensure()]);
  assert.equal(loads, 1);
  assert.deepEqual(log, [["create", "* latest"], ["theme", "dark"]]);
  assert.equal(s.reformatPending(), true);
  s.setContent("next");
  s.replaceContent("agent edit");
  assert.deepEqual(log.slice(2), [["set", "next"], ["replace", "agent edit"]]);
  assert.equal(s.getContent(), "agent edit");
});

test("content set while the surface is still mounting reaches it", async () => {
  const log = [];
  let release;
  const ready = new Promise(r => { release = r; });
  const s = lazy(async () => fakeFactory(log, { ready }), "a");
  const done = s.ensure();
  await new Promise(r => setTimeout(r, 0)); // factory has run; `ready` is pending
  s.setContent("b");
  release();
  await done;
  assert.deepEqual(log, [["create", "a"], ["set", "b"]]);
});

test("a failed load rejects and can be retried", async () => {
  let attempt = 0;
  const log = [];
  const s = lazy(async () => {
    if (attempt++ === 0) throw new Error("offline");
    return fakeFactory(log);
  }, "x");
  await assert.rejects(s.ensure(), /offline/);
  assert.equal(s.getContent(), "x");
  await s.ensure();
  assert.deepEqual(log, [["create", "x"]]);
});

test("destroy before the load finishes never mounts", async () => {
  const log = [];
  const s = lazy(async () => fakeFactory(log), "x");
  const done = s.ensure();
  s.destroy();
  await done;
  assert.deepEqual(log, []);
});

test("view modes: Markdown cycles through wysiwyg; code and text stay edit", () => {
  assert.deepEqual([...VIEW_MODES], ["both", "edit", "preview", "wysiwyg"]);
  assert.ok(isViewMode("wysiwyg"));
  assert.ok(!isViewMode("cycle"));
  const state = new AppState();
  const md = state.addTab("/a.md", "# a");
  assert.equal(state.activeTab().viewMode, "both");
  const seen = [1, 2, 3, 4].map(() => state.cycleViewMode(md));
  assert.deepEqual(seen, ["edit", "preview", "wysiwyg", "both"]);
  const rs = state.addTab("/a.rs", "fn main() {}", "code");
  assert.equal(state.activeTab().viewMode, "edit");
  assert.equal(state.cycleViewMode(rs), "edit");
});
