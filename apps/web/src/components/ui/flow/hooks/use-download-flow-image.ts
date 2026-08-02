import { Effect } from "effect";

import {
  downloadFlowImage,
  type DownloadFlowImageOptions,
} from "@/components/ui/flow/helpers/download-flow-image";

export function useDownloadFlowImage() {
  return (options: DownloadFlowImageOptions) =>
    Effect.runPromise(downloadFlowImage(options));
}
