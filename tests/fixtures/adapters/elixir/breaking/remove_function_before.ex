defmodule MyApp.Api do
  def hello(name) do
    "Hello, #{name}"
  end

  def deprecated do
    :ok
  end
end
