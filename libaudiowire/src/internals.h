#ifndef _INTERNALS_H_
#define _INTERNALS_H_

#include "../include/audiowire.h"

#include <stddef.h>

#define MAX_BUFFER_FRAMES 65536

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

#define AW_RESULT_NO_ERROR aw_result(0, NULL)

#endif
