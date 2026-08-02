import { env } from "@lindaflor/env/server";
import { mailer } from "@lindaflor/mail";

export type DeleteAccountEmailProps = {
  recipientName: string;
  deleteUrl: string;
};

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function getDeleteAccountSubject(): string {
  return "Confirm account deletion";
}

export function deleteAccountEmailHtml(props: DeleteAccountEmailProps): string {
  const { recipientName, deleteUrl } = props;
  const safeName = escapeHtml(recipientName);
  const safeUrl = escapeHtml(deleteUrl);

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Confirm account deletion</title>
</head>
<body style="margin: 0; padding: 0; font-family: system-ui, -apple-system, sans-serif; background-color: #f4f4f5;">
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="background-color: #f4f4f5;">
    <tr>
      <td align="center" style="padding: 32px 16px;">
        <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="max-width: 480px; background-color: #ffffff; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.08);">
          <tr>
            <td style="padding: 32px 24px;">
              <h1 style="margin: 0 0 8px; font-size: 18px; font-weight: 600; color: #b91c1c;">Confirm account deletion</h1>
              <p style="margin: 0 0 24px; font-size: 14px; line-height: 1.5; color: #3f3f46;">Hi ${safeName}, click below to permanently delete your account and all associated data. <strong>This cannot be undone.</strong></p>
              <p style="margin: 0 0 24px;"><a href="${safeUrl}" style="display: inline-block; padding: 10px 16px; font-size: 14px; font-weight: 500; color: #ffffff; background-color: #b91c1c; text-decoration: none; border-radius: 6px;">Delete my account</a></p>
              <p style="margin: 0 0 8px; font-size: 12px; line-height: 1.5; color: #71717a;">This link will expire in 1 hour.</p>
              <p style="margin: 0; font-size: 12px; line-height: 1.5; color: #71717a;">If you did not request this, change your password immediately and contact support.</p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>`;
}

export type SendDeleteAccountEmailProps = {
  to: string;
  recipientName: string;
  deleteUrl: string;
};

export async function sendDeleteAccountEmail(
  props: SendDeleteAccountEmailProps,
): Promise<void> {
  const { to, recipientName, deleteUrl } = props;

  const html = deleteAccountEmailHtml({ recipientName, deleteUrl });
  const subject = getDeleteAccountSubject();

  await mailer.emails.send({
    from: env.MAIL_FROM,
    to,
    subject,
    html,
  });
}
