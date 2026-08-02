import {
  type Node,
  type NodeProps,
  Position,
  useReactFlow,
} from "@xyflow/react";
import { EllipsisVertical } from "lucide-react";

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

export type NumNode = Node<{
  value: number;
}>;

export function NumNode({ id, data }: NodeProps<NumNode>) {
  const { updateNodeData, setNodes } = useReactFlow();

  function handleReset() {
    updateNodeData(id, { value: 0 });
  }

  function handleDelete() {
    setNodes((nodes) => nodes.filter((node) => node.id !== id));
  }

  function handleIncr() {
    updateNodeData(id, { value: data.value + 1 });
  }

  function handleDecr() {
    updateNodeData(id, { value: data.value - 1 });
  }

  return (
    <BaseNode>
      <BaseNodeHeader className="border-b">
        <BaseNodeHeaderTitle>Num</BaseNodeHeaderTitle>

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
            <DropdownMenuItem onClick={handleReset}>Reset</DropdownMenuItem>
            <DropdownMenuItem onClick={handleDelete}>Delete</DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </BaseNodeHeader>

      <BaseNodeContent>
        <div className="flex items-center gap-2">
          <Button onClick={handleDecr}>-</Button>
          <pre>{String(data.value).padStart(3, " ")}</pre>
          <Button onClick={handleIncr}>+</Button>
        </div>
      </BaseNodeContent>

      <BaseNodeFooter className="bg-card w-full items-end rounded-b-md px-0 py-1">
        <LabeledHandle title="out" type="source" position={Position.Right} />
      </BaseNodeFooter>
    </BaseNode>
  );
}
