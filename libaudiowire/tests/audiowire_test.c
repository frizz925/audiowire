#include "audiowire.h"
#include "ringbuf.h"

#include <assert.h>
#include <unistd.h>

int read_callback(const char *data, size_t bufsize, void *userdata) {
    ringbuf_t *rb = (ringbuf_t *)userdata;
    ringbuf_write(rb, data, bufsize);
    return AW_STREAM_CONTINUE;
}

int write_callback(char *data, size_t bufsize, void *userdata) {
    ringbuf_t *rb = (ringbuf_t *)userdata;
    if (ringbuf_remaining(rb) >= bufsize)
        ringbuf_read(rb, data, bufsize);
    else
        memset(data, 0, bufsize);
    return AW_STREAM_CONTINUE;
}

int main() {
    aw_stream_t *record, *playback;
    ringbuf_t *rb = ringbuf_create(65536);

    assert(aw_init() == 0);
    assert(aw_start_record(&record, read_callback, rb) == 0);
    assert(aw_start_playback(&playback, write_callback, rb) == 0);

    sleep(3);

    assert(aw_stop(playback) == 0);
    assert(aw_stop(record) == 0);
    assert(aw_terminate() == 0);

    return 0;
}