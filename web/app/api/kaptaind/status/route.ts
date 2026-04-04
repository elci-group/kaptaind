import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { resolveRepoPath } from "@/lib/kaptaind/reader";
import { readStatus } from "@/lib/kaptaind/status";

export async function GET(req: Request) {
  const session = await getServerSession(authOptions);
  if (!session?.user) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const { searchParams } = new URL(req.url);
  const projectId = searchParams.get("projectId");
  if (!projectId) {
    return NextResponse.json(
      { error: "projectId required" },
      { status: 400 }
    );
  }

  try {
    const repoPath = await resolveRepoPath(projectId);
    const status = await readStatus(repoPath);
    return NextResponse.json(status);
  } catch (error) {
    return NextResponse.json(
      { error: "Failed to read status" },
      { status: 500 }
    );
  }
}
