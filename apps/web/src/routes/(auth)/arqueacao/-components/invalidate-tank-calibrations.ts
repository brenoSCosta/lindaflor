import { orpc, queryClient } from "@/lib/orpc";

export async function invalidateTankCalibrations(
  tankId: string,
  focusId?: string | null,
): Promise<void> {
  await queryClient.invalidateQueries({
    queryKey: orpc.tanks.v1.calibration.listBy.tank.key({
      input: { tank_id: tankId },
    }),
  });
  await queryClient.invalidateQueries({
    queryKey: orpc.tanks.v1.calibration.list.current.key(),
  });
  if (focusId != null) {
    await queryClient.invalidateQueries({
      queryKey: orpc.tanks.v1.calibration.getBy.id.key({
        input: { id: focusId },
      }),
    });
  }
}
