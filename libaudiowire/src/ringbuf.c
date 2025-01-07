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

static size_t ringbuf_write(ringbuf_t *rb, const char *buf, size_t bufsize) {
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
    return bufsize;
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
inline size_t ringbuf_capacity(ringbuf_t *rb) {
    return rb->mask;
}

inline size_t ringbuf_available(ringbuf_t *rb) {
    if (rb->head > rb->tail)
        return rb->head - rb->tail - 1;
    return rb->capacity - rb->tail + rb->head - 1;
}

inline size_t ringbuf_remaining(ringbuf_t *rb) {
    if (rb->head <= rb->tail)
        return rb->tail - rb->head;
    return rb->capacity - rb->head + rb->tail;
}

inline size_t ringbuf_push(ringbuf_t *rb, const char *buf, size_t bufsize) {
    size_t capacity = ringbuf_capacity(rb);
    size_t available = ringbuf_available(rb);
    if (bufsize >= capacity) {
        rb->tail = 0;
        rb->head = 1;
        size_t offset = bufsize - capacity;
        memcpy(rb->data + 1, buf + offset, capacity);
    } else {
        if (bufsize > available) {
            size_t offset = bufsize - available;
            rb->head = (rb->head + offset) & rb->mask;
        }
        ringbuf_write(rb, buf, bufsize);
    }
    return bufsize;
}

inline size_t ringbuf_pop(ringbuf_t *rb, char *buf, size_t bufsize) {
    size_t remaining = ringbuf_remaining(rb);
    return ringbuf_read(rb, buf, min(bufsize, remaining));
}

inline void ringbuf_flush(ringbuf_t *rb) {
    rb->head = rb->tail = 0;
}

inline void ringbuf_free(ringbuf_t *rb) {
    free(rb);
}