import { orpc, queryClient } from "@/lib/orpc";

export async function invalidateTankDay(args: {
  tankId: string;
  operationalDay: string;
}): Promise<void> {
  const { tankId, operationalDay } = args;
  await Promise.all([
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.bulletin.getBy.day.key({
        input: { tank_id: tankId, operational_day: operationalDay },
      }),
    }),
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.bulletin.event.listBy.day.key({
        input: { tank_id: tankId, operational_day: operationalDay },
      }),
    }),
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.bulletin.listBy.tank.key({
        input: { tank_id: tankId },
      }),
    }),
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.tankage.listBy.tank.key({
        input: { tank_id: tankId },
      }),
    }),
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.transfer.listBy.tank.key({
        input: { tank_id: tankId, operational_day: operationalDay },
      }),
    }),
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.summary.listBy.tank.key({
        input: { tank_id: tankId },
      }),
    }),
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.tank.get.snapshot.key({
        input: { id: tankId },
      }),
    }),
    queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.tank.list.snapshot.key(),
    }),
  ]);
}
