#include "internals.h"

#define STREAM_FIELD(s, f) ((aw_stream_base_t *)s)->f
#define STREAM_RINGBUF(s) ((aw_stream_base_t *)s)->ringbuf

inline size_t aw_sample_size(aw_sample_format_t format) {
    switch (format) {
    case AW_SAMPLE_FORMAT_S16:
        return sizeof(uint16_t);
    case AW_SAMPLE_FORMAT_F32:
        return sizeof(float);
    }
    return 0;
}

inline const char *aw_device_name(aw_stream_t *s) {
    return STREAM_FIELD(s, devname);
}

inline uint32_t aw_sample_rate(aw_stream_t *s) {
    return STREAM_FIELD(s, sample_rate);
}