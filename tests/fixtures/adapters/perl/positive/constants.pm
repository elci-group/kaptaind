package Config::Limits;

use constant MAX_RETRIES => 3;
use constant TIMEOUT_SEC => 30;
use constant DEFAULT_HOST => 'localhost';

sub limit_for {
    my ($key) = @_;
    return $key;
}

1;
