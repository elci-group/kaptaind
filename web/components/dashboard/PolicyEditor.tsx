"use client";

import { useState } from "react";
import Button from "@/components/ui/Button";
import Spinner from "@/components/ui/Spinner";

interface PolicyData {
  versionBumpRules?: unknown;
  branchProtections?: unknown;
  minimumTests?: unknown;
  disallowedFilePatterns?: unknown;
  releaseQualificationThresholds?: unknown;
}

export default function PolicyEditor({
  projectId,
  initialPolicy,
}: {
  projectId: string;
  initialPolicy: PolicyData | null;
}) {
  const [policy, setPolicy] = useState<PolicyData>(
    initialPolicy || {
      versionBumpRules: null,
      branchProtections: null,
      minimumTests: null,
      disallowedFilePatterns: null,
      releaseQualificationThresholds: null,
    }
  );
  const [saving, setSaving] = useState(false);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [parseErrors, setParseErrors] = useState<Record<string, string>>({});

  const fields: { key: keyof PolicyData; label: string }[] = [
    { key: "versionBumpRules", label: "Version Bump Rules" },
    { key: "branchProtections", label: "Branch Protections" },
    { key: "minimumTests", label: "Minimum Tests" },
    { key: "disallowedFilePatterns", label: "Disallowed File Patterns" },
    { key: "releaseQualificationThresholds", label: "Release Qualification Thresholds" },
  ];

  function handleChange(key: keyof PolicyData, value: string) {
    try {
      const parsed = value.trim() === "" ? null : JSON.parse(value);
      setPolicy((prev) => ({ ...prev, [key]: parsed }));
      setParseErrors((prev) => ({ ...prev, [key]: "" }));
    } catch {
      setParseErrors((prev) => ({ ...prev, [key]: "Invalid JSON" }));
      // Do NOT update policy — keep previous valid value
    }
  }

  function isValidPolicy(p: PolicyData): boolean {
    return p !== null && typeof p === "object";
  }

  async function handleSave() {
    if (!isValidPolicy(policy)) {
      setError("Policy must be a valid object.");
      return;
    }

    setSaving(true);
    setMessage("");
    setError("");
    try {
      const res = await fetch("/api/policy", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ projectId, ...policy }),
      });
      if (!res.ok) {
        const data = await res.json();
        throw new Error(data.error || "Failed to save policy");
      }
      setMessage("Policy saved successfully.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete() {
    if (!confirm("Delete this policy?")) return;
    setSaving(true);
    setMessage("");
    setError("");
    try {
      const res = await fetch(`/api/policy?projectId=${projectId}`, {
        method: "DELETE",
      });
      if (!res.ok) {
        const data = await res.json();
        throw new Error(data.error || "Failed to delete policy");
      }
      setPolicy({
        versionBumpRules: null,
        branchProtections: null,
        minimumTests: null,
        disallowedFilePatterns: null,
        releaseQualificationThresholds: null,
      });
      setMessage("Policy deleted.");
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setSaving(false);
    }
  }

  return (
    <div className="space-y-4">
      {fields.map(({ key, label }) => (
        <div key={key}>
          <label className="mb-1 block text-sm font-medium text-zinc-700 dark:text-zinc-300">
            {label}
          </label>
          <textarea
            className="w-full rounded-lg border border-zinc-300 bg-white px-3 py-2 text-sm text-zinc-900 shadow-sm focus:border-violet-500 focus:outline-none focus:ring-1 focus:ring-violet-500 dark:border-zinc-700 dark:bg-zinc-800 dark:text-zinc-100"
            rows={4}
            defaultValue={
              policy[key] ? JSON.stringify(policy[key], null, 2) : ""
            }
            onChange={(e) => handleChange(key, e.target.value)}
            placeholder={`Enter JSON for ${label}`}
          />
          {parseErrors[key] && (
            <p className="mt-1 text-sm text-red-600 dark:text-red-400">
              {parseErrors[key]}
            </p>
          )}
        </div>
      ))}

      <div className="flex items-center gap-3 pt-2">
        <Button onClick={handleSave} disabled={saving}>
          {saving ? <Spinner className="h-4 w-4" /> : "Save Policy"}
        </Button>
        <Button variant="danger" onClick={handleDelete} disabled={saving}>
          Delete Policy
        </Button>
      </div>

      {message && (
        <p className="text-sm text-emerald-600 dark:text-emerald-400">
          {message}
        </p>
      )}
      {error && (
        <p className="text-sm text-red-600 dark:text-red-400">{error}</p>
      )}
    </div>
  );
}
