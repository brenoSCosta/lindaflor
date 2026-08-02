import { Position, type NodeProps } from "@xyflow/react";

import { BaseHandle } from "@/components/ui/flow/components/handles/base-handle";
import { flowNodeLabel } from "@/components/ui/flow/helpers/node-label";

export function EasyConnectNode({ data }: NodeProps) {
  return (
    <div
      data-slot="flow-easy-connect-node"
      className="bg-card relative rounded-full border px-6 py-4 text-center text-sm shadow-sm"
    >
      <BaseHandle
        type="target"
        position={Position.Left}
        className="h-full! w-full! rounded-full! border-0! bg-transparent!"
        style={{ left: 0, top: 0, transform: "none" }}
      />
      {flowNodeLabel(data, "Node")}
      <BaseHandle
        type="source"
        position={Position.Right}
        className="h-full! w-full! rounded-full! border-0! bg-transparent!"
        style={{ left: 0, top: 0, transform: "none" }}
      />
    </div>
  );
}
