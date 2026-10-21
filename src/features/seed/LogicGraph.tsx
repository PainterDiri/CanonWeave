import type { NarrativeLogicEdge, NarrativeLogicNode } from "@/lib/ipc";

const NODE_WIDTH = 178;
const NODE_HEIGHT = 62;
const COLUMN_GAP = 54;
const ROW_HEIGHT = 118;
const PADDING = 28;

export function LogicGraph({
  nodes,
  edges,
  connecting,
  sourceId,
  selectedEdgeId,
  onNodeClick,
  onEdgeClick,
}: {
  nodes: NarrativeLogicNode[];
  edges: NarrativeLogicEdge[];
  connecting: boolean;
  sourceId: string | null;
  selectedEdgeId: string | null;
  onNodeClick: (nodeId: string) => void;
  onEdgeClick: (edge: NarrativeLogicEdge) => void;
}) {
  const ordered = [...nodes].sort((a, b) => a.orderNo - b.orderNo);
  const columns = Math.max(1, Math.min(4, ordered.length));
  const rows = Math.ceil(ordered.length / columns);
  const width = Math.max(
    760,
    PADDING * 2 + columns * NODE_WIDTH + Math.max(0, columns - 1) * COLUMN_GAP,
  );
  const height = Math.max(150, PADDING * 2 + rows * NODE_HEIGHT + Math.max(0, rows - 1) * (ROW_HEIGHT - NODE_HEIGHT));
  const positions = new Map<string, { x: number; y: number }>();
  ordered.forEach((node, index) => {
    const column = index % columns;
    const row = Math.floor(index / columns);
    positions.set(node.id, {
      x: PADDING + column * (NODE_WIDTH + COLUMN_GAP),
      y: PADDING + row * ROW_HEIGHT,
    });
  });

  if (!ordered.length) {
    return (
      <div className="flex h-48 items-center justify-center rounded-xl border border-dashed border-line text-[11px] text-muted">
        尚未建立逻辑节点
      </div>
    );
  }

  return (
    <div className="overflow-x-auto rounded-xl border border-line bg-paper/50">
      <div className="relative" style={{ width, height, minWidth: width }}>
        <svg
          className="absolute inset-0 h-full w-full"
          viewBox={`0 0 ${width} ${height}`}
          aria-label="叙事因果图"
        >
          <defs>
            <marker
              id="logic-edge-arrow"
              viewBox="0 0 10 10"
              refX="9"
              refY="5"
              markerWidth="7"
              markerHeight="7"
              orient="auto-start-reverse"
            >
              <path d="M 0 0 L 10 5 L 0 10 z" fill="#8a806f" />
            </marker>
            <marker
              id="logic-edge-arrow-selected"
              viewBox="0 0 10 10"
              refX="9"
              refY="5"
              markerWidth="7"
              markerHeight="7"
              orient="auto-start-reverse"
            >
              <path d="M 0 0 L 10 5 L 0 10 z" fill="#d9604a" />
            </marker>
          </defs>
          {edges.map((edge) => {
            const from = positions.get(edge.fromNodeId);
            const to = positions.get(edge.toNodeId);
            if (!from || !to) return null;
            const sx = from.x + NODE_WIDTH;
            const sy = from.y + NODE_HEIGHT / 2;
            const tx = to.x;
            const ty = to.y + NODE_HEIGHT / 2;
            const sameRow = Math.abs(from.y - to.y) < 2;
            const direction = sameRow ? 1 : to.y > from.y ? 1 : -1;
            const bend = sameRow ? Math.max(42, Math.abs(tx - sx) * 0.35) : 54 * direction;
            const path = sameRow
              ? `M ${sx} ${sy} C ${sx + bend} ${sy}, ${tx - bend} ${ty}, ${tx} ${ty}`
              : `M ${sx} ${sy} C ${sx + 64} ${sy}, ${tx - 64} ${ty}, ${tx} ${ty}`;
            const selected = edge.id === selectedEdgeId;
            const midX = (sx + tx) / 2;
            const midY = (sy + ty) / 2 - 6;
            return (
              <g
                key={edge.id}
                data-testid={`logic-edge-${edge.id}`}
                role="button"
                tabIndex={0}
                className="cursor-pointer"
                onClick={() => onEdgeClick(edge)}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") onEdgeClick(edge);
                }}
              >
                <path
                  d={path}
                  fill="none"
                  stroke={selected ? "#d9604a" : "#8a806f"}
                  strokeWidth={selected ? 2 : 1.3}
                  markerEnd={
                    selected
                      ? "url(#logic-edge-arrow-selected)"
                      : "url(#logic-edge-arrow)"
                  }
                />
                <path d={path} fill="none" stroke="transparent" strokeWidth={14} />
                <text
                  x={midX}
                  y={midY}
                  textAnchor="middle"
                  fill={selected ? "#d9604a" : "#6f6a60"}
                  fontSize={9}
                  paintOrder="stroke"
                  stroke="#f7f3ea"
                  strokeWidth={3}
                >
                  {edge.relation || "因果"}
                </text>
              </g>
            );
          })}
        </svg>

        {ordered.map((node) => {
          const position = positions.get(node.id)!;
          const source = sourceId === node.id;
          return (
            <button
              key={node.id}
              type="button"
              data-testid={`logic-node-${node.id}`}
              onClick={() => onNodeClick(node.id)}
              className={`focus-ring absolute z-10 rounded-xl border px-3 py-2 text-left transition ${
                source
                  ? "border-coral bg-coral-soft shadow-[0_0_0_2px_rgba(217,96,74,0.14)]"
                  : connecting
                    ? "border-teal/45 bg-paper-raised hover:-translate-y-0.5 hover:border-teal"
                    : "border-line-strong bg-paper-raised hover:border-teal/40"
              }`}
              style={{ left: position.x, top: position.y, width: NODE_WIDTH, height: NODE_HEIGHT }}
              aria-label={`逻辑节点 #${node.orderNo} ${node.title}`}
            >
              <div className="flex items-center gap-1.5">
                <span className="rounded-full bg-teal-soft px-1.5 py-0.5 text-[9px] font-semibold text-teal">
                  #{node.orderNo}
                </span>
                <span className="truncate text-[11px] font-semibold text-ink">{node.title}</span>
              </div>
              <div className="mt-1 flex items-center justify-between gap-2 text-[9px] text-muted">
                <span className="truncate">{node.threadTitle || "主线"}</span>
                <span className="shrink-0 rounded bg-ink/5 px-1.5 py-0.5">{node.kind}</span>
              </div>
              {source ? (
                <span className="mt-1 block text-[8.5px] text-coral">起点已选，请选择终点</span>
              ) : null}
            </button>
          );
        })}
      </div>
    </div>
  );
}
