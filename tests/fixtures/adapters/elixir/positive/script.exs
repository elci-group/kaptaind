defmodule Script.Main do
  @moduledoc "Entry point for the script"

  def run(args) do
    Enum.each(args, &IO.puts/1)
  end
end
