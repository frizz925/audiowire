#include "../include/audiowire2.h"

#include <assert.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define CHANNELS 2
#define SAMPLE_RATE 48000
#define SAMPLE_FORMAT AW_SAMPLE_FORMAT_S16
#define PACKET_SAMPLE_COUNT 960
#define BUFFER_SAMPLE_COUNT 5760
#define AUDIO_BUFSIZE 65536

#define assert_aw_result(res) check_aw_result(res, __FUNCTION__, __FILE_NAME__, __LINE__, #res)

void check_aw_result(aw_result_t res, const char *function, const char *filename, int line, const char *expr) {
    if (AW_RESULT_IS_OK(res))
        return;
    printf("Result assertion failed in function %s, file %s, line %d: %s\n", function, filename, line, expr);
    printf("Error %d: %s\n", res.code, res.message);
    abort();
}

void on_read(const char *buf, size_t len, void *userdata) {
    (void)(buf);
    *((atomic_size_t *)userdata) += len;
}

void on_write(char *buf, size_t len, void *userdata) {
    (void)(buf);
    *((atomic_size_t *)userdata) += len;
}

void on_error(int err, const char *message, void *userdata) {
    (void)(userdata);
    printf("Error %d: %s\n", err, message);
}

int main() {
    aw_stream_t *record, *playback;
    aw_config_t config = {
        .channels = CHANNELS,
        .sample_rate = SAMPLE_RATE,
        .sample_format = SAMPLE_FORMAT,
        .buffer_samples = PACKET_SAMPLE_COUNT,
        .max_buffer_samples = BUFFER_SAMPLE_COUNT,
    };

    atomic_size_t read_bytes = 0;
    atomic_size_t write_bytes = 0;

    assert_aw_result(aw_initialize());
    assert_aw_result(aw_start(&record, NULL, "record-test", config, on_read, NULL, on_error, &read_bytes));
    assert_aw_result(aw_start(&playback, NULL, "playback-test", config, NULL, on_write, on_error, &write_bytes));

    assert(aw_device_name(record) != NULL);
    assert(aw_sample_rate(record) > 0);

    assert(aw_device_name(playback) != NULL);
    assert(aw_sample_rate(playback) > 0);

    for (;;) {
        if (read_bytes > 0 && write_bytes > 0)
            break;
        usleep(20 * 1000);
    }

    assert_aw_result(aw_stop(playback));
    assert_aw_result(aw_stop(record));
    assert_aw_result(aw_terminate());

    return 0;
}
