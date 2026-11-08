// Model data to diagram: nodes and edges for React Flow, without React.
// A block with a layout entry stays where the layout puts it; every other
// block is auto-placed in columns by signal depth (spec Section 5).

import type { Edge, Node } from "@xyflow/react";

/** A model file as `model.get` gives it: the TOML data as JSON. */
export interface ModelData {
  name?: string;
  blocks?: Record<string, BlockData>;
  nets?: Record<string, { ports: string[] }>;
}

export interface BlockData {
  name: string;
  type: string;
  /** Subsystem file, relative to the model file. */
  ref?: string;
  /** Input port -> source `<block id>.<port>`. */
  inputs?: Record<string, string>;
}

/** A `.sclayout` file as JSON. */
export interface LayoutData {
  blocks?: Record<string, { at?: number[]; size?: number[] }>;
  annotations?: Record<string, { text: string; at: number[] }>;
}

export type BlockNodeData = {
  name: string;
  type: string;
  ref?: string;
  inputs: string[];
  outputs: string[];
  /** Physical connectors, from the nets. */
  connectors: string[];
  placed: "layout" | "auto";
};

export type NoteNodeData = { text: string };

/** Sizes in screen pixels. */
export const GRID = { column: 220, row: 110, width: 150, height: 56 };
/** Layout coordinates are small (Simulink-like); the viewer draws them larger. */
export const LAYOUT_SCALE = 2;

export interface Flow {
  nodes: Node[];
  edges: Edge[];
  /** References that point at no block. The rest still renders. */
  problems: string[];
}

const naturally = (a: string, b: string) => a.localeCompare(b, undefined, { numeric: true });

export function toFlow(model: ModelData, layout: LayoutData | null): Flow {
  const blocks = model.blocks ?? {};
  const ids = Object.keys(blocks).sort();
  const problems: string[] = [];
  const outputs = new Map<string, Set<string>>(ids.map((id) => [id, new Set()]));
  const connectors = new Map<string, Set<string>>(ids.map((id) => [id, new Set()]));
  const sources = new Map<string, string[]>(ids.map((id) => [id, []]));
  const edges: Edge[] = [];

  const split = (endpoint: string): [string, string] => {
    const dot = endpoint.lastIndexOf(".");
    return dot < 0 ? [endpoint, ""] : [endpoint.slice(0, dot), endpoint.slice(dot + 1)];
  };

  for (const id of ids) {
    for (const [port, source] of Object.entries(blocks[id].inputs ?? {}).sort(([a], [b]) => naturally(a, b))) {
      const [from, out] = split(source);
      if (!(from in blocks)) {
        problems.push(`${blocks[id].name}.${port}: source block \`${from}\` is not in this model`);
        continue;
      }
      outputs.get(from)!.add(out);
      sources.get(id)!.push(from);
      edges.push({ id: `w:${id}.${port}`, source: from, sourceHandle: `out:${out}`, target: id, targetHandle: `in:${port}` });
    }
  }
  for (const [netId, net] of Object.entries(model.nets ?? {}).sort(([a], [b]) => naturally(a, b))) {
    const ends = net.ports.map(split).filter(([block, port]) => {
      const known = block in blocks;
      if (!known) problems.push(`net ${netId}: block \`${block}\` is not in this model`);
      else connectors.get(block)!.add(port);
      return known;
    });
    // A net is drawn as a star from its first port.
    ends.slice(1).forEach(([block, port], i) =>
      edges.push({
        id: `n:${netId}:${i}`,
        source: ends[0][0],
        sourceHandle: `net:${ends[0][1]}`,
        target: block,
        targetHandle: `net:${port}`,
        type: "straight",
        className: "net",
      }),
    );
  }

  const position = place(ids, sources, edges, layout);
  const nodes: Node[] = ids.map((id) => {
    const b = blocks[id];
    const data: BlockNodeData = {
      name: b.name,
      type: b.type,
      ref: b.ref,
      inputs: Object.keys(b.inputs ?? {}).sort(naturally),
      outputs: [...outputs.get(id)!].sort(naturally),
      connectors: [...connectors.get(id)!].sort(naturally),
      placed: layout?.blocks?.[id]?.at ? "layout" : "auto",
    };
    return { id, type: "block", position: position.get(id)!, data };
  });
  for (const [id, a] of Object.entries(layout?.annotations ?? {}).sort(([a], [b]) => naturally(a, b))) {
    const data: NoteNodeData = { text: a.text };
    nodes.push({ id, type: "note", position: { x: a.at[0] * LAYOUT_SCALE, y: a.at[1] * LAYOUT_SCALE }, data, selectable: false });
  }
  return { nodes, edges, problems };
}

