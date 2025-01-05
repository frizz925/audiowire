#include "ringbuf.h"

#include <assert.h>
#include <string.h>

#define SAMPLE "hello"
#define LONG_SAMPLE "Hello, world!"
#define TRUNCATED_SAMPLE "world!"

int main() {
    char buf[16];
    const char *sample = SAMPLE;
    size_t sz_buf = sizeof(buf);
    size_t sz_sample = sizeof(SAMPLE);
    size_t capacity = 7; // Power of 2 subtracted by 1

    ringbuf_t *rb = ringbuf_create(capacity - 1);

    // Test for wrapping and overlapping, overwriting oldest data
    assert(ringbuf_capacity(rb) == capacity);
    assert(ringbuf_size(rb) == 0);
    for (int i = 0; i < 3; i++)
        assert(ringbuf_push(rb, sample, sz_sample) == sz_sample);
    assert(ringbuf_size(rb) == capacity);
    assert(ringbuf_pop(rb, buf, sz_buf) == capacity);
    assert(memcmp(buf + capacity - sz_sample, sample, sz_sample) == 0);

    // Test for flush function
    ringbuf_flush(rb);
    assert(ringbuf_size(rb) == 0);

    // Test for overflow truncation
    const char *long_sample = LONG_SAMPLE;
    size_t sz_long_sample = sizeof(LONG_SAMPLE);
    assert(ringbuf_push(rb, long_sample, sz_long_sample) == sz_long_sample);
    assert(ringbuf_size(rb) == capacity);
    assert(ringbuf_pop(rb, buf, sz_buf) == capacity);
    assert(memcmp(buf, TRUNCATED_SAMPLE, capacity) == 0);
}