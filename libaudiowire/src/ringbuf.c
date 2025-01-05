#include "ringbuf.h"

#include <stdatomic.h>
#include <stdlib.h>
#include <string.h>

#define min(a, b) ((a) < (b) ? (a) : (b))

struct ringbuf {
    char *data;
    size_t capacity;
    size_t mask;
    atomic_size_t head;
    atomic_size_t tail;
};

static inline size_t ringbuf_available(ringbuf_t *rb) {
    if (rb->head > rb->tail)
        return rb->head - rb->tail - 1;
    return rb->capacity - rb->tail + rb->head - 1;
}

static inline size_t ringbuf_remaining(ringbuf_t *rb) {
    if (rb->head <= rb->tail)
        return rb->tail - rb->head;
    return rb->capacity - rb->head + rb->tail;
}

static void ringbuf_write(ringbuf_t *rb, const char *buf, size_t bufsize) {
    size_t tail = rb->tail;
    size_t offset = 0;
    while (offset < bufsize) {
        size_t available = rb->capacity - tail;
        size_t remaining = bufsize - offset;
        size_t length = min(available, remaining);
        memcpy(rb->data + tail, buf + offset, length);
        rb->tail = tail = (tail + length) & rb->mask;
        offset += length;
    }
}

static size_t ringbuf_read(ringbuf_t *rb, char *buf, size_t bufsize) {
    size_t head = rb->head;
    size_t offset = 0;
    while (offset < bufsize) {
        size_t remaining = rb->capacity - head;
        size_t available = bufsize - offset;
        size_t length = min(remaining, available);
        memcpy(buf + offset, rb->data + head, length);
        rb->head = head = (head + length) & rb->mask;
        offset += length;
    }
    return bufsize;
}

ringbuf_t *ringbuf_create(size_t cap) {
    size_t capacity = 1;
    while (capacity < cap)
        capacity <<= 1;

    size_t memsize = sizeof(ringbuf_t) + capacity;
    ringbuf_t *rb = calloc(1, memsize);
    rb->data = (void *)rb + memsize - capacity;
    rb->capacity = capacity;
    rb->mask = capacity - 1;
    rb->head = rb->tail = 0;
    return rb;
}

// We return the mask value because that's how much data that can be written
// into the ring buffer instead of the internal capacity value.
size_t ringbuf_capacity(ringbuf_t *rb) {
    return rb->mask;
}

// Size is how much data that can be read from the ring buffer.
size_t ringbuf_size(ringbuf_t *rb) {
    return ringbuf_remaining(rb);
}

size_t ringbuf_push(ringbuf_t *rb, const char *buf, size_t bufsize) {
    size_t available = ringbuf_available(rb);
    if (bufsize >= rb->mask) {
        memcpy(rb->data, buf + bufsize - rb->mask, rb->mask);
        rb->head = 0;
        rb->tail = rb->mask;
    } else {
        ringbuf_write(rb, buf, bufsize);
        if (bufsize > available)
            rb->head = (rb->head + bufsize - available) & rb->mask;
    }
    return bufsize;
}

size_t ringbuf_pop(ringbuf_t *rb, char *buf, size_t bufsize) {
    size_t remaining = ringbuf_remaining(rb);
    if (bufsize < remaining) {
        rb->head = (rb->head + remaining - bufsize) & rb->mask;
        return ringbuf_read(rb, buf, bufsize);
    }
    return ringbuf_read(rb, buf, remaining);
}

void ringbuf_flush(ringbuf_t *rb) {
    rb->head = rb->tail = 0;
}

void ringbuf_free(ringbuf_t *rb) {
    free(rb);
}