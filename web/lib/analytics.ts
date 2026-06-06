import { prisma } from "@/lib/prisma";

export interface AnalyticsEvent {
  userId?: string;
  orgId?: string;
  eventName: string;
  metadata?: Record<string, unknown>;
  source: "daemon" | "web" | "desktop" | "api";
}

function getMetaString(
  meta: Record<string, unknown> | undefined,
  key: string
): string | null {
  const val = meta?.[key];
  return typeof val === "string" ? val : null;
}

/**
 * Clean path/repo metadata to prevent leaking sensitive directories or repo names
 */
function sanitizeMetadata(metadata?: Record<string, unknown>): Record<string, unknown> {
  if (!metadata) return {};
  const clean = { ...metadata };

  // Remove common keys that leak directory structures or private files
  const sensitiveKeys = [
    "repoPath",
    "path",
    "filePath",
    "absolutePath",
    "commitMessage",
    "authorEmail",
    "authorName",
    "branchName",
    "gitUrl",
    "repository",
  ];

  for (const key of sensitiveKeys) {
    if (key in clean) {
      // Replace with safe representation or hash
      if (typeof clean[key] === "string") {
        clean[key] = `[redacted_hash:${hashString(clean[key])}]`;
      } else {
        delete clean[key];
      }
    }
  }

  return clean;
}

function hashString(str: string): string {
  let hash = 0;
  for (let i = 0; i < str.length; i++) {
    hash = (hash << 5) - hash + str.charCodeAt(i);
    hash |= 0; // Convert to 32bit integer
  }
  return Math.abs(hash).toString(16);
}

/**
 * Track an analytics event server-side, ensuring data compliance
 */
export async function trackEvent(event: AnalyticsEvent): Promise<void> {
  const sanitized = sanitizeMetadata(event.metadata);
  
  // In a real SaaS setup, you would forward this to Amplitude, Mixpanel, or custom Postgres.
  // Here we log to telemetry/stdout securely.
  console.log(JSON.stringify({
    timestamp: new Date().toISOString(),
    event: event.eventName,
    source: event.source,
    userId: event.userId ? `usr_${hashString(event.userId)}` : undefined,
    orgId: event.orgId ? `org_${hashString(event.orgId)}` : undefined,
    properties: sanitized,
  }));

  // Log to AuditLog if it is a governance event
  if (event.eventName.startsWith("governance.") || event.eventName.startsWith("policy.")) {
    try {
      await prisma.auditLog.create({
        data: {
          actor: event.userId || "daemon",
          action: event.eventName,
          resource: getMetaString(event.metadata, "resourceId") || "unknown",
          beforeHash: getMetaString(event.metadata, "beforeHash"),
          afterHash: getMetaString(event.metadata, "afterHash"),
          source: event.source,
          orgId: event.orgId || null,
          requestId: getMetaString(event.metadata, "requestId"),
        },
      });
    } catch (e) {
      console.error("Failed to write to AuditLog database:", e);
    }
  }
}
