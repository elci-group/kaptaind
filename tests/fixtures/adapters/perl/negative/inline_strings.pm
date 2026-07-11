use strict;
use warnings;

my $pkg = "package Fake::Pkg";
my $fn  = 'sub not_a_real_sub';
my $c   = "use constant FOO => 1";

# The keywords live inside string values; the trimmed lines start with
# 'my'/'print', so the anchored prefix match does not fire.
print $pkg, $fn, $c, "\n";
