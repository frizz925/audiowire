#ifndef _INTERNALS_H_
#define _INTERNALS_H_

#include "../include/audiowire.h"
#include "../include/ringbuf.h"

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

#define MAX_BUFFER_FRAMES 65536

#define AW_RESULT_DEVICE_NOT_FOUND aw_result(-1, "Device not found")

typedef struct aw_stream_base {
    ringbuf_t *ringbuf;
    const char *devname;
    uint32_t sample_rate;
    size_t max_bufsize;
    aw_config_t config;
    aw_error_callback_t error_cb;
    void *userdata;
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

static inline aw_result_t aw_result(int code, const char *message) {
    aw_result_t result = {code, message};
    return result;
}

static inline void aw_stream_base_init(aw_stream_base_t *base,
                                       aw_config_t cfg,
                                       const char *devname,
                                       aw_error_callback_t error_cb,
                                       void *userdata) {
    base->max_bufsize = frame_buffer_size(&cfg, cfg.max_buffer_frames);
    base->ringbuf = ringbuf_create(base->max_bufsize);
    base->config = cfg;
    base->devname = devname;
    base->sample_rate = 0;
    base->error_cb = error_cb;
    base->userdata = userdata;
}

static inline void aw_stream_base_deinit(aw_stream_base_t *base) {
    if (base->ringbuf)
        ringbuf_free(base->ringbuf);
    base->ringbuf = NULL;
    base->devname = NULL;
    base->max_bufsize = 0;
    memset(&base->config, 0, sizeof(aw_config_t));
}

static inline void aw_stream_base_error(aw_stream_base_t *base, int err, const char *message) {
    if (base->error_cb)
        base->error_cb(err, message, base->userdata);
}

#define AW_RESULT_NO_ERROR aw_result(0, NULL)

#endif
