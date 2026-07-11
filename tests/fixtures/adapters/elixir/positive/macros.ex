defmodule MyApp.Macros do
  defmacro unless(expr, do: block) do
    quote do
      if !unquote(expr), do: unquote(block)
    end
  end

  defmacro dbg_ast(ast) do
    ast
  end
end
