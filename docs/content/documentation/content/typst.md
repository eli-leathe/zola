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
show rules instead. Typst handles syntax highlighting, MathML and embedded
images/SVG. Zola does not currently construct a table of contents, collect links
for link checking/backlinks, or calculate reading analytics for Typst pages.

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
