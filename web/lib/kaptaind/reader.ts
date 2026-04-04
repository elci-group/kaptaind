
import path from "path";
import { prisma } from "@/lib/prisma";

export async function resolveRepoPath(projectId: string): Promise<string> {
  const project = await prisma.project.findUniqueOrThrow({
    where: { id: projectId },
  });

  const p = project.repoPath;
  if (!path.isAbsolute(p)) {
    throw new Error(`repoPath must be absolute, got: ${p}`);
  }

  return p;
}

export function kaptaindDir(repoPath: string): string {
  return path.join(repoPath, ".kaptaind");
}
