<!-- BEGIN:nextjs-agent-rules -->
# This is NOT the Next.js you know

This version has breaking changes — APIs, conventions, and file structure may all differ from your training data. Read the relevant guide in `node_modules/next/dist/docs/` before writing any code. Heed deprecation notices.
<!-- END:nextjs-agent-rules -->

## TempCheq

- Submit every artifact containing LLM inference calls to tempcheq before delivery: run `tempcheq <path>` on the workspace that produced the artifact, review the temperature audit, and include the findings with the artifact (`tempcheq --report` for JSON output, `tempcheq report <path>` for Markdown/HTML report files).
