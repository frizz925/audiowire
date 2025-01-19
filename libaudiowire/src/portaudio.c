#include "internals.h"

#include <assert.h>
#include <portaudio.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#define WINDOWS_HOST_API "Windows WASAPI"
#endif

struct aw_stream {
    aw_stream_base_t base;
    PaStream *handle;
};

#ifdef _WIN32
static PaHostApiIndex host_api;
#endif

static inline bool
device_is_valid(const aw_config_t *cfg, const PaDeviceInfo *info, const char *devname, bool is_input) {
#ifdef _WIN32
    if (info->hostApi != host_api)
        return false;
#endif
    if (is_input && info->maxInputChannels < cfg->channels)
        return false;
    if (!is_input && info->maxOutputChannels < cfg->channels)
        return false;
    if (devname && !strstr(info->name, devname))
        return false;
    return true;
}

static int on_stream_read(const void *input,
                          void *output,
                          unsigned long count,
                          const PaStreamCallbackTimeInfo *info,
                          PaStreamCallbackFlags flags,
                          void *userdata) {
    aw_stream_base_t *stream = (aw_stream_base_t *)userdata;
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
    aw_stream_base_t *stream = (aw_stream_base_t *)userdata;
    size_t bufsize = count * frame_size(&stream->config);
    if (ringbuf_remaining(stream->ringbuf) >= bufsize)
        ringbuf_pop_back_from(stream->ringbuf, output, bufsize, stream->max_bufsize);
    else
        memset(output, 0, bufsize);
    return paContinue;
}

static inline void free_stream(aw_stream_t *s) {
    aw_stream_base_deinit(&s->base);
    free(s);
}

static aw_result_t start_stream(aw_stream_t **s,
                                const char *devname,
                                aw_config_t cfg,
                                bool is_input,
                                aw_error_callback_t error_cb,
                                void *userdata) {
    assert(cfg.buffer_frames > 0);
    assert(cfg.max_buffer_frames > 0);
    assert(cfg.max_buffer_frames >= cfg.buffer_frames);
    assert(cfg.max_buffer_frames <= MAX_BUFFER_FRAMES);

    const char *message = NULL;
    PaError err = paNoError;

    PaDeviceIndex device = is_input ? Pa_GetDefaultInputDevice() : Pa_GetDefaultOutputDevice();
    const PaDeviceInfo *info = Pa_GetDeviceInfo(device);
    if (!device_is_valid(&cfg, info, devname, is_input)) {
        device = paNoDevice;
        for (PaDeviceIndex idx = 0; idx < Pa_GetDeviceCount(); idx++) {
            info = Pa_GetDeviceInfo(idx);
            if (device_is_valid(&cfg, info, devname, is_input)) {
                device = idx;
                break;
            }
        }
    }
    if (device == paNoDevice)
        return AW_RESULT_DEVICE_NOT_FOUND;

    aw_stream_t *stream = calloc(1, sizeof(aw_stream_t));
    aw_stream_base_t *base = &stream->base;
    aw_stream_base_init(base, cfg, info->name, error_cb, userdata);

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
                        cfg.buffer_frames,
                        paNoFlag,
                        is_input ? on_stream_read : on_stream_write,
                        stream);
    if (err)
        goto error;
    base->sample_rate = Pa_GetStreamInfo(stream->handle)->sampleRate;

    if ((err = Pa_StartStream(stream->handle)))
        goto error;

    *s = stream;
    return AW_RESULT_NO_ERROR;

error:
    message = Pa_GetErrorText(err);
    free_stream(stream);
    return aw_result(err, message);
}

inline aw_result_t aw_initialize() {
    PaError err = Pa_Initialize();

#ifdef _WIN32
    if (err)
        return aw_result(err, Pa_GetErrorText(err));

    host_api = Pa_GetDefaultHostApi();
    for (PaHostApiIndex idx = 0; idx < Pa_GetHostApiCount(); idx++) {
        const PaHostApiInfo *info = Pa_GetHostApiInfo(idx);
        if (strstr(info->name, WINDOWS_HOST_API)) {
            host_api = idx;
            break;
        }
    }

    return AW_RESULT_NO_ERROR;
#else
    return err ? aw_result(err, Pa_GetErrorText(err)) : AW_RESULT_NO_ERROR;
#endif
}

inline aw_result_t aw_start_record(aw_stream_t **stream,
                                   const char *devname,
                                   const char *name,
                                   aw_config_t cfg,
                                   aw_error_callback_t error_cb,
                                   void *userdata) {
    return start_stream(stream, devname, cfg, true, error_cb, userdata);
}

inline aw_result_t aw_start_playback(aw_stream_t **stream,
                                     const char *devname,
                                     const char *name,
                                     aw_config_t cfg,
                                     aw_error_callback_t error_cb,
                                     void *userdata) {
    return start_stream(stream, devname, cfg, false, error_cb, userdata);
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
        stream->handle = NULL;
    }
    free_stream(stream);
    return AW_RESULT_NO_ERROR;
}

inline aw_result_t aw_terminate() {
    PaError err = Pa_Terminate();
    return err ? aw_result(err, Pa_GetErrorText(err)) : AW_RESULT_NO_ERROR;
}
