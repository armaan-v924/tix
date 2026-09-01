// @ts-check
import { defineConfig } from "astro/config";
import starlight from "@astrojs/starlight";

const repository = "https://github.com/armaan-v924/tix";

export default defineConfig({
  site: "https://tix.armaanv.dev",
  // Trailing slashes keep the generated in-page anchors
  // (`/reference/cli/ticket/#tix-ticket-setup`) resolving the same way on
  // GitHub Pages as they do locally.
  trailingSlash: "always",
  integrations: [
    starlight({
      title: "tix",
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
          label: "CLI reference",
          items: [{ autogenerate: { directory: "reference/cli" } }],
        },
        {
          // Rustdoc is not part of the Astro build; CI copies `cargo doc`
          // output into `dist/crates/` after this site is built.
          label: "Crate docs",
          link: "/crates/",
        },
      ],
    }),
  ],
});
