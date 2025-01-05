#include "audiowire.h"
#include "internals.h"
#include "ringbuf.h"

#include <portaudio.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#if FORMAT_TYPE == FORMAT_S16
#define PA_SAMPLE_FORMAT paInt16
#endif

#define PA_FRAMES_PER_BUFFER (PACKET_DURATION_MS * CHANNELS * SAMPLE_RATE / 1000)

#define audio_bufsize(count) (CHANNELS * SAMPLE_SIZE * count)

struct aw_stream {
    PaStream *handle;
    ringbuf_t *ringbuf;
    const char *devname;
};

static inline bool device_is_valid(const PaDeviceInfo *info, bool is_input) {
    return (is_input && info->maxInputChannels >= CHANNELS) || (!is_input && info->maxOutputChannels >= CHANNELS);
}

static int on_stream_read(const void *input,
                          void *output,
                          unsigned long count,
                          const PaStreamCallbackTimeInfo *info,
                          PaStreamCallbackFlags flags,
                          void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    ringbuf_push(stream->ringbuf, input, audio_bufsize(count));
    return paContinue;
}

static int on_stream_write(const void *input,
                           void *output,
                           unsigned long count,
                           const PaStreamCallbackTimeInfo *info,
                           PaStreamCallbackFlags flags,
                           void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    size_t offset = 0;
    size_t bufsize = audio_bufsize(count);
    size_t size = ringbuf_size(stream->ringbuf);
    if (size < bufsize) {
        offset = bufsize - size;
        memset(output, 0, offset);
    } else if (size > bufsize) {
        size = bufsize;
    }
    ringbuf_pop(stream->ringbuf, output + offset, size);
    return paContinue;
}

static aw_result_t start_stream(aw_stream_t **s, const char *name, bool is_input) {
    const char *message = NULL;
    PaError err = paNoError;

    aw_stream_t *stream = calloc(1, sizeof(aw_stream_t));
    stream->ringbuf = ringbuf_create(RINGBUF_SIZE);

    PaDeviceIndex device = is_input ? Pa_GetDefaultInputDevice() : Pa_GetDefaultOutputDevice();
    const PaDeviceInfo *info = Pa_GetDeviceInfo(device);
    if (name != NULL || !device_is_valid(info, is_input)) {
        device = paNoDevice;
        for (PaDeviceIndex idx = 0; idx < Pa_GetDeviceCount(); idx++) {
            info = Pa_GetDeviceInfo(idx);
            if (!strstr(info->name, name))
                continue;
            if (!device_is_valid(info, is_input))
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

    PaStreamParameters params = {
        .device = device,
        .channelCount = CHANNELS,
        .sampleFormat = PA_SAMPLE_FORMAT,
        .suggestedLatency = is_input ? info->defaultLowInputLatency : info->defaultLowOutputLatency,
        .hostApiSpecificStreamInfo = 0,
    };
    err = Pa_OpenStream(&stream->handle,
                        is_input ? &params : NULL,
                        is_input ? NULL : &params,
                        SAMPLE_RATE,
                        PA_FRAMES_PER_BUFFER,
                        paClipOff,
                        is_input ? on_stream_read : on_stream_write,
                        stream);
    if (err)
        goto pa_error;
    if ((err = Pa_StartStream(stream->handle)))
        goto pa_error;

    *s = stream;
    return aw_result_no_error;

pa_error:
    message = Pa_GetErrorText(err);

error:
    free(stream);
    return aw_result(err, message);
}

aw_result_t aw_initialize() {
    PaError err = Pa_Initialize();
    return err ? aw_result(err, Pa_GetErrorText(err)) : aw_result_no_error;
}

aw_result_t aw_start_record(aw_stream_t **stream, const char *name) {
    return start_stream(stream, name, true);
}

aw_result_t aw_start_playback(aw_stream_t **stream, const char *name) {
    return start_stream(stream, name, false);
}

size_t aw_record_read(aw_stream_t *stream, char *buf, size_t bufsize) {
    return ringbuf_pop(stream->ringbuf, buf, bufsize);
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
    free(stream);
    return aw_result_no_error;
}

aw_result_t aw_terminate() {
    PaError err = Pa_Terminate();
    return err ? aw_result(err, Pa_GetErrorText(err)) : aw_result_no_error;
}