-ifndef(MY_HEADER_HRL).
-define(MY_HEADER_HRL, true).

%% Public type shared via header.
-record(user, {id :: integer(), name :: string()}).

%% Public macros shared via header.
-define(APP_NAME, my_app).
-define(TIMEOUT_MS, 5000).

-endif.
