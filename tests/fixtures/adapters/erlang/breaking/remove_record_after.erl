-module(remove_record).
-export([new/0]).

%% -record(state, ...) removed: the adapter treats records as always-public,
%% so losing the record symbol is breaking.
new() -> ok.
