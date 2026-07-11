-module(codegen_string).
-export([]).

%% A string literal holding generated Erlang spans multiple physical lines.
%% The adapter is string-unaware: the inner `-export([generated/0]).` line
%% (no leading quote) pollutes the export set, and the inner
%% `generated() -> ...` head then matches it, emitting a PHANTOM public
%% function generated/0 that does not exist in this module's real API.
spec() ->
    "-module(generated).
-export([generated/0]).
generated() -> ok.".
