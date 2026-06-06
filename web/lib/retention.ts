import fs from "fs/promises";
import path from "path";
import { kaptaindDir } from "@/lib/kaptaind/reader";

export interface PruneResult {
  deleted: number;
  errors: number;
}

/**
 * Prune analysis artifacts for a repo older than the given retention threshold.
 * @param repoPath Absolute path to the repository.
 * @param retentionDays Number of days to retain. 0 deletes everything.
 */
export async function pruneAnalysisArtifacts(
  repoPath: string,
  retentionDays: number
): Promise<PruneResult> {
  const dir = path.join(kaptaindDir(repoPath), "analysis");
  const cutoff = Date.now() - retentionDays * 24 * 60 * 60 * 1000;

  let entries: string[];
  try {
    entries = await fs.readdir(dir);
  } catch {
    return { deleted: 0, errors: 0 };
  }

  const jsonFiles = entries.filter((f) => f.endsWith(".json"));
  let deleted = 0;
  let errors = 0;

  for (const file of jsonFiles) {
    const filePath = path.join(dir, file);
    try {
      const raw = await fs.readFile(filePath, "utf-8");
      const artifact = JSON.parse(raw) as { ended_at?: string };
      const endedAt = artifact.ended_at ? new Date(artifact.ended_at).getTime() : 0;
      if (endedAt <= cutoff) {
        await fs.unlink(filePath);
        deleted++;
      }
    } catch {
      errors++;
    }
  }

  return { deleted, errors };
}
