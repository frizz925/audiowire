#include "ringbuf.h"

#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

#define min(a, b) (a < b ? a : b)

struct ringbuf {
    char *data;
    size_t capacity;
    size_t mask;
    atomic_size_t ridx;
    atomic_size_t widx;
};

ringbuf_t *ringbuf_create(size_t cap) {
    size_t capacity = 1;
    while (capacity < cap)
        capacity <<= 1;

    size_t memsize = sizeof(ringbuf_t) + capacity;
    ringbuf_t *rb = calloc(1, memsize);
    rb->data = (void *)rb + memsize - capacity;
    rb->capacity = capacity;
    rb->mask = capacity - 1;
    rb->ridx = 0;
    rb->widx = 0;
    return rb;
}

size_t ringbuf_available(ringbuf_t *rb) {
    if (rb->ridx > rb->widx)
        return rb->ridx - rb->widx;
    return rb->capacity - rb->widx + rb->ridx;
}

size_t ringbuf_remaining(ringbuf_t *rb) {
    if (rb->widx >= rb->ridx)
        return rb->widx - rb->ridx;
    return rb->capacity - rb->ridx + rb->widx;
}

size_t ringbuf_write(ringbuf_t *rb, const char *buf, size_t bufsize) {
    size_t off = 0;
    while (off < bufsize) {
        size_t srclen = bufsize - off;
        size_t dstlen = (rb->ridx > rb->widx ? rb->ridx : rb->capacity) - rb->widx;
        size_t size = min(srclen, dstlen);
        if (size <= 0)
            break;
        memcpy(rb->data + rb->widx, buf + off, size);
        rb->widx = (rb->widx + size) & rb->mask;
        off += size;
    }
    return off;
}

size_t ringbuf_read(ringbuf_t *rb, char *buf, size_t bufsize) {
    size_t off = 0;
    while (off < bufsize) {
        size_t srclen = (rb->widx >= rb->ridx ? rb->widx : rb->capacity) - rb->ridx;
        size_t dstlen = bufsize - off;
        size_t size = min(srclen, dstlen);
        if (size <= 0)
            break;
        memcpy(buf + off, rb->data + rb->ridx, size);
        rb->ridx = (rb->ridx + size) & rb->mask;
        off += size;
    }
    return off;
}

void ringbuf_free(ringbuf_t *rb) {
    free(rb);
}