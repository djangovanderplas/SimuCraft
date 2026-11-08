import { Background, Controls, Handle, type Node, type NodeProps, Position, ReactFlow } from "@xyflow/react";
import { type FormEvent, useCallback, useEffect, useMemo, useState } from "react";

import { type BlockNodeData, type LayoutData, type ModelData, type NoteNodeData, resolve, toFlow } from "./graph";
import { CommandError, initialProject, rpc } from "./rpc";

interface Opened {
  top: string;
  models: string[];
}

interface Loaded {
  file: string;
  model: ModelData;
  layout: LayoutData | null;
}

/** Handles spread along one side of a block. */
function handles(ports: string[], kind: "in" | "out" | "net", position: Position) {
  const side = position === Position.Bottom ? "left" : "top";
  return ports.map((p, i) => (
    <Handle
      key={`${kind}:${p}`}
      id={`${kind}:${p}`}
      type={kind === "out" ? "source" : "target"}
      position={position}
      style={{ [side]: `${((i + 1) / (ports.length + 1)) * 100}%` }}
      title={p}
      isConnectable={false}
    />
  ));
}

function BlockNode({ data }: NodeProps<Node<BlockNodeData>>) {
  // A net edge starts at a `source` handle; give every connector one of each kind.
  return (
    <div className={`block${data.ref ? " subsystem" : ""}`} title={data.ref ? "Double-click to open" : data.type}>
      {handles(data.inputs, "in", Position.Left)}
      <div className="name">{data.name}</div>
      <div className="type">{data.ref ? `▸ ${data.ref}` : data.type}</div>
      {handles(data.outputs, "out", Position.Right)}
      {handles(data.connectors, "net", Position.Bottom)}
      {data.connectors.map((p, i) => (
        <Handle
          key={`src:${p}`}
          id={`net:${p}`}
          type="source"
          position={Position.Bottom}
          style={{ left: `${((i + 1) / (data.connectors.length + 1)) * 100}%`, opacity: 0 }}
          isConnectable={false}
        />
      ))}
    </div>
  );
}

function NoteNode({ data }: NodeProps<Node<NoteNodeData>>) {
  return <div className="note">{data.text}</div>;
}

const nodeTypes = { block: BlockNode, note: NoteNode };

export default function App() {
  const [path, setPath] = useState("");
  const [opened, setOpened] = useState<Opened | null>(null);
  const [stack, setStack] = useState<string[]>([]);
  const [loaded, setLoaded] = useState<Loaded | null>(null);
  const [error, setError] = useState("");

  const fail = (e: unknown) => {
    const details = e instanceof CommandError ? e.diagnostics.map((d) => `${d.code}: ${d.message}`) : [];
    setError([e instanceof Error ? e.message : String(e), ...details].join("\n"));
  };

  const open = useCallback(async (folder: string) => {
    if (!folder) return;
    setError("");
    try {
      const o = await rpc<Opened>("project.open", { path: folder });
      setOpened(o);
      setStack([o.top]);
    } catch (e) {
      fail(e);
    }
  }, []);

  useEffect(() => {
    initialProject().then((p) => {
      setPath(p);
      open(p);
    });
  }, [open]);

  const file = stack[stack.length - 1];
  useEffect(() => {
    if (!file) return;
    rpc<Loaded>("model.get", { path: file }).then(setLoaded, fail);
  }, [file]);

  const flow = useMemo(() => (loaded ? toFlow(loaded.model, loaded.layout) : null), [loaded]);

  const enter = (_: unknown, node: Node) => {
    const ref = (node.data as BlockNodeData).ref;
    if (ref && file) setStack([...stack, resolve(file, ref)]);
  };

  const submit = (e: FormEvent) => {
    e.preventDefault();
    open(path);
  };

  return (
    <div className="app">
      <header>
        <form onSubmit={submit}>
          <label htmlFor="project">Project folder</label>
          <input id="project" value={path} onChange={(e) => setPath(e.target.value)} placeholder="/path/to/project" />
          <button type="submit">Open</button>
        </form>
        {opened && (
          <select aria-label="Model" value={stack[0]} onChange={(e) => setStack([e.target.value])}>
            {opened.models.map((m) => (
              <option key={m}>{m}</option>
            ))}
          </select>
        )}
        <nav aria-label="Subsystem path">
          {stack.map((f, i) => (
            <button key={f + i} disabled={i === stack.length - 1} onClick={() => setStack(stack.slice(0, i + 1))}>
              {f.split("/").pop()}
            </button>
          ))}
        </nav>
      </header>
      {error && <pre className="error" role="alert">{error}</pre>}
      {flow && flow.problems.length > 0 && (
        <ul className="problems" aria-label="Problems">
          {flow.problems.map((p) => (
            <li key={p}>{p}</li>
          ))}
        </ul>
      )}
      <main>
        {flow ? (
          <ReactFlow
            key={file}
            nodes={flow.nodes}
            edges={flow.edges}
            nodeTypes={nodeTypes}
            onNodeDoubleClick={enter}
            nodesDraggable={false}
            nodesConnectable={false}
            edgesFocusable={false}
            zoomOnDoubleClick={false}
            fitView
          >
            <Background />
            <Controls showInteractive={false} />
          </ReactFlow>
        ) : (
          <p className="empty">Open a project folder to see its models.</p>
        )}
      </main>
    </div>
  );
}
