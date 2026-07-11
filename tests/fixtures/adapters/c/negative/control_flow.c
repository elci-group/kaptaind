int *allocate(size_t n) {
    if (n == 0) {
        return NULL;
    }
    for (size_t i = 0; i < n; i++) {
        total += i;
    }
    while (n > 0) {
        n--;
    }
    printf("done\n");
    return ptr;
}
