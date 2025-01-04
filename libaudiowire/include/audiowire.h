#ifndef _AUDIOWIRE_H_
#define _AUDIOWIRE_H_

#include <stddef.h>

struct aw_stream;
typedef struct aw_stream aw_stream_t;

typedef enum aw_stream_callback_result {
    AW_STREAM_CONTINUE,
    AW_STREAM_STOP,
    AW_STREAM_ABORT,
} aw_stream_callback_result_t;

typedef int aw_stream_read_callback_t(const char *data, size_t bufsize, void *userdata);
typedef int aw_stream_write_callback_t(char *data, size_t bufsize, void *userdata);

int aw_initialize();
int aw_start_record(aw_stream_t **stream, const char *name, aw_stream_read_callback_t *callback, void *userdata);
int aw_start_playback(aw_stream_t **stream, const char *name, aw_stream_write_callback_t *callback, void *userdata);
int aw_stop(aw_stream_t *stream);
int aw_terminate();

#endif