// Fail-fast environment validation for production boots.
// Dependency-free by design so it can run before anything else.

const REQUIRED_IN_PRODUCTION = [
  "NEXTAUTH_SECRET",
  "NEXTAUTH_URL",
  "DATABASE_URL",
] as const;

/**
 * In production, throws if any required environment variable is missing.
 * If STRIPE_SECRET_KEY is configured, STRIPE_WEBHOOK_SECRET is also required
 * so webhook signature verification cannot silently degrade.
 */
export function assertEnv(): void {
  if (process.env.NODE_ENV !== "production") return;

  const missing: string[] = [];

  for (const key of REQUIRED_IN_PRODUCTION) {
    if (!process.env[key]) missing.push(key);
  }

  if (process.env.STRIPE_SECRET_KEY && !process.env.STRIPE_WEBHOOK_SECRET) {
    missing.push("STRIPE_WEBHOOK_SECRET");
  }

  if (missing.length > 0) {
    throw new Error(
      `Missing required environment variable(s): ${missing.join(", ")}`
    );
  }
}

/** Parsed environment values (empty string when unset). */
export const env = {
  NEXTAUTH_SECRET: process.env.NEXTAUTH_SECRET ?? "",
  NEXTAUTH_URL: process.env.NEXTAUTH_URL ?? "",
  DATABASE_URL: process.env.DATABASE_URL ?? "",
  STRIPE_SECRET_KEY: process.env.STRIPE_SECRET_KEY ?? "",
  STRIPE_WEBHOOK_SECRET: process.env.STRIPE_WEBHOOK_SECRET ?? "",
} as const;
