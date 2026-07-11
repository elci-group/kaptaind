-module(private_functions).
-export([]).

%% Nothing is exported: helpers must NOT be public.
helper_a(X) -> X + 1.

helper_b(X, Y) -> X * Y.
