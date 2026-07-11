import { NextResponse } from "next/server";
import {
  requireAuth,
  requireProjectAccess,
  requireEntitlement,
  isAuthError,
} from "@/lib/api-auth";
import { queryAuditLogs } from "@/lib/audit";
import { prisma } from "@/lib/prisma";

export async function GET(req: Request) {
  try {
    const session = await requireAuth(req);

    // Audit log reads/export are Enterprise-only.
    await requireEntitlement(req, "canExportAuditLogs");

    const { searchParams } = new URL(req.url);
    const orgId = searchParams.get("orgId") || undefined;
    const projectId = searchParams.get("projectId") || undefined;
    const limit = parseInt(searchParams.get("limit") || "50", 10);
    const offset = parseInt(searchParams.get("offset") || "0", 10);

    if (!orgId && !projectId) {
      return NextResponse.json(
        { error: "orgId or projectId is required" },
        { status: 400 }
      );
    }

    if (projectId) {
      await requireProjectAccess(req, projectId);
    }

    if (orgId && !projectId) {
      const membership = await prisma.organizationMembership.findUnique({
        where: {
          orgId_userId: { orgId, userId: session.user.id },
        },
        select: { id: true },
      });
      if (!membership) {
        return NextResponse.json({ error: "Forbidden" }, { status: 403 });
      }
    }

    const result = await queryAuditLogs({ orgId, projectId, limit, offset });
    return NextResponse.json(result);
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to query audit logs" },
      { status: 500 }
    );
  }
}
