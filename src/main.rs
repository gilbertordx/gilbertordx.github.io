//! grdx: reads `site.toml`, `content/log/*.md` and `static/`, writes `dist/`.
//!
//!   cargo run            build the site
//!   cargo run -- serve   build, then serve dist/ on http://127.0.0.1:4321

use pulldown_cmark::{html, Options, Parser};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use std::{env, fs, io};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const OUT: &str = "dist";
const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July", "August", "September",
    "October", "November", "December",
];

// ---------------------------------------------------------------- data ----

#[derive(Deserialize)]
struct Site {
    brand: String,
    name: String,
    role: String,
    location: String,
    tagline: String,
    description: String,
    base_url: String,
    about: String,
    /// Short bio for the home page (Markdown).
    #[serde(default)]
    intro: String,
    #[serde(default)]
    facts: Vec<Fact>,
    #[serde(default)]
    links: Vec<Link>,
    #[serde(default)]
    experience: Vec<Job>,
    #[serde(default)]
    education: Vec<School>,
    #[serde(default)]
    credentials: Vec<Credential>,
    #[serde(default)]
    skills: Vec<SkillGroup>,
    #[serde(default)]
    projects: Vec<Project>,
}

#[derive(Deserialize)]
struct Link {
    label: String,
    url: String,
}

#[derive(Deserialize)]
struct Fact {
    label: String,
    value: String,
}

#[derive(Deserialize)]
struct Job {
    role: String,
    org: String,
    period: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    points: Vec<String>,
}

#[derive(Deserialize)]
struct School {
    school: String,
    degree: String,
    period: String,
}

#[derive(Deserialize)]
struct Credential {
    title: String,
    issuer: String,
    date: String,
    /// Optional link to the badge or certificate.
    #[serde(default)]
    url: String,
    /// Shown on the home page.
    #[serde(default)]
    featured: bool,
}

#[derive(Deserialize)]
struct SkillGroup {
    label: String,
    items: Vec<String>,
}

#[derive(Deserialize)]
struct Project {
    name: String,
    /// Empty for closed-source work: the card is then not a link.
    #[serde(default)]
    url: String,
    year: String,
    summary: String,
    #[serde(default)]
    stack: Vec<String>,
}

#[derive(Deserialize)]
struct FrontMatter {
    title: String,
    date: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    draft: bool,
}

struct Post {
    slug: String,
    number: usize,
    meta: FrontMatter,
    date: Date,
    html: String,
    minutes: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct Date {
    y: u32,
    m: u32,
    d: u32,
}

impl Date {
    fn parse(s: &str) -> Option<Date> {
        let mut p = s.split('-').map(|x| x.parse::<u32>().ok());
        let (y, m, d) = (p.next()??, p.next()??, p.next()??);
        ((1..=12).contains(&m) && (1..=31).contains(&d)).then_some(Date { y, m, d })
    }
    fn today() -> Date {
        // Civil-from-days (Howard Hinnant), so we need no date crate.
        let secs = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let z = (secs / 86_400) as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let y = (yoe + era * 400 + if m <= 2 { 1 } else { 0 }) as u32;
        Date { y, m, d }
    }
    fn iso(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.y, self.m, self.d)
    }
    fn dotted(&self) -> String {
        format!("{:04}.{:02}.{:02}", self.y, self.m, self.d)
    }
    fn month(&self) -> &'static str {
        MONTHS[(self.m - 1) as usize]
    }
    fn rfc822(&self) -> String {
        // Sakamoto's day-of-week.
        const T: [u32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if self.m < 3 { self.y - 1 } else { self.y };
        let dow = (y + y / 4 - y / 100 + y / 400 + T[(self.m - 1) as usize] + self.d) % 7;
        let day = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"][dow as usize];
        format!("{day}, {:02} {} {} 00:00:00 GMT", self.d, &self.month()[..3], self.y)
    }
}

// ------------------------------------------------------------- helpers ----

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn markdown(src: &str) -> String {
    let opts = Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_FOOTNOTES;
    let mut out = String::new();
    html::push_html(&mut out, Parser::new_ext(src, opts));
    out
}

