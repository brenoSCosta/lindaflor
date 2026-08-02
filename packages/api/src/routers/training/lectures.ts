import { authorizedProcedure } from "@lindaflor/api/middlewares/authorized";
import { requireActiveOrganization } from "@lindaflor/api/middlewares/require-active-organization";
import { manageTrainingProcedure } from "@lindaflor/api/routers/training/utils";
import {
  downloadLecturePdf,
  markLectureProgress,
  uploadLecturePdf,
} from "@lindaflor/core/training/lectures";
import { schema } from "@lindaflor/shared/schemas/training";

export const lecturesRouter = {
  pdf: {
    upload: manageTrainingProcedure
      .route({
        method: "POST",
        path: "/training/lectures/upload-pdf",
        description: "Upload a PDF file for a training lecture",
        summary: "v1 Upload Lecture PDF",
      })
      .input(schema.v1.lectures.pdf.upload.input)
      .output(schema.v1.lectures.pdf.upload.output)
      .handler(async ({ input }) => uploadLecturePdf({ input })),

    download: authorizedProcedure
      .use(requireActiveOrganization())
      .route({
        method: "GET",
        path: "/training/lectures/{lecture_id}/download",
        description: "Get a presigned download URL for a PDF lecture",
        summary: "v1 Get Lecture Download URL",
      })
      .input(schema.v1.lectures.pdf.download.input)
      .output(schema.v1.lectures.pdf.download.output)
      .handler(async ({ input, context }) =>
        downloadLecturePdf({
          input,
          organizationId: context.activeOrganizationId,
          userId: context.session.user.id,
          ability: context.ability,
        }),
      ),
  },

  progress: {
    mark: authorizedProcedure
      .use(requireActiveOrganization())
      .route({
        method: "POST",
        path: "/training/lectures/progress",
        description: "Mark lecture progress for the current user",
        summary: "v1 Mark Lecture Progress",
      })
      .input(schema.v1.lectures.progress.mark.input)
      .output(schema.v1.lectures.progress.mark.output)
      .handler(async ({ input, context }) =>
        markLectureProgress({
          input,
          organizationId: context.activeOrganizationId,
          userId: context.session.user.id,
          ability: context.ability,
        }),
      ),
  },
};
