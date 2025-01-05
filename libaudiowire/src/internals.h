#define CHANNELS 2
#define SAMPLE_RATE 48000
#define PACKET_DURATION_MS 20
#define RINGBUF_SIZE 65536

#define FORMAT_S16 0

#define FORMAT_TYPE FORMAT_S16

#if FORMAT_TYPE == FORMAT_S16
#define SAMPLE_SIZE 2
#endif

#define error_result(err, ptr, message) \
    if (ptr != NULL) \
        *ptr = message; \
    return err