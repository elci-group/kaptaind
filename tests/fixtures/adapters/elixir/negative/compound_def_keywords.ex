# Compound def* keywords: no space after `def`, so the prefix scanner
# (`def ` / `defmacro ` / `defmodule ` / `defprotocol `) skips them.
# NOTE: defdelegate/defguard ARE real public API in Elixir — the adapter
# misses them (recall gap). Per source rules this file yields 0 symbols.
defstruct name: nil, age: 0
defexception message: "boom"
defdelegate abs(x), to: :erlang
defguard is_positive(x) when is_integer(x) and x > 0
@callback init(args :: term) :: {:ok, state :: term}
@behaviour SomeBehaviour
