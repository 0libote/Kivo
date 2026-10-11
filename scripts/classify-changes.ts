import { changeScope } from "./change-scope";

const base = process.env.CHANGE_BASE;
const head = process.env.CHANGE_HEAD;
const output = process.env.GITHUB_OUTPUT;
if (!base || !head || !output || !/^[0-9a-f]{40}$/.test(base) || !/^[0-9a-f]{40}$/.test(head))
  throw new Error("Expected GitHub base/head SHAs and output path.");
const all = process.env.GITHUB_EVENT_NAME !== "pull_request";
const diff = Bun.spawnSync(["git", "diff", "--name-only", `${base}...${head}`]);
if (diff.exitCode !== 0) throw new Error(diff.stderr.toString());
const scope = all
  ? { frontend: true, native: true, desktop: true, website: true }
  : changeScope(diff.stdout.toString().trim().split("\n").filter(Boolean));
const file = Bun.file(output);
await Bun.write(
  output,
  `${await file.text()}${Object.entries(scope)
    .map(([key, value]) => `${key}=${value}`)
    .join("\n")}\n`,
);
console.info(scope);
