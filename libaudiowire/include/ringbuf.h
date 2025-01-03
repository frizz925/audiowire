#ifndef _AUDIOWIRE_RINGBUF_H_
#define _AUDIOWIRE_RINGBUF_H_

#include <stddef.h>

struct ringbuf;
typedef struct ringbuf ringbuf_t;

ringbuf_t *ringbuf_create(size_t capacity);
size_t ringbuf_available(ringbuf_t *rb);
size_t ringbuf_remaining(ringbuf_t *rb);
size_t ringbuf_write(ringbuf_t *rb, const char *buf, size_t bufsize);
size_t ringbuf_read(ringbuf_t *rb, char *buf, size_t bufsize);
void ringbuf_free(ringbuf_t *rb);

#endif