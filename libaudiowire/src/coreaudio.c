#include "internals.h"

#include <AudioToolbox/AudioToolbox.h>

#define AW_RESULT_NOT_IMPLEMENTED aw_result(-1, "Not implemented")

#define WRAP_ERR(e) wrap_error(e, __FUNCTION__, __FILE__, __LINE__, #e)
#define CATCH_ERR(e) \
    result = WRAP_ERR(e); \
    if (result.code != 0) \
    goto error

#define OUTPUT_ELEMENT 0
#define INPUT_ELEMENT 1

#define STREAM_FIELDS \
    aw_stream_base_t base; \
    AudioUnit unit; \
    bool is_input;

struct aw_stream {
    STREAM_FIELDS
};

typedef struct aw_stream_playback {
    STREAM_FIELDS
} aw_stream_playback_t;

typedef struct aw_stream_record {
    STREAM_FIELDS
    AudioBufferList *buflist;
    UInt32 bufsize;
} aw_stream_record_t;

typedef struct audio_device {
    UInt32 id;
    const char *name;
    UInt32 in_channels;
    UInt32 out_channels;
} audio_device_t;

static char err_msg[512];

static UInt32 device_count = 0;
static audio_device_t *devices = NULL;
static int default_input_idx = -1;
static int default_output_idx = -1;

static inline aw_result_t wrap_error(OSStatus status, const char *fn, const char *file, int line, const char *expr) {
    if (!status)
        return AW_RESULT_NO_ERROR;
    snprintf(err_msg, sizeof(err_msg), "Function %s file %s line %d: %s\n", fn, file, line, expr);
    return aw_result(status, err_msg);
}

static OSStatus output_proc(void *refcon,
                            AudioUnitRenderActionFlags *flags,
                            const AudioTimeStamp *timestamp,
                            UInt32 bus,
                            UInt32 frames,
                            AudioBufferList *io_data) {
    aw_stream_base_t *s = (aw_stream_base_t *)refcon;
    for (int i = 0; i < io_data->mNumberBuffers; i++) {
        AudioBuffer *buf = &io_data->mBuffers[i];
        UInt32 bufsize = buf->mDataByteSize;
        if (ringbuf_remaining(s->ringbuf) >= bufsize)
            ringbuf_pop_back_from(s->ringbuf, buf->mData, bufsize, s->max_bufsize);
        else
            memset(buf->mData, 0, bufsize);
    }
    return noErr;
}

static OSStatus input_proc(void *refcon,
                           AudioUnitRenderActionFlags *flags,
                           const AudioTimeStamp *timestamp,
                           UInt32 bus,
                           UInt32 frames,
                           AudioBufferList *io_data) {
    OSStatus err;
    aw_stream_record_t *s = (aw_stream_record_t *)refcon;
    if ((err = AudioUnitRender(s->unit, flags, timestamp, bus, frames, s->buflist))) {
        return err;
    }
    ringbuf_t *rb = s->base.ringbuf;
    for (int i = 0; i < s->buflist->mNumberBuffers; i++) {
        AudioBuffer *buf = &s->buflist->mBuffers[i];
        UInt32 bufsize = buf->mDataByteSize;
        if (ringbuf_available(rb) >= bufsize)
            ringbuf_push(rb, buf->mData, bufsize);
    }
    return noErr;
}

static inline bool is_valid_device(const aw_config_t *cfg, const char *devname, int idx, bool is_output) {
    const audio_device_t *device = devices + idx;
    if (is_output && device->out_channels < cfg->channels)
        return false;
    if (!is_output && device->in_channels < cfg->channels)
        return false;
    if (devname && !strstr(device->name, devname))
        return false;
    return true;
}

