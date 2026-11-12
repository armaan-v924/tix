// The agent skill as plain Markdown, at `<base>/skill.md`: the form an agent
// fetches, where the page under guides/ is the form a person reads. Both come
// from the one source file, so they cannot disagree, and both are rebuilt
// under every version's base — `/v3.3/skill.md` is the skill for tix 3.3.
import type { APIRoute } from "astro";
import source from "../content/docs/guides/agents.md?raw";

const site = "https://tix.armaanv.dev";

export const GET: APIRoute = () => {
  // Frontmatter is the page's, not the skill's: lift its title and
  // description into the Markdown, and drop the rest.
  const match = source.match(/^---\n([\s\S]*?)\n---\n/);
  const frontmatter = match?.[1] ?? "";
  const field = (name: string) =>
    frontmatter.match(new RegExp(`^${name}: (.*)$`, "m"))?.[1] ?? "";
  const body = match ? source.slice(match[0].length) : source;

  const base = import.meta.env.BASE_URL.replace(/\/?$/, "/");
  const header = [
    `# ${field("title")}`,
    "",
    `> ${field("description")}`,
    `> Published at ${site}${base}skill.md (page: ${site}${base}guides/agents/).`,
    "",
  ].join("\n");

  return new Response(`${header}\n${body.trimStart()}`, {
    headers: { "Content-Type": "text/markdown; charset=utf-8" },
  });
};
