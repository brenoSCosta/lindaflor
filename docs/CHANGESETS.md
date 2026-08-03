# Versioning and Changelog (Changesets)

This project uses [Changesets](https://github.com/changesets/changesets) for versioning and changelog generation. Commit types follow [Conventional Commits v1.0.0](https://www.conventionalcommits.org/en/v1.0.0/).

**Scope → package:**

| Scope  | Package           |
| ------ | ----------------- |
| api    | @lindaflor/api    |
| core   | @lindaflor/core   |
| shared | @lindaflor/shared |
| web    | @lindaflor/web    |
| native | @lindaflor/native |
| db     | @lindaflor/db     |
| auth   | @lindaflor/auth   |
| config | @lindaflor/config |
| env    | @lindaflor/env    |
| server | @lindaflor/server |
| mail   | @lindaflor/mail   |
| valkey | @lindaflor/valkey |
| e2e    | @lindaflor/e2e    |

## Agent skill: atomic semantic commits

The project has an agent skill that uses this file: [.agents/skills/atomic-semantic-commits/SKILL.md](../.agents/skills/atomic-semantic-commits/SKILL.md). It analyzes your git diff and proposes **atomic** commits with **Conventional Commits** messages.

- **How it uses this doc:** The scope → package table above is the source of truth for commit scopes; the skill follows Conventional Commits v1.0.0 as stated here.
- **When to use:** Ask the agent to split a diff into commits, create atomic/semantic commits, or format commits with conventional messages.

## Custom changelog

[.changeset/changelog.js](../.changeset/changelog.js): **getReleaseLine** fetches PR/commit/author from GitHub API and appends links; **getDependencyReleaseLine** adds "### Updated Dependencies". Repo: `GITHUB_REPOSITORY` in CI, or set `CHANGESET_REPO` locally.

## CI: Create changeset (from push to main)

[.github/workflows/release.yml](../.github/workflows/release.yml) runs on **push to main**. It runs [.github/scripts/create-changeset.ts](../.github/scripts/create-changeset.ts) to generate changeset file(s) from conventional commits in the push range, then commits and pushes them to branch `changeset-release/main`. **Commits must use a known scope** (see the scope → package table above) for the script to add a changeset (e.g. `feat(api): add filter`, not `feat: add filter`). If no such commits exist, no changeset is added.

## CI: Release workflow

[.github/workflows/release.yml](../.github/workflows/release.yml) runs on push to `main`. It uses branch `changeset-release/main`: when there are unreleased changesets, it runs `bun run version`, commits, pushes to that branch, and creates or updates the **"ci: release"** PR from `changeset-release/main` into `main`. **CHANGELOG.md and version bumps land on main when you merge that PR**, not when you merge the feature PR. A snapshot job then runs `version --snapshot next` and `publish --tag next` to publish packages with tag `next`.

## References

- [.agents/skills/atomic-semantic-commits/SKILL.md](../.agents/skills/atomic-semantic-commits/SKILL.md) – Atomic semantic commits skill (uses this doc for scopes).
- [.changeset/README.md](../.changeset/README.md) – Optional metadata (pr, commit, author).
- [Changesets](https://github.com/changesets/changesets/blob/main/docs/common-questions.md)
