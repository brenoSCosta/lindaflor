import { getInfo, getInfoFromPullRequest } from "@changesets/get-github-info";

/**
 * @returns {string}
 */
function getRepo() {
  return (
    process.env.GITHUB_REPOSITORY ||
    process.env.CHANGESET_REPO ||
    "og-service/og-service"
  );
}

/** Extract hash from commit link or string and return [shortHash](url) without backticks. */
function buildCommitLink(repo, linkStr) {
  if (!linkStr || typeof linkStr !== "string") return linkStr;
  const m = linkStr.match(/\/commit\/([a-f0-9]+)/i);
  const hash = m ? m[1] : linkStr;
  const short = hash.slice(0, 7);
  return `[${short}](https://github.com/${repo}/commit/${hash})`;
}

/** @type {import("@changesets/types").ChangelogFunctions} */
export default {
  getDependencyReleaseLine: async (_, dependenciesUpdated) => {
    if (!dependenciesUpdated.length) {
      return "";
    }

    const list = dependenciesUpdated.map(
      (dependency) => ` - ${dependency.name}@${dependency.newVersion}`,
    );

    return ["### Updated Dependencies:", ...list].join("\n");
  },
  getReleaseLine: async (changeset) => {
    const repo = getRepo();

    /** @type number | undefined */
    let prFromSummary;
    /** @type string | undefined */
    let commitFromSummary;
    /** @type string[] */
    const usersFromSummary = [];

    // Remove PR, commit, author/user lines from summary
    const replacedChangelog = changeset.summary
      .replace(/^\s*(?:pr|pull|pull\s+request):\s*#?(\d+)/im, (_, pr) => {
        const num = Number(pr);
        if (!Number.isNaN(num)) {
          prFromSummary = num;
        }
        return "";
      })
      .replace(/^\s*commit:\s*([^\s]+)/im, (_, commit) => {
        commitFromSummary = commit;
        return "";
      })
      .replace(/^\s*(?:author|user):\s*@?([^\s]+)/gim, (_, user) => {
        usersFromSummary.push(user);
        return "";
      })
      .trim();

    const links = await (async () => {
      if (prFromSummary !== undefined) {
        let { links } = await getInfoFromPullRequest({
          pull: prFromSummary,
          repo,
        });
        if (commitFromSummary) {
          const shortCommitId = commitFromSummary.slice(0, 7);
          links = {
            ...links,
            commit: `[${shortCommitId}](https://github.com/${repo}/commit/${commitFromSummary})`,
          };
        }
        return links;
      }
      const commitToFetchFrom = commitFromSummary || changeset.commit;
      if (commitToFetchFrom) {
        let { links } = await getInfo({
          commit: commitToFetchFrom,
          repo,
        });
        const shortCommitId = commitToFetchFrom.slice(0, 7);
        links = {
          ...links,
          commit: `[${shortCommitId}](https://github.com/${repo}/commit/${commitToFetchFrom})`,
        };
        return links;
      }
      return {
        commit: null,
        pull: null,
        user: null,
      };
    })();

    const finalLinks =
      links.commit != null
        ? { ...links, commit: buildCommitLink(repo, links.commit) }
        : links;

    const users =
      usersFromSummary.length > 0
        ? usersFromSummary
            .map(
              (userFromSummary) =>
                `[@${userFromSummary}](https://github.com/${userFromSummary})`,
            )
            .join(", ")
        : links.user;

    const metadata = [
      finalLinks.pull === null ? "" : ` (${finalLinks.pull})`,
      finalLinks.commit === null ? "" : ` (${finalLinks.commit})`,
      users === null ? "" : ` by ${users}`,
    ].join("");

    const [firstLine, ...rest] = replacedChangelog.split("\n");
    const restSummary = rest.join("\n").trim();

    let releaseLine = `\n- ${firstLine}${metadata}`;
    if (restSummary) {
      releaseLine += "\n\n" + restSummary;
    }
    return releaseLine;
  },
};
