// WP1.7 done-when: all example models render, including ones without layout entries.

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, relative } from "node:path";
import { parse } from "smol-toml";
import { describe, expect, test } from "vitest";

import { type BlockNodeData, GRID, LAYOUT_SCALE, type LayoutData, type ModelData, resolve, toFlow } from "./graph";

const repo = join(import.meta.dirname, "../..");

function models(dir: string): string[] {
  return readdirSync(dir).flatMap((f) => {
    const p = join(dir, f);
    if (statSync(p).isDirectory()) return models(p);
    return p.endsWith(".scmodel") ? [p] : [];
  });
}

const all = [...models(join(repo, "examples")), ...models(join(repo, "conformance/models"))];

function load(path: string): { model: ModelData; layout: LayoutData | null } {
  const twin = path.replace(/\.scmodel$/, ".sclayout");
  return {
    model: parse(readFileSync(path, "utf8")) as ModelData,
    layout: existsSync(twin) ? (parse(readFileSync(twin, "utf8")) as LayoutData) : null,
  };
}

test("the test sees every example model", () => {
  // speed_control, dc_motor and the 9 conformance models.
  expect(all.length).toBe(11);
});

describe.each(all.map((p) => [relative(repo, p), p]))("%s renders", (_, path) => {
  const { model, layout } = load(path);
  const flow = toFlow(model, layout);
  const blocks = flow.nodes.filter((n) => n.type === "block");

  test("one node per block, at a finite position, with no broken references", () => {
    expect(blocks.map((n) => n.id).sort()).toEqual(Object.keys(model.blocks ?? {}).sort());
    for (const n of flow.nodes) expect(Number.isFinite(n.position.x) && Number.isFinite(n.position.y)).toBe(true);
    expect(flow.problems).toEqual([]);
  });

  test("every edge ends on a handle that its node has", () => {
    const byId = new Map(blocks.map((n) => [n.id, n.data as BlockNodeData]));
    const handles = (d: BlockNodeData) =>
      new Set([...d.inputs.map((p) => `in:${p}`), ...d.outputs.map((p) => `out:${p}`), ...d.connectors.map((p) => `net:${p}`)]);
    for (const e of flow.edges) {
      expect(handles(byId.get(e.source)!)).toContain(e.sourceHandle);
      expect(handles(byId.get(e.target)!)).toContain(e.targetHandle);
    }
    const wires = Object.values(model.blocks ?? {}).reduce((n, b) => n + Object.keys(b.inputs ?? {}).length, 0);
    const nets = Object.values(model.nets ?? {}).reduce((n, net) => n + net.ports.length - 1, 0);
    expect(flow.edges.length).toBe(wires + nets);
  });

  test("auto-placed blocks do not overlap any block", () => {
    for (const a of blocks) {
      if ((a.data as BlockNodeData).placed !== "auto") continue;
      for (const b of blocks) {
        if (a === b) continue;
        const apart = Math.abs(a.position.x - b.position.x) >= GRID.width || Math.abs(a.position.y - b.position.y) >= GRID.height;
        expect(apart, `${a.id} and ${b.id}`).toBe(true);
      }
    }
  });
});

test("a block with a layout entry stays where the layout puts it", () => {
  const { model, layout } = load(join(repo, "examples/motor_demo/models/speed_control.scmodel"));
  const flow = toFlow(model, layout);
  const sum = flow.nodes.find((n) => n.id === "b_4a10f3k8m2qx")!;
  expect(sum.position).toEqual({ x: 140 * LAYOUT_SCALE, y: 80 * LAYOUT_SCALE });
  expect((sum.data as BlockNodeData).placed).toBe("layout");
  expect(flow.nodes.filter((n) => n.type === "block" && (n.data as BlockNodeData).placed === "layout")).toHaveLength(1);
  expect(flow.nodes.find((n) => n.type === "note")?.data).toEqual({ text: "Speed loop, 1 kHz" });
});

test("a subsystem block opens its file", () => {
  const { model } = load(join(repo, "examples/motor_demo/models/speed_control.scmodel"));
  const sub = Object.values(model.blocks!).find((b) => b.type === "subsystem")!;
  const file = resolve("models/speed_control.scmodel", sub.ref!);
  expect(file).toBe("models/subsystems/dc_motor.scmodel");
  expect(existsSync(join(repo, "examples/motor_demo", file))).toBe(true);
  expect(resolve("models/a/b.scmodel", "../c.scmodel")).toBe("models/c.scmodel");
});

test("a source that is not in the model is a problem, not a crash", () => {
  const flow = toFlow({ blocks: { b_1: { name: "y", type: "core.Outport", inputs: { u: "b_9.y" } } } }, null);
  expect(flow.nodes).toHaveLength(1);
  expect(flow.edges).toEqual([]);
  expect(flow.problems).toHaveLength(1);
});
