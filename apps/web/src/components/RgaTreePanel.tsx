import type { OperationId, RgaTreeNode } from "../collaboration/types";
import { colorForReplica, shortReplicaId } from "../collaboration/types";
import { useCollaborationStore } from "../stores/collaborationStore";

const ROOT_KEY = "HEAD";

export function RgaTreePanel() {
  const tree = useCollaborationStore((state) => state.rgaTree);
  const nodesByLeft = groupNodesByLeft(tree.nodes);
  const linkedNodes = linkedListOrder(tree.nodes);
  const rootNodes = nodesByLeft.get(ROOT_KEY) ?? [];

  return (
    <section className="border p-5" aria-label="RGA tree">
      <div className="mb-4 flex flex-col gap-2 md:flex-row md:items-baseline md:justify-between">
        <h2 className="text-base font-semibold">RGA Tree</h2>
        <div className="text-sm font-semibold">{tree.nodes.length} nodes</div>
      </div>

      <div className="grid gap-5 lg:grid-cols-[minmax(0,1fr)_minmax(280px,420px)]">
        <div className="min-w-0 overflow-x-auto">
          <div className="mb-2 text-sm font-semibold">Anchors</div>
          {tree.nodes.length === 0 ? (
            <p className="text-sm">Empty</p>
          ) : (
            <div className="flex min-w-max items-start text-sm">
              <div className="mt-1 border px-2 py-1 font-semibold">HEAD</div>
              <TreeBranch nodes={rootNodes} nodesByLeft={nodesByLeft} />
            </div>
          )}
        </div>

        <div className="min-w-0 overflow-x-auto">
          <div className="mb-2 text-sm font-semibold">Linked Order</div>
          {linkedNodes.length === 0 ? (
            <p className="text-sm">Empty</p>
          ) : (
            <ol className="rga-linked-order min-w-max space-y-2 text-sm">
              <li className="border px-2 py-1 font-semibold">HEAD</li>
              {linkedNodes.map((node) => (
                <li className="rga-linked-order-item" key={operationKey(node.id)}>
                  <NodeBadge node={node} />
                </li>
              ))}
            </ol>
          )}
        </div>
      </div>
    </section>
  );
}

function TreeBranch({
  nodes,
  nodesByLeft,
}: {
  nodes: RgaTreeNode[];
  nodesByLeft: Map<string, RgaTreeNode[]>;
}) {
  if (nodes.length === 0) return null;

  return (
    <ul className="rga-branch min-w-max space-y-2">
      {nodes.map((node) => {
        const children = nodesByLeft.get(operationKey(node.id)) ?? [];

        return (
          <li className="rga-branch-item" key={operationKey(node.id)}>
            <div className="flex items-center gap-2">
              <NodeBadge node={node} />
              {children.length > 1 ? (
                <span className="border px-1.5 py-0.5 text-xs font-semibold">
                  {children.length} branches
                </span>
              ) : null}
            </div>
            <TreeBranch nodes={children} nodesByLeft={nodesByLeft} />
          </li>
        );
      })}
    </ul>
  );
}

function NodeBadge({ node }: { node: RgaTreeNode }) {
  const color = colorForReplica(node.id.replica_id);
  const label = node.tombstone ? "deleted" : `@ ${node.visible_index ?? "-"}`;

  return (
    <div className="grid grid-cols-[auto_auto_auto] items-center gap-2 border px-2 py-1">
      <span className="h-2.5 w-2.5 rounded-full" style={{ backgroundColor: color }} />
      <code className="font-mono">
        {formatValue(node.value)} {shortOperationId(node.id)}
      </code>
      <span className="text-xs font-semibold">{label}</span>
    </div>
  );
}

function groupNodesByLeft(nodes: RgaTreeNode[]): Map<string, RgaTreeNode[]> {
  const groups = new Map<string, RgaTreeNode[]>();

  for (const node of nodes) {
    const key = node.left ? operationKey(node.left) : ROOT_KEY;
    const group = groups.get(key) ?? [];
    group.push(node);
    groups.set(key, group);
  }

  for (const group of groups.values()) {
    group.sort((left, right) => left.index - right.index);
  }

  return groups;
}

function linkedListOrder(nodes: RgaTreeNode[]): RgaTreeNode[] {
  const nextTargets = new Set(
    nodes.flatMap((node) => (node.next ? [operationKey(node.next)] : [])),
  );
  const byId = new Map(nodes.map((node) => [operationKey(node.id), node]));
  const byNext = new Map(nodes.map((node) => [operationKey(node.id), node.next]));
  const head = nodes.find((node) => !nextTargets.has(operationKey(node.id)));
  const order: RgaTreeNode[] = [];
  let current = head;

  while (current) {
    order.push(current);
    const next = byNext.get(operationKey(current.id));
    current = next ? byId.get(operationKey(next)) : undefined;
  }

  return order;
}

function operationKey(id: OperationId): string {
  return `${id.session_id}:${id.replica_id}:${id.lamport}:${id.seq}`;
}

function shortOperationId(id: OperationId): string {
  return `${shortReplicaId(id.replica_id)}:${id.lamport}.${id.seq}`;
}

function formatValue(value: string): string {
  if (value === "\n") return '"\\n"';
  if (value === " ") return '"space"';
  return `"${value}"`;
}
