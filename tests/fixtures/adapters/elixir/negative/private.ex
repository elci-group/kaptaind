# Private helpers — must NOT be flagged as public API.
# (Fixture note: kept top-level on purpose so the line scanner sees 0 public
# symbols. defp/defmacrop are excluded because the scanner requires a space
# after `def`/`defmacro`.)
defp secret(key) do
  :crypto.hash(:sha256, key)
end

defmacrop helper(expr) do
  quote do: unquote(expr)
end