static aw_result_t start_stream(aw_stream_t **s,
                                const char *devname,
                                const char *name,
                                aw_config_t cfg,
                                aw_error_callback_t error_cb,
                                void *userdata,
                                bool is_output) {
    int device_idx = is_output ? default_output_idx : default_input_idx;
    if (!is_valid_device(&cfg, devname, device_idx, is_output)) {
        device_idx = -1;
        for (int idx = 0; idx < device_count; idx++) {
            if (is_valid_device(&cfg, devname, idx, is_output)) {
                device_idx = idx;
                break;
            }
        }
    }
    if (device_idx < 0)
        return aw_result(-1, "Device not found");
    const audio_device_t *device = devices + device_idx;
    const AudioDeviceID device_id = device->id;
    const char *device_name = device->name;

    aw_result_t result;
    aw_stream_t *stream = calloc(1, is_output ? sizeof(aw_stream_playback_t) : sizeof(aw_stream_record_t));
    aw_stream_base_t *base = &stream->base;
    aw_stream_base_init(base, cfg, device_name, error_cb, userdata);

    AudioObjectPropertyAddress address = {0};
    AudioComponentInstance unit = NULL;
    AudioBufferList *buflist = NULL;

    // Set device buffer frame size
    UInt32 frames = cfg.buffer_frames;
    address.mSelector = kAudioDevicePropertyBufferFrameSize;
    address.mScope = is_output ? kAudioObjectPropertyScopeOutput : kAudioObjectPropertyScopeInput;
    address.mElement = is_output ? OUTPUT_ELEMENT : INPUT_ELEMENT;
    CATCH_ERR(AudioObjectSetPropertyData(device_id, &address, 0, NULL, sizeof(frames), &frames));

    // Set up buffer list
    UInt32 propsize;
    if (!is_output) {
        AudioObjectPropertyAddress address = {
            .mSelector = kAudioDevicePropertyStreamConfiguration,
            .mScope = kAudioObjectPropertyScopeInput,
            .mElement = kAudioObjectPropertyElementMain,
        };
        CATCH_ERR(AudioObjectGetPropertyDataSize(device_id, &address, 0, NULL, &propsize));

        buflist = calloc(1, propsize);
        CATCH_ERR(AudioObjectGetPropertyData(device_id, &address, 0, NULL, &propsize, buflist));

        aw_stream_record_t *record = (aw_stream_record_t *)stream;
        record->buflist = buflist;
        record->bufsize = propsize;
    }

    // Instantiate audio unit
    AudioComponentDescription desc = {
        .componentType = kAudioUnitType_Output,
        .componentSubType = kAudioUnitSubType_HALOutput,
        .componentManufacturer = kAudioUnitManufacturer_Apple,
        .componentFlags = 0,
        .componentFlagsMask = 0,
    };
    AudioComponent comp = AudioComponentFindNext(NULL, &desc);
    if (!comp)
        return aw_result(-1, "Audio component not found");
    if (AudioComponentInstanceNew(comp, &unit))
        return aw_result(-1, "Failed to create instance");
    stream->unit = unit;

    UInt32 enable_io;

    // Enable/disable output
    enable_io = is_output ? 1 : 0;
    CATCH_ERR(AudioUnitSetProperty(
        unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Output, OUTPUT_ELEMENT, &enable_io, propsize));

    // Enable/disable input
    enable_io = is_output ? 0 : 1;
    CATCH_ERR(AudioUnitSetProperty(
        unit, kAudioOutputUnitProperty_EnableIO, kAudioUnitScope_Input, INPUT_ELEMENT, &enable_io, propsize));

    // Set audio unit device
    CATCH_ERR(AudioUnitSetProperty(unit,
                                   kAudioOutputUnitProperty_CurrentDevice,
                                   kAudioUnitScope_Global,
                                   kAudioObjectPropertyElementMain,
                                   &device_id,
                                   sizeof(AudioDeviceID)));

    // Get device format
    AudioStreamBasicDescription format;
    propsize = sizeof(format);
    CATCH_ERR(AudioUnitGetProperty(unit,
                                   kAudioUnitProperty_StreamFormat,
                                   is_output ? kAudioUnitScope_Output : kAudioUnitScope_Input,
                                   is_output ? OUTPUT_ELEMENT : INPUT_ELEMENT,
                                   &format,
                                   &propsize));

    UInt32 framesize = frame_size(&cfg);
    format.mFormatID = kAudioFormatLinearPCM;
    // Device and application sample rates must match
    // format.mSampleRate = cfg.sample_rate;
    format.mFramesPerPacket = 1;
    format.mBytesPerPacket = framesize;
    format.mBytesPerFrame = framesize;
    format.mChannelsPerFrame = cfg.channels;

    switch (cfg.sample_format) {
    case AW_SAMPLE_FORMAT_S16:
        format.mBitsPerChannel = 16;
        format.mFormatFlags = kAudioFormatFlagIsSignedInteger;
        break;
    case AW_SAMPLE_FORMAT_F32:
        format.mBitsPerChannel = 32;
        format.mFormatFlags = kAudioFormatFlagsNativeFloatPacked;
        break;
    }

    // Set application format
    CATCH_ERR(AudioUnitSetProperty(unit,
                                   kAudioUnitProperty_StreamFormat,
                                   is_output ? kAudioUnitScope_Input : kAudioUnitScope_Output,
                                   is_output ? OUTPUT_ELEMENT : INPUT_ELEMENT,
                                   &format,
                                   sizeof(format)));

    // Set application maximum frames
    CATCH_ERR(AudioUnitSetProperty(unit,
                                   kAudioUnitProperty_MaximumFramesPerSlice,
                                   is_output ? kAudioUnitScope_Input : kAudioUnitScope_Output,
                                   is_output ? OUTPUT_ELEMENT : INPUT_ELEMENT,
                                   &frames,
                                   sizeof(frames)));

    // Set audio unit callback
    AURenderCallbackStruct input = {is_output ? output_proc : input_proc, stream};
    CATCH_ERR(AudioUnitSetProperty(unit,
                                   is_output ? kAudioUnitProperty_SetRenderCallback
                                             : kAudioOutputUnitProperty_SetInputCallback,
                                   kAudioUnitScope_Global,
                                   kAudioObjectPropertyElementMain,
                                   &input,
                                   sizeof(input)));

    // Initialize and start audio unit
    CATCH_ERR(AudioUnitInitialize(unit));
    CATCH_ERR(AudioOutputUnitStart(unit));

    *s = (aw_stream_t *)stream;
    return AW_RESULT_NO_ERROR;

error:
    aw_stream_base_deinit(&stream->base);
    free(stream);
    if (buflist)
        free(buflist);
    if (unit)
        AudioComponentInstanceDispose(unit);

    *s = NULL;
    return result;
}