/** Positions: the layout's where it has one, else a column by depth and a row by order. */
function place(
  ids: string[],
  sources: Map<string, string[]>,
  edges: Edge[],
  layout: LayoutData | null,
): Map<string, { x: number; y: number }> {
  // Signal depth: the longest path from a block with no inputs. An edge back
  // into the current path (feedback) does not count.
  const depth = new Map<string, number>();
  const onPath = new Set<string>();
  const visit = (id: string): number => {
    if (depth.has(id)) return depth.get(id)!;
    onPath.add(id);
    let d = 0;
    for (const s of sources.get(id)!) if (!onPath.has(s)) d = Math.max(d, visit(s) + 1);
    onPath.delete(id);
    depth.set(id, d);
    return d;
  };
  ids.forEach(visit);

  // A block with physical connections and no signal inputs goes one column
  // after the net neighbour that reaches it first (breadth first), so a motor
  // is drawn after its voltage source. A network with no signal block starts
  // at its first block.
  const neighbours = new Map<string, string[]>(ids.map((id) => [id, []]));
  for (const e of edges) {
    if (e.className !== "net") continue;
    neighbours.get(e.source)!.push(e.target);
    neighbours.get(e.target)!.push(e.source);
  }
  const settled = new Set(ids.filter((id) => sources.get(id)!.length > 0 || neighbours.get(id)!.length === 0));
  let queue = [...settled].sort((a, b) => depth.get(a)! - depth.get(b)!);
  for (const seed of [null, ...ids]) {
    if (seed !== null) {
      if (settled.has(seed)) continue;
      settled.add(seed);
      depth.set(seed, 0);
      queue = [seed];
    }
    while (queue.length) {
      const a = queue.shift()!;
      for (const b of neighbours.get(a)!) {
        if (settled.has(b)) continue;
        settled.add(b);
        depth.set(b, depth.get(a)! + 1);
        queue.push(b);
      }
    }
  }

  // A sink (an outport, a scope) goes after its sources, also when a source moved right in the net pass.
  for (const id of ids) {
    const sink = neighbours.get(id)!.length === 0 && !ids.some((other) => sources.get(other)!.includes(id));
    const from = sources.get(id)!;
    if (sink && from.length) depth.set(id, Math.max(...from.map((s) => depth.get(s)! + 1)));
  }

  const fixed = new Map<string, { x: number; y: number }>();
  for (const id of ids) {
    const at = layout?.blocks?.[id]?.at;
    if (at) fixed.set(id, { x: at[0] * LAYOUT_SCALE, y: at[1] * LAYOUT_SCALE });
  }
  // Auto-placed blocks go below the laid-out ones, so they never overlap.
  const top = fixed.size ? Math.max(...[...fixed.values()].map((p) => p.y)) + GRID.row : 0;
  const rows = new Map<number, number>();
  const out = new Map(fixed);
  for (const id of ids) {
    if (fixed.has(id)) continue;
    const column = depth.get(id)!;
    const row = rows.get(column) ?? 0;
    rows.set(column, row + 1);
    out.set(id, { x: column * GRID.column, y: top + row * GRID.row });
  }
  return out;
}

/** The file of a subsystem `ref`, from the project root. `ref` is relative to the model file. */
export function resolve(modelFile: string, ref: string): string {
  const parts = modelFile.split("/").slice(0, -1);
  for (const p of ref.split("/")) {
    if (p === "..") parts.pop();
    else if (p !== "." && p !== "") parts.push(p);
  }
  return parts.join("/");
}
