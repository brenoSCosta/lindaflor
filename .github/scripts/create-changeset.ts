/**
 * Creates changeset files from conventional commits in a PR or push.
 * Used by .github/workflows/release.yml.
 * Aggregates all commits in the range and generates one .changeset file per affected package.
 * Commit hashes in the body are written as Markdown links to GitHub.
 *
 * Env: BASE_SHA, HEAD_SHA (or GITHUB_SHA) for head, PR_NUMBER (optional: omit or "0" for push mode),
 * GITHUB_REPOSITORY (for commit links)
 */

const SCOPE_TO_PACKAGE: Record<string, string> = {
  api: "@lindaflor/api",
  web: "@lindaflor/web",
  native: "@lindaflor/native",
  db: "@lindaflor/db",
  auth: "@lindaflor/auth",
  config: "@lindaflor/config",
  env: "@lindaflor/env",
  server: "@lindaflor/server",
  mail: "@lindaflor/mail",
  valkey: "@lindaflor/valkey",
  s3: "@lindaflor/s3",
  e2e: "@lindaflor/e2e",
  shared: "@lindaflor/shared",
  core: "@lindaflor/core",
};

const TYPE_TO_BUMP: Record<string, "patch" | "minor" | "major"> = {
  feat: "minor",
  fix: "patch",
  refactor: "patch",
  chore: "patch",
  ci: "patch",
  docs: "patch",
};

const BUMP_ORDER = { patch: 0, minor: 1, major: 2 } as const;

type Bump = keyof typeof BUMP_ORDER;

const BOT_PATTERNS = [
  /^chore\s*\(\s*release\s*\)\s*:/i,
  /^chore\s*\(\s*changesets\s*\)\s*:/i,
  /Version Packages/i,
  /add changesets from conventional commits/i,
];

function isBotCommit(subject: string): boolean {
  return BOT_PATTERNS.some((re) => re.test(subject));
}

function parseSubject(line: string): {
  type: string;
  scope: string | null;
  subject: string;
  breaking: boolean;
} | null {
  const match = line.match(/^(\w+)(!)?(?:\(([^)]+)\))?(!)?:\s*(.+)$/);
  if (!match) return null;
  const [, type, bang, scope, afterScopeBang, subject] = match;
  return {
    type: type ?? "",
    scope: scope ?? null,
    subject: subject?.trim() ?? "",
    breaking: bang === "!" || afterScopeBang === "!",
  };
}

function typeToBump(type: string, breaking: boolean): Bump | null {
  if (breaking) return "major";
  const b = TYPE_TO_BUMP[type];
  return b ?? null;
}

function maxBump(a: Bump, b: Bump): Bump {
  return BUMP_ORDER[a] >= BUMP_ORDER[b] ? a : b;
}

export type CommitLine = {
  shortSha: string;
  fullSha: string;
  subject: string;
  author: string;
};

