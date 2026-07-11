static int helper(int x) {
    return x * 2;
}

int public_api(int x) {
    int y = helper(x);
    return y;
}
