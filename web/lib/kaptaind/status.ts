
import fs from "fs/promises";
import path from "path";
import { kaptaindDir } from "./reader";
import type { StatusReport } from "@/types/kaptaind";

export async function readStatus(repoPath: string): Promise<StatusReport> {
  const filePath = path.join(kaptaindDir(repoPath), "status.json");

  try {
    const raw = await fs.readFile(filePath, "utf-8");
    return JSON.parse(raw) as StatusReport;
  } catch {
    return {
      status: "Idle",
      last_version: null,
      last_action_time: new Date().toISOString(),
      last_error: null,
      current_task: null,
      progress_percent: null,
    };
  }
}