async function gitLog(base: string, head: string): Promise<CommitLine[]> {
  const proc = Bun.spawn(
    ["git", "log", "--pretty=format:%h%x00%H%x00%s%x00%an", `${base}..${head}`],
    {
      cwd: process.cwd(),
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const out = await new Response(proc.stdout).text();
  const err = await new Response(proc.stderr).text();
  const exit = await proc.exited;
  if (exit !== 0) {
    throw new Error(`git log failed: ${err}`);
  }
  return out
    .trim()
    .split("\n")
    .filter((s) => s.length > 0)
    .map((line) => {
      const parts = line.split("\0");
      const shortSha = parts[0] ?? "";
      const fullSha = parts[1] ?? "";
      const subject = parts[2] ?? "";
      const author = parts[3] ?? "";
      return { shortSha, fullSha, subject, author };
    });
}

async function gitRevParse(ref: string, short: number): Promise<string> {
  const proc = Bun.spawn(["git", "rev-parse", `--short=${short}`, ref], {
    cwd: process.cwd(),
    stdout: "pipe",
    stderr: "pipe",
  });
  const out = await new Response(proc.stdout).text();
  const err = await new Response(proc.stderr).text();
  const exit = await proc.exited;
  if (exit !== 0) {
    throw new Error(`git rev-parse failed: ${err}`);
  }
  return out.trim();
}

function packageToSlug(pkg: string): string {
  return pkg.replace(/^@lindaflor\//, "");
}

const PUSH_MODE_PR = "0";

/**
 * Aggregates all commit subjects and returns one changeset entry per affected package.
 * Each entry has slug (for filename) and full file content. Exported for tests.
 * Commit hashes in the body are formatted as Markdown links to GitHub when repo is set.
 * When prNumber is PUSH_MODE_PR ("0"), body omits pr line (push mode).
 */
export function computeChangesetEntries(
  commits: CommitLine[],
  prNumber: string,
  repoUrl?: string,
): Array<{ slug: string; content: string }> {
  const isPushMode = prNumber === PUSH_MODE_PR;
  const bumpsByPackage = new Map<string, Bump>();
  const commitsByPackage = new Map<string, CommitLine[]>();

  for (const { shortSha, fullSha, subject: line, author } of commits) {
    if (isBotCommit(line)) continue;
    const parsed = parseSubject(line);
    if (!parsed) continue;
    const { type, scope, subject, breaking } = parsed;
    const pkg = scope ? SCOPE_TO_PACKAGE[scope] : null;
    if (!pkg) continue;

    const bump = typeToBump(type, breaking);
    if (!bump) continue;

    const list = commitsByPackage.get(pkg);
    if (list)
      list.push({ shortSha, fullSha, subject: subject || line, author });
    else
      commitsByPackage.set(pkg, [
        { shortSha, fullSha, subject: subject || line, author },
      ]);
    const existing = bumpsByPackage.get(pkg);
    bumpsByPackage.set(pkg, existing ? maxBump(existing, bump) : bump);
  }

  const sortedPackages = [...bumpsByPackage.keys()].sort((a, b) =>
    packageToSlug(a).localeCompare(packageToSlug(b)),
  );

  return sortedPackages.map((pkg) => {
    const bump = bumpsByPackage.get(pkg)!;
    const list = commitsByPackage.get(pkg) ?? [];
    // Oldest first (commits from git log are newest first)
    const lines = [...list].reverse();
    const commitLines = lines.map((c) => {
      const hashLink = repoUrl
        ? `[${c.shortSha}](${repoUrl}/commit/${c.fullSha})`
        : c.shortSha;
      return `- ${c.subject} (${hashLink} by ${c.author})`;
    });
    const body = isPushMode
      ? lines.length > 0
        ? commitLines.join("\n")
        : "Push to branch"
      : lines.length > 0
        ? `${commitLines.join("\n")}\n\npr: #${prNumber}`
        : `PR #${prNumber}\n\npr: #${prNumber}`;
    const content = `---
"${pkg}": ${bump}
---

${body}
`;
    return { slug: packageToSlug(pkg), content };
  });
}

async function main(): Promise<void> {
  const baseSha = process.env.BASE_SHA;
  const headSha = process.env.HEAD_SHA ?? process.env.GITHUB_SHA ?? "HEAD";
  const prNumberRaw = process.env.PR_NUMBER;

  if (!baseSha) {
    console.error("BASE_SHA env is required");
    process.exit(1);
  }
  if (/^0+$/.test(baseSha)) {
    process.exit(0);
  }
  const prNumber =
    prNumberRaw?.trim() === "" || prNumberRaw?.trim() === "0"
      ? "0"
      : (prNumberRaw?.trim() ?? "0");
  if (prNumber !== "0" && !/^\d+$/.test(prNumber)) {
    console.error('PR_NUMBER must be a number or "0" for push mode');
    process.exit(1);
  }

  await run(baseSha, headSha, prNumber);
}

async function run(
  baseSha: string,
  headSha: string,
  prNumber: string,
): Promise<void> {
  const commits = await gitLog(baseSha, headSha);
  const shortSha = await gitRevParse(headSha, 7);
  const repoUrl = process.env.GITHUB_REPOSITORY
    ? `https://github.com/${process.env.GITHUB_REPOSITORY}`
    : undefined;

  const entries = computeChangesetEntries(commits, prNumber, repoUrl);
  if (entries.length === 0) {
    const scopes = Object.keys(SCOPE_TO_PACKAGE).join(", ");
    console.log(
      "No conventional commits with known scope; skipping changeset.",
    );
    if (commits.length === 0) {
      console.log(`No commits in range ${baseSha}..${headSha}.`);
    } else {
      console.log(
        `Commits in range (${commits.length}): ${commits.map((c) => c.subject).join(" | ")}`,
      );
      console.log(
        `Use a scope from: ${scopes}. Example: feat(api): add date filter`,
      );
    }
    return;
  }

  const outDir = `${process.cwd()}/.changeset`;
  const filePrefix = prNumber === PUSH_MODE_PR ? "push" : `pr-${prNumber}`;
  for (const { slug, content } of entries) {
    const filename = `${filePrefix}-${shortSha}-${slug}.md`;
    const filepath = `${outDir}/${filename}`;
    await Bun.write(filepath, content);
    console.log(`Wrote ${filepath}`);
  }
  console.log("CHANGESETS_CREATED");
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err);
    process.exit(1);
  });
}
