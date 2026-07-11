/* Legacy API, removed in 2.0:
 * int legacy_connect(int fd) {
 *     return -1;
 * }
 */

// int old_init(void);

#define VERSION 3

struct Session {
    int fd;
};

int session_open(const char *path) {
    return 0;
}
