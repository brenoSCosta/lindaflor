import {
  type Node,
  type NodeProps,
  Position,
  useReactFlow,
  useStore,
} from "@xyflow/react";
import { EllipsisVertical } from "lucide-react";
import { useEffect } from "react";

import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { LabeledHandle } from "@/components/ui/flow/components/handles/labeled-handle";
import {
  BaseNode,
  BaseNodeContent,
  BaseNodeFooter,
  BaseNodeHeader,
  BaseNodeHeaderTitle,
} from "@/components/ui/flow/components/nodes/base-node";

export type SumNode = Node<{
  value: number;
}>;

function getHandleValue(
  connections: Array<{ source: string }>,
  lookup: Map<string, Node>,
) {
  return connections.reduce((acc, { source }) => {
    const node = lookup.get(source);
    if (!node) {
      return acc;
    }
    const value = node.data["value"];
    return typeof value === "number" ? acc + value : acc;
  }, 0);
}

export function SumNode({ id }: NodeProps<SumNode>) {
  const { updateNodeData, getNodeConnections, setNodes, setEdges } =
    useReactFlow();
  const { x, y } = useStore((state) => ({
    x: getHandleValue(
      getNodeConnections({ nodeId: id, handleId: "x", type: "target" }),
      state.nodeLookup,
    ),
    y: getHandleValue(
      getNodeConnections({ nodeId: id, handleId: "y", type: "target" }),
      state.nodeLookup,
    ),
  }));

  useEffect(() => {
    updateNodeData(id, { value: x + y });
  }, [id, updateNodeData, x, y]);

  function handleDelete() {
    setNodes((nodes) => nodes.filter((node) => node.id !== id));
    setEdges((edges) => edges.filter((edge) => edge.source !== id));
  }

  return (
    <BaseNode className="w-32">
      <BaseNodeHeader className="border-b">
        <BaseNodeHeaderTitle>Sum</BaseNodeHeaderTitle>

        <DropdownMenu>
          <DropdownMenuTrigger
            render={
              <Button
                variant="ghost"
                className="nodrag p-1"
                aria-label="Node Actions"
                title="Node Actions"
              />
            }
          >
            <EllipsisVertical className="size-4" />
          </DropdownMenuTrigger>
          <DropdownMenuContent>
            <DropdownMenuLabel className="font-bold">
              Node Actions
            </DropdownMenuLabel>
            <DropdownMenuItem onClick={handleDelete}>Delete</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </BaseNodeHeader>

      <BaseNodeContent className="px-0">
        <LabeledHandle
          title="x"
          id="x"
          type="target"
          position={Position.Left}
        />
        <LabeledHandle
          title="y"
          id="y"
          type="target"
          position={Position.Left}
        />
      </BaseNodeContent>
      <BaseNodeFooter className="bg-card w-full items-end rounded-b-md px-0 py-1">
        <LabeledHandle title="out" type="source" position={Position.Right} />
      </BaseNodeFooter>
    </BaseNode>
  );
}
