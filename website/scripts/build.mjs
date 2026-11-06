// Builds the website into website/dist: puts the shared header and footer into both pages, fills
// the release notes from GitHub Releases and copies the styles and assets. Node 20 or later, no
// dependencies. GITHUB_TOKEN, when set, lifts the API's rate limit.
//
//   node website/scripts/build.mjs

import { cp, mkdir, readFile, rm, writeFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const src = new URL("src/", root);
const dist = new URL("dist/", root);
const repo = "amenophis/arbalete";

const read = (path) => readFile(new URL(path, src), "utf8");

const escape = (text) =>
  String(text).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

const months = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
// "6 Nov 2026"
const day = (iso) => {
  const date = new Date(iso);
  return `${date.getUTCDate()} ${months[date.getUTCMonth()]} ${date.getUTCFullYear()}`;
};

/** Every published release, newest first: drafts skipped, previews kept. */
async function releases() {
  const headers = { Accept: "application/vnd.github.html+json", "User-Agent": "arbalete-website" };
  if (process.env.GITHUB_TOKEN) headers.Authorization = `Bearer ${process.env.GITHUB_TOKEN}`;
  const all = [];
  for (let page = 1; ; page++) {
    const response = await fetch(`https://api.github.com/repos/${repo}/releases?per_page=100&page=${page}`, { headers });
    if (!response.ok) throw new Error(`GitHub Releases: ${response.status} ${await response.text()}`);
    const batch = await response.json();
    all.push(...batch);
    if (batch.length < 100) break;
  }
  return all
    .filter((release) => !release.draft && release.published_at)
    .sort((a, b) => b.published_at.localeCompare(a.published_at));
}

/** The notes as GitHub renders them, with the New, Improved and Fixed headings as colored labels. */
function notes(release) {
  const html = release.body_html?.trim() || "";
  return html.replace(/<h([1-6])([^>]*)>\s*(New|Improved|Fixed)\s*<\/h\1>/gi, (_, level, attributes, title) =>
    `<h${level}${attributes}><span class="tag tag-${title.toLowerCase()}">${title}</span></h${level}>`);
}

const title = (release) => escape(release.name || release.tag_name);
const download = (release) =>
  release.assets?.find((asset) => asset.name.endsWith(".dmg"))?.browser_download_url || release.html_url;

function latest(release) {
  const preview = release.prerelease ? " · Preview" : "";
  return `<article class="latest" aria-labelledby="latest-title">
  <div class="latest-head">
    <div class="latest-meta">
      <div class="latest-line"><span class="badge">Latest</span><span>${day(release.published_at)}${preview}</span></div>
      <h2 id="latest-title">${title(release)}</h2>
    </div>
    <a class="btn btn-primary" href="${escape(download(release))}">
      <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d="M12 3v12"/><path d="M7 10l5 5 5-5"/><path d="M5 21h14"/></svg>
      Download for macOS
    </a>
  </div>
  <div class="notes">${notes(release)}</div>
  <p class="small">Full changelog on <a href="${escape(release.html_url)}">GitHub</a>.</p>
</article>`;
}

function earlier(list) {
  if (!list.length) return "";
  const versions = list
    .map((release) => {
      const preview = release.prerelease ? " · Preview" : "";
      return `<details class="version">
      <summary><span class="when"><span class="name">${title(release)}</span><span class="date">${day(release.published_at)}${preview}</span></span></summary>
      <div class="version-body">
        <div class="notes">${notes(release)}</div>
        <a class="small" href="${escape(release.html_url)}">Download ${escape(release.tag_name)} on GitHub</a>
      </div>
    </details>`;
    })
    .join("\n    ");
  return `<details class="earlier">
  <summary>Earlier versions&nbsp;<span class="count">(${list.length})</span></summary>
  <div class="earlier-list">
    ${versions}
  </div>
</details>`;
}

async function page(template, current, extra = {}) {
  const header = (await read("partials/header.html")).replace(
    "{{current-releases}}", current === "releases" ? ' aria-current="page"' : "");
  const rss = current === "releases"
    ? `<a href="https://github.com/${repo}/releases.atom">RSS</a>\n      `
    : "";
  const footer = (await read("partials/footer.html")).replace("{{rss}}", rss);
  let html = template.replace("<!-- @header -->", header).replace("<!-- @footer -->", footer);
  for (const [marker, value] of Object.entries(extra)) html = html.replace(marker, value);
  return html;
}

const list = await releases();
const releasesHtml = list.length
  ? `${latest(list[0])}\n${earlier(list.slice(1))}`
  : `<p class="empty">No releases yet.</p>`;

await rm(dist, { recursive: true, force: true });
await mkdir(new URL("assets/", dist), { recursive: true });
await writeFile(new URL("index.html", dist), await page(await read("index.html"), "home"));
await writeFile(
  new URL("releases.html", dist),
  await page(await read("releases.template.html"), "releases", { "<!-- @releases -->": releasesHtml }));
await cp(new URL("styles.css", src), new URL("styles.css", dist));
await cp(new URL("assets/", src), new URL("assets/", dist), { recursive: true });
// The logo's files stay in the repository's assets folder, the one place they live.
await cp(new URL("../assets/favicon.svg", root), new URL("assets/favicon.svg", dist));
console.log(`Built website/dist with ${list.length} release(s).`);