fn write(rel: &str, contents: &str) -> Result<()> {
    let path = Path::new(OUT).join(rel);
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let dest = to.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

fn load_posts() -> Result<Vec<Post>> {
    let mut posts = Vec::new();
    for entry in fs::read_dir("content/log")? {
        let path = entry?.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let name = path.display().to_string();
        // Windows checkouts may have CRLF line endings; normalise so the "\n+++" split works.
        let raw = fs::read_to_string(&path)?.replace("\r\n", "\n");
        let rest = raw.strip_prefix("+++").ok_or(format!("{name}: missing +++ front matter"))?;
        let (front, body) =
            rest.split_once("\n+++").ok_or(format!("{name}: unterminated front matter"))?;
        let meta: FrontMatter = toml::from_str(front).map_err(|e| format!("{name}: {e}"))?;
        if meta.draft {
            continue;
        }
        let date = Date::parse(&meta.date)
            .ok_or(format!("{name}: date must be YYYY-MM-DD, got {:?}", meta.date))?;
        posts.push(Post {
            slug: path.file_stem().unwrap().to_string_lossy().into_owned(),
            number: 0,
            minutes: (body.split_whitespace().count() / 220).max(1),
            html: markdown(body),
            meta,
            date,
        });
    }
    // Number entries in chronological order, then show newest first.
    posts.sort_by(|a, b| (a.date, &a.slug).cmp(&(b.date, &b.slug)));
    for (i, p) in posts.iter_mut().enumerate() {
        p.number = i + 1;
    }
    posts.reverse();
    Ok(posts)
}

// ----------------------------------------------------------- templates ----

/// A bordered HUD panel with a cut corner and a small label.
fn panel(label: &str, class: &str, inner: &str) -> String {
    format!(
        r#"<section class="panel {class}"><div class="panel-in"><div class="panel-label"><span>// {}</span><i></i></div>{inner}</div></section>"#,
        esc(label)
    )
}

fn tags_html(tags: &[String]) -> String {
    tags.iter()
        .map(|t| format!(r#"<a class="tag" href="/tags/{0}/">#{0}</a>"#, esc(t)))
        .collect()
}

fn post_row(p: &Post) -> String {
    format!(
        r#"<li class="entry"><a class="entry-link" href="/log/{slug}/"><span class="entry-no">#{no:03}</span><time datetime="{iso}">{date}</time><span class="entry-body"><strong>{title}</strong><span class="entry-sum">{sum}</span></span></a><span class="entry-tags">{tags}</span></li>"#,
        slug = p.slug,
        no = p.number,
        iso = p.date.iso(),
        date = p.date.dotted(),
        title = esc(&p.meta.title),
        sum = esc(&p.meta.summary),
        tags = tags_html(&p.meta.tags),
    )
}

/// Posts grouped under "Month YYYY — n entries" headings.
fn post_groups(posts: &[&Post]) -> String {
    let mut out = String::new();
    let mut i = 0;
    while i < posts.len() {
        let (y, m) = (posts[i].date.y, posts[i].date.m);
        let n = posts[i..].iter().take_while(|p| p.date.y == y && p.date.m == m).count();
        out += &format!(
            r#"<h2 class="month"><span>{} {}</span><small>{} {}</small></h2><ul class="entries">"#,
            posts[i].date.month(),
            y,
            n,
            if n == 1 { "entry" } else { "entries" }
        );
        for p in &posts[i..i + n] {
            out += &post_row(p);
        }
        out += "</ul>";
        i += n;
    }
    out
}

fn credential_row(c: &Credential) -> String {
    let view = if c.url.is_empty() {
        String::new()
    } else {
        format!(r#"<a class="cred-view" href="{}" rel="noopener" target="_blank">view credential &#9656;</a>"#, esc(&c.url))
    };
    format!(
        r#"<li><strong>{}</strong><span>{} // {}</span>{view}</li>"#,
        esc(&c.title),
        esc(&c.issuer),
        esc(&c.date)
    )
}

fn project_card(p: &Project, idx: usize) -> String {
    let stack: String = p.stack.iter().map(|s| format!("<li>{}</li>", esc(s))).collect();
    let (open, close, go) = if p.url.is_empty() {
        (r#"<div class="panel card">"#.to_string(), "</div>", "Closed source")
    } else {
        (
            format!(r#"<a class="panel card" href="{}" rel="noopener">"#, esc(&p.url)),
            "</a>",
            "View source &#9656;",
        )
    };
    format!(
        r#"{open}<div class="panel-in"><div class="panel-label"><span>// {idx:02}</span><i></i><span>{year}</span></div><h3>{name}</h3><p>{sum}</p><ul class="stack">{stack}</ul><span class="card-go">{go}</span></div>{close}"#,
        year = esc(&p.year),
        name = esc(&p.name),
        sum = esc(&p.summary),
    )
}

fn page(site: &Site, title: &str, desc: &str, active: &str, path: &str, body: &str) -> String {
    let nav: String = [("log", "Log"), ("projects", "Projects"), ("about", "About")]
        .iter()
        .enumerate()
        .map(|(i, (key, label))| {
            let cur = if *key == active { r#" aria-current="page""# } else { "" };
            format!(r#"<a href="/{key}/"{cur}><small>0{}</small>{label}</a>"#, i + 1)
        })
        .collect();
    let full_title = if title.is_empty() {
        format!("{} // {}", site.brand, site.name)
    } else {
        format!("{} // {}", title, site.brand)
    };
    let links: String = site
        .links
        .iter()
        .map(|l| format!(r#"<a href="{}" rel="me noopener">{}</a>"#, esc(&l.url), esc(&l.label)))
        .collect();
    format!(
        r#"<!doctype html>
<html lang="en" data-theme="dark">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title}</title>
<meta name="description" content="{desc}">
<meta property="og:title" content="{title}">
<meta property="og:description" content="{desc}">
<meta property="og:type" content="website">
<meta property="og:url" content="{base}{path}">
<link rel="canonical" href="{base}{path}">
<link rel="alternate" type="application/rss+xml" title="{brand} log" href="/feed.xml">
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link href="https://fonts.googleapis.com/css2?family=JetBrains+Mono:wght@400;500;700&display=swap" rel="stylesheet">
<link rel="stylesheet" href="/style.css">
<script>try{{var t=localStorage.getItem("theme");if(t)document.documentElement.dataset.theme=t}}catch(e){{}}</script>
</head>
<body>
<div class="scan" aria-hidden="true"></div>
<header class="top">
<a class="brand" href="/">{brand}</a>
<nav aria-label="Main">{nav}</nav>
<button id="invert" type="button" title="Switch between dark and light">theme</button>
</header>
<main>
{body}
</main>
<footer class="foot">
<div class="foot-links">{links}</div>
<p>{name} // Built with Rust // Updated {build}</p>
</footer>
<script src="/app.js" defer></script>
</body>
</html>
"#,
        title = esc(&full_title),
        desc = esc(desc),
        base = site.base_url.trim_end_matches('/'),
        brand = esc(&site.brand),
        name = esc(&site.name),
        build = Date::today().dotted(),
    )
}

fn page_head(kicker: &str, title: &str, lead: &str) -> String {
    format!(
        r#"<div class="page-head"><p class="kicker">{}</p><h1 class="glitch" data-text="{t}">{t}</h1><p class="lead">{}</p></div>"#,
        esc(kicker),
        esc(lead),
        t = esc(title),
    )
}

// --------------------------------------------------------------- build ----

fn build() -> Result<()> {
    let site: Site = toml::from_str(&fs::read_to_string("site.toml")?)?;
    let posts = load_posts()?;
    let all: Vec<&Post> = posts.iter().collect();

    if Path::new(OUT).exists() {
        fs::remove_dir_all(OUT)?;
    }
    copy_dir(Path::new("static"), Path::new(OUT))?;

    // Home
    let stats: String = site
        .facts
        .iter()
        .map(|f| format!("<div><dt>{}</dt><dd>{}</dd></div>", esc(&f.label), esc(&f.value)))
        .collect();
    let log_button =
        if posts.is_empty() { "" } else { r#"<a class="btn" href="/log/">Read the log</a>"# };
    let hero = format!(
        r#"<div class="hero"><div class="hero-main"><p class="kicker">{role} // {location}</p><h1 class="glitch" data-text="{name}">{name}</h1><p class="lead">{tagline}</p><p class="actions">{log_button}<a class="btn ghost" href="/about/">About</a></p></div><dl class="stats">{stats}</dl></div>"#,
        role = esc(&site.role),
        location = esc(&site.location),
        name = esc(&site.name),
        tagline = esc(&site.tagline),
    );
    let recent: String = posts.iter().take(5).map(post_row).collect();
    let cards: String =
        site.projects.iter().enumerate().map(|(i, p)| project_card(p, i + 1)).collect();
    let featured: String =
        site.credentials.iter().filter(|c| c.featured).map(credential_row).collect();
    let mut body = panel("Profile", "hero-panel", &hero);
    if !site.intro.is_empty() {
        body += &format!(r#"<div class="prose intro">{}<p><a href="/about/">More about me &#9656;</a></p></div>"#, markdown(&site.intro));
    }
    if !featured.is_empty() {
        body += &format!(
            r#"<div class="section-head"><h2>Credentials</h2></div><ul class="creds">{featured}</ul>"#
        );
    }
    body += &format!(
        r#"<div class="section-head"><h2>Tools and experiments</h2></div><div class="cards">{cards}</div>"#
    );
    if !posts.is_empty() {
        body += &panel(
            "Recent log entries",
            "",
            &format!(r#"<ul class="entries">{recent}</ul><p class="more"><a href="/log/">All log entries &#9656;</a></p>"#),
        );
    }
    write("index.html", &page(&site, "", &site.description, "", "/", &body))?;

    // Log index
    let mut tag_map: BTreeMap<&str, Vec<&Post>> = BTreeMap::new();
    for p in &posts {
        for t in &p.meta.tags {
            tag_map.entry(t).or_default().push(p);
        }
    }
    let topics: String = tag_map
        .iter()
        .map(|(t, ps)| format!(r#"<a class="tag" href="/tags/{0}/">#{0} <small>{1}</small></a>"#, esc(t), ps.len()))
        .collect();
    let listing = if posts.is_empty() {
        r#"<p class="lead">No entries yet.</p>"#.to_string()
    } else {
        format!(r#"<div class="topics"><span class="kicker">Topics</span>{topics}</div>{}"#, post_groups(&all))
    };
    let body = format!(
        "{}{listing}",
        page_head("All entries", "Log", "Notes on what I am studying and building, including what went wrong. Newest first."),
    );
    write("log/index.html", &page(&site, "Log", "Running log of studies and builds.", "log", "/log/", &body))?;

    // Tag pages
    for (tag, ps) in &tag_map {
        let body = format!(
            "{}{}",
            page_head("Topic", &format!("#{tag}"), &format!("{} log entries tagged #{tag}.", ps.len())),
            post_groups(ps),
        );
        let path = format!("/tags/{tag}/");
        write(
            &format!("tags/{tag}/index.html"),
            &page(&site, &format!("#{tag}"), &format!("Log entries tagged {tag}."), "log", &path, &body),
        )?;
    }

    // Posts (index 0 is newest)
    for (i, p) in posts.iter().enumerate() {
        let link = |q: Option<&Post>, label: &str| match q {
            Some(q) => format!(
                r#"<a href="/log/{}/"><small>{label}</small>{}</a>"#,
                q.slug,
                esc(&q.meta.title)
            ),
            None => "<span></span>".to_string(),
        };
        let older = link(posts.get(i + 1), "&#9666; Older");
        let newer = link(i.checked_sub(1).and_then(|j| posts.get(j)), "Newer &#9656;");
        let body = format!(
            r#"<article class="post"><header class="page-head"><p class="kicker">#{no:03} // <time datetime="{iso}">{date}</time> // {min} min read</p><h1 class="glitch" data-text="{title}">{title}</h1><p class="entry-tags">{tags}</p></header><div class="prose">{html}</div></article><nav class="pager" aria-label="Entries">{older}{newer}</nav>"#,
            no = p.number,
            iso = p.date.iso(),
            date = p.date.dotted(),
            min = p.minutes,
            title = esc(&p.meta.title),
            tags = tags_html(&p.meta.tags),
            html = p.html,
        );
        let path = format!("/log/{}/", p.slug);
        write(
            &format!("log/{}/index.html", p.slug),
            &page(&site, &p.meta.title, &p.meta.summary, "log", &path, &body),
        )?;
    }

    // Projects
    let cards: String =
        site.projects.iter().enumerate().map(|(i, p)| project_card(p, i + 1)).collect();
    let body = format!(
        r#"{}<div class="cards">{cards}</div>"#,
        page_head("Work and studies", "Tools and experiments", "Things I have built and studied. Most link to their source code."),
    );
    write("projects/index.html", &page(&site, "Tools and experiments", "Tools and experiments.", "projects", "/projects/", &body))?;

    // About
    let mut body = page_head(&format!("{} // {}", site.role, site.location), "About", &site.tagline);
    body += &format!(r#"<div class="prose">{}</div>"#, markdown(&site.about));
    if !site.experience.is_empty() {
        let jobs: String = site
            .experience
            .iter()
            .map(|j| {
                format!(
                    r#"<li><span class="job-period">{}</span><div><strong>{}</strong><span class="job-org">{}</span><ul class="points">{}</ul></div></li>"#,
                    esc(&j.period),
                    esc(&j.role),
                    esc(&[j.org.as_str(), j.location.as_str()].iter().filter(|x| !x.is_empty()).copied().collect::<Vec<_>>().join(" // ")),
                    j.points.iter().map(|p| format!("<li>{}</li>", esc(p))).collect::<String>()
                )
            })
            .collect();
        body += &panel("Experience", "", &format!(r#"<ul class="jobs">{jobs}</ul>"#));
    }
    if !site.education.is_empty() || !site.credentials.is_empty() {
        let mut items: String = site
            .education
            .iter()
            .map(|e| {
                format!(
                    r#"<li><span class="job-period">{}</span><div><strong>{}</strong><span class="job-org">{}</span></div></li>"#,
                    esc(&e.period),
                    esc(&e.degree),
                    esc(&e.school)
                )
            })
            .collect();
        for c in &site.credentials {
            items += &format!(
                r#"<li><span class="job-period">{}</span><div><strong>{}</strong><span class="job-org">{}</span>{}</div></li>"#,
                esc(&c.date),
                esc(&c.title),
                esc(&c.issuer),
                if c.url.is_empty() { String::new() } else { format!(r#"<a class="cred-view" href="{}" rel="noopener" target="_blank">view credential &#9656;</a>"#, esc(&c.url)) }
            );
        }
        body += &panel("Education", "", &format!(r#"<ul class="jobs">{items}</ul>"#));
    }
    if !site.skills.is_empty() {
        let groups: String = site
            .skills
            .iter()
            .map(|g| {
                let items: String = g.items.iter().map(|s| format!("<li>{}</li>", esc(s))).collect();
                format!(r#"<div class="skill"><dt>{}</dt><dd><ul class="stack">{items}</ul></dd></div>"#, esc(&g.label))
            })
            .collect();
        body += &panel("Skills", "", &format!(r#"<dl class="skills">{groups}</dl>"#));
    }
    let contact: String = site
        .links
        .iter()
        .map(|l| format!(r#"<a class="btn ghost" href="{}" rel="me noopener">{}</a>"#, esc(&l.url), esc(&l.label)))
        .collect();
    body += &panel("Contact", "", &format!(r#"<p class="actions">{contact}</p>"#));
    write("about/index.html", &page(&site, "About", &site.description, "about", "/about/", &body))?;

    // 404
    let body = page_head("404", "Page not found", "This page does not exist.")
        + r#"<p class="actions"><a class="btn" href="/">Back to home</a></p>"#;
    write("404.html", &page(&site, "404", "Not found.", "", "/404.html", &body))?;

    // RSS
    let base = site.base_url.trim_end_matches('/');
    let items: String = posts
        .iter()
        .take(30)
        .map(|p| {
            format!(
                "<item><title>{}</title><link>{base}/log/{slug}/</link><guid>{base}/log/{slug}/</guid><pubDate>{}</pubDate><description>{}</description></item>\n",
                esc(&p.meta.title),
                p.date.rfc822(),
                esc(&p.meta.summary),
                slug = p.slug,
            )
        })
        .collect();
    write(
        "feed.xml",
        &format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\"><channel><title>{} // log</title><link>{base}/</link><description>{}</description>\n{items}</channel></rss>\n",
            esc(&site.brand),
            esc(&site.description),
        ),
    )?;

    println!("built {} entries, {} projects -> {OUT}/", posts.len(), site.projects.len());
    Ok(())
}

// --------------------------------------------------------------- serve ----

fn serve(port: u16) -> io::Result<()> {
    let listener = TcpListener::bind(("127.0.0.1", port))?;
    println!("serving {OUT}/ on http://127.0.0.1:{port}");
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else { continue };
        let mut buf = [0u8; 4096];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]);
        let target = req.split_whitespace().nth(1).unwrap_or("/");
        let rel = target.split(['?', '#']).next().unwrap_or("").trim_start_matches('/');
        let mut file = Path::new(OUT).join(rel);
        if file.is_dir() {
            file = file.join("index.html");
        }
        let safe = !rel.split('/').any(|part| part == "..");
        let (status, body) = match fs::read(&file) {
            Ok(body) if safe => ("200 OK", body),
            _ => {
                file = Path::new(OUT).join("404.html");
                ("404 Not Found", fs::read(&file).unwrap_or_default())
            }
        };
        let mime = match file.extension().and_then(|e| e.to_str()) {
            Some("html") => "text/html; charset=utf-8",
            Some("css") => "text/css; charset=utf-8",
            Some("js") => "text/javascript; charset=utf-8",
            Some("xml") => "application/xml; charset=utf-8",
            Some("svg") => "image/svg+xml",
            Some("png") => "image/png",
            Some("jpg" | "jpeg") => "image/jpeg",
            _ => "application/octet-stream",
        };
        let head = format!(
            "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = stream.write_all(head.as_bytes()).and_then(|_| stream.write_all(&body));
    }
    Ok(())
}

fn main() -> Result<()> {
    build()?;
    let args: Vec<String> = env::args().collect();
    if args.get(1).map(String::as_str) == Some("serve") {
        let port = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(4321);
        serve(port)?;
    }
    Ok(())
}
