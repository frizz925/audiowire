#include "audiowire.h"

#include <assert.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define CHANNELS 2
#define SAMPLE_RATE 48000
#define SAMPLE_FORMAT AW_SAMPLE_FORMAT_S16
#define PACKET_FRAME_SIZE 960
#define BUFFER_FRAME_SIZE 5760
#define AUDIO_BUFSIZE 65536

#define assert_aw_result(res) check_aw_result(res, __FUNCTION__, __FILE_NAME__, __LINE__, #res)

void check_aw_result(aw_result_t res, const char *function, const char *filename, int line, const char *expr) {
    if (AW_RESULT_IS_OK(res))
        return;
    printf("Result assertion failed in function %s, file %s, line %d: %s\n", function, filename, line, expr);
    printf("Error %d: %s\n", res.code, res.message);
    abort();
}

void on_error(int err, const char *message, void *userdata) {
    (void)(userdata);
    printf("Error %d: %s\n", err, message);
}

int main() {
    char buf[AUDIO_BUFSIZE];
    aw_stream_t *record, *playback;
    aw_config_t config = {
        .channels = CHANNELS,
        .sample_rate = SAMPLE_RATE,
        .sample_format = SAMPLE_FORMAT,
        .buffer_frames = PACKET_FRAME_SIZE,
        .max_buffer_frames = BUFFER_FRAME_SIZE,
    };
    size_t bufsize = sizeof(buf);

    assert_aw_result(aw_initialize());
    assert_aw_result(aw_start_record(&record, NULL, "record-test", config, on_error, NULL));
    assert_aw_result(aw_start_playback(&playback, NULL, "playback-test", config, on_error, NULL));

    assert(aw_device_name(record) != NULL);
    assert(aw_device_name(playback) != NULL);

    size_t read = 0;
    for (;;) {
        read = aw_record_read(record, buf, bufsize);
        if (read > 0) {
            size_t write = aw_playback_write(playback, buf, read);
            assert(read == write);
            break;
        }
        usleep(20 * 1000);
    }

    assert_aw_result(aw_stop(playback));
    assert_aw_result(aw_stop(record));
    assert_aw_result(aw_terminate());

    return 0;
}
