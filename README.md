# security-student

Portfolio and log. A small static site generator in Rust: no framework, three dependencies.

```text
site.toml         profile, links, skills, experience, projects
content/log/*.md  one Markdown file per log entry
static/           style.css, app.js (copied as-is)
src/main.rs       reads the above, writes ./dist
```

## Use

```bash
cargo run            # build into ./dist
cargo run -- serve   # build, then serve on http://127.0.0.1:4321
```

## New log entry

Create `content/log/my-entry.md` (the file name becomes the URL):

```markdown
+++
title = "Entry title"
date = "2026-11-16"
tags = ["rust"]
summary = "One line for the index."
+++

Markdown body.
```

Add `draft = true` to keep an entry out of the build.

## Deploy

Pushing to `main` runs `.github/workflows/deploy.yml`, which builds the site and publishes `dist/` to GitHub Pages.
