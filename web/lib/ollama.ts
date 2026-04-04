const BASE_URL = process.env.OLLAMA_BASE_URL ?? "http://localhost:11434";
const MODEL = process.env.OLLAMA_MODEL ?? "llama3.2";
const TIMEOUT_MS = parseInt(process.env.OLLAMA_TIMEOUT_MS ?? "15000", 10);

export interface OllamaMessage {
  role: "system" | "user" | "assistant";
  content: string;
}

/**
 * Call Ollama's /api/chat endpoint with a list of messages.
 * Returns the assistant's response content as a string.
 * Throws on timeout, network error, or non-200 response.
 */
export async function ollamaChat(messages: OllamaMessage[]): Promise<string> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), TIMEOUT_MS);

  try {
    const res = await fetch(`${BASE_URL}/api/chat`, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ model: MODEL, stream: false, messages }),
      signal: controller.signal,
    });

    if (!res.ok) {
      throw new Error(`Ollama responded with ${res.status}`);
    }

    const data = await res.json();
    return data?.message?.content ?? "";
  } finally {
    clearTimeout(timer);
  }
}
