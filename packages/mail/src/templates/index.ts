import { env } from "@lindaflor/env/server";
import { mailer } from "@lindaflor/mail";

export type OrganizationCreatedEmailProps = {
  recipientName: string;
  organizationName: string;
  organizationSlug?: string;
  dashboardUrl?: string;
};

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function getOrganizationCreatedSubject(
  organizationName: string,
): string {
  return `Your organization "${organizationName}" is ready`;
}

export function organizationCreatedEmailHtml(
  props: OrganizationCreatedEmailProps,
): string {
  const { recipientName, organizationName, organizationSlug, dashboardUrl } =
    props;
  const safeName = escapeHtml(recipientName);
  const safeOrgName = escapeHtml(organizationName);
  const linkUrl =
    dashboardUrl && organizationSlug
      ? `${dashboardUrl.replace(/\/$/, "")}/dashboard`
      : (dashboardUrl ?? "#");
  const linkLabel = dashboardUrl && organizationSlug ? "Open dashboard" : "";

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Organization created</title>
</head>
<body style="margin: 0; padding: 0; font-family: system-ui, -apple-system, sans-serif; background-color: #f4f4f5;">
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="background-color: #f4f4f5;">
    <tr>
      <td align="center" style="padding: 32px 16px;">
        <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="max-width: 480px; background-color: #ffffff; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.08);">
          <tr>
            <td style="padding: 32px 24px;">
              <h1 style="margin: 0 0 8px; font-size: 18px; font-weight: 600; color: #18181b;">Organization created</h1>
              <p style="margin: 0 0 24px; font-size: 14px; line-height: 1.5; color: #3f3f46;">Hi ${safeName}, your organization has been created successfully.</p>
              <p style="margin: 0 0 24px; font-size: 14px; line-height: 1.5; color: #3f3f46;"><strong>${safeOrgName}</strong> is ready to use.</p>
              ${linkLabel ? `<p style="margin: 0 0 24px;"><a href="${linkUrl}" style="display: inline-block; padding: 10px 16px; font-size: 14px; font-weight: 500; color: #ffffff; background-color: #18181b; text-decoration: none; border-radius: 6px;">${linkLabel}</a></p>` : ""}
              <p style="margin: 0; font-size: 12px; line-height: 1.5; color: #71717a;">If you did not create this organization, you can ignore this email.</p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>`;
}

export type SendOrganizationCreatedEmailProps = {
  to: string;
  recipientName: string;
  organizationName: string;
  organizationSlug?: string;
};

export async function sendOrganizationCreatedEmail(
  props: SendOrganizationCreatedEmailProps,
): Promise<void> {
  const { to, recipientName, organizationName, organizationSlug } = props;

  const dashboardUrl = env.WEB_ORIGIN;

  const html = organizationCreatedEmailHtml({
    recipientName,
    organizationName,
    organizationSlug,
    dashboardUrl,
  });

  const subject = getOrganizationCreatedSubject(organizationName);

  await mailer.emails.send({
    from: env.MAIL_FROM,
    to,
    subject,
    html,
  });
}
