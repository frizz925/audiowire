#include "ringbuf.h"

#include <assert.h>
#include <string.h>

#define SAMPLE "hello"
#define LONG_SAMPLE "Hello, world!"

int main() {
    char buf[16];
    size_t sz_buf = sizeof(buf);
    size_t capacity = 7; // Power of 2 subtracted by 1

    ringbuf_t *rb = ringbuf_create(capacity - 1);
    assert(ringbuf_capacity(rb) == capacity);

    // Test for wrapping and overlapping, dropping latest data when overflowed
    assert(ringbuf_available(rb) == capacity);
    assert(ringbuf_remaining(rb) == 0);

    const char *sample = SAMPLE;
    size_t sz_sample = sizeof(SAMPLE);
    assert(ringbuf_push(rb, sample, sz_sample) == sz_sample);
    assert(ringbuf_push(rb, sample, sz_sample) == capacity - sz_sample);
    assert(ringbuf_push(rb, sample, sz_sample) == 0);

    assert(ringbuf_available(rb) == 0);
    assert(ringbuf_remaining(rb) == capacity);
    assert(ringbuf_pop(rb, buf, sz_buf) == capacity);
    assert(memcmp(buf, sample, sz_sample) == 0);

    // Test for flush function
    ringbuf_flush(rb);
    assert(ringbuf_available(rb) == capacity);
    assert(ringbuf_remaining(rb) == 0);

    // Test for overflow truncation
    const char *long_sample = LONG_SAMPLE;
    size_t sz_long_sample = sizeof(LONG_SAMPLE);
    assert(ringbuf_push(rb, long_sample, sz_long_sample) == capacity);
    assert(ringbuf_available(rb) == 0);
    assert(ringbuf_remaining(rb) == capacity);
    assert(ringbuf_pop(rb, buf, sz_buf) == capacity);
    assert(memcmp(buf, long_sample, capacity) == 0);
}