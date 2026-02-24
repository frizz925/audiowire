#ifndef _INTERNALS_H_
#define _INTERNALS_H_

#include "../include/audiowire2.h"

#include <stddef.h>
#include <stdlib.h>
#include <string.h>

#define MAX_BUFFER_SAMPLES 65536

#define AW_RESULT_DEVICE_NOT_FOUND aw_result(-1, "Device not found")

typedef struct aw_stream_base {
    aw_config_t config;
    const char *devname;
    uint32_t sample_rate;
    aw_read_callback_t read_cb;
    aw_write_callback_t write_cb;
    aw_error_callback_t error_cb;
    void *userdata;
} aw_stream_base_t;

static inline size_t sample_buffer_size(const aw_config_t *cfg, size_t count) {
    return count * cfg->channels * aw_sample_size(cfg->sample_format);
}

static inline aw_result_t aw_result(int code, const char *message) {
    aw_result_t result = {code, message};
    return result;
}

static inline void aw_stream_base_init(aw_stream_base_t *base,
                                       aw_config_t cfg,
                                       const char *devname,
                                       aw_read_callback_t read_cb,
                                       aw_write_callback_t write_cb,
                                       aw_error_callback_t error_cb,
                                       void *userdata) {
    base->config = cfg;
    base->devname = devname;
    base->sample_rate = 0;
    base->read_cb = read_cb;
    base->write_cb = write_cb;
    base->error_cb = error_cb;
    base->userdata = userdata;
}

static inline void aw_stream_base_deinit(aw_stream_base_t *base) {
    memset(&base->config, 0, sizeof(aw_config_t));

    base->devname = NULL;
    base->sample_rate = 0;
    base->read_cb = NULL;
    base->write_cb = NULL;
    base->error_cb = NULL;
    base->userdata = NULL;
}

static inline void aw_stream_base_error(aw_stream_base_t *base, int err, const char *message) {
    if (base->error_cb)
        base->error_cb(err, message, base->userdata);
}

#define AW_RESULT_NO_ERROR aw_result(0, NULL)

#endif
