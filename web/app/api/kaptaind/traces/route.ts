import { NextResponse } from "next/server";
import {
  requireAuth,
  requireProjectAccess,
  isAuthError,
} from "@/lib/api-auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { readTracesForAoc } from "@/lib/kaptaind/traces";

export async function GET(req: Request) {
  try {
    await requireAuth(req);

    const { searchParams } = new URL(req.url);
    const projectId = searchParams.get("projectId");
    const aocId = searchParams.get("aocId");
    const limit = parseInt(searchParams.get("limit") || "100", 10);

    if (!projectId || !aocId) {
      return NextResponse.json(
        { error: "projectId and aocId required" },
        { status: 400 }
      );
    }

    await requireProjectAccess(req, projectId);

    const repoPath = await resolveRepoPath(projectId);
    const traces = await readTracesForAoc(repoPath, aocId, limit);
    return NextResponse.json(traces);
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to read traces" },
      { status: 500 }
    );
  }
}
