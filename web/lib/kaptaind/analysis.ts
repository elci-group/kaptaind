
import fs from "fs/promises";
import path from "path";
import { kaptaindDir } from "./reader";
import type { AnalysisArtifact } from "@/types/kaptaind";

export async function listAnalysisArtifacts(
  repoPath: string,
  limit = 50
): Promise<AnalysisArtifact[]> {
  const dir = path.join(kaptaindDir(repoPath), "analysis");
  let entries: string[];

  try {
    entries = await fs.readdir(dir);
  } catch {
    return [];
  }

  const jsonFiles = entries.filter((f) => f.endsWith(".json"));
  const artifacts: AnalysisArtifact[] = [];

  for (const file of jsonFiles) {
    try {
      const raw = await fs.readFile(path.join(dir, file), "utf-8");
      artifacts.push(JSON.parse(raw) as AnalysisArtifact);
    } catch {
      // skip malformed files
    }
  }

  artifacts.sort((a, b) => (a.ended_at < b.ended_at ? 1 : -1));
  return artifacts.slice(0, limit);
}

export async function getAnalysisArtifact(
  repoPath: string,
  clusterId: string
): Promise<AnalysisArtifact | null> {
  const filePath = path.join(
    kaptaindDir(repoPath),
    "analysis",
    `${clusterId}.json`
  );

  try {
    const raw = await fs.readFile(filePath, "utf-8");
    return JSON.parse(raw) as AnalysisArtifact;
  } catch {
    return null;
  }
}
