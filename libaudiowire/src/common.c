#include "internals.h"

#define STREAM_FIELD(s, f) ((aw_stream_base_t *)s)->f
#define STREAM_RINGBUF(s) ((aw_stream_base_t *)s)->ringbuf

size_t aw_sample_size(aw_sample_format_t format) {
    switch (format) {
    case AW_SAMPLE_FORMAT_S16:
        return sizeof(uint16_t);
    case AW_SAMPLE_FORMAT_F32:
        return sizeof(float);
    }
    return 0;
}

inline size_t aw_buffer_capacity(aw_stream_t *s) {
    return ringbuf_capacity(STREAM_RINGBUF(s));
}

inline size_t aw_record_peek(aw_stream_t *s) {
    return ringbuf_remaining(STREAM_RINGBUF(s));
}

inline size_t aw_record_read(aw_stream_t *s, char *buf, size_t bufsize) {
    return ringbuf_pop_back_from(STREAM_RINGBUF(s), buf, bufsize, STREAM_FIELD(s, max_bufsize));
}

inline size_t aw_playback_peek(aw_stream_t *s) {
    return ringbuf_available(STREAM_RINGBUF(s));
}

inline size_t aw_playback_write(aw_stream_t *s, const char *buf, size_t bufsize) {
    return ringbuf_push(STREAM_RINGBUF(s), buf, bufsize);
}

inline const char *aw_device_name(aw_stream_t *s) {
    return STREAM_FIELD(s, devname);
}

inline uint32_t aw_sample_rate(aw_stream_t *s) {
    return STREAM_FIELD(s, sample_rate);
}