aw_result_t aw_initialize() {
    aw_result_t result;
    char *name = NULL;
    AudioBufferList *buflist = NULL;

    UInt32 propsize = sizeof(AudioDeviceID);
    AudioObjectPropertyAddress address = {
        .mSelector = 0,
        .mScope = kAudioObjectPropertyScopeGlobal,
        .mElement = kAudioObjectPropertyElementMain,
    };

    // Get default input device id
    AudioDeviceID default_input_id = 0;
    address.mSelector = kAudioHardwarePropertyDefaultInputDevice;
    AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, NULL, &propsize, &default_input_id);

    // Get default output device id
    AudioDeviceID default_output_id = 0;
    address.mSelector = kAudioHardwarePropertyDefaultOutputDevice;
    AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, NULL, &propsize, &default_output_id);

    // Get device count
    address.mSelector = kAudioHardwarePropertyDevices;
    CATCH_ERR(AudioObjectGetPropertyDataSize(kAudioObjectSystemObject, &address, 0, NULL, &propsize));
    device_count = propsize / sizeof(AudioDeviceID);

    // Get device IDs
    AudioDeviceID *device_ids = calloc(device_count, sizeof(AudioDeviceID));
    CATCH_ERR(AudioObjectGetPropertyData(kAudioObjectSystemObject, &address, 0, NULL, &propsize, device_ids));

    // Gather device information
    devices = calloc(device_count, sizeof(audio_device_t));
    for (int idx = 0; idx < device_count; idx++) {
        AudioDeviceID device_id = device_ids[idx];
        if (default_input_id == device_id || default_input_id == 0)
            default_input_idx = idx;
        if (default_output_id == device_id || default_output_id == 0)
            default_output_idx = idx;

        audio_device_t *device = devices + idx;
        device->id = device_id;

        // Get device name
        CFStringRef name_ref;
        UInt32 propsize = sizeof(CFStringRef);
        AudioObjectPropertyAddress address = {
            .mSelector = kAudioDevicePropertyDeviceNameCFString,
            .mScope = kAudioObjectPropertyScopeGlobal,
            .mElement = kAudioObjectPropertyElementMain,
        };
        if (AudioObjectGetPropertyData(device_id, &address, 0, NULL, &propsize, &name_ref)) {
            address.mSelector = kAudioDevicePropertyDeviceName;
            CATCH_ERR(AudioObjectGetPropertyDataSize(device_id, &address, 0, NULL, &propsize));
            name = calloc(propsize + 1, sizeof(char));
            CATCH_ERR(AudioObjectGetPropertyData(device_id, &address, 0, NULL, &propsize, name));
        } else {
            propsize = CFStringGetMaximumSizeForEncoding(CFStringGetLength(name_ref), kCFStringEncodingUTF8);
            name = calloc(propsize + 1, sizeof(char));
            CFStringGetCString(name_ref, name, propsize + 1, kCFStringEncodingUTF8);
        }
        device->name = name;
        name = NULL;

        // Get device input channels
        address.mSelector = kAudioDevicePropertyStreamConfiguration;
        address.mScope = kAudioObjectPropertyScopeInput;
        address.mElement = INPUT_ELEMENT;
        CATCH_ERR(AudioObjectGetPropertyDataSize(device_id, &address, 0, NULL, &propsize));

        buflist = calloc(1, propsize);
        CATCH_ERR(AudioObjectGetPropertyData(device_id, &address, 0, NULL, &propsize, buflist));

        for (int i = 0; i < buflist->mNumberBuffers; i++)
            device->in_channels += buflist->mBuffers[i].mNumberChannels;
        free(buflist);
        buflist = NULL;

        // Get device output channels
        address.mScope = kAudioObjectPropertyScopeOutput;
        address.mElement = OUTPUT_ELEMENT;
        CATCH_ERR(AudioObjectGetPropertyDataSize(device_id, &address, 0, NULL, &propsize));

        buflist = calloc(1, propsize);
        CATCH_ERR(AudioObjectGetPropertyData(device_id, &address, 0, NULL, &propsize, buflist));

        for (int i = 0; i < buflist->mNumberBuffers; i++)
            device->out_channels += buflist->mBuffers[i].mNumberChannels;
        free(buflist);
        buflist = NULL;
    }

    return AW_RESULT_NO_ERROR;

