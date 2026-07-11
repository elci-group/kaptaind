defprotocol Size do
  @doc "Returns the size of data"
  def size(data)
end

defmodule Greeter do
  def greet(name) do
    "hi #{name}"
  end
end
