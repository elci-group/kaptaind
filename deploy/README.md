# Kaptaind Deployment Package

This directory contains the production build artifacts for both the Kaptaind web frontend and the Rust daemon.

## Web Frontend (`deploy/web/`)

Built from `web/` as a **static HTML export** using Next.js 16 App Router.

- **Contents**: Static HTML, CSS, JS, and assets (including prerendered `/whitepapers/[slug]` pages).
- **Excluded routes**: Dynamic dashboard pages (`/dashboard/*`) and API routes (`/api/*`) are not included because they require a running Node.js server. Deploy them separately if needed.
- **Deployment**: Serve `deploy/web/` with any static file server, CDN, or object storage (e.g., Nginx, Vercel, Netlify, AWS S3).

Example with Nginx:
```nginx
server {
    listen 80;
    root /var/www/kaptaind/web;
    index index.html;
    location / {
        try_files $uri $uri/ $uri.html =404;
    }
}
```

## Daemon (`deploy/daemon/`)

Built from the repository root with `cargo build --release`.

- **Binaries**:
  - `kaptaind` — main daemon binary (20 MB)
  - `kaptaind-cli` — CLI companion binary (8.6 MB)
- **Deployment**: Copy the binaries to a location in `$PATH` (e.g., `/usr/local/bin/`) and run `kaptaind`. The daemon expects a `kaptaind.toml` config file in its working directory.

Example systemd service:
```ini
[Unit]
Description=Kaptaind Daemon
After=network.target

[Service]
ExecStart=/usr/local/bin/kaptaind
WorkingDirectory=/opt/kaptaind
Restart=always

[Install]
WantedBy=multi-user.target
```

## Build Notes

- `web/next.config.ts` was updated with `output: 'export'` and `distDir: 'dist'`.
- `web/app/whitepapers/[slug]/page.tsx` was updated with `generateStaticParams()` to enable static generation of whitepaper pages.
- API routes and dashboard pages were temporarily excluded from the build because they rely on server-side runtime features (auth, cookies, APIs).
