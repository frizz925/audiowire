#include "internals.h"

#include <pulse/pulseaudio.h>
#include <stdbool.h>
#include <stdio.h>

#define APPLICATION_NAME "Audiowire"

struct aw_stream {
    aw_stream_base_t base;
    pa_sample_spec sample_spec;
    pa_buffer_attr buffer_attr;

    pa_threaded_mainloop *mainloop;
    pa_mainloop_api *mainloop_api;
    pa_context *context;
    pa_stream *handle;
};

static aw_result_t error_stream(aw_stream_t *stream) {
    int errno = pa_context_errno(stream->context);
    return aw_result(errno, pa_strerror(errno));
}

static void error_stream_callback(aw_stream_t *stream) {
    int errno = pa_context_errno(stream->context);
    aw_stream_base_error(&stream->base, errno, pa_strerror(errno));
    pa_stream_set_read_callback(stream->handle, NULL, NULL);
    pa_stream_set_write_callback(stream->handle, NULL, NULL);
}

static void on_context_state(pa_context *c, void *userdata) {
    pa_threaded_mainloop *mainloop = (pa_threaded_mainloop *)userdata;

    switch (pa_context_get_state(c)) {
    case PA_CONTEXT_UNCONNECTED:
    case PA_CONTEXT_CONNECTING:
    case PA_CONTEXT_AUTHORIZING:
    case PA_CONTEXT_SETTING_NAME:
        break;
    case PA_CONTEXT_READY:
    case PA_CONTEXT_FAILED:
    case PA_CONTEXT_TERMINATED:
        pa_threaded_mainloop_signal(mainloop, 0);
        break;
    }
}

static void on_stream_state(pa_stream *s, void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;

    switch (pa_stream_get_state(s)) {
    case PA_STREAM_READY:
        stream->base.devname = pa_stream_get_device_name(s);
        pa_threaded_mainloop_signal(stream->mainloop, 0);
        break;
    case PA_STREAM_UNCONNECTED:
    case PA_STREAM_CREATING:
    case PA_STREAM_FAILED:
    case PA_STREAM_TERMINATED:
        break;
    }
}

static void on_stream_moved(pa_stream *s, void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    stream->base.devname = pa_stream_get_device_name(s);
}

static void on_stream_read(pa_stream *s, size_t length, void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    aw_stream_base_t *base = &stream->base;

    const void *data;
    while (pa_stream_readable_size(s) > 0) {
        if (pa_stream_peek(s, &data, &length))
            goto error;
        if (length <= 0)
            continue;
        if (data && ringbuf_available(base->ringbuf) >= length)
            ringbuf_push(base->ringbuf, data, length);
        if (pa_stream_drop(s))
            goto error;
    }
    return;

error:
    error_stream_callback(stream);
}

static void on_stream_write(pa_stream *s, size_t length, void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    aw_stream_base_t *base = &stream->base;

    void *data;
    size_t nbytes = length;
    if (pa_stream_begin_write(s, &data, &nbytes) || !data)
        goto error;

    if (ringbuf_remaining(base->ringbuf) >= nbytes)
        ringbuf_pop_back_from(base->ringbuf, data, nbytes, base->max_bufsize);
    else
        memset(data, 0, nbytes);

    if (pa_stream_write(s, data, nbytes, NULL, 0, PA_SEEK_RELATIVE))
        goto error;

    return;

error:
    error_stream_callback(stream);
}

static void free_stream(aw_stream_t *stream) {
    if (stream->handle) {
        pa_stream_unref(stream->handle);
        stream->handle = NULL;
    }
    if (stream->context) {
        pa_context_disconnect(stream->context);
        pa_context_unref(stream->context);
        stream->context = NULL;
    }
    if (stream->mainloop) {
        pa_threaded_mainloop_stop(stream->mainloop);
        pa_threaded_mainloop_free(stream->mainloop);
        stream->mainloop_api = NULL;
        stream->mainloop = NULL;
    }
    aw_stream_base_deinit(&stream->base);
    free(stream);
}

