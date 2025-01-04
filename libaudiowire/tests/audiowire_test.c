#include "audiowire.h"
#include "ringbuf.h"

#include <assert.h>
#include <stdio.h>
#include <unistd.h>

#ifdef __WIN32__
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <winsock2.h>
#endif

#define AUDIO_BUFSIZE 65536

struct sockaddr_in saddr;
int saddrlen = sizeof(saddr);

int read_callback(const char *data, size_t bufsize, void *userdata) {
    int sock = socket(AF_INET, SOCK_DGRAM, 0);
    int send = sendto(sock, data, bufsize, 0, (struct sockaddr *)&saddr, saddrlen);
    ringbuf_t *rb = (ringbuf_t *)userdata;
    ringbuf_write(rb, data, send);
    return AW_STREAM_STOP;
}

int write_callback(char *data, size_t bufsize, void *userdata) {
    ringbuf_t *rb = (ringbuf_t *)userdata;
    if (ringbuf_remaining(rb) >= bufsize) {
        size_t read = ringbuf_read(rb, data, bufsize);
        int sock = socket(AF_INET, SOCK_DGRAM, 0);
        sendto(sock, data, read, 0, (struct sockaddr *)&saddr, saddrlen);
        return AW_STREAM_STOP;
    } else {
        memset(data, 0, bufsize);
        return AW_STREAM_CONTINUE;
    }
}

int main() {
    char rbuf[AUDIO_BUFSIZE], wbuf[AUDIO_BUFSIZE];

#ifdef __WIN32__
    WSADATA wsa_data;
    assert(WSAStartup(MAKEWORD(2, 2), &wsa_data) == 0);
#endif

    int sock = socket(AF_INET, SOCK_DGRAM, 0);
    saddr.sin_family = AF_INET;
    saddr.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    saddr.sin_port = 0;
    assert(bind(sock, (struct sockaddr *)&saddr, sizeof(saddr)) == 0);
    assert(getsockname(sock, (struct sockaddr *)&saddr, &saddrlen) == 0);

    aw_stream_t *record, *playback;
    ringbuf_t *rb = ringbuf_create(AUDIO_BUFSIZE);

    assert(aw_init() == 0);
    assert(aw_start_record(&record, read_callback, rb) == 0);
    assert(aw_start_playback(&playback, write_callback, rb) == 0);

    int rlen = recvfrom(sock, rbuf, sizeof(rbuf), 0, NULL, NULL);
    int wlen = recvfrom(sock, wbuf, sizeof(wbuf), 0, NULL, NULL);
    assert(rlen == wlen);
    assert(memcmp(rbuf, wbuf, rlen) == 0);

    assert(aw_stop(playback) == 0);
    assert(aw_stop(record) == 0);
    assert(aw_terminate() == 0);

    close(sock);

#ifdef __WIN32__
    WSACleanup();
#endif

    return 0;
}