# package Fake::FromComment;
# sub not_real { return 0; }
# use constant BOGUS => 42;
#
# All of the keyword-bearing lines above start with '#', so the
# adapter's prefix checks ("package "/"sub "/"use constant ") miss them.
my $x = 1;  # sub also_not_real stays inside a trailing comment
print $x, "\n";