static aw_result_t start_stream(aw_stream_t **s, const char *devname, const char *name, aw_config_t cfg, bool is_input,
                                aw_error_callback_t error_cb, void *userdata) {
    aw_result_t result = AW_RESULT_NO_ERROR;
    aw_stream_t *stream = calloc(1, sizeof(aw_stream_t));
    aw_stream_base_init(&stream->base, cfg, devname, error_cb, userdata);

    pa_sample_spec *ss = &stream->sample_spec;
    ss->channels = cfg.channels;
    ss->rate = cfg.sample_rate;
    switch (cfg.sample_format) {
    case AW_SAMPLE_FORMAT_S16:
        ss->format = PA_SAMPLE_S16LE;
        break;
    case AW_SAMPLE_FORMAT_F32:
        ss->format = PA_SAMPLE_FLOAT32LE;
        break;
    }

    size_t bufsize = frame_buffer_size(&cfg, cfg.buffer_frames);
    pa_buffer_attr *ba = &stream->buffer_attr;
    ba->fragsize = bufsize;
    ba->minreq = bufsize;
    ba->tlength = bufsize;
    ba->maxlength = (uint32_t)-1;
    ba->prebuf = (uint32_t)-1;

    stream->mainloop = pa_threaded_mainloop_new();
    stream->mainloop_api = pa_threaded_mainloop_get_api(stream->mainloop);
    stream->context = pa_context_new(stream->mainloop_api, APPLICATION_NAME);
    pa_context_set_state_callback(stream->context, on_context_state, stream->mainloop);

    if (pa_context_connect(stream->context, NULL, 0, NULL))
        goto error;

    pa_threaded_mainloop_lock(stream->mainloop);
    if (pa_threaded_mainloop_start(stream->mainloop)) {
        pa_threaded_mainloop_unlock(stream->mainloop);
        result = aw_result(-1, "Failed to start mainloop");
        goto cleanup;
    }

    {
        pa_context_state_t state = pa_context_get_state(stream->context);
        while (state != PA_CONTEXT_READY) {
            pa_threaded_mainloop_wait(stream->mainloop);
            state = pa_context_get_state(stream->context);
            if (!PA_CONTEXT_IS_GOOD(state))
                goto unlock_error;
        }
    }

    stream->handle = pa_stream_new(stream->context, name, ss, NULL);
    pa_stream_set_state_callback(stream->handle, on_stream_state, stream);
    pa_stream_set_moved_callback(stream->handle, on_stream_moved, stream);
    if (is_input)
        pa_stream_set_read_callback(stream->handle, on_stream_read, stream);
    else
        pa_stream_set_write_callback(stream->handle, on_stream_write, stream);

    int res = is_input ? pa_stream_connect_record(stream->handle, devname, ba, PA_STREAM_ADJUST_LATENCY)
                       : pa_stream_connect_playback(stream->handle, devname, ba, PA_STREAM_ADJUST_LATENCY, NULL, NULL);
    if (res)
        goto unlock_error;

    {
        pa_stream_state_t state = pa_stream_get_state(stream->handle);
        while (state != PA_STREAM_READY) {
            pa_threaded_mainloop_wait(stream->mainloop);
            state = pa_stream_get_state(stream->handle);
            if (!PA_STREAM_IS_GOOD(state))
                goto unlock_error;
        }
    }

    pa_threaded_mainloop_unlock(stream->mainloop);

    *s = stream;
    return result;

unlock_error:
    pa_threaded_mainloop_unlock(stream->mainloop);

error:
    result = error_stream(stream);

cleanup:
    free_stream(stream);
    return result;
}

inline aw_result_t aw_initialize() {
    return AW_RESULT_NO_ERROR;
}

inline aw_result_t aw_start_record(aw_stream_t **stream, const char *devname, const char *name, aw_config_t cfg,
                                   aw_error_callback_t error_cb, void *userdata) {
    return start_stream(stream, devname, name, cfg, true, error_cb, userdata);
}

inline aw_result_t aw_start_playback(aw_stream_t **stream, const char *devname, const char *name, aw_config_t cfg,
                                     aw_error_callback_t error_cb, void *userdata) {
    return start_stream(stream, devname, name, cfg, false, error_cb, userdata);
}

aw_result_t aw_stop(aw_stream_t *stream) {
    if (stream->handle && pa_stream_disconnect(stream->handle))
        return error_stream(stream);
    free_stream(stream);
    return AW_RESULT_NO_ERROR;
}

inline aw_result_t aw_terminate() {
    return AW_RESULT_NO_ERROR;
}