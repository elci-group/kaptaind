import { NextResponse } from "next/server";
import {
  requireAuth,
  requireProjectAccess,
  isAuthError,
} from "@/lib/api-auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { readStatus } from "@/lib/kaptaind/status";

export async function GET(req: Request) {
  try {
    await requireAuth(req);

    const { searchParams } = new URL(req.url);
    const projectId = searchParams.get("projectId");
    if (!projectId) {
      return NextResponse.json(
        { error: "projectId required" },
        { status: 400 }
      );
    }

    await requireProjectAccess(req, projectId);

    const repoPath = await resolveRepoPath(projectId);
    const status = await readStatus(repoPath);
    return NextResponse.json(status);
  } catch (error) {
    const authError = isAuthError(error);
    if (authError) {
      return NextResponse.json(
        { error: authError.message },
        { status: authError.status }
      );
    }
    return NextResponse.json(
      { error: "Failed to read status" },
      { status: 500 }
    );
  }
}
