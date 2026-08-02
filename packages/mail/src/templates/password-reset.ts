import { env } from "@lindaflor/env/server";
import { mailer } from "@lindaflor/mail";

export type PasswordResetEmailProps = {
  recipientName: string;
  resetUrl: string;
};

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

export function getPasswordResetSubject(): string {
  return "Reset your password";
}

export function passwordResetEmailHtml(props: PasswordResetEmailProps): string {
  const { recipientName, resetUrl } = props;
  const safeName = escapeHtml(recipientName);
  const safeUrl = escapeHtml(resetUrl);

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>Reset your password</title>
</head>
<body style="margin: 0; padding: 0; font-family: system-ui, -apple-system, sans-serif; background-color: #f4f4f5;">
  <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="background-color: #f4f4f5;">
    <tr>
      <td align="center" style="padding: 32px 16px;">
        <table role="presentation" width="100%" cellspacing="0" cellpadding="0" style="max-width: 480px; background-color: #ffffff; border-radius: 8px; box-shadow: 0 1px 3px rgba(0,0,0,0.08);">
          <tr>
            <td style="padding: 32px 24px;">
              <h1 style="margin: 0 0 8px; font-size: 18px; font-weight: 600; color: #18181b;">Reset your password</h1>
              <p style="margin: 0 0 24px; font-size: 14px; line-height: 1.5; color: #3f3f46;">Hi ${safeName}, we received a request to reset the password on your account.</p>
              <p style="margin: 0 0 24px;"><a href="${safeUrl}" style="display: inline-block; padding: 10px 16px; font-size: 14px; font-weight: 500; color: #ffffff; background-color: #18181b; text-decoration: none; border-radius: 6px;">Reset password</a></p>
              <p style="margin: 0 0 8px; font-size: 12px; line-height: 1.5; color: #71717a;">This link will expire in 1 hour.</p>
              <p style="margin: 0; font-size: 12px; line-height: 1.5; color: #71717a;">If you did not request this, you can safely ignore this email.</p>
            </td>
          </tr>
        </table>
      </td>
    </tr>
  </table>
</body>
</html>`;
}

export type SendPasswordResetEmailProps = {
  to: string;
  recipientName: string;
  resetUrl: string;
};

export async function sendPasswordResetEmail(
  props: SendPasswordResetEmailProps,
): Promise<void> {
  const { to, recipientName, resetUrl } = props;

  const html = passwordResetEmailHtml({ recipientName, resetUrl });
  const subject = getPasswordResetSubject();

  await mailer.emails.send({
    from: env.MAIL_FROM,
    to,
    subject,
    html,
  });
}
