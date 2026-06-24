# Popovic

Run ordinary static websites from a small Linux machine.

[Quick Start](#quick-start) · [How It Works](#how-it-works) · [Deploy a Site](#deploy-a-site) · [Multiple Sites](#multiple-sites) · [Cloudflare Tunnel](#cloudflare-tunnel) · [Reference](#reference) · [FAQ](#faq)

Popovic is a tiny Rust static-site runner for people who already have HTML
files and just want them online.

It is intentionally boring:

1. Put a folder with `index.html` somewhere.
2. Mount it into Popovic.
3. Popovic copies it into an immutable release.
4. Popovic serves the release over HTTP.
5. Put Cloudflare Tunnel, Caddy, Nginx, a reverse proxy, or your LAN in front of
   it if you want public access.

No Python app server. No Node runtime. No Nginx requirement. No Kubernetes. No
framework build pipeline. JavaScript still works because JavaScript is just a
static file.

## What It Is

Popovic is a single binary that provides two local HTTP surfaces:

- **Static origin** - serves your published static site releases.
- **Dashboard** - shows registered apps, health, deploy history, rollback state,
  local host metrics, and settings.

It supports:

- plain `.html` files
- JavaScript, ES modules, CSS, JSON, source maps, and web manifests
- images, fonts, PDF, audio, video, and WASM
- document-style sites with many HTML files
- browser-routed SPAs with `index.html` fallback
- one site or many sites on the same process
- local folders and Git repositories as app sources
- atomic release publishing
- one-step rollback to the previous release
- low-power x86 Linux hosts
- Cloudflare Tunnel as an optional public ingress path

Popovic does not try to become a CMS. It does not build your frontend. It does
not manage DNS by default. It is the small thing between "I have static files"
and "they are being served reliably."

## How It Works

```text
folder or git repo
        |
        v
Popovic deploy step
        |
        v
immutable release directory
        |
        v
Popovic static origin
        |
        v
browser / reverse proxy / Cloudflare Tunnel
```

When an app is deployed, Popovic copies the allowed static files into a release
directory under `POPOVIC_HOME`. The running origin serves only the current
release. A later deploy creates a new release and switches the app pointer. The
previous release remains available for rollback.

This means the source folder can change, disappear, or be rebuilt while Popovic
keeps serving the last good release.

## Core Primitives

Popovic uses a small model.

### Static App

A deployable static website.

An app has:

- a name
- a source path or Git URL
- an optional source subdirectory
- zero or more hostnames
- a current release
- a previous release
- health and deploy status

If an app has hostnames, Popovic selects it by the incoming `Host` header. If an
app has no hostnames, it can act as the fallback app.

### Source

Where Popovic reads static files from.

Sources can be:

- a local directory such as `/site`
- one child directory under `/sites`
- a Git repository URL registered through the dashboard/API

Every source root must contain `index.html`.

### Release

An immutable copy of a source at deploy time.

Releases live under:

```text
${POPOVIC_HOME}/releases/<app-name>/<timestamp>/
```

### Static Origin

The HTTP server that browsers and reverse proxies talk to.

Default local development address:

```text
http://127.0.0.1:7627
```

Docker default inside the container:

```text
http://popovic:80
```

### Dashboard

A small local management UI and JSON API.

Default:

```text
http://127.0.0.1:7626
```

## Why Popovic?

Static hosting is easy until it becomes slightly annoying.

You can run `python3 -m http.server`, but it has no release model, no rollback,
no host routing, and no dashboard. You can run Nginx, but then you are editing
server blocks and wiring deploy scripts. You can use a full platform, but that
is a lot of machinery for a folder of HTML.

Popovic is the middle path:

- **Plain files in, plain HTTP out** - HTML, JS, CSS, assets.
- **One binary** - easy to run on small machines.
- **No runtime build chain** - if the files already exist, Popovic can serve
  them.
- **Release-oriented** - each deploy creates a stable copy.
- **Host-aware** - multiple sites can share one process.
- **Proxy-friendly** - works well behind Cloudflare Tunnel, Caddy, Nginx, or any
  HTTP reverse proxy.
- **Local-first** - state is stored in a directory you control.

## Quick Start

Run the included example site:

```bash
git clone https://github.com/studyreadbook4ever/popovic.git
cd popovic
docker compose -f compose.example.yml up --build
```

Open the static site:

```text
http://127.0.0.1:7627
```

Open the dashboard:

```text
http://127.0.0.1:7626
```

Check health:

```bash
curl http://127.0.0.1:7627/healthz
```

Expected:

```text
ok
```

## Deploy a Site

Assume this is your site:

```text
my-site/
  index.html
  main.js
  styles.css
  images/
    logo.svg
```

Use this compose file:

```yaml
services:
  popovic:
    build: .
    restart: unless-stopped
    environment:
      POPOVIC_HOME: /data
      POPOVIC_DASHBOARD_ADDR: 0.0.0.0:7626
      POPOVIC_STATIC_ADDR: 0.0.0.0:80
      POPOVIC_BOOTSTRAP: "1"
      POPOVIC_BOOTSTRAP_REDEPLOY: "1"
    volumes:
      - popovic_data:/data
      - ./my-site:/site:ro
    ports:
      - "127.0.0.1:7626:7626"
      - "8080:80"

volumes:
  popovic_data:
```

Start it:

```bash
docker compose up --build
```

Open:

```text
http://127.0.0.1:8080
```

The `/site` mount is the simplest path. If `/site/index.html` exists, Popovic
auto-registers it as an app named `site`.

## Multiple Sites

Mount a directory of sites into `/sites`.

```text
sites/
  docs/
    index.html
    .popovic-hosts
  blog/
    index.html
    .popovic-hosts
  landing/
    index.html
```

Compose:

```yaml
services:
  popovic:
    build: .
    environment:
      POPOVIC_HOME: /data
      POPOVIC_STATIC_ADDR: 0.0.0.0:80
      POPOVIC_DASHBOARD_ADDR: 0.0.0.0:7626
      POPOVIC_BOOTSTRAP: "1"
    volumes:
      - popovic_data:/data
      - ./sites:/sites:ro
    ports:
      - "127.0.0.1:7626:7626"
      - "8080:80"

volumes:
  popovic_data:
```

Each direct child of `/sites` that contains `index.html` becomes one app.

Hostnames can be declared inside the site folder:

```text
# sites/docs/.popovic-hosts
docs.example.com
www.docs.example.com
```

```text
# sites/blog/.popovic-hosts
blog.example.com
```

Test host routing locally:

```bash
curl -H 'Host: docs.example.com' http://127.0.0.1:8080/
curl -H 'Host: blog.example.com' http://127.0.0.1:8080/
```

If an app has no hostname, it is used as the fallback app when no host-specific
app matches.

## Hostname Configuration

You can configure hostnames in three ways.

### Single Site Env

```env
POPOVIC_SITE_HOSTS=example.com,www.example.com
```

### Multi Site Env

For `/sites/docs`, use:

```env
POPOVIC_HOSTS_DOCS=docs.example.com,www.docs.example.com
```

For `/sites/my-blog`, use:

```env
POPOVIC_HOSTS_MY_BLOG=blog.example.com
```

Folder names are converted to uppercase env keys. Non-alphanumeric characters
become `_`.

### `.popovic-hosts`

Inside a site folder:

```text
example.com
www.example.com
```

Both comma-separated and newline-separated values are accepted.

Environment variables take priority over `.popovic-hosts`.

## Routing Rules

For a request path, Popovic resolves files in this order:

1. exact file path
2. directory `index.html`
3. sibling `.html`
4. root `index.html` fallback

Examples:

| Request | Tried Paths |
|---|---|
| `/` | `index.html` |
| `/about` | `about`, `about/index.html`, `about.html`, `index.html` |
| `/docs/intro` | `docs/intro`, `docs/intro/index.html`, `docs/intro.html`, `index.html` |
| `/main.js` | `main.js` |

The final `index.html` fallback makes browser-routed SPAs work.

## Supported Files

Popovic copies and serves common static assets:

```text
html htm css js mjs json map csv tsv md markdown
png jpg jpeg avif gif webp svg ico
woff woff2 ttf otf
txt xml webmanifest pdf mp3 mp4 wasm
robots.txt agents.txt sitemap.xml favicon.ico
```

Unknown extensions are not published during deploy. This keeps release
directories from accidentally containing source databases, secrets, editor
state, or build cache files.

## Cache Behavior

Popovic sets conservative cache headers:

- HTML, text, XML, JSON, and web manifests: `no-cache`
- assets such as JS, CSS, images, fonts, WASM: long-lived immutable cache

Because each deploy creates a new release directory, immutable caching is safe
when your generated files use content hashes. If your assets do not use content
hashes, browsers may cache them until the URL changes.

## Cloudflare Tunnel

Popovic works well behind Cloudflare Tunnel.

Typical container-to-container origin:

```text
http://popovic:80
```

Example:

```yaml
services:
  popovic:
    build: .
    networks: [web]
    volumes:
      - popovic_data:/data
      - ./site:/site:ro

  cloudflared:
    image: cloudflare/cloudflared:latest
    command: tunnel --no-autoupdate run --token ${TUNNEL_TOKEN}
    depends_on:
      - popovic
    networks: [web]

networks:
  web:

volumes:
  popovic_data:
```

In Cloudflare Zero Trust, set the Public Hostname service URL to:

```text
http://popovic:80
```

Cloudflare handles public TLS. Popovic remains a local HTTP origin.

## Running Without Docker

Install Rust and run:

```bash
cargo run
```

Serve a specific local folder:

```bash
POPOVIC_SITE_ROOT=/path/to/site cargo run
```

Use custom ports:

```bash
POPOVIC_DASHBOARD_ADDR=127.0.0.1:9000 \
POPOVIC_STATIC_ADDR=127.0.0.1:9001 \
cargo run
```

## State Directory

By default, Popovic stores state under:

```text
~/.local/share/popovic
```

Set a server path with:

```env
POPOVIC_HOME=/data
```

Popovic creates:

```text
repos/       cloned Git repositories
releases/    immutable release copies
staging/     reserved workspace for proposed edits
logs/        reserved local logs
popovic.json app state, settings, metrics, alerts, tasks
```

Back up `POPOVIC_HOME` if you care about release history and dashboard state.

## Dashboard API

The dashboard UI uses a small JSON API.

### Status

```bash
curl http://127.0.0.1:7626/api/status
```

Returns running apps, tunnel status, host metrics, alerts, and recent RED
metrics.

### Register an App

```bash
curl -X POST http://127.0.0.1:7626/api/apps \
  -H 'Content-Type: application/x-www-form-urlencoded' \
  --data-urlencode 'name=docs' \
  --data-urlencode 'repo_url=/path/to/docs-site' \
  --data-urlencode 'repo_subdir=' \
  --data-urlencode 'hostnames=docs.example.com,www.docs.example.com'
```

`repo_url` can be a local path or a Git repository URL.

### Deploy

```bash
curl -X POST http://127.0.0.1:7626/api/apps/<app-id>/deploy
```

### Rollback

```bash
curl -X POST http://127.0.0.1:7626/api/apps/<app-id>/rollback
```

## Reference

### Environment Variables

| Variable | Default | Meaning |
|---|---|---|
| `POPOVIC_HOME` | `~/.local/share/popovic` | State, releases, cloned repos |
| `POPOVIC_DASHBOARD_ADDR` | `127.0.0.1:7626` | Dashboard listen address |
| `POPOVIC_STATIC_ADDR` | `127.0.0.1:7627` | Static origin listen address |
| `POPOVIC_BOOTSTRAP` | `1` | Auto-register mounted `/site` and `/sites` apps |
| `POPOVIC_BOOTSTRAP_REDEPLOY` | `1` | Redeploy mounted apps on restart |
| `POPOVIC_SITE_ROOT` | `/site` | Single-site source root |
| `POPOVIC_SITE_NAME` | `site` | Single-site app name |
| `POPOVIC_SITE_HOSTS` | empty | Comma/newline hostnames for `/site` |
| `POPOVIC_SITES_ROOT` | `/sites` | Multi-site source root |
| `POPOVIC_HOSTS_<NAME>` | empty | Hostnames for `/sites/<name>` |

### Ports

| Surface | Container Default | Local Dev Default |
|---|---:|---:|
| Dashboard | `7626` | `127.0.0.1:7626` |
| Static origin | `80` in Dockerfile | `127.0.0.1:7627` |

The binary default remains `127.0.0.1:7627` for static HTTP. The Dockerfile
sets `POPOVIC_STATIC_ADDR=0.0.0.0:80`.

### Health

```bash
curl http://127.0.0.1:7627/healthz
```

Expected:

```text
ok
```

## Patterns

### A Single Hand-Written HTML Site

```text
site/
  index.html
  contact.html
  styles.css
```

Mount it to `/site`.

### A Static Export From a Framework

Build the site first:

```bash
npm run build
```

Mount the static output directory:

```yaml
volumes:
  - ./dist:/site:ro
```

Popovic does not run `npm`. It serves the files produced by your build.

### A Browser-Routed SPA

```text
dist/
  index.html
  assets/
    app.abc123.js
```

Requests like `/settings/profile` fall back to `index.html`.

### Several Small Sites On One Box

```text
sites/
  homepage/
  docs/
  wiki/
```

Mount `sites/` to `/sites` and give each folder `.popovic-hosts`.

## Security Notes

Popovic is a static file server and local dashboard. Treat it like local
infrastructure.

Recommendations:

- Bind the dashboard to `127.0.0.1` unless you explicitly need remote access.
- Put public TLS and access control in a reverse proxy or Cloudflare.
- Do not mount folders containing secrets.
- Keep `.env`, private keys, databases, and build caches outside your static
  source root.
- Review the supported extension allowlist before publishing unusual assets.
- Back up `POPOVIC_HOME` if release history matters.

## Troubleshooting

### I get 404 for every request.

Check that an app is registered:

```bash
curl http://127.0.0.1:7626/api/status
```

If `running_apps` is empty, Popovic did not find `/site/index.html` or any
`/sites/<name>/index.html`.

### My Host-based site does not match.

Send the same Host header your proxy sends:

```bash
curl -H 'Host: docs.example.com' http://127.0.0.1:7627/
```

Check `.popovic-hosts` or `POPOVIC_HOSTS_<NAME>`.

### My JavaScript loads as the wrong type.

Make sure the file extension is `.js` or `.mjs`.

```bash
curl -I http://127.0.0.1:7627/main.js
```

Expected:

```text
content-type: text/javascript; charset=utf-8
```

### My SPA route returns the home page.

That is expected. Unknown paths fall back to `index.html` so browser routers can
handle them.

### My file is missing from the release.

The extension may not be in the publish allowlist. See
[Supported Files](#supported-files).

## Development

```bash
cargo fmt
cargo check
cargo run
```

Run with the example site:

```bash
POPOVIC_SITE_ROOT=examples/basic-site cargo run
```

Check the example compose file:

```bash
docker compose -f compose.example.yml config --quiet
```

## FAQ

### Is Popovic a replacement for GitHub Pages, Netlify, or Vercel?

No. It is for running static sites from your own machine or server. It is useful
when you want local control, Cloudflare Tunnel, simple rollback, and no hosted
platform workflow.

### Does Popovic run JavaScript?

Browsers run JavaScript. Popovic serves JavaScript files with the correct MIME
type.

### Does Popovic build my frontend?

No. Build your site before mounting it. Popovic serves the output.

### Does Popovic require Nginx?

No. Popovic is its own static HTTP origin. You can still put Nginx, Caddy, or
Cloudflare Tunnel in front of it.

### Can I run many domains?

Yes. Use `/sites`, `.popovic-hosts`, and Host-header routing.

### Can I use private Git repositories?

The dashboard has settings for GitHub tokens and can clone Git URLs. For the
simplest deployment, mount local folders instead.

## License

[Unlicense](LICENSE). Use it, fork it, rename it, strip it down.
