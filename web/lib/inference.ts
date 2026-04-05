export type InferenceMessage = {
  role: "system" | "user" | "assistant";
  content: string;
};

const ANTHROPIC_API_KEY = process.env.ANTHROPIC_API_KEY;
const ANTHROPIC_MODEL = process.env.ANTHROPIC_MODEL ?? "claude-haiku-4-5-20251001";

const OPENAI_API_KEY = process.env.OPENAI_API_KEY;
const OPENAI_MODEL = process.env.OPENAI_MODEL ?? "gpt-4o-mini";

const OLLAMA_BASE_URL = process.env.OLLAMA_BASE_URL ?? "http://localhost:11434";
const OLLAMA_MODEL = process.env.OLLAMA_MODEL ?? "llama3.2";

/** Returns the active provider based on env vars. */
function resolveProvider(): "anthropic" | "openai" | "ollama" {
  if (ANTHROPIC_API_KEY) return "anthropic";
  if (OPENAI_API_KEY) return "openai";
  return "ollama";
}

/** Unified inference call — routes to the appropriate provider. */
export async function inferenceChat(messages: InferenceMessage[]): Promise<string> {
  const provider = resolveProvider();

  switch (provider) {
    case "anthropic":
      return callAnthropic(messages);
    case "openai":
      return callOpenAI(messages);
    default:
      return callOllama(messages);
  }
}

/** Call Anthropic Messages API */
async function callAnthropic(messages: InferenceMessage[]): Promise<string> {
  if (!ANTHROPIC_API_KEY) {
    throw new Error("ANTHROPIC_API_KEY not set");
  }

  const response = await fetch("https://api.anthropic.com/v1/messages", {
    method: "POST",
    headers: {
      "x-api-key": ANTHROPIC_API_KEY,
      "anthropic-version": "2023-06-01",
      "content-type": "application/json",
    },
    body: JSON.stringify({
      model: ANTHROPIC_MODEL,
      max_tokens: 100,
      system: messages[0].content,
      messages: messages.slice(1),
    }),
  });

  if (!response.ok) {
    throw new Error(`Anthropic API error: ${response.status}`);
  }

  const data = await response.json();
  if (!data.content || !data.content[0]) {
    throw new Error("Anthropic returned no content");
  }

  return data.content[0].text;
}

/** Call OpenAI Chat Completions API */
async function callOpenAI(messages: InferenceMessage[]): Promise<string> {
  if (!OPENAI_API_KEY) {
    throw new Error("OPENAI_API_KEY not set");
  }

  const response = await fetch("https://api.openai.com/v1/chat/completions", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${OPENAI_API_KEY}`,
      "content-type": "application/json",
    },
    body: JSON.stringify({
      model: OPENAI_MODEL,
      max_tokens: 100,
      messages,
    }),
  });

  if (!response.ok) {
    throw new Error(`OpenAI API error: ${response.status}`);
  }

  const data = await response.json();
  if (!data.choices || !data.choices[0]) {
    throw new Error("OpenAI returned no choices");
  }

  return data.choices[0].message.content;
}

/** Call Ollama /api/chat endpoint */
async function callOllama(messages: InferenceMessage[]): Promise<string> {
  const response = await fetch(`${OLLAMA_BASE_URL}/api/chat`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
    },
    body: JSON.stringify({
      model: OLLAMA_MODEL,
      stream: false,
      messages,
    }),
  });

  if (!response.ok) {
    throw new Error(`Ollama API error: ${response.status}`);
  }

  const data = await response.json();
  return data?.message?.content ?? "";
}
