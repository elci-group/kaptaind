# Kaptaind Pro — SaaS Dashboard

This directory contains the Kaptaind Pro web application: a Next.js-based SaaS dashboard for monitoring and managing kaptaind daemons across projects.

## Overview

Kaptaind Pro provides:

- **Multi-project dashboard**: Monitor versions, scores, and commits across repositories.
- **Team collaboration**: Share version history and analysis artifacts with teammates.
- **Notifications & webhooks**: Real-time alerts for version bumps and test failures.
- **API history**: Browse semantic versioning decisions and diff analysis.
- **Integration**: OAuth-based login via GitHub/GitLab; configurable webhook ingestion.

## Quick Start

```bash
cd web
npm install
npm run dev
```

Open [http://localhost:3000](http://localhost:3000) in your browser.

## Stack

- **Framework**: Next.js (App Router)
- **Styling**: Tailwind CSS
- **Auth**: NextAuth with OAuth providers
- **Database**: Prisma ORM
- **Hosting**: Deploy to Vercel (recommended) or any Node.js host

## Development

- `npm run dev` — Start development server (hot reload)
- `npm run build` — Build for production
- `npm run start` — Run production build
- `npm run test` — Run tests (if configured)

## Environment Setup

Create a `.env.local` file:

```env
NEXTAUTH_URL=http://localhost:3000
NEXTAUTH_SECRET=<random-secret>
DATABASE_URL=postgres://...
GITHUB_ID=<your-oauth-app-id>
GITHUB_SECRET=<your-oauth-secret>
```

## Deployment

Deploy to Vercel:

```bash
vercel deploy
```

Or any Node.js host. See [Next.js deployment docs](https://nextjs.org/docs/deployment).

## Documentation

- **Parent README**: See `/README.md` in repository root for kaptaind daemon documentation.
- **Next.js docs**: [https://nextjs.org/docs](https://nextjs.org/docs)

## Troubleshooting

### "Cannot find module 'next/...'"

Run `npm install && npm run build` to ensure dependencies are up-to-date.

### "Database connection failed"

Verify `DATABASE_URL` in `.env.local` is correct and your database is running.

### "OAuth login fails"

Ensure `GITHUB_ID`, `GITHUB_SECRET`, and `NEXTAUTH_URL` match your OAuth app configuration on GitHub.

