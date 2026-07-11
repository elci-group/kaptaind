@moduledoc false
defmodule MyApp.Internal do
  @doc false
  def hidden_state do
    %{}
  end

  @doc "Public and documented"
  def visible do
    :ok
  end
end
