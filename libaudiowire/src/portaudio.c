#include "internals.h"
#include "ringbuf.h"

#include <assert.h>
#include <portaudio.h>
#include <stdio.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

struct aw_stream {
    aw_config_t config;
    PaStream *handle;
    ringbuf_t *ringbuf;
    const char *devname;
};

static inline bool device_is_valid(const aw_config_t *cfg, const PaDeviceInfo *info, bool is_input) {
    return (is_input && info->maxInputChannels >= cfg->channels) ||
           (!is_input && info->maxOutputChannels >= cfg->channels);
}

static int on_stream_read(const void *input,
                          void *output,
                          unsigned long count,
                          const PaStreamCallbackTimeInfo *info,
                          PaStreamCallbackFlags flags,
                          void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    size_t bufsize = count * frame_size(&stream->config);
    if (ringbuf_available(stream->ringbuf) >= bufsize)
        ringbuf_push(stream->ringbuf, input, bufsize);
    return paContinue;
}

static int on_stream_write(const void *input,
                           void *output,
                           unsigned long count,
                           const PaStreamCallbackTimeInfo *info,
                           PaStreamCallbackFlags flags,
                           void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    size_t bufsize = count * frame_size(&stream->config);
    if (ringbuf_remaining(stream->ringbuf) >= bufsize)
        ringbuf_pop(stream->ringbuf, output, bufsize);
    else
        memset(output, 0, bufsize);
    return paContinue;
}

static inline void free_stream(aw_stream_t *stream) {
    if (stream->ringbuf)
      ringbuf_free(stream->ringbuf);
    free(stream);
}

static aw_result_t start_stream(aw_stream_t **s, const char *name, aw_config_t cfg, bool is_input) {
    assert(cfg.max_buffer_duration >= cfg.buffer_duration);
    assert(cfg.max_buffer_duration < MAX_BUFFER_DURATION_MS);

    const char *message = NULL;
    PaError err = paNoError;

    aw_stream_t *stream = calloc(1, sizeof(aw_stream_t));
    stream->config = cfg;
    stream->ringbuf = ringbuf_create(size_per_duration(&cfg, cfg.max_buffer_duration));
    printf("ringbuf capacity: %zu\n", ringbuf_capacity(stream->ringbuf));

    PaDeviceIndex device = is_input ? Pa_GetDefaultInputDevice() : Pa_GetDefaultOutputDevice();
    const PaDeviceInfo *info = Pa_GetDeviceInfo(device);
    if (name != NULL || !device_is_valid(&cfg, info, is_input)) {
        device = paNoDevice;
        for (PaDeviceIndex idx = 0; idx < Pa_GetDeviceCount(); idx++) {
            info = Pa_GetDeviceInfo(idx);
            if (!strstr(info->name, name))
                continue;
            if (!device_is_valid(&cfg, info, is_input))
                continue;
            device = idx;
            break;
        }
    }
    if (device == paNoDevice) {
        err = -1;
        message = "Device not found";
        goto error;
    }
    stream->devname = info->name;

    PaSampleFormat format;
    switch (cfg.sample_format) {
    case AW_SAMPLE_FORMAT_S16:
        format = paInt16;
        break;
    case AW_SAMPLE_FORMAT_F32:
        format = paFloat32;
        break;
    }

    PaStreamParameters params = {
        .device = device,
        .channelCount = cfg.channels,
        .sampleFormat = format,
        .suggestedLatency = is_input ? info->defaultLowInputLatency : info->defaultLowOutputLatency,
        .hostApiSpecificStreamInfo = 0,
    };
    err = Pa_OpenStream(&stream->handle,
                        is_input ? &params : NULL,
                        is_input ? NULL : &params,
                        cfg.sample_rate,
                        frames_per_duration(&cfg, cfg.buffer_duration),
                        paNoFlag,
                        is_input ? on_stream_read : on_stream_write,
                        stream);
    if (err)
        goto pa_error;
    if ((err = Pa_StartStream(stream->handle)))
        goto pa_error;

    *s = stream;
    return AW_RESULT_NO_ERROR;

pa_error:
    message = Pa_GetErrorText(err);

error:
    free_stream(stream);
    return aw_result(err, message);
}

aw_result_t aw_initialize() {
    PaError err = Pa_Initialize();
    return err ? aw_result(err, Pa_GetErrorText(err)) : AW_RESULT_NO_ERROR;
}

aw_result_t aw_start_record(aw_stream_t **stream, const char *name, aw_config_t cfg) {
    return start_stream(stream, name, cfg, true);
}

aw_result_t aw_start_playback(aw_stream_t **stream, const char *name, aw_config_t cfg) {
    return start_stream(stream, name, cfg, false);
}

size_t aw_record_peek(aw_stream_t *stream) {
    return ringbuf_remaining(stream->ringbuf);
}

size_t aw_record_read(aw_stream_t *stream, char *buf, size_t bufsize) {
    return ringbuf_pop(stream->ringbuf, buf, bufsize);
}

size_t aw_playback_peek(aw_stream_t *stream) {
    return ringbuf_available(stream->ringbuf);
}

size_t aw_playback_write(aw_stream_t *stream, const char *buf, size_t bufsize) {
    return ringbuf_push(stream->ringbuf, buf, bufsize);
}

const char *aw_device_name(aw_stream_t *stream) {
    return stream->devname;
}

aw_result_t aw_stop(aw_stream_t *stream) {
    if (stream->handle) {
        PaError err = paNoError;
        if (Pa_IsStreamActive(stream->handle)) {
            if ((err = Pa_StopStream(stream->handle)))
                return aw_result(err, Pa_GetErrorText(err));
        }
        if ((err = Pa_CloseStream(stream->handle)))
            return aw_result(err, Pa_GetErrorText(err));
    }
    free_stream(stream);
    return AW_RESULT_NO_ERROR;
}

aw_result_t aw_terminate() {
    PaError err = Pa_Terminate();
    return err ? aw_result(err, Pa_GetErrorText(err)) : AW_RESULT_NO_ERROR;
}
