"use client";

import { useState } from "react";
import { Button } from "@/components/ui/button";

interface AiGenerateButtonProps {
  endpoint: string;
  payload: Record<string, string>;
  label: string;
  resultLabel?: string;
}

export function AiGenerateButton({
  endpoint,
  payload,
  label,
  resultLabel = "Result",
}: AiGenerateButtonProps) {
  const [loading, setLoading] = useState(false);
  const [result, setResult] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleGenerate = async () => {
    setLoading(true);
    setError(null);
    setResult(null);

    try {
      const res = await fetch(endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });

      if (!res.ok) {
        const errorData = await res.json().catch(() => ({}));
        throw new Error(errorData.error || `HTTP ${res.status}`);
      }

      const data = await res.json();
      // Determine which key to extract (narrative, reasoning, changelog)
      const resultKey = Object.keys(data)[0];
      setResult(data[resultKey]);
    } catch (err) {
      const message =
        err instanceof Error ? err.message : "Unknown error occurred";
      setError(message);
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      <Button
        onClick={handleGenerate}
        disabled={loading}
        variant="outline"
        className="w-full"
      >
        {loading ? "⏳ Generating..." : `✨ ${label}`}
      </Button>

      {error && (
        <div className="rounded-lg bg-red-50 p-3 text-sm text-red-700">
          <strong>⚠️ Error:</strong> {error}
        </div>
      )}

      {result && (
        <div className="rounded-lg bg-zinc-50 p-3 text-sm text-zinc-700">
          <p className="font-semibold mb-2">{resultLabel}:</p>
          <p className="whitespace-pre-wrap break-words">{result}</p>
        </div>
      )}
    </div>
  );
}
