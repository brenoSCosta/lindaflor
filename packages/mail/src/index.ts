import { env } from "@lindaflor/env/server";
import { Effect } from "effect";
import { Resend } from "resend";

function createNoOpMailer() {
  return {
    emails: {
      send: async (params: {
        from: string;
        to: string;
        subject: string;
        html: string;
      }) => {
        await Effect.runPromise(
          Effect.log("[mail] skipped (MAIL_ENABLED=false)", {
            to: params.to,
            subject: params.subject,
          }),
        );
      },
    },
  };
}

export const mailer = env.MAIL_ENABLED
  ? new Resend(env.RESEND_API_KEY)
  : createNoOpMailer();
