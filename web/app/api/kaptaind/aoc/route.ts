import { NextResponse } from "next/server";
import {
  requireAuth,
  requireProjectAccess,
  isAuthError,
} from "@/lib/api-auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { readActiveSession, listAocManifests } from "@/lib/kaptaind/aoc";

export async function GET(req: Request) {
  try {
    await requireAuth(req);

    const { searchParams } = new URL(req.url);
    const projectId = searchParams.get("projectId");
    const active = searchParams.get("active") === "true";
    const limit = parseInt(searchParams.get("limit") || "50", 10);

    if (!projectId) {
      return NextResponse.json(
        { error: "projectId required" },
        { status: 400 }
      );
    }

    await requireProjectAccess(req, projectId);

    const repoPath = await resolveRepoPath(projectId);

    if (active) {
      const session = await readActiveSession(repoPath);
      return NextResponse.json(session);
    }

    const manifests = await listAocManifests(repoPath, limit);
    return NextResponse.json(manifests);
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to read AoC data" },
      { status: 500 }
    );
  }
}
