defmodule MyApp.Math do
  def add(a, b), do: a + b

  def hello(name) do
    "Hello, #{name}"
  end

  def parse(x) when is_binary(x), do: String.trim(x)
end
