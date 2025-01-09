#ifndef _INTERNALS_H_
#define _INTERNALS_H_

#include "../include/audiowire.h"
#include "../include/ringbuf.h"

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

#define MAX_BUFFER_FRAMES 65536

typedef struct aw_stream_base {
    ringbuf_t *ringbuf;
    const char *devname;
    size_t max_bufsize;
    aw_config_t config;
} aw_stream_base_t;

// Sample is a single unit of value, eg. u16 or f32.
// Frame is a collection of samples from all channels.
// eg. a frame of stereo channel is basically sample[2]
// Frames per duration is a collection of frames within a certain duration.
// eg. 20ms duration with 48k sample rate contains 960 frames

static inline size_t frame_size(const aw_config_t *cfg) {
    return cfg->channels * aw_sample_size(cfg->sample_format);
}

static inline size_t frame_buffer_size(const aw_config_t *cfg, size_t count) {
    return count * frame_size(cfg);
}

#define error_result(err, ptr, message) \
    if (ptr != NULL) \
        *ptr = message; \
    return err

static inline aw_result_t aw_result(int code, const char *message) {
    aw_result_t result = {code, message};
    return result;
}

static inline void aw_stream_base_init(aw_stream_base_t *base, aw_config_t cfg, const char *devname) {
    base->max_bufsize = frame_buffer_size(&cfg, cfg.max_buffer_frames);
    base->ringbuf = ringbuf_create(base->max_bufsize);
    base->config = cfg;
    base->devname = devname;
}

static inline void aw_stream_base_deinit(aw_stream_base_t *base) {
    if (base->ringbuf)
        ringbuf_free(base->ringbuf);
    base->ringbuf = NULL;
    base->devname = NULL;
    base->max_bufsize = 0;
    memset(&base->config, 0, sizeof(aw_config_t));
}

#define AW_RESULT_NO_ERROR aw_result(0, NULL)

#endif
