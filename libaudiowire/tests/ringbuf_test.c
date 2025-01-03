#include "ringbuf.h"

#include <assert.h>
#include <string.h>

#define SAMPLE "hello"

int main() {
    char buf[16];
    const char *sample = SAMPLE;
    size_t bufsize = sizeof(buf);
    size_t size = sizeof(SAMPLE);
    size_t capacity = 8;

    ringbuf_t *rb = ringbuf_create(capacity);
    for (int i = 0; i < 3; i++) {
        memset(buf, 0, bufsize);
        assert(ringbuf_available(rb) == capacity);
        assert(ringbuf_remaining(rb) == 0);
        assert(ringbuf_write(rb, sample, size) == size);

        assert(ringbuf_available(rb) == (capacity - size));
        assert(ringbuf_remaining(rb) == size);
        assert(ringbuf_read(rb, buf, bufsize) == size);
        assert(memcmp(buf, sample, size) == 0);
    }
}