import { prisma } from "@/lib/prisma";
import { createHash } from "crypto";

export interface AuditLogEntryInput {
  actor: string;
  action: string;
  resource: string;
  source: "daemon" | "web" | "desktop" | "api";
  requestId?: string;
  orgId?: string;
  projectId?: string;
  details?: Record<string, unknown>;
}

/**
 * Writes an audit log entry with a simple SHA-256 hash chain.
 * Each entry hashes the previous entry's afterHash + current JSON data.
 */
export async function writeAuditLogEntry(input: AuditLogEntryInput) {
  const latest = await prisma.auditLog.findFirst({
    orderBy: { timestamp: "desc" },
    select: { afterHash: true },
  });

  const beforeHash = latest?.afterHash ?? "";
  const dataString = JSON.stringify({
    actor: input.actor,
    action: input.action,
    resource: input.resource,
    source: input.source,
    requestId: input.requestId,
    orgId: input.orgId,
    projectId: input.projectId,
    details: input.details,
    timestamp: new Date().toISOString(),
  });

  const afterHash = createHash("sha256")
    .update(beforeHash + dataString)
    .digest("hex");

  return prisma.auditLog.create({
    data: {
      actor: input.actor,
      action: input.action,
      resource: input.resource,
      source: input.source,
      requestId: input.requestId,
      orgId: input.orgId,
      projectId: input.projectId,
      details: input.details ? JSON.stringify(input.details) : null,
      beforeHash,
      afterHash,
    },
  });
}

/**
 * Query audit logs scoped to an org and/or project.
 */
export async function queryAuditLogs(params: {
  orgId?: string;
  projectId?: string;
  limit?: number;
  offset?: number;
}) {
  const { orgId, projectId, limit = 50, offset = 0 } = params;
  const where: Record<string, unknown> = {};
  if (orgId) where.orgId = orgId;
  if (projectId) where.projectId = projectId;

  const [items, total] = await Promise.all([
    prisma.auditLog.findMany({
      where,
      orderBy: { timestamp: "desc" },
      take: limit,
      skip: offset,
    }),
    prisma.auditLog.count({ where }),
  ]);

  return { items, total };
}