error:
    if (name)
        free(name);
    if (buflist)
        free(buflist);
    return result;
}

inline aw_result_t aw_start_record(aw_stream_t **stream,
                                   const char *devname,
                                   const char *name,
                                   aw_config_t cfg,
                                   aw_error_callback_t error_cb,
                                   void *userdata) {
    return start_stream(stream, devname, name, cfg, error_cb, userdata, false);
}

inline aw_result_t aw_start_playback(aw_stream_t **stream,
                                     const char *devname,
                                     const char *name,
                                     aw_config_t cfg,
                                     aw_error_callback_t error_cb,
                                     void *userdata) {
    return start_stream(stream, devname, name, cfg, error_cb, userdata, true);
}

aw_result_t aw_stop(aw_stream_t *stream) {
    aw_result_t res;
    res = WRAP_ERR(AudioOutputUnitStop(stream->unit));
    if (AW_RESULT_IS_ERR(res))
        return res;
    res = WRAP_ERR(AudioComponentInstanceDispose(stream->unit));
    if (AW_RESULT_IS_ERR(res))
        return res;

    if (stream->is_input) {
        aw_stream_record_t *record = (aw_stream_record_t *)stream;
        free(record->buflist);
    }
    aw_stream_base_deinit(&stream->base);
    free(stream);

    return AW_RESULT_NO_ERROR;
}

aw_result_t aw_terminate() {
    for (int i = 0; i < device_count; i++) {
        audio_device_t *device = devices + i;
        if (device->name)
            free((void *)device->name);
    }
    free(devices);

    default_input_idx = -1;
    default_output_idx = -1;
    device_count = 0;
    devices = NULL;

    return AW_RESULT_NO_ERROR;
}