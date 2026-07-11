# Builds a widget instance.
sub Widget {
    my ($class, %args) = @_;
    return bless { %args }, $class;
}

1;
