import { describe, expect, it, mock } from "bun:test";

void mock.module("@lindaflor/env/server", () => ({
  env: {
    MAIL_FROM: "test@example.com",
    RESEND_API_KEY: "re_test",
  },
}));

void mock.module("@lindaflor/mail", () => ({
  mailer: { emails: { send: () => Promise.resolve() } },
}));

const { getOrganizationInvitationSubject, organizationInvitationEmailHtml } =
  await import("@lindaflor/mail/templates/organization-invitation");

describe("organizationInvitationEmailHtml", () => {
  const baseProps = {
    inviterName: "Alice",
    organizationName: "Acme Corp",
    role: "admin",
    acceptUrl:
      "https://app.example.com/accept-invitation?id=inv-1&email=b%40x.com",
  };

  it("includes inviter, organization, role, and accept URL", () => {
    const html = organizationInvitationEmailHtml(baseProps);
    expect(html).toContain("Alice");
    expect(html).toContain("Acme Corp");
    expect(html).toContain("admin");
    // The URL is HTML-attribute-encoded in the href (& -> &amp;), which
    // browsers decode back when following the link.
    expect(html).toContain(
      'href="https://app.example.com/accept-invitation?id=inv-1&amp;email=b%40x.com"',
    );
  });

  it("escapes HTML in user-controlled fields to prevent injection", () => {
    const html = organizationInvitationEmailHtml({
      ...baseProps,
      inviterName: "<script>alert(1)</script>",
      organizationName: 'Evil & Co "test"',
    });
    expect(html).not.toContain("<script>alert(1)</script>");
    expect(html).toContain("&lt;script&gt;alert(1)&lt;/script&gt;");
    expect(html).toContain("Evil &amp; Co &quot;test&quot;");
  });
});

describe("getOrganizationInvitationSubject", () => {
  it("names the organization in the subject", () => {
    expect(getOrganizationInvitationSubject("Acme Corp")).toBe(
      "You've been invited to join Acme Corp",
    );
  });
});
