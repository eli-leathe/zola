+++
title = "Typst pages"
description = "Render Typst documents as native Zola pages"
weight = 90
+++

This fork supports Typst 0.15.1 HTML export alongside Markdown. Enable it in
`zola.toml`:

```toml
[typst]
enabled = true
# Optional module exporting a `prelude(body)` show rule. Paths use Typst's
# project-root-relative syntax.
prelude = "/content/prelude.typ"
# Defaults to "zola".
metadata_label = "zola"
```

Each `.typ` page must emit exactly one labelled metadata dictionary. Its fields
use the same schema and validation as Markdown front matter:

```typst
#metadata((
  title: "Hello from Typst",
  description: "A native Typst page",
  date: "2026-09-24",
  taxonomies: (tags: ("math",)),
  extra: (example: true),
)) <zola>

= A heading

An equation: $a^2 + b^2 = c^2$.
```

`content/posts/hello/index.typ` becomes `/posts/hello/`; other `.typ` files follow
the same filename, language, slug and path conventions as Markdown pages.
Sections still use `_index.md`. Markdown and Typst pages may coexist, but their
output paths must not collide.

Templates receive rendered HTML through `page.content`. **Include the generated
styles** in the document head:

```jinja2
{{ page.extra.typst_head | default(value="") | safe }}
```

`extra.typst_head` is reserved for generated Typst styles. HTML output bypasses
both Tera content interpolation and Markdown rendering; Zola shortcodes and
Markdown `@/` links are not processed inside Typst pages. Use Typst links and
show rules instead. Typst handles syntax highlighting, MathML and SVG diagrams.
Zola does not currently construct a table of contents, collect links for link
checking/backlinks, or calculate reading analytics for Typst pages.

## Images

Typst exports raster images as embedded data URLs. The adapter queues PNG, JPEG
and WebP images through **Zola's existing image processor**, then replaces their
`src` attributes with the resulting asset URLs before template/feed rendering.
Resizing, encoding, caching and pruning use the same pipeline as `resize_image`.
Identical embedded images share an output asset across pages. No temporary
source files or separate optimizer are needed.

The defaults limit images to 1600 × 1600 pixels without upscaling, and encode
WebP at quality 80. Configure these independently of your Typst show rules:

```toml
[typst.images]
enabled = true
max_width = 1600
max_height = 1600
format = "webp" # same output formats as resize_image, including "auto"
quality = 80
```

Processed assets live in `static/processed_images/` and are copied to the build
output and served normally by `zola serve`. Add `static/processed_images/` to
`.gitignore`, as with other Zola-processed images. Generated asset URLs follow
the configured base URL, including any path prefix. SVG diagrams remain inline;
GIF and animated PNG/WebP images retain their original embedded representation
to avoid losing animation. Set `enabled = false` to keep all embedded images
unchanged.

The site directory is the Typst project root. Package imports use Typst Kit's
normal cache/downloader. Ignore helper `.typ` modules so they aren't discovered
as pages:

```toml
# Top-level configuration, before [typst].
ignored_content = ["**/prelude.typ", "**/helpers/**"]
```

When Typst is enabled, content changes trigger a full rebuild even with
`zola serve --fast`, so changes to imported modules, bibliographies and images
are reflected. Keep these dependencies under `content/` (or `static/` for
ordinary static assets); arbitrary files outside Zola's watched directories do
not trigger rebuilds. Typst HTML export is experimental upstream.
