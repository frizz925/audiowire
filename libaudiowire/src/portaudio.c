#include "audiowire.h"
#include "internals.h"

#include <assert.h>
#include <portaudio.h>
#include <stdbool.h>
#include <stdlib.h>
#include <string.h>

#define PA_FRAMES_PER_BUFFER (PACKET_DURATION_MS * CHANNELS * SAMPLE_RATE / 1000)
#define audio_bufsize(count) (CHANNELS * SAMPLE_SIZE * count)

#if FORMAT_TYPE == FORMAT_S16
#define PA_SAMPLE_FORMAT paInt16
#endif

struct aw_stream {
    PaStream *handle;
    aw_stream_read_callback_t *read_cb;
    aw_stream_write_callback_t *write_cb;
    const char *devname;
    void *userdata;
};

static int on_stream_read(const void *input,
                          void *output,
                          unsigned long count,
                          const PaStreamCallbackTimeInfo *info,
                          PaStreamCallbackFlags flags,
                          void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    switch (stream->read_cb(input, audio_bufsize(count), stream->userdata)) {
    case AW_STREAM_STOP:
        return paComplete;
    case AW_STREAM_ABORT:
        return paAbort;
    default:
        return paContinue;
    }
}

static int on_stream_write(const void *input,
                           void *output,
                           unsigned long count,
                           const PaStreamCallbackTimeInfo *info,
                           PaStreamCallbackFlags flags,
                           void *userdata) {
    aw_stream_t *stream = (aw_stream_t *)userdata;
    switch (stream->write_cb(output, audio_bufsize(count), stream->userdata)) {
    case AW_STREAM_STOP:
        return paComplete;
    case AW_STREAM_ABORT:
        return paAbort;
    default:
        return paContinue;
    }
}

static aw_result_t start_stream(aw_stream_t **s, const char *name, void *callback, void *userdata, bool is_input) {
    assert(callback != NULL);

    const char *message = NULL;
    PaError err = paNoError;

    aw_stream_t *stream = malloc(sizeof(aw_stream_t));
    stream->userdata = userdata;
    if (is_input)
        stream->read_cb = (aw_stream_read_callback_t *)callback;
    else
        stream->write_cb = (aw_stream_write_callback_t *)callback;

    PaDeviceIndex device = is_input ? Pa_GetDefaultInputDevice() : Pa_GetDefaultOutputDevice();
    if (name != NULL) {
        device = paNoDevice;
        for (PaDeviceIndex idx = 0; idx < Pa_GetDeviceCount(); idx++) {
            const PaDeviceInfo *info = Pa_GetDeviceInfo(idx);
            if (!strstr(info->name, name))
                continue;
            if ((is_input && info->maxInputChannels < CHANNELS) || (!is_input && info->maxOutputChannels < CHANNELS))
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

    const PaDeviceInfo *info = Pa_GetDeviceInfo(device);
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

aw_result_t
aw_start_record(aw_stream_t **stream, const char *name, aw_stream_read_callback_t *callback, void *userdata) {
    return start_stream(stream, name, callback, userdata, true);
}

aw_result_t
aw_start_playback(aw_stream_t **stream, const char *name, aw_stream_write_callback_t *callback, void *userdata) {
    return start_stream(stream, name, callback, userdata, false);
}

const char *aw_device_name(aw_stream_t *stream) {
    return stream->devname;
}

aw_result_t aw_stop(aw_stream_t *stream) {
    PaError err = paNoError;
    if (Pa_IsStreamActive(stream->handle)) {
        if ((err = Pa_StopStream(stream->handle)))
            return aw_result(err, Pa_GetErrorText(err));
    }
    if ((err = Pa_CloseStream(stream->handle)))
        return aw_result(err, Pa_GetErrorText(err));
    free(stream);
    return aw_result_no_error;
}

aw_result_t aw_terminate() {
    PaError err = Pa_Terminate();
    return err ? aw_result(err, Pa_GetErrorText(err)) : aw_result_no_error;
}