#ifndef _INTERNALS_H_
#define _INTERNALS_H_

#include "../include/audiowire.h"

#include <stddef.h>

#define MAX_BUFFER_DURATION_MS 10000

// Sample is a single unit of value, eg. u16 or f32.
// Frame is a collection of samples from all channels.
// eg. a frame of stereo channel is basically sample[2]
// Frames per duration is a collection of frames within a certain duration.
// eg. 20ms duration with 48k sample rate contains 960 frames

static inline size_t frames_per_duration(const aw_config_t *cfg, uint32_t duration) {
    return (cfg->sample_rate / 1000) * duration;
}

static inline size_t frame_size(const aw_config_t *cfg) {
    return cfg->channels * aw_sample_size(cfg->sample_format);
}

static inline size_t size_per_duration(const aw_config_t *cfg, uint32_t duration) {
    return frame_size(cfg) * frames_per_duration(cfg, duration);
}

#define error_result(err, ptr, message) \
    if (ptr != NULL) \
        *ptr = message; \
    return err

static inline aw_result_t aw_result(int code, const char *message) {
    aw_result_t result = {code, message};
    return result;
}

#define AW_RESULT_NO_ERROR aw_result(0, NULL)

#endif
