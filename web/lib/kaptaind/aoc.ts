
import fs from "fs/promises";
import path from "path";
import { kaptaindDir } from "./reader";
import type { AocManifest, AocSession } from "@/types/kaptaind";

export async function readActiveSession(
  repoPath: string
): Promise<AocSession | null> {
  const filePath = path.join(kaptaindDir(repoPath), "aoc", "active.json");

  try {
    const raw = await fs.readFile(filePath, "utf-8");
    return JSON.parse(raw) as AocSession;
  } catch {
    return null;
  }
}

export async function listAocManifests(
  repoPath: string,
  limit = 50
): Promise<AocManifest[]> {
  const dir = path.join(kaptaindDir(repoPath), "aoc");
  let entries: string[];

  try {
    entries = await fs.readdir(dir);
  } catch {
    return [];
  }

  const jsonFiles = entries
    .filter((f) => f.endsWith(".json") && f !== "active.json")
    .slice(0, limit);
  const manifests: AocManifest[] = [];

  for (const file of jsonFiles) {
    try {
      const raw = await fs.readFile(path.join(dir, file), "utf-8");
      manifests.push(JSON.parse(raw) as AocManifest);
    } catch {
      // skip malformed files
    }
  }

  manifests.sort((a, b) => (a.shipped_at < b.shipped_at ? 1 : -1));
  return manifests.slice(0, limit);
}
