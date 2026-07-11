package Dyno;

sub real_thing {
    return 1;
}

# Typeglob alias: defines 'generated_thing' without a 'sub ' line.
# The adapter cannot see it -> dynamic API is missed.
*generated_thing = \&real_thing;

our $AUTOLOAD;

# Literally starts with 'sub ', so AUTOLOAD itself IS emitted.
sub AUTOLOAD {
    return $AUTOLOAD;
}

1;
