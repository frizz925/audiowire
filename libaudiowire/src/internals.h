#ifndef _INTERNALS_H_
#define _INTERNALS_H_

#include "../include/audiowire.h"

#include <stddef.h>

#define RINGBUF_SIZE 65536

static inline size_t frames_per_duration(const aw_config_t *cfg, uint32_t duration) {
    return cfg->channels * duration;
}

static inline size_t frames_bufsize(const aw_config_t *cfg, size_t count) {
    return count * cfg->channels * aw_sample_size(cfg->sample_format);
}

#define error_result(err, ptr, message) \
    if (ptr != NULL) \
        *ptr = message; \
    return err

#endif
