// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

const repository = "https://github.com/armaan-v924/tix";

export default defineConfig({
  site: "https://tix.armaanv.dev",
  // Set by the publish workflow: "/dev/" for main, "/latest/" and "/vX.Y/"
  // for a release tag. Astro bakes this into every URL it generates, which is
  // why a release is built twice rather than copied — a copy of /v3.1/ would
  // have every link still pointing at /v3.1/.
  base: process.env.DOCS_BASE ?? "/",
  // Trailing slashes keep the generated in-page anchors
  // (`/reference/cli/ticket/#tix-ticket-setup`) resolving the same way on
  // GitHub Pages as they do locally.
  trailingSlash: "always",
  integrations: [
    starlight({
      title: "tix",
      favicon: "/favicon.svg",
      // Only the development build wears a banner; see the component.
      components: { Banner: "./src/components/Banner.astro" },
      description:
        "A ticket-scoped workspace manager built on git worktrees.",
      customCss: ["./src/styles/tix.css"],
      social: [
        { icon: "github", label: "GitHub", href: repository },
      ],
      editLink: { baseUrl: `${repository}/edit/main/docs/` },
      lastUpdated: true,
      sidebar: [
        {
          label: "Start Here",
          items: [{ autogenerate: { directory: "start" } }],
        },
        {
          label: "Guides",
          items: [{ autogenerate: { directory: "guides" } }],
        },
        {
          label: "Concepts",
          items: [{ autogenerate: { directory: "concepts" } }],
        },
        {
          label: "Plugins",
          items: [{ autogenerate: { directory: "plugins" } }],
        },
        {
          label: "Reference",
          items: [
            { autogenerate: { directory: "reference/cli" } },
            { slug: "reference/configuration" },
            { slug: "reference/pytix" },
            {
              // Rustdoc is not part of the Astro build; the publish workflow
              // copies `cargo doc` output into `crates/` alongside it.
              label: "Crate Docs",
              link: "/crates/",
            },
          ],
        },
      ],
    }),
  ],
});
