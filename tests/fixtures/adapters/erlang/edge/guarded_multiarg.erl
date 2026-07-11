-module(guarded_multiarg).
-export([max/2]).

%% Multi-argument clauses whose every head carries a guard: the head text
%% "(A, B) when A >= B" does not end with ')', so the adapter skips it.
max(A, B) when A >= B -> A;
max(A, B) when B > A -> B.
