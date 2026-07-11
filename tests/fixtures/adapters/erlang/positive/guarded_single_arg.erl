-module(guarded_single_arg).
-export([classify/1]).

%% Single-argument guarded clause: parsed fine by the adapter
%% because the head still ends with ')'.
classify(Reason) when is_atom(Reason) -> atom;
classify(Reason) when is_integer(Reason) -> integer;
classify(_Other) -> unknown.
