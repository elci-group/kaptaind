-module(change_arity).
-export([connect/2]).

%% Arity changed 1 -> 2: the old symbol connect/1 disappears.
connect(Host, Port) ->
    {Host, Port}.
