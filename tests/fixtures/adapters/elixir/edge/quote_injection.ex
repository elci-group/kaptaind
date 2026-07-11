defmodule MyApp.Helpers do
  defmacro __using__(_opts) do
    quote do
      def injected(value) do
        value
      end
    end
  end
end
