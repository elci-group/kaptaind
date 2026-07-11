-module(remove_record).
-export([new/0]).

-record(state, {count :: integer()}).

new() -> #state{count = 0}.
