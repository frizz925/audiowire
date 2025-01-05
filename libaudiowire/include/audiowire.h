#ifndef _AUDIOWIRE_H_
#define _AUDIOWIRE_H_

#include <stddef.h>

struct aw_stream;
typedef struct aw_stream aw_stream_t;

typedef struct aw_result {
    int code;
    const char *message;
} aw_result_t;

static aw_result_t aw_result_no_error = {0};

static inline aw_result_t aw_result(int code, const char *message) {
    aw_result_t result = {code, message};
    return result;
}

#define aw_result_is_ok(res) (res.code == 0)
#define aw_result_is_err(res) (res.code != 0)

typedef enum aw_stream_callback_result {
    AW_STREAM_CONTINUE,
    AW_STREAM_STOP,
    AW_STREAM_ABORT,
} aw_stream_callback_result_t;

aw_result_t aw_initialize();
aw_result_t aw_start_record(aw_stream_t **stream, const char *name);
aw_result_t aw_start_playback(aw_stream_t **stream, const char *name);
size_t aw_record_read(aw_stream_t *stream, char *buf, size_t bufsize);
size_t aw_playback_write(aw_stream_t *stream, const char *buf, size_t bufsize);
const char *aw_device_name(aw_stream_t *stream);
aw_result_t aw_stop(aw_stream_t *stream);
aw_result_t aw_terminate();

#endif