#ifndef _AUDIOWIRE_H_
#define _AUDIOWIRE_H_

#include <stddef.h>
#include <stdint.h>

typedef struct aw_stream aw_stream_t;

typedef struct aw_result {
    int code;
    const char *message;
} aw_result_t;

#define AW_RESULT_IS_OK(res) (res.code == 0)
#define AW_RESULT_IS_ERR(res) (res.code != 0)

typedef enum aw_sample_format {
    AW_SAMPLE_FORMAT_S16,
    AW_SAMPLE_FORMAT_F32,
} aw_sample_format_t;

size_t aw_sample_size(aw_sample_format_t format);

typedef struct aw_config {
    uint8_t channels;
    uint32_t sample_rate;
    aw_sample_format_t sample_format;
    uint32_t buffer_frames;
    uint32_t max_buffer_frames;
} aw_config_t;

typedef void (*aw_read_callback_t)(const char *buf, size_t len, void *userdata);
typedef void (*aw_write_callback_t)(char *buf, size_t len, void *userdata);
typedef void (*aw_error_callback_t)(int err, const char *msg, void *userdata);

aw_result_t aw_initialize();
aw_result_t aw_start(aw_stream_t **stream,
                     const char *devname,
                     const char *name,
                     aw_config_t cfg,
                     aw_read_callback_t read_cb,
                     aw_write_callback_t write_cb,
                     aw_error_callback_t error_cb,
                     void *userdata);
const char *aw_device_name(aw_stream_t *stream);
uint32_t aw_sample_rate(aw_stream_t *stream);
aw_result_t aw_stop(aw_stream_t *stream);
aw_result_t aw_terminate();

#endif
