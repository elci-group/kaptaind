import { NextResponse } from "next/server";
import {
  requireAuth,
  requireProjectAccess,
  isAuthError,
} from "@/lib/api-auth";
import { prisma } from "@/lib/prisma";
import { requirePolicyPacks } from "@/lib/policy";
import { writeAuditLogEntry } from "@/lib/audit";

export async function GET(req: Request) {
  try {
    await requireAuth(req);

    const { searchParams } = new URL(req.url);
    const projectId = searchParams.get("projectId");
    if (!projectId) {
      return NextResponse.json(
        { error: "projectId is required" },
        { status: 400 }
      );
    }

    await requireProjectAccess(req, projectId);

    const policy = await prisma.policy.findUnique({
      where: { projectId },
    });
    if (!policy) {
      return NextResponse.json({ policy: null }, { status: 404 });
    }
    return NextResponse.json({
      policy: {
        ...policy,
        versionBumpRules: policy.versionBumpRules
          ? JSON.parse(policy.versionBumpRules)
          : null,
        branchProtections: policy.branchProtections
          ? JSON.parse(policy.branchProtections)
          : null,
        minimumTests: policy.minimumTests
          ? JSON.parse(policy.minimumTests)
          : null,
        disallowedFilePatterns: policy.disallowedFilePatterns
          ? JSON.parse(policy.disallowedFilePatterns)
          : null,
        releaseQualificationThresholds: policy.releaseQualificationThresholds
          ? JSON.parse(policy.releaseQualificationThresholds)
          : null,
      },
    });
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to fetch policy" },
      { status: 500 }
    );
  }
}

export async function POST(req: Request) {
  try {
    const session = await requireAuth(req);

    const userId = session.user.id;
    const body = (await req.json()) as {
      projectId: string;
      versionBumpRules?: unknown;
      branchProtections?: unknown;
      minimumTests?: unknown;
      disallowedFilePatterns?: unknown;
      releaseQualificationThresholds?: unknown;
    };

    const { projectId } = body;
    if (!projectId) {
      return NextResponse.json(
        { error: "projectId is required" },
        { status: 400 }
      );
    }

    const project = await requireProjectAccess(req, projectId);

    try {
      await requirePolicyPacks({ userId, orgId: project.orgId || undefined });
    } catch {
      return NextResponse.json(
        { error: "Policy packs are not enabled for this plan" },
        { status: 403 }
      );
    }

    const policy = await prisma.policy.upsert({
      where: { projectId },
      update: {
        versionBumpRules: body.versionBumpRules
          ? JSON.stringify(body.versionBumpRules)
          : undefined,
        branchProtections: body.branchProtections
          ? JSON.stringify(body.branchProtections)
          : undefined,
        minimumTests: body.minimumTests
          ? JSON.stringify(body.minimumTests)
          : undefined,
        disallowedFilePatterns: body.disallowedFilePatterns
          ? JSON.stringify(body.disallowedFilePatterns)
          : undefined,
        releaseQualificationThresholds: body.releaseQualificationThresholds
          ? JSON.stringify(body.releaseQualificationThresholds)
          : undefined,
      },
      create: {
        projectId,
        versionBumpRules: body.versionBumpRules
          ? JSON.stringify(body.versionBumpRules)
          : null,
        branchProtections: body.branchProtections
          ? JSON.stringify(body.branchProtections)
          : null,
        minimumTests: body.minimumTests
          ? JSON.stringify(body.minimumTests)
          : null,
        disallowedFilePatterns: body.disallowedFilePatterns
          ? JSON.stringify(body.disallowedFilePatterns)
          : null,
        releaseQualificationThresholds: body.releaseQualificationThresholds
          ? JSON.stringify(body.releaseQualificationThresholds)
          : null,
      },
    });

    await writeAuditLogEntry({
      actor: session.user.email || userId,
      action: "policy.update",
      resource: `project:${projectId}`,
      source: "api",
      orgId: project.orgId || undefined,
      projectId,
      details: { policyId: policy.id },
    });

    return NextResponse.json({ policy });
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to save policy" },
      { status: 500 }
    );
  }
}

export async function DELETE(req: Request) {
  try {
    const session = await requireAuth(req);

    const userId = session.user.id;
    const { searchParams } = new URL(req.url);
    const projectId = searchParams.get("projectId");
    if (!projectId) {
      return NextResponse.json(
        { error: "projectId is required" },
        { status: 400 }
      );
    }

    const project = await requireProjectAccess(req, projectId);

    try {
      await requirePolicyPacks({ userId, orgId: project.orgId || undefined });
    } catch {
      return NextResponse.json(
        { error: "Policy packs are not enabled for this plan" },
        { status: 403 }
      );
    }

    await prisma.policy.delete({ where: { projectId } });

    await writeAuditLogEntry({
      actor: session.user.email || userId,
      action: "policy.delete",
      resource: `project:${projectId}`,
      source: "api",
      orgId: project.orgId || undefined,
      projectId,
    });

    return NextResponse.json({ ok: true });
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to delete policy" },
      { status: 500 }
    );
  }
}
