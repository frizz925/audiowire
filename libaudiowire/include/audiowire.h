#ifndef _AUDIOWIRE_H_
#define _AUDIOWIRE_H_

#include <stddef.h>

struct aw_stream;
typedef struct aw_stream aw_stream_t;

typedef struct aw_result {
    int code;
    const char *message;
} aw_result_t;

aw_result_t aw_result_no_error = {0};

aw_result_t aw_result(int code, const char *message) {
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

typedef int aw_stream_read_callback_t(const char *data, size_t bufsize, void *userdata);
typedef int aw_stream_write_callback_t(char *data, size_t bufsize, void *userdata);

aw_result_t aw_initialize();
aw_result_t aw_start_record(aw_stream_t **stream, const char *name, aw_stream_read_callback_t *callback, void *userdata);
aw_result_t aw_start_playback(aw_stream_t **stream, const char *name, aw_stream_write_callback_t *callback, void *userdata);
const char* aw_device_name(aw_stream_t *stream);
aw_result_t aw_stop(aw_stream_t *stream);
aw_result_t aw_terminate();

#endif