// Stub: @anthropic-ai/sdk is not installed in this workspace.
// The application uses direct fetch calls in lib/inference.ts instead.
export const anthropic = {
  messages: {
    create: async () => {
      throw new Error("Anthropic SDK is not available. Use lib/inference.ts instead.");
    },
  },
};
