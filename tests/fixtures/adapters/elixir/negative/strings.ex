msg = "def not_a_function(x) do"
template = ~S(defmodule NotReal do)
pattern = ~r/defmacro\s+\w+/
code = "defmodule Ghost do\n  def run do\n  end\nend"
heredoc_first = "def nope do"
