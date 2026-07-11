// Preprocessor directives and static_assert: none are API symbols.
#include <vector>
#include "widget.hpp"
#define MAX_SIZE 1024
#define SQUARE(x) ((x) * (x))
static_assert(sizeof(int) >= 4, "int too small");
