#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 3) return 125;
    char *end;
    long fd = strtol(argv[1], &end, 10);
    if (*end || fd < 0) return 125;
    FILE *stats = fdopen((int)fd, "w");
    if (!stats) return 125;
    struct timespec start, finish;
    if (clock_gettime(CLOCK_MONOTONIC, &start)) return 125;
    pid_t pid = fork();
    if (pid < 0) return 125;
    if (pid == 0) {
        fclose(stats);
        execvp(argv[2], argv + 2);
        perror("execvp");
        _exit(127);
    }
    int status;
    struct rusage usage;
    while (wait4(pid, &status, 0, &usage) < 0) {
        if (errno != EINTR) return 125;
    }
    if (clock_gettime(CLOCK_MONOTONIC, &finish)) return 125;
    double seconds = (finish.tv_sec - start.tv_sec) +
                     (finish.tv_nsec - start.tv_nsec) / 1e9;
    if (fprintf(stats, "%.9f %ld\n", seconds, usage.ru_maxrss) < 0 ||
        fclose(stats)) return 125;
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
