// The beta channel (Settings > Updates > Beta versions): one build holding every change
// not released yet, so it can be tried with a click. The Beta workflow
// (.github/workflows/beta.yml) runs it from the repository's root:
//
//   node beta.mjs merge <state.json>
//       main, then every open pull request of this repository merged into it, oldest
//       first. Only those whose CI passed on their latest commit go in; one whose checks
//       fail or still run, or that does not merge, is left out this time and the notes
//       say why. Pull requests from forks never take part, nor those that change nothing
//       in the app. When nothing is left that the last release does not have, `changes`
//       is false and the workflow takes the beta down instead.
//   node beta.mjs version <run number>
//       the beta's version, written in the three files that carry it: the next patch with
//       the run as its beta number (0.3.0 -> 0.3.1-beta.42), or main's own version while
//       it is not released yet (0.3.0 before its tag -> 0.3.0-beta.42). Either way the
//       release that follows is newer than the beta, so it reaches beta users as a plain update.
//   node beta.mjs notes <state.json> <version> <app-notes.txt>
//       prints the release's notes (Markdown, for GitHub) and writes the short list the
//       app shows (one pull request title per line) to <app-notes.txt>.
//
// The workflow copies this file aside before anything is merged, so a pull request
// cannot change how the beta is made.

import { execFileSync } from "node:child_process";
import { appendFileSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { pathToFileURL } from "node:url";

const root = process.cwd();

/** The workflow whose result decides whether a pull request goes in (ci.yml). */
export const CI_WORKFLOW = "CI";

/** What makes up the installed app: everything but docs, notes and the workflows. */
export const APP_PATHS = [".", ":(exclude).github", ":(exclude)docs", ":(exclude)scripts", ":(exclude,glob)**/*.md"];

/** Titles longer than this are cut. */
const MAX_TITLE = 120;

/** The beta's version: see the header. */
export function betaVersion(base, run, released) {
  const m = /^(\d+)\.(\d+)\.(\d+)$/.exec(String(base).trim());
  if (!m) throw new Error(`not a release version: ${base}`);
  const n = Number(run);
  if (!Number.isInteger(n) || n < 1) throw new Error(`not a run number: ${run}`);
  return `${m[1]}.${m[2]}.${Number(m[3]) + (released ? 1 : 0)}-beta.${n}`;
}

/** "pass", "fail" or "pending" for a pull request's latest commit, from GitHub's statusCheckRollup. */
export function ciState(rollup) {
  const runs = (Array.isArray(rollup) ? rollup : []).filter((c) => c && c.workflowName === CI_WORKFLOW);
  if (runs.length === 0) return "pending";
  if (runs.some((c) => c.status !== "COMPLETED")) return "pending";
  const ok = new Set(["SUCCESS", "NEUTRAL", "SKIPPED"]);
  return runs.every((c) => ok.has(c.conclusion)) ? "pass" : "fail";
}

/** A title on one line, cut to a readable length. */
export function cleanTitle(title) {
  const one = String(title ?? "").replace(/\s+/g, " ").trim();
  return one.length > MAX_TITLE ? `${one.slice(0, MAX_TITLE - 1).trimEnd()}…` : one;
}

/** The release's notes on GitHub. */
export function releaseNotes(state, version) {
  const why = { checks: "its checks fail", pending: "its checks are still running", conflict: "it conflicts with the others" };
  const lines = [
    `Easy Uninstaller beta ${version}: main (${state.main.slice(0, 7)}) with the open pull requests, to try them before they are released.`,
    "Install it from the app: Settings > Updates > Beta versions.",
    "",
    state.included.length ? "In this beta:" : "No pull request is in it: this is main, with what has not been released yet.",
    ...state.included.map((pr) => `- #${pr.number} ${cleanTitle(pr.title)}`),
  ];
  if (state.excluded.length) {
    lines.push("", "Left out this time:", ...state.excluded.map((pr) => `- #${pr.number} ${cleanTitle(pr.title)} (${why[pr.reason]})`));
  }
  return lines.join("\n") + "\n";
}

/** What the app shows under "What's new": the titles, one per line. */
export function appNotes(state) {
  return state.included.map((pr) => cleanTitle(pr.title)).join("\n");
}

// ── The steps ────────────────────────────────────────────────────────────────

function git(args) {
  return execFileSync("git", ["-c", "user.name=Easy Uninstaller beta", "-c", "user.email=beta@invalid", ...args], {
    cwd: root,
    encoding: "utf8",
    stdio: ["ignore", "pipe", "pipe"],
  });
}

function tryGit(args) {
  try {
    git(args);
    return true;
  } catch {
    return false;
  }
}

/** True when `range` (as git diff takes it) changes the app; true too when git cannot tell. */
function changesApp(range) {
  return !tryGit(["diff", "--quiet", ...range, "--", ...APP_PATHS]);
}

function mainVersion() {
  return JSON.parse(readFileSync(join(root, "src-tauri", "tauri.conf.json"), "utf8")).version;
}

function openPullRequests() {
  const repo = process.env.GITHUB_REPOSITORY;
  if (!repo) throw new Error("GITHUB_REPOSITORY is not set");
  const out = execFileSync(
    "gh",
    ["pr", "list", "--repo", repo, "--base", "main", "--state", "open", "--limit", "100",
      "--json", "number,title,headRefOid,isCrossRepository,isDraft,statusCheckRollup"],
    { encoding: "utf8" },
  );
  return JSON.parse(out)
    .filter((pr) => !pr.isCrossRepository)
    .sort((a, b) => a.number - b.number);
}

function merge(statePath) {
  const main = git(["rev-parse", "HEAD"]).trim();
  const included = [];
  const excluded = [];
  for (const pr of openPullRequests()) {
    const ref = `refs/beta/pr-${pr.number}`;
    git(["fetch", "--no-tags", "origin", `+refs/pull/${pr.number}/head:${ref}`]);
    const head = git(["rev-parse", ref]).trim();
    if (!changesApp([`${main}...${head}`])) {
      console.log(`#${pr.number} skipped: it changes nothing in the app`);
      continue;
    }
    const entry = { number: pr.number, title: pr.title };
    const ci = ciState(pr.statusCheckRollup);
    // A push after the checks were read brings a commit no CI has passed yet.
    const state = ci === "pass" && head !== pr.headRefOid ? "pending" : ci;
    if (state !== "pass") {
      excluded.push({ ...entry, reason: state === "fail" ? "checks" : "pending" });
      console.log(`#${pr.number} left out: CI ${state}`);
      continue;
    }
    if (tryGit(["merge", "--no-ff", "--no-edit", "-m", `beta: #${pr.number}`, ref])) {
      included.push(entry);
      console.log(`#${pr.number} merged`);
    } else {
      tryGit(["merge", "--abort"]);
      excluded.push({ ...entry, reason: "conflict" });
      console.log(`#${pr.number} left out: it does not merge`);
    }
  }
  const release = `v${mainVersion()}`;
  const changes = included.length > 0 || changesApp([release, main]);
  if (!changes) console.log(`nothing that ${release} does not have: no beta`);
  writeFileSync(statePath, JSON.stringify({ main, included, excluded, changes }, null, 2));
  if (process.env.GITHUB_OUTPUT) appendFileSync(process.env.GITHUB_OUTPUT, `changes=${changes}\n`);
}

function setVersion(run) {
  const base = mainVersion();
  const released = tryGit(["rev-parse", "--verify", "--quiet", `refs/tags/v${base}`]);
  const version = betaVersion(base, run, released);
  const swapJson = (path) => {
    const text = readFileSync(path, "utf8");
    writeFileSync(path, text.replace(/("version"\s*:\s*")[^"]*(")/, `$1${version}$2`));
  };
  swapJson(join(root, "src-tauri", "tauri.conf.json"));
  swapJson(join(root, "package.json"));
  const cargo = join(root, "src-tauri", "Cargo.toml");
  writeFileSync(cargo, readFileSync(cargo, "utf8").replace(/^version\s*=\s*"[^"]*"/m, `version = "${version}"`));
  console.log(version);
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) {
  const [step, ...args] = process.argv.slice(2);
  if (step === "merge" && args[0]) merge(args[0]);
  else if (step === "version" && args[0]) setVersion(args[0]);
  else if (step === "notes" && args[2]) {
    const state = JSON.parse(readFileSync(args[0], "utf8"));
    writeFileSync(args[2], appNotes(state));
    process.stdout.write(releaseNotes(state, args[1]));
  } else {
    console.error("usage: beta.mjs merge <state.json> | version <run> | notes <state.json> <version> <app-notes.txt>");
    process.exit(2);
  }
}
