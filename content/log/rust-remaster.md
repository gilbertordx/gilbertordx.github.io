+++
title = "Rewriting my projects in Rust, with an AI doing the typing"
date = "2026-11-21"
tags = ["rust", "ai", "testing", "projects"]
summary = "I took three of my own projects, rebuilt each one in Rust next to the original, and checked every rewrite against the old code on the same inputs. What I did, what the AI did, and what broke."
+++

I want to learn Rust. I also want a portfolio that shows how I work with AI
tools, so I did both at once. This entry says plainly who did what.

**I chose the projects, set the rules and tested the results. Claude Code wrote
the Rust.** I do not claim to have typed it. What I can show is that the
rewrites behave like the originals, and that I can explain how they are built.

## The rules

1. **Keep the original.** The Rust version lives in a `rust/` folder next to the
   Python or JavaScript code. Nothing was deleted.
2. **Prove it, do not trust it.** Each rewrite is run against the old code on
   identical inputs, and the outputs are compared.
3. **Document it as a learning guide.** Every repo has a
   `docs/RUST_LEARNING.md` that explains the code to someone who does not know
   Rust yet.
4. **Drop what is not worth porting.** The Garmin and Spotify parts of the
   workout log were left behind on purpose.

## The three remasters

### tbjp-oracle: forum archive search

[Repo](https://github.com/gilbertordx/tbjp-oracle). A search interface over a
scraped forum archive. The Rust version uses `axum` and `tokio` for the web
server, with its own chunker, index and keyword search.

How it was checked: ingesting the same data produced **15,609 identical
chunks** in Python and Rust, and **144 different search configurations**
(scope, sort, exact match and so on) returned the same results.

### logbook: workout log

[Repo](https://github.com/gilbertordx/logbook). The original runs in the
browser with plain JavaScript. The Rust version is split into a core library
(models, dates, statistics, storage) and a web front end written in **Leptos**
and compiled to WebAssembly.

How it was checked: the same inputs went through the JavaScript and the Rust
core and the outputs were compared, then a headless browser compared the two
interfaces.

### rhcp-chatbot: trivia chatbot

[Repo](https://github.com/gilbertordx/rhcp-chatbot). My first project, and the
hardest port. The Python version leans on NLTK and scikit-learn, so the Rust
version had to re-implement them: the Punkt sentence splitter, the word
tokenizer, the Porter stemmer, TF-IDF and a logistic regression classifier.

How it was checked: class probabilities match scikit-learn to about 1e-15. The
password hashing and login tokens are compatible with the same SQLite file the
Python version uses, so either server can read the other's database.

## A launcher to run all of it

Three projects with different commands got tedious, so I also built
[launchpad](https://github.com/gilbertordx/launchpad): a terminal menu (Rust and
`ratatui`) where I pick a project with the arrow keys or the mouse, press Enter,
and it starts and opens in the browser. It can also update a project from
GitHub and rebuild itself. It has 35 unit tests and a 25-check end-to-end test
that drives the real program through a pseudo-terminal.

## What went wrong

- **Windows line endings broke my own portfolio.** Git on Windows turned the
  log files into CRLF, and this site's front-matter parser choked on a stray
  carriage return. The fix was one line.
- **`trunk` was not installed.** The logbook needs it to build the WebAssembly
  front end, and my setup script forgot to install it.
- **A Leptos page showed stale values.** Patching the page in place left old
  text in a `<select>` and an `<input>`. The fix was to re-create the list
  whenever its data changed.
- **A fake terminal is not a real one.** My launcher crashed after running an
  interactive action because it asked the terminal for the cursor position.
  The end-to-end test caught it.

## What I learned

Writing tests that compare two programs is the most useful habit I took from
this: it turns "looks right" into "is the same". I also learned how much of
Rust is about the compiler making me say who owns each piece of data. The
learning guides in each repo are where I wrote that down.

The code is all public and the guides are in `docs/RUST_LEARNING.md`.